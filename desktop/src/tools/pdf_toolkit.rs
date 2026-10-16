//! PDF Toolkit: pages from one or more PDFs in a grid, to select, turn, reorder and delete, then
//! save as one PDF, extract the selection, or split into a PDF per page, optionally compressed.
//! Wires the `PdfToolkit` global (ui/tools/pdf-toolkit.slint) to `engine::pdf`; the thumbnails
//! render on a worker and arrive one by one.
//!
//! It also holds what every PDF tool shares: `Task`, a tool's one background job behind its
//! status line, and the helpers in the first section.

use std::cell::RefCell;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use ::image::RgbaImage;
use pdfium_render::prelude::Pdfium;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::engine::pdf::{self, CompressMode, CompressSettings, Compressed, PageRef};
use crate::file_queue::{FileQueue, Summary};
use crate::files::{self, file_name, format_size};
use crate::jobs::{self, CancelToken, JobError};
use crate::select;
use crate::{AppWindow, FileStatus, PdfPageTile, PdfTaskState, PdfTaskStatus, PdfToolkit};

pub const SLUG: &str = "pdf-toolkit";
/// Thumbnails render at twice the tile's size, for high-density screens.
const THUMB_EDGE: u32 = 256;
/// Page counts read at once when files are added.
pub const CONCURRENT_COUNTS: usize = 4;

// ---- Shared by the PDF tools --------------------------------------------------------------

pub const PDF_FILTER: files::Filter = ("PDF", &["pdf"]);
/// Resolutions images are downscaled to when compressing.
pub const DPI_CHOICES: [u32; 5] = [72, 96, 150, 200, 300];
pub const DEFAULT_DPI: i32 = 2;

pub fn dpi_labels(choices: &[u32]) -> ModelRc<slint::SharedString> {
    let labels: Vec<String> = choices.iter().map(|d| format!("{d} dpi")).collect();
    select::options(labels.iter().map(String::as_str))
}

/// Whether a dropped path names a PDF. Its content is checked when pdfium opens it.
pub fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// The missing-library screen's action.
pub fn get_pdfium() {
    if let Err(e) = open::that(pdf::PDFIUM_DOWNLOADS) {
        eprintln!("couldn't open the PDFium download page: {e}");
    }
}

/// A rendered thumbnail, for the UI thread.
pub fn slint_image(image: &RgbaImage) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        image.as_raw(),
        image.width(),
        image.height(),
    ))
}

/// "1 page", "3 pages".
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The note for pages past `pdf::MAX_THUMBNAIL_PAGES`, which get no thumbnail.
pub fn unpictured_note(left_out: usize) -> String {
    format!(
        "{} past the first {} have no preview, but are still included.",
        count(left_out, "page", "pages"),
        pdf::MAX_THUMBNAIL_PAGES
    )
}

/// A `FileQueue` row's page count: `None` until it's read.
#[derive(Default)]
pub struct PageCount(pub Option<u32>);

impl Summary for PageCount {
    fn summary(&self) -> String {
        self.0
            .map(|n| count(n as usize, "page", "pages"))
            .unwrap_or_default()
    }
}

/// Lists PDFs in a `FileQueue` (from a drop, a dialog or the snapshot hook) and counts their
/// pages in the background: rows turn Ready, or Failed with the reason. Returns the workers.
pub fn add_to_queue(
    app: &AppWindow,
    queue: &mut FileQueue<PageCount>,
    paths: Vec<PathBuf>,
    apply_count: fn(&AppWindow, u64, Result<u32, String>),
) -> Vec<JoinHandle<()>> {
    let Some(pdfium) = pdf::pdfium() else {
        return Vec::new();
    };
    let added = queue.add(paths.into_iter().filter(|p| is_pdf(p)).collect(), FileStatus::Reading, PageCount::default);
    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_COUNTS, move |(id, path)| {
        let pages = pdf::page_count(pdfium, &path);
        jobs::post(&weak, move |app| apply_count(&app, id, pages));
    })
}

