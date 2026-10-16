//! PDF Splitter: one PDF, split into a PDF per page, per run of N pages, or per typed range
//! (into a new folder `<name> pages`), or narrowed to the typed pages (Extract, order and
//! repeats kept) or to the rest (Remove). Wires the `PdfSplitter` global
//! (ui/tools/pdf-splitter.slint); thumbnails render on a worker. In the range modes, clicking
//! pages writes their ranges into the text field and typing ranges selects the pages, so the
//! text is always what will be split.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use ::image::RgbaImage;
use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};

use super::pdf_toolkit::{PDF_FILTER, Outcome, Task, TaskUpdate, count, get_pdfium, is_pdf, slint_image, unpictured_note};
use crate::engine::pdf::{self, PageRef};
use crate::files::{self, candidate_name, file_name, format_size, reserve_dir, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, PdfPageTile, PdfSplitter};

pub const SLUG: &str = "pdf-splitter";
/// Thumbnails render at twice the tile's size, for high-density screens.
const THUMB_EDGE: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    EveryPage,
    Chunks,
    Ranges,
    Extract,
    Remove,
}

impl Mode {
    const ALL: [Mode; 5] = [Mode::EveryPage, Mode::Chunks, Mode::Ranges, Mode::Extract, Mode::Remove];

    fn label(self) -> &'static str {
        match self {
            Mode::EveryPage => "Every page",
            Mode::Chunks => "Every few pages",
            Mode::Ranges => "By ranges",
            Mode::Extract => "Extract pages",
            Mode::Remove => "Remove pages",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Mode::EveryPage => "Saves each page as its own PDF, in a new folder.",
            Mode::Chunks => "Saves each run of pages as its own PDF, in a new folder.",
            Mode::Ranges => "Saves each range as its own PDF, in a new folder. Click pages to add them.",
            Mode::Extract => "Saves the pages typed as one PDF, in the order typed. Repeats are kept.",
            Mode::Remove => "Saves a copy without the pages typed or clicked.",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Mode::Extract => "Extract",
            Mode::Remove => "Remove Pages",
            _ => "Split",
        }
    }

    fn uses_ranges(self) -> bool {
        matches!(self, Mode::Ranges | Mode::Extract | Mode::Remove)
    }

    /// Whether the result is one PDF rather than a folder of them.
    fn single(self) -> bool {
        matches!(self, Mode::Extract | Mode::Remove)
    }
}

/// The PDFs a split writes, as (file name, 0-based pages). `stem` names the single-file modes'
/// output. Every name is unique, so no output overwrites another.
fn plan(mode: Mode, page_count: u32, ranges: &str, chunk: u32, stem: &str) -> Result<Vec<(String, Vec<u32>)>, String> {
    let pad = page_count.max(1).to_string().len();
    let run_name = |a: u32, b: u32| {
        if a == b {
            format!("Page {:0pad$}", a + 1)
        } else {
            format!("Pages {:0pad$}-{:0pad$}", a + 1, b + 1)
        }
    };
    let named: Vec<(String, Vec<u32>)> = match mode {
        Mode::EveryPage => (0..page_count).map(|i| (run_name(i, i), vec![i])).collect(),
        Mode::Chunks => {
            let chunk = chunk.max(1);
            (0..page_count)
                .step_by(chunk as usize)
                .map(|start| {
                    let end = (start + chunk).min(page_count) - 1;
                    (run_name(start, end), (start..=end).collect())
                })
                .collect()
        }
        Mode::Ranges => pdf::parse_ranges(ranges, page_count)?
            .into_iter()
            .map(|r| (run_name(*r.start() - 1, *r.end() - 1), r.map(|p| p - 1).collect()))
            .collect(),
        Mode::Extract => {
            let pages = pdf::flatten(&pdf::parse_ranges(ranges, page_count)?);
            vec![(format!("{stem} extract"), pages)]
        }
        Mode::Remove => {
            let gone: HashSet<u32> = pdf::flatten(&pdf::parse_ranges(ranges, page_count)?).into_iter().collect();
            let kept: Vec<u32> = (0..page_count).filter(|p| !gone.contains(p)).collect();
            if kept.is_empty() {
                return Err("That removes every page, which would leave an empty PDF.".into());
            }
            vec![(format!("{stem} edited"), kept)]
        }
    };
    let mut used = HashSet::new();
    Ok(named
        .into_iter()
        .map(|(name, pages)| {
            let unique = (0..)
                .map(|n| candidate_name(&name, "pdf", n))
                .find(|c| used.insert(c.clone()))
                .unwrap_or_default();
            (unique, pages)
        })
        .collect())
}

