//! Convert from PDF: a batch of PDFs, each rendered (`engine::pdf::render_to_folder`) into a new
//! folder `<name> images` holding `Page 01.png`, …, at a chosen resolution and format, all
//! pages or typed ranges. Wires the `ConvertFromPdf` global (ui/tools/convert-from-pdf.slint);
//! the list is a `FileQueue`. Every run writes a new folder, so converting again never mixes
//! with an earlier result (the website appended re-extracted pages to the old ones).
//!
//! `file_queue::run` keeps a single output file, so the jobs run here: the same steps (skip a
//! row cancelled while queued, report Running, catch a panic), with a `ReservedDir` that is
//! deleted unless the whole PDF rendered.

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;

use pdfium_render::prelude::Pdfium;
use slint::ComponentHandle;

use super::pdf_toolkit::{PDF_FILTER, PageCount, add_to_queue, get_pdfium, set_count};
use crate::engine::pdf::{self, Raster};
use crate::file_queue::{FileQueue, Job, Update};
use crate::files::{self, reserve_dir};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, ConvertFromPdf, FileStatus};

pub const SLUG: &str = "convert-from-pdf";
const CONCURRENT_JOBS: usize = 2;
const FORMATS: [&str; 3] = ["PNG", "JPEG", "WebP (lossless)"];
const JPEG: usize = 1;
const RENDER_DPI: [u32; 4] = [72, 150, 300, 600];
const DEFAULT_RENDER_DPI: i32 = 1;

#[derive(Default)]
struct State {
    queue: FileQueue<PageCount>,
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
    let g = app.global::<ConvertFromPdf>();
    g.set_pdfium_found(pdf::pdfium().is_some());
    g.set_formats(select::options(FORMATS.into_iter()));
    let dpi: Vec<String> = RENDER_DPI.iter().map(|d| format!("{d} dpi")).collect();
    g.set_resolutions(select::options(dpi.iter().map(String::as_str)));
    g.set_resolution_index(DEFAULT_RENDER_DPI);
    with(|s| g.set_files(s.queue.rows()));
    sync(app);

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose PDFs", Some(PDF_FILTER)) {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.remove(i as usize));
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_cancel(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.cancel(i as usize));
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_cancel_all(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.cancel_all());
        sync(&app);
    });
    g.on_reveal(|i| {
        if let Some(output) = with(|s| s.queue.output(i as usize)) {
            files::reveal(&output);
        }
    });
    let weak = app.as_weak();
    g.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Image Folders To") {
            with(|s| s.queue.set_out_dir(Some(dir)));
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_reset_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.set_out_dir(None));
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_convert_all(move || {
        if let Some(app) = weak.upgrade() {
            convert_all(&app);
        }
    });
    g.on_get_pdfium(get_pdfium);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<ConvertFromPdf>();
    g.set_drop_hover(hover && g.get_pdfium_found());
}

pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    with(|s| {
        let counters = add_to_queue(app, &mut s.queue, paths, apply_count);
        s.counters.retain(|c| !c.is_finished());
        s.counters.extend(counters);
    });
    sync(app);
}

fn apply_count(app: &AppWindow, id: u64, pages: Result<u32, String>) {
    with(|s| set_count(&mut s.queue, id, pages));
    sync(app);
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        let mut workers = s.queue.shutdown();
        workers.append(&mut s.counters);
        workers
    })
}

/// The pages to render out of `count`: every one when `text` is empty, else the typed ranges.
fn pick_pages(text: &str, count: u32) -> Result<Vec<u32>, String> {
    if text.trim().is_empty() {
        return Ok((0..count).collect());
    }
    pdf::parse_ranges(text, count).map(|r| pdf::flatten(&r))
}

/// A mistake in the typed ranges that no PDF could fix (a backwards range, a word); ranges past
/// a short PDF's end are reported on that PDF's row instead.
fn syntax_error(text: &str) -> Option<String> {
    pick_pages(text, u32::MAX).err()
}