/// A page count arriving for a `FileQueue` row (see `add_to_queue`).
pub fn set_count(queue: &mut FileQueue<PageCount>, id: u64, pages: Result<u32, String>) {
    queue.update(id, |e| match pages {
        Ok(n) => {
            e.data = PageCount(Some(n));
            e.status = FileStatus::Ready;
        }
        Err(message) => {
            e.status = FileStatus::Failed;
            e.message = message;
        }
    });
}

/// What a finished job says, and the file or folder Show in Folder reveals.
pub struct Outcome {
    pub message: String,
    pub reveal: PathBuf,
}

pub enum TaskUpdate {
    Progress(f32),
    Finished(Result<Outcome, JobError>),
}

/// A tool's one background job (a merge, a split, a combined PDF) and its status line. Like a
/// `FileQueue` row: each run has its own token, and updates from a replaced run are ignored.
#[derive(Default)]
pub struct Task {
    token: Option<Arc<CancelToken>>,
    workers: Vec<JoinHandle<()>>,
    state: PdfTaskState,
    text: String,
    progress: f32,
    reveal: Option<PathBuf>,
}

impl Task {
    pub fn is_running(&self) -> bool {
        self.token.is_some()
    }

    pub fn status(&self) -> PdfTaskStatus {
        PdfTaskStatus {
            state: self.state,
            text: self.text.as_str().into(),
            progress: self.progress,
        }
    }

    /// What Show in Folder reveals after a success.
    pub fn output(&self) -> Option<PathBuf> {
        self.reveal.clone()
    }

    pub fn cancel(&self) {
        if let Some(token) = &self.token {
            token.cancel();
        }
    }

    /// Shows a problem that isn't the job's own (a file that couldn't be added), unless a job is
    /// running and its status matters more.
    pub fn show_error(&mut self, message: String) {
        if !self.is_running() {
            self.state = PdfTaskState::Failed;
            self.text = message;
            self.reveal = None;
        }
    }

    /// Clears the last result once the input changes, so it never describes other files.
    pub fn clear(&mut self) {
        if !self.is_running() {
            self.state = PdfTaskState::Idle;
            self.text.clear();
            self.reveal = None;
        }
    }

    /// Cancels the job and hands back its thread, for `tools::shutdown_all` to join.
    pub fn shutdown(&mut self) -> Vec<JoinHandle<()>> {
        self.cancel();
        std::mem::take(&mut self.workers)
    }

    /// Runs `work` on its own thread, showing `activity` ("Merging") with its progress. Every
    /// report reaches the tool through `apply`, which hands it to `update` and refreshes. A
    /// panic fails the job instead of leaving it running forever.
    pub fn start(
        &mut self,
        app: &AppWindow,
        activity: &str,
        apply: fn(&AppWindow, &Arc<CancelToken>, TaskUpdate),
        work: impl FnOnce(&CancelToken, &dyn Fn(f32)) -> Result<Outcome, JobError> + Send + 'static,
    ) {
        let token = Arc::new(CancelToken::default());
        self.token = Some(token.clone());
        self.state = PdfTaskState::Running;
        self.text = activity.into();
        self.progress = 0.0;
        self.reveal = None;
        self.workers.retain(|w| !w.is_finished());
        let weak = app.as_weak();
        self.workers.push(thread::spawn(move || {
            let post = |update: TaskUpdate| {
                let token = token.clone();
                jobs::post(&weak, move |app| apply(&app, &token, update));
            };
            let progress = |p| post(TaskUpdate::Progress(p));
            let result = panic::catch_unwind(AssertUnwindSafe(|| work(&token, &progress)))
                .unwrap_or_else(|_| {
                    eprintln!("a PDF job panicked");
                    Err(JobError::Failed(
                        "Something went wrong. Try again, or try other files.".into(),
                    ))
                });
            post(TaskUpdate::Finished(result));
        }));
    }

