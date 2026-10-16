//! PDF Merger: PDFs in order, joined into one through a save dialog (`engine::pdf::assemble`).
//! Wires the `PdfMerger` global (ui/tools/pdf-merger.slint). Each file's pages are counted on a
//! worker as it's added; one that isn't a readable PDF says why in its row and blocks Merge.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use slint::{ComponentHandle, ModelRc, VecModel};

use super::pdf_toolkit::{
    CONCURRENT_COUNTS, Outcome, PDF_FILTER, Task, TaskUpdate, count, get_pdfium, is_pdf,
};
use crate::engine::pdf::{self, PageRef};
use crate::files::{self, file_name, format_size, same_file};
use crate::jobs::{self, CancelToken, JobError};
use crate::{AppWindow, PdfListItem, PdfMerger};

pub const SLUG: &str = "pdf-merger";

struct Entry {
    id: u64,
    path: PathBuf,
    size: u64,
    /// `None` while counting.
    pages: Option<Result<u32, String>>,
}

#[derive(Default)]
struct State {
    files: Vec<Entry>,
    next_id: u64,
    rows: Rc<VecModel<PdfListItem>>,
    task: Task,
    counters: Vec<JoinHandle<()>>,
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
    let g = app.global::<PdfMerger>();
    g.set_pdfium_found(pdf::pdfium().is_some());
    with(|s| g.set_files(ModelRc::from(s.rows.clone())));
    sync(app);

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose PDFs to Merge", Some(PDF_FILTER)) {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_move(move |i, delta| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let (i, j) = (i as usize, (i + delta) as usize);
            if !s.task.is_running() && i < s.files.len() && j < s.files.len() {
                s.files.swap(i, j);
                s.task.clear();
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            if !s.task.is_running() && (i as usize) < s.files.len() {
                s.files.remove(i as usize);
                s.task.clear();
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_merge(move || {
        if let Some(app) = weak.upgrade() {
            merge(&app);
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
    let g = app.global::<PdfMerger>();
    g.set_drop_hover(hover && g.get_pdfium_found());
}

/// Lists the PDFs in `paths` after the ones already there, skipping folders and files already
/// listed, and counts their pages in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let added = with(|s| {
        if s.task.is_running() {
            return Vec::new();
        }
        s.task.clear();
        let mut added = Vec::new();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() || s.files.iter().any(|e| same_file(&e.path, &path)) {
                continue;
            }
            if !is_pdf(&path) {
                s.task.show_error(format!("{} isn't a PDF, so it wasn't added.", file_name(&path)));
                continue;
            }
            s.next_id += 1;
            s.files.push(Entry {
                id: s.next_id,
                path: path.clone(),
                size: meta.len(),
                pages: None,
            });
            added.push((s.next_id, path));
        }
        added
    });
    sync(app);

    let weak = app.as_weak();
    let counters = jobs::run_pool(added, CONCURRENT_COUNTS, move |(id, path)| {
        let pages = pdf::page_count(pdfium, &path);
        jobs::post(&weak, move |app| {
            with(|s| {
                if let Some(e) = s.files.iter_mut().find(|e| e.id == id) {
                    e.pages = Some(pages);
                }
            });
            sync(&app);
        });
    });
    with(|s| {
        s.counters.retain(|c| !c.is_finished());
        s.counters.extend(counters);
    });
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        let mut workers = s.task.shutdown();
        workers.append(&mut s.counters);
        workers
    })
}

/// Why Merge can't run yet, or the pages to merge, file by file.
fn plan(files: &[Entry]) -> Result<Vec<PageRef>, &'static str> {
    if files.len() < 2 {
        return Err("Add at least two PDFs to merge.");
    }
    let mut pages = Vec::new();
    for (i, e) in files.iter().enumerate() {
        match &e.pages {
            None => return Err("Counting pages…"),
            Some(Err(_)) => return Err("Remove the files that can't be read, then merge."),
            Some(Ok(n)) => pages.extend(pdf::all_pages(i, *n)),
        }
    }
    Ok(pages)
}

fn merge(app: &AppWindow) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let planned = with(|s| {
        if s.task.is_running() {
            return None;
        }
        let sources: Vec<PathBuf> = s.files.iter().map(|e| e.path.clone()).collect();
        plan(&s.files).ok().map(|pages| (sources, pages))
    });
    let Some((sources, pages)) = planned else { return };
    let Some(target) = files::choose_save_path("Save Merged PDF", "merged.pdf") else {
        return;
    };
    with(|s| {
        s.task.start(app, "Merging", apply, move |cancel, progress| {
            let out = files::reserve_replacing(&target).map_err(|e| {
                JobError::Failed(format!("Couldn't create {}: {e}", file_name(&target)))
            })?;
            pdf::assemble(pdfium, &sources, &pages, out.path(), cancel, progress)?;
            let saved = out.keep().map_err(|e| {
                JobError::Failed(format!("Couldn't save {}: {e}", file_name(&target)))
            })?;
            Ok(Outcome {
                message: format!(
                    "Saved {} · {}",
                    file_name(&saved),
                    count(pages.len(), "page", "pages")
                ),
                reveal: saved,
            })
        })
    });
    sync(app);
}

fn apply(app: &AppWindow, token: &Arc<CancelToken>, update: TaskUpdate) {
    with(|s| s.task.update(token, update));
    sync(app);
}

fn row(e: &Entry) -> PdfListItem {
    let size = format_size(e.size);
    let (detail, failed) = match &e.pages {
        None => (format!("{size} · Reading…"), false),
        Some(Ok(n)) => (format!("{} · {size}", count(*n as usize, "page", "pages")), false),
        Some(Err(message)) => (message.clone(), true),
    };
    PdfListItem {
        name: file_name(&e.path).into(),
        detail: detail.into(),
        thumb: Default::default(),
        failed,
    }
}

fn sync(app: &AppWindow) {
    let g = app.global::<PdfMerger>();
    with(|s| {
        s.rows.set_vec(s.files.iter().map(row).collect::<Vec<_>>());
        let total: u32 = s
            .files
            .iter()
            .filter_map(|e| e.pages.as_ref().and_then(|p| p.as_ref().ok()))
            .sum();
        g.set_summary(count(s.files.len(), "PDF", "PDFs").into());
        g.set_pages(count(total as usize, "page", "pages").into());
        let planned = plan(&s.files);
        g.set_can_merge(planned.is_ok() && !s.task.is_running());
        g.set_note(planned.err().unwrap_or_default().into());
        g.set_running(s.task.is_running());
        g.set_status(s.task.status());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pages: Option<Result<u32, String>>) -> Entry {
        Entry {
            id: 0,
            path: PathBuf::from("a.pdf"),
            size: 0,
            pages,
        }
    }

    #[test]
    fn merging_one_file_asks_for_another() {
        let files = [entry(Some(Ok(3)))];

        assert_eq!(plan(&files), Err("Add at least two PDFs to merge."));
    }

    #[test]
    fn an_unreadable_file_blocks_the_merge() {
        let files = [entry(Some(Ok(3))), entry(Some(Err("bad".into())))];

        assert_eq!(plan(&files), Err("Remove the files that can't be read, then merge."));
    }

    #[test]
    fn every_page_of_every_file_is_merged_in_list_order() {
        let files = [entry(Some(Ok(2))), entry(Some(Ok(1)))];

        let pages = plan(&files).unwrap();

        assert_eq!(pages, [PageRef::new(0, 0), PageRef::new(0, 1), PageRef::new(1, 0)]);
    }
}