struct Settings {
    format: Raster,
    dpi: f32,
    pages: String,
}

fn convert_all(app: &AppWindow) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let g = app.global::<ConvertFromPdf>();
    if syntax_error(&g.get_pages()).is_some() {
        return;
    }
    let quality = g.get_quality().round().clamp(1.0, 100.0) as u8;
    let settings = Arc::new(Settings {
        format: match index(g.get_format_index(), FORMATS.len()) {
            0 => Raster::Png,
            JPEG => Raster::Jpeg(quality),
            _ => Raster::Webp,
        },
        dpi: RENDER_DPI[index(g.get_resolution_index(), RENDER_DPI.len())] as f32,
        pages: g.get_pages().into(),
    });
    let (jobs, out_dir) = with(|s| {
        let jobs = s.queue.start(|e| e.data.0);
        (jobs, s.queue.out_dir().map(PathBuf::from))
    });
    sync(app);

    let weak = app.as_weak();
    let workers = jobs::run_pool(jobs, CONCURRENT_JOBS, move |job| {
        if job.cancel.is_cancelled() {
            return; // Cancelled while queued; the row already says so.
        }
        let post = |update: Update| {
            let (id, token) = (job.id, job.cancel.clone());
            jobs::post(&weak, move |app| apply(&app, id, &token, update));
        };
        post(Update::Running);
        let progress = |p| post(Update::Progress(p));
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            render(pdfium, &job, &settings, out_dir.as_deref(), &progress)
        }))
        .unwrap_or_else(|_| {
            eprintln!("rendering {} panicked", job.input.display());
            Err(JobError::Failed(
                "Something went wrong with this file. Try again, or remove it.".into(),
            ))
        });
        post(Update::Finished(result));
    });
    with(|s| s.queue.track(workers));
}

fn render(
    pdfium: &Pdfium,
    job: &Job<u32>,
    settings: &Settings,
    out_dir: Option<&Path>,
    progress: &dyn Fn(f32),
) -> Result<PathBuf, JobError> {
    let pages = pick_pages(&settings.pages, job.work).map_err(JobError::Failed)?;
    let folder = reserve_dir(&job.input, out_dir, " images")
        .map_err(|e| JobError::Failed(format!("Couldn't create the output folder: {e}")))?;
    pdf::render_to_folder(
        pdfium,
        &job.input,
        &pages,
        pdf::PageImages {
            dpi: settings.dpi,
            format: settings.format,
        },
        folder.path(),
        &job.cancel,
        progress,
    )?;
    Ok(folder.keep())
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
}

fn sync(app: &AppWindow) {
    let g = app.global::<ConvertFromPdf>();
    let error = syntax_error(&g.get_pages());
    g.set_quality_applies(index(g.get_format_index(), FORMATS.len()) == JPEG);
    g.set_pages_error(error.clone().unwrap_or_default().into());
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        let ready = q.entries().iter().all(|e| e.status != FileStatus::Reading);
        g.set_running(running);
        g.set_can_convert(
            ready && !running && error.is_none() && q.can_start(|e| e.data.0.is_some()),
        );
        g.set_list_summary(q.count_label("PDF", "PDFs").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pages_mean_every_page() {
        let pages = pick_pages("  ", 3).unwrap();

        assert_eq!(pages, [0, 1, 2]);
    }

    #[test]
    fn a_range_past_a_short_pdf_fails_only_that_pdf() {
        let short = pick_pages("2-5", 3).unwrap_err();

        assert_eq!(short, "“2-5” is past the last page, 3.");
        assert_eq!(syntax_error("2-5"), None);
    }

    #[test]
    fn a_backwards_range_is_caught_before_converting() {
        let error = syntax_error("5-2");

        assert_eq!(error.as_deref(), Some("“5-2” is backwards."));
    }
}