    /// Applies a worker's report, unless a newer run has replaced the one that sent it.
    pub fn update(&mut self, token: &Arc<CancelToken>, update: TaskUpdate) {
        if !self.token.as_ref().is_some_and(|t| Arc::ptr_eq(t, token)) {
            return;
        }
        match update {
            TaskUpdate::Progress(p) => self.progress = p,
            TaskUpdate::Finished(result) => {
                self.token = None;
                self.progress = 0.0;
                match result {
                    Ok(outcome) => {
                        self.state = PdfTaskState::Done;
                        self.text = outcome.message;
                        self.reveal = Some(outcome.reveal);
                    }
                    Err(JobError::Cancelled) => {
                        self.state = PdfTaskState::Idle;
                        self.text.clear();
                    }
                    Err(JobError::Failed(message)) => {
                        self.state = PdfTaskState::Failed;
                        self.text = message;
                    }
                }
            }
        }
    }
}

/// Moves every item `selected` picks one place earlier (`delta` < 0) or later, as a block: an
/// item only trades places with an unpicked neighbour, so a selection at an edge stays put.
pub fn shift<T>(items: &mut [T], selected: impl Fn(&T) -> bool, delta: i32) {
    if delta < 0 {
        for i in 1..items.len() {
            if selected(&items[i]) && !selected(&items[i - 1]) {
                items.swap(i - 1, i);
            }
        }
    } else {
        for i in (0..items.len().saturating_sub(1)).rev() {
            if selected(&items[i]) && !selected(&items[i + 1]) {
                items.swap(i, i + 1);
            }
        }
    }
}

// ---- The toolkit ---------------------------------------------------------------------------

struct Page {
    source: usize,
    index: u32,
    /// Quarter turns clockwise, not wrapped, so the thumbnail always turns the way it was asked.
    turns: i32,
    selected: bool,
    thumb: Option<Image>,
}

impl Page {
    fn reference(&self) -> PageRef {
        PageRef {
            source: self.source,
            index: self.index,
            turns: self.turns.rem_euclid(4) as u8,
        }
    }
}