struct Source {
    path: PathBuf,
    size: u64,
    /// `None` while counting.
    pages: Option<Result<u32, String>>,
}

#[derive(Default)]
struct State {
    source: Option<Source>,
    selected: Vec<bool>,
    thumbs: Vec<Option<Image>>,
    tiles: Rc<VecModel<PdfPageTile>>,
    task: Task,
    /// Cancels the counting and thumbnails for the current file; replaced with each new file.
    reading: Arc<CancelToken>,
    readers: Vec<JoinHandle<()>>,
    out_dir: Option<PathBuf>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn init(app: &AppWindow) {
    let g = app.global::<PdfSplitter>();
    g.set_pdfium_found(pdf::pdfium().is_some());
    g.set_modes(select::options(Mode::ALL.iter().map(|m| m.label())));
    with(|s| g.set_pages(ModelRc::from(s.tiles.clone())));
    sync(app);

    let weak = app.as_weak();
    g.on_choose_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose a PDF to Split", Some(PDF_FILTER)) {
            add_paths(&app, vec![path]);
        }
    });
    let weak = app.as_weak();
    g.on_toggle(move |i| {
        let Some(app) = weak.upgrade() else { return };
        let ranges = with(|s| {
            let selected = s.selected.get_mut(i as usize)?;
            *selected = !*selected;
            Some(selection_text(&s.selected))
        });
        if let Some(ranges) = ranges {
            app.global::<PdfSplitter>().set_ranges(ranges.into());
        }
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_select_all(move |on| {
        let Some(app) = weak.upgrade() else { return };
        let ranges = with(|s| {
            s.selected.iter_mut().for_each(|p| *p = on);
            selection_text(&s.selected)
        });
        app.global::<PdfSplitter>().set_ranges(ranges.into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_settings_changed(move || {
        let Some(app) = weak.upgrade() else { return };
        select_typed(&app);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_split(move || {
        if let Some(app) = weak.upgrade() {
            split(&app);
        }
    });
    let weak = app.as_weak();
    g.on_cancel(move || {
        with(|s| s.task.cancel());
        if let Some(app) = weak.upgrade() {
            sync(&app);
        }
    });
    g.on_reveal(|| {
        if let Some(output) = with(|s| s.task.output()) {
            files::reveal(&output);
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Split PDFs To") {
            with(|s| s.out_dir = Some(dir));
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_reset_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.out_dir = None);
        sync(&app);
    });
    g.on_get_pdfium(get_pdfium);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<PdfSplitter>();
    g.set_drop_hover(hover && g.get_pdfium_found());
}

/// Opens the first PDF in `paths` in place of the current one, then counts its pages and
/// renders its thumbnails on a worker.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let Some(path) = paths.into_iter().find(|p| is_pdf(p) && p.is_file()) else {
        with(|s| s.task.show_error("Only a PDF can be split. Drop a .pdf file.".into()));
        sync(app);
        return;
    };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let token = with(|s| {
        if s.task.is_running() {
            return None;
        }
        s.reading.cancel();
        s.reading = Arc::default();
        s.source = Some(Source { path: path.clone(), size, pages: None });
        s.selected.clear();
        s.thumbs.clear();
        s.task.clear();
        Some(s.reading.clone())
    });
    let Some(token) = token else { return };
    app.global::<PdfSplitter>().set_cursor(0);
    refresh(app);

    let weak = app.as_weak();
    let reader = thread::spawn(move || {
        let counted = pdf::page_count(pdfium, &path);
        let ok = counted.is_ok();
        let token_ = token.clone();
        jobs::post(&weak, move |app| {
            with(|s| {
                if !Arc::ptr_eq(&s.reading, &token_) {
                    return;
                }
                if let (Some(source), Ok(n)) = (s.source.as_mut(), &counted) {
                    s.selected = vec![false; *n as usize];
                    s.thumbs = vec![None; *n as usize];
                    source.pages = Some(Ok(*n));
                } else if let (Some(source), Err(message)) = (s.source.as_mut(), counted) {
                    source.pages = Some(Err(message));
                }
            });
            select_typed(&app);
            refresh(&app);
        });
        if !ok {
            return;
        }
        let rendered = pdf::thumbnails(pdfium, &path, THUMB_EDGE, &token, |index, image| {
            let token = token.clone();
            jobs::post(&weak, move |_| set_thumb(&token, index, &image));
        });
        match rendered {
            Ok(0) => {}
            Ok(left_out) => jobs::post(&weak, move |app| {
                with(|s| {
                    if Arc::ptr_eq(&s.reading, &token) {
                        s.task.show_error(unpictured_note(left_out));
                    }
                });
                refresh(&app);
            }),
            Err(e) => eprintln!("thumbnails for {}: {e}", path.display()),
        }
    });
    with(|s| {
        s.readers.retain(|r| !r.is_finished());
        s.readers.push(reader);
    });
}

fn set_thumb(token: &Arc<CancelToken>, index: u32, image: &RgbaImage) {
    let thumb = slint_image(image);
    with(|s| {
        let i = index as usize;
        if !Arc::ptr_eq(&s.reading, token) || i >= s.thumbs.len() {
            return;
        }
        s.thumbs[i] = Some(thumb);
        if let Some(mut tile) = s.tiles.row_data(i) {
            tile.image = s.thumbs[i].clone().unwrap_or_default();
            s.tiles.set_row_data(i, tile);
        }
    });
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        s.reading.cancel();
        let mut workers = s.task.shutdown();
        workers.append(&mut s.readers);
        workers
    })
}

fn selection_text(selected: &[bool]) -> String {
    let pages: Vec<u32> = (0..selected.len() as u32).filter(|&i| selected[i as usize]).collect();
    pdf::format_pages(&pages)
}

/// Selects the pages the typed ranges name, when they can be read.
fn select_typed(app: &AppWindow) {
    let text = app.global::<PdfSplitter>().get_ranges();
    with(|s| {
        let Some(Some(Ok(n))) = s.source.as_ref().map(|src| src.pages.clone()) else {
            return;
        };
        if let Ok(ranges) = pdf::parse_ranges(&text, n) {
            let picked: HashSet<u32> = pdf::flatten(&ranges).into_iter().collect();
            for (i, selected) in s.selected.iter_mut().enumerate() {
                *selected = picked.contains(&(i as u32));
            }
        }
    });
}

fn mode(g: &PdfSplitter) -> Mode {
    Mode::ALL[index(g.get_mode_index(), Mode::ALL.len())]
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into())
}

fn split(app: &AppWindow) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let g = app.global::<PdfSplitter>();
    let mode = mode(&g);
    let chunk = g.get_chunk_size().round().max(1.0) as u32;
    let ranges = g.get_ranges();
    let picked = with(|s| {
        let source = s.source.as_ref()?;
        let Some(Ok(n)) = source.pages else { return None };
        let files = plan(mode, n, &ranges, chunk, &stem(&source.path)).ok()?;
        (!s.task.is_running()).then(|| (source.path.clone(), files, s.out_dir.clone()))
    });
    let Some((input, planned, out_dir)) = picked else { return };
    let activity = if mode.single() { "Saving" } else { "Splitting" };
    with(|s| {
        s.task.start(app, activity, apply, move |cancel, progress| {
            let sources = [input.clone()];
            let refs = |pages: &[u32]| pages.iter().map(|&i| PageRef::new(0, i)).collect::<Vec<_>>();
            if mode.single() {
                let (name, pages) = &planned[0];
                let out = reserve_output(&input.with_file_name(name), out_dir.as_deref(), "pdf")
                    .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
                pdf::assemble(pdfium, &sources, &refs(pages), out.path(), cancel, progress)?;
                let saved = out.keep().map_err(|e| JobError::Failed(format!("Couldn't save the PDF: {e}")))?;
                return Ok(Outcome {
                    message: format!("Saved {} · {}", file_name(&saved), count(pages.len(), "page", "pages")),
                    reveal: saved,
                });
            }
            let folder = reserve_dir(&input, out_dir.as_deref(), " pages")
                .map_err(|e| JobError::Failed(format!("Couldn't create the output folder: {e}")))?;
            let outputs: Vec<(PathBuf, Vec<PageRef>)> = planned
                .iter()
                .map(|(name, pages)| (folder.path().join(name), refs(pages)))
                .collect();
            pdf::assemble_each(pdfium, &sources, &outputs, cancel, progress)?;
            let kept = folder.keep();
            Ok(Outcome {
                message: format!("Saved {} in “{}”", count(outputs.len(), "PDF", "PDFs"), file_name(&kept)),
                reveal: kept,
            })
        })
    });
    sync(app);
}