#[derive(Default)]
struct State {
    /// Every PDF added, in order; pages refer to them by position, so failed ones stay listed.
    sources: Vec<PathBuf>,
    pages: Vec<Page>,
    tiles: Rc<VecModel<PdfPageTile>>,
    task: Task,
    /// Cancels the page counting and thumbnails for the current pages; replaced on Clear.
    reading: Arc<CancelToken>,
    readers: Vec<JoinHandle<()>>,
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
    let g = app.global::<PdfToolkit>();
    g.set_pdfium_found(pdf::pdfium().is_some());
    g.set_resolutions(dpi_labels(&DPI_CHOICES));
    g.set_resolution_index(DEFAULT_DPI);
    with(|s| g.set_pages(ModelRc::from(s.tiles.clone())));
    sync(app);

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose PDFs", Some(PDF_FILTER)) {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_toggle(move |i| {
        edit(&weak, |s| {
            if let Some(page) = s.pages.get_mut(i as usize) {
                page.selected = !page.selected;
            }
        })
    });
    let weak = app.as_weak();
    g.on_select_all(move |on| edit(&weak, |s| s.pages.iter_mut().for_each(|p| p.selected = on)));
    let weak = app.as_weak();
    g.on_rotate_selected(move |delta| {
        edit(&weak, |s| {
            s.pages
                .iter_mut()
                .filter(|p| p.selected)
                .for_each(|p| p.turns += delta);
        })
    });
    let weak = app.as_weak();
    g.on_move_selected(move |delta| {
        let Some(app) = weak.upgrade() else { return };
        edit(&weak, |s| shift(&mut s.pages, |p| p.selected, delta));
        // The current page follows the selection it was on.
        let g = app.global::<PdfToolkit>();
        if let Some(i) = with(|s| s.pages.iter().position(|p| p.selected)) {
            g.set_cursor(i as i32);
        }
    });
    let weak = app.as_weak();
    g.on_delete_selected(move || edit(&weak, |s| s.pages.retain(|p| !p.selected)));
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            if s.task.is_running() {
                return;
            }
            s.reading.cancel();
            s.reading = Arc::default();
            s.sources.clear();
            s.pages.clear();
            s.task.clear();
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_save(move || {
        if let Some(app) = weak.upgrade() {
            save(&app, false);
        }
    });
    let weak = app.as_weak();
    g.on_extract_selected(move || {
        if let Some(app) = weak.upgrade() {
            save(&app, true);
        }
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
    g.on_get_pdfium(get_pdfium);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<PdfToolkit>();
    g.set_drop_hover(hover && g.get_pdfium_found());
}

/// Appends the pages of every PDF in `paths`. Each is counted, then its thumbnails rendered, on
/// one worker per drop; the pages appear as soon as they're counted.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let (pdfs, others): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| is_pdf(p));
    if let Some(other) = others.first() {
        with(|s| {
            s.task.show_error(format!(
                "{} isn't a PDF. Only PDFs can be added here.",
                file_name(other)
            ))
        });
        sync(app);
    }
    if pdfs.is_empty() || with(|s| s.task.is_running()) {
        return;
    }
    let (first, token) = with(|s| {
        let first = s.sources.len();
        s.sources.extend(pdfs.iter().cloned());
        (first, s.reading.clone())
    });
    let weak = app.as_weak();
    let reader = thread::spawn(move || {
        for (k, path) in pdfs.into_iter().enumerate() {
            let source = first + k;
            let token_ = token.clone();
            let counted = pdf::page_count(pdfium, &path);
            let ok = counted.is_ok();
            jobs::post(&weak, move |app| {
                with(|s| {
                    if !Arc::ptr_eq(&s.reading, &token_) {
                        return;
                    }
                    match counted {
                        Ok(n) => s.pages.extend((0..n).map(|index| Page {
                            source,
                            index,
                            turns: 0,
                            selected: false,
                            thumb: None,
                        })),
                        Err(message) => s.task.show_error(message),
                    }
                });
                refresh(&app);
            });
            if !ok {
                continue;
            }
            let weak = weak.clone();
            let rendered = pdf::thumbnails(pdfium, &path, THUMB_EDGE, &token, |index, image| {
                let token = token.clone();
                jobs::post(&weak, move |_| set_thumb(&token, source, index, &image));
            });
            match rendered {
                Ok(0) => {}
                Ok(left_out) => {
                    let token = token.clone();
                    jobs::post(&weak, move |app| {
                        with(|s| {
                            if Arc::ptr_eq(&s.reading, &token) {
                                s.task.show_error(unpictured_note(left_out));
                            }
                        });
                        refresh(&app);
                    });
                }
                Err(e) => eprintln!("thumbnails for {}: {e}", path.display()),
            }
            if token.is_cancelled() {
                return;
            }
        }
    });
    with(|s| {
        s.readers.retain(|r| !r.is_finished());
        s.readers.push(reader);
    });
}