fn apply(app: &AppWindow, token: &Arc<CancelToken>, update: TaskUpdate) {
    with(|s| s.task.update(token, update));
    sync(app);
}

/// Rebuilds the tiles from the selection and thumbnails, then everything else.
fn refresh(app: &AppWindow) {
    with(|s| {
        let tiles: Vec<PdfPageTile> = s
            .selected
            .iter()
            .zip(&s.thumbs)
            .enumerate()
            .map(|(i, (selected, thumb))| PdfPageTile {
                image: thumb.clone().unwrap_or_default(),
                label: (i + 1).to_string().into(),
                selected: *selected,
                turns: 0,
            })
            .collect();
        s.tiles.set_vec(tiles);
    });
    sync(app);
}

fn sync(app: &AppWindow) {
    let g = app.global::<PdfSplitter>();
    let mode = mode(&g);
    let chunk = g.get_chunk_size().round().max(1.0) as u32;
    let ranges = g.get_ranges();
    g.set_uses_ranges(mode.uses_ranges());
    g.set_uses_chunks(mode == Mode::Chunks);
    g.set_mode_note(mode.note().into());
    g.set_action_text(mode.action().into());
    with(|s| {
        g.set_has_file(s.source.is_some());
        let (name, detail, failed, planned) = match &s.source {
            None => (String::new(), String::new(), false, Err(String::new())),
            Some(src) => {
                let size = format_size(src.size);
                match &src.pages {
                    None => (file_name(&src.path), format!("{size} · Reading…"), false, Err(String::new())),
                    Some(Err(message)) => (file_name(&src.path), message.clone(), true, Err(String::new())),
                    Some(Ok(n)) => (
                        file_name(&src.path),
                        format!("{} · {size}", count(*n as usize, "page", "pages")),
                        false,
                        plan(mode, *n, &ranges, chunk, &stem(&src.path)),
                    ),
                }
            }
        };
        g.set_file_name(name.into());
        g.set_file_detail(detail.into());
        g.set_file_failed(failed);
        g.set_ranges_error(planned.as_ref().err().cloned().unwrap_or_default().into());
        g.set_can_split(planned.is_ok() && !s.task.is_running());
        g.set_running(s.task.is_running());
        g.set_status(s.task.status());
        let last = s.selected.len().saturating_sub(1) as i32;
        g.set_cursor(g.get_cursor().clamp(0, last));
        g.set_custom_output(s.out_dir.is_some());
        g.set_output_folder(
            s.out_dir
                .as_deref()
                .map(file_name)
                .unwrap_or_else(|| "Same as original".into())
                .into(),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_names_files_padded_to_the_page_count() {
        let files = plan(Mode::EveryPage, 12, "", 1, "doc").unwrap();

        assert_eq!((files[0].0.as_str(), files.len()), ("Page 01.pdf", 12));
    }

    #[test]
    fn chunks_end_with_the_pages_left_over() {
        let files = plan(Mode::Chunks, 12, "", 5, "doc").unwrap();

        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Pages 01-05.pdf", "Pages 06-10.pdf", "Pages 11-12.pdf"]);
    }

    #[test]
    fn typing_a_range_twice_still_gives_unique_names() {
        let files = plan(Mode::Ranges, 9, "1-3, 1-3", 1, "doc").unwrap();

        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Pages 1-3.pdf", "Pages 1-3 (1).pdf"]);
    }

    #[test]
    fn removing_keeps_the_other_pages_in_order() {
        let files = plan(Mode::Remove, 5, "2, 4", 1, "doc").unwrap();

        assert_eq!(files, [("doc edited.pdf".to_string(), vec![0, 2, 4])]);
    }

    #[test]
    fn removing_every_page_is_refused() {
        let error = plan(Mode::Remove, 3, "1-3", 1, "doc").unwrap_err();

        assert_eq!(error, "That removes every page, which would leave an empty PDF.");
    }

    #[test]
    fn clicked_pages_become_ranges() {
        let text = selection_text(&[true, true, false, true]);

        assert_eq!(text, "1-2, 4");
    }
}