fn set_thumb(token: &Arc<CancelToken>, source: usize, index: u32, image: &RgbaImage) {
    let thumb = slint_image(image);
    with(|s| {
        if !Arc::ptr_eq(&s.reading, token) {
            return;
        }
        let multi = has_many_sources(&s.pages);
        for (i, page) in s.pages.iter_mut().enumerate() {
            if page.source == source && page.index == index {
                page.thumb = Some(thumb.clone());
                s.tiles.set_row_data(i, tile(&s.sources, page, multi));
            }
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

/// Runs a change to the pages (unless a job is using them) and redraws the grid.
fn edit(weak: &slint::Weak<AppWindow>, f: impl FnOnce(&mut State)) {
    let Some(app) = weak.upgrade() else { return };
    with(|s| {
        if !s.task.is_running() {
            f(s);
            s.task.clear();
        }
    });
    refresh(&app);
}

fn has_many_sources(pages: &[Page]) -> bool {
    pages.iter().any(|p| p.source != pages[0].source)
}

fn tile(sources: &[PathBuf], page: &Page, multi: bool) -> PdfPageTile {
    let number = page.index + 1;
    let label = if multi {
        let stem = sources[page.source]
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{stem} · {number}")
    } else {
        number.to_string()
    };
    PdfPageTile {
        image: page.thumb.clone().unwrap_or_default(),
        label: label.into(),
        selected: page.selected,
        turns: page.turns,
    }
}

/// Rebuilds every tile, then the summaries.
fn refresh(app: &AppWindow) {
    with(|s| {
        let multi = has_many_sources(&s.pages);
        let tiles: Vec<PdfPageTile> = s.pages.iter().map(|p| tile(&s.sources, p, multi)).collect();
        s.tiles.set_vec(tiles);
    });
    sync(app);
}

fn sync(app: &AppWindow) {
    let g = app.global::<PdfToolkit>();
    with(|s| {
        let selected = s.pages.iter().filter(|p| p.selected).count();
        let files: std::collections::HashSet<usize> = s.pages.iter().map(|p| p.source).collect();
        g.set_summary(
            format!(
                "{} from {}",
                count(s.pages.len(), "page", "pages"),
                count(files.len(), "PDF", "PDFs")
            )
            .into(),
        );
        g.set_selected_count(selected as i32);
        g.set_selection_summary(
            match selected {
                0 => "None selected".to_string(),
                n => format!("{n} selected"),
            }
            .into(),
        );
        let last = s.pages.len().saturating_sub(1) as i32;
        g.set_cursor(g.get_cursor().clamp(0, last));
        g.set_running(s.task.is_running());
        g.set_status(s.task.status());
    });
}

fn apply(app: &AppWindow, token: &Arc<CancelToken>, update: TaskUpdate) {
    with(|s| s.task.update(token, update));
    sync(app);
}

/// The first page's file, which names the outputs.
fn first_source(s: &State) -> Option<PathBuf> {
    s.pages.first().map(|p| s.sources[p.source].clone())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into())
}

/// Save PDF… (every page) or Extract… (the selected pages), through a save dialog.
fn save(app: &AppWindow, only_selected: bool) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let g = app.global::<PdfToolkit>();
    let picked = with(|s| {
        if s.task.is_running() {
            return None;
        }
        let pages: Vec<PageRef> = s
            .pages
            .iter()
            .filter(|p| p.selected || !only_selected)
            .map(Page::reference)
            .collect();
        (!pages.is_empty()).then(|| (s.sources.clone(), pages, first_source(s)))
    });
    let Some((sources, pages, Some(first))) = picked else {
        return;
    };
    let suffix = if only_selected { "extract" } else { "edited" };
    let title = if only_selected { "Save Selected Pages" } else { "Save PDF" };
    let Some(target) = files::choose_save_path(title, &format!("{} {suffix}.pdf", stem(&first))) else {
        return;
    };
    let settings = g.get_compress().then(|| CompressSettings {
        mode: CompressMode::Images,
        dpi: DPI_CHOICES[select::index(g.get_resolution_index(), DPI_CHOICES.len())] as f32,
        quality: g.get_quality().round().clamp(1.0, 100.0) as u8,
    });
    with(|s| {
        s.task.start(app, "Saving", apply, move |cancel, progress| {
            let out = files::reserve_replacing(&target).map_err(|e| {
                JobError::Failed(format!("Couldn't create {}: {e}", file_name(&target)))
            })?;
            let share = if settings.is_some() { 0.5 } else { 1.0 };
            pdf::assemble(pdfium, &sources, &pages, out.path(), cancel, &|p| progress(p * share))?;
            let note = match settings {
                Some(settings) => {
                    compress_in_place(pdfium, out.path(), settings, cancel, &|p| progress(0.5 + p * 0.5))?
                }
                None => String::new(),
            };
            let saved = out.keep().map_err(|e| {
                JobError::Failed(format!("Couldn't save {}: {e}", file_name(&target)))
            })?;
            Ok(Outcome {
                message: format!("Saved {}{note}", file_name(&saved)),
                reveal: saved,
            })
        })
    });
    sync(app);
}

/// Compresses the finished PDF at `path` in place when that makes it smaller; returns what the
/// status line adds.
fn compress_in_place(
    pdfium: &Pdfium,
    path: &Path,
    settings: CompressSettings,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<String, JobError> {
    let smaller = files::reserve_output(path, None, "pdf")
        .map_err(|e| JobError::Failed(format!("Couldn't create a temporary file: {e}")))?;
    match pdf::compress(pdfium, path, smaller.path(), settings, cancel, progress)? {
        Compressed::Smaller { before, after } => {
            fs::rename(smaller.path(), path)
                .map_err(|e| JobError::Failed(format!("Couldn't save the compressed PDF: {e}")))?;
            Ok(format!(
                ", compressed from {} to {}",
                format_size(before),
                format_size(after)
            ))
        }
        Compressed::NoGain => Ok(" (compressing wouldn't have made it smaller)".into()),
    }
}

/// Split…: one PDF per page, in grid order, into a new folder inside the one the user picks.
fn split(app: &AppWindow) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let picked = with(|s| {
        if s.task.is_running() || s.pages.is_empty() {
            return None;
        }
        let pages: Vec<PageRef> = s.pages.iter().map(Page::reference).collect();
        first_source(s).map(|first| (s.sources.clone(), pages, first))
    });
    let Some((sources, pages, first)) = picked else {
        return;
    };
    let Some(dir) = files::choose_folder("Choose Where to Save the Pages") else {
        return;
    };
    with(|s| {
        s.task.start(app, "Splitting", apply, move |cancel, progress| {
            let folder = files::reserve_dir(&first, Some(&dir), " pages")
                .map_err(|e| JobError::Failed(format!("Couldn't create the folder: {e}")))?;
            let n = pages.len() as u32;
            let outputs: Vec<(PathBuf, Vec<PageRef>)> = pages
                .iter()
                .enumerate()
                .map(|(i, p)| (folder.path().join(pdf::page_file_name(i as u32, n, "pdf")), vec![*p]))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn picked(c: &char) -> bool {
        c.is_uppercase()
    }

    #[test]
    fn shifting_earlier_moves_each_selected_block_past_one_neighbour() {
        let mut items = ['a', 'B', 'c', 'D'];

        shift(&mut items, picked, -1);

        assert_eq!(items, ['B', 'a', 'D', 'c']);
    }

    #[test]
    fn shifting_later_moves_a_block_together() {
        let mut items = ['A', 'B', 'c'];

        shift(&mut items, picked, 1);

        assert_eq!(items, ['c', 'A', 'B']);
    }

    #[test]
    fn a_selection_at_the_edge_stays_put() {
        let mut items = ['A', 'b'];

        shift(&mut items, picked, -1);

        assert_eq!(items, ['A', 'b']);
    }

    #[test]
    fn pdfs_are_recognised_by_extension_in_any_case() {
        let names = [Path::new("a.PDF"), Path::new("a.pdf.png"), Path::new("pdf")];

        let pdfs: Vec<bool> = names.iter().map(|p| is_pdf(p)).collect();

        assert_eq!(pdfs, [true, false, false]);
    }

    #[test]
    fn a_stale_run_cannot_finish_a_newer_one() {
        let mut task = Task::default();
        let (old, new) = (Arc::new(CancelToken::default()), Arc::new(CancelToken::default()));
        task.token = Some(new.clone());

        task.update(&old, TaskUpdate::Finished(Err(JobError::Failed("old".into()))));

        assert!(task.is_running());
        assert_eq!(task.status().state, PdfTaskState::Idle);
    }

    #[test]
    fn a_finished_run_shows_what_it_saved() {
        let mut task = Task::default();
        let token = Arc::new(CancelToken::default());
        task.token = Some(token.clone());
        let outcome = Outcome { message: "Saved a.pdf".into(), reveal: PathBuf::from("a.pdf") };

        task.update(&token, TaskUpdate::Finished(Ok(outcome)));

        assert_eq!((task.status().state, task.status().text.as_str()), (PdfTaskState::Done, "Saved a.pdf"));
        assert_eq!(task.output(), Some(PathBuf::from("a.pdf")));
    }
}
