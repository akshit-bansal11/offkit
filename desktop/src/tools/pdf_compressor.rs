//! PDF Compressor: a batch of PDFs, each saved as `<name> compressed.pdf` by
//! `engine::pdf::compress`, which re-encodes the embedded photos (text stays text) or, when
//! asked, rasterises every page. An output that wouldn't be smaller is never kept: its row says
//! "Already as small as it gets". Wires the `PdfCompressor` global
//! (ui/tools/pdf-compressor.slint); the file list is a `FileQueue`, like Video Converter's.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use slint::ComponentHandle;

use super::pdf_toolkit::{
    DEFAULT_DPI, DPI_CHOICES, PDF_FILTER, PageCount, add_to_queue, dpi_labels, get_pdfium,
    set_count,
};
use crate::engine::pdf::{self, CompressMode, CompressSettings, Compressed};
use crate::file_queue::{self, FileQueue, Update};
use crate::files::{self, file_name, format_size, reserve_output};
use crate::jobs::{CancelToken, JobError};
use crate::select;
use crate::{AppWindow, FileStatus, PdfCompressMode, PdfCompressor};

pub const SLUG: &str = "pdf-compressor";
/// Each job is single-threaded image work; two at once keeps the machine responsive.
const CONCURRENT_JOBS: usize = 2;
/// A worker's way of saying the output wasn't smaller. `apply` turns it into a finished row
/// that saved nothing: being compact already isn't a failure.
const ALREADY_SMALL: &str = "Already as small as it gets";

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
    let g = app.global::<PdfCompressor>();
    g.set_pdfium_found(pdf::pdfium().is_some());
    g.set_resolutions(dpi_labels(&DPI_CHOICES));
    g.set_resolution_index(DEFAULT_DPI);
    with(|s| g.set_files(s.queue.rows()));
    sync(app);

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose PDFs to Compress", Some(PDF_FILTER)) {
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
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Compressed PDFs To") {
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
    g.on_compress_all(move || {
        if let Some(app) = weak.upgrade() {
            compress_all(&app);
        }
    });
    g.on_get_pdfium(get_pdfium);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<PdfCompressor>();
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

fn compress_all(app: &AppWindow) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let g = app.global::<PdfCompressor>();
    let settings = CompressSettings {
        mode: match g.get_mode() {
            PdfCompressMode::Images => CompressMode::Images,
            PdfCompressMode::Rasterise => CompressMode::Rasterise,
        },
        dpi: DPI_CHOICES[select::index(g.get_resolution_index(), DPI_CHOICES.len())] as f32,
        quality: g.get_quality().round().clamp(1.0, 100.0) as u8,
    };
    let (jobs, out_dir) = with(|s| {
        let jobs = s.queue.start(|e| e.data.0);
        (jobs, s.queue.out_dir().map(PathBuf::from))
    });
    sync(app);

    let workers = file_queue::run(app, jobs, CONCURRENT_JOBS, apply, move |job, progress| {
        let stem = job
            .input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let named = job.input.with_file_name(format!("{stem} compressed.pdf"));
        let out = reserve_output(&named, out_dir.as_deref(), "pdf")
            .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
        match pdf::compress(
            pdfium,
            &job.input,
            out.path(),
            settings,
            &job.cancel,
            progress,
        )? {
            Compressed::Smaller { .. } => Ok(out),
            Compressed::NoGain => Err(JobError::Failed(ALREADY_SMALL.into())),
        }
    });
    with(|s| s.queue.track(workers));
}

/// The row's line once it's done: how much smaller, and where it went.
fn saved_message(before: u64, after: u64, output: &std::path::Path) -> String {
    let saved = 100 - (after * 100 / before.max(1)).min(100);
    format!(
        "{} → {}, {saved}% smaller · Saved as {}",
        format_size(before),
        format_size(after),
        file_name(output)
    )
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| {
        let input = s
            .queue
            .entries()
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.path.clone(), e.size));
        match (update, input) {
            (Update::Finished(Err(JobError::Failed(m))), Some((path, _))) if m == ALREADY_SMALL => {
                // Show in Folder then shows the original, which is what's worth keeping.
                s.queue.apply(id, token, Update::Finished(Ok(path.clone())));
                s.queue.update(id, |e| {
                    if e.output.as_ref() == Some(&path) {
                        e.message = format!("{ALREADY_SMALL}, so nothing was saved");
                    }
                });
            }
            (Update::Finished(Ok(out)), Some((_, before))) => {
                let after = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(before);
                s.queue.apply(id, token, Update::Finished(Ok(out.clone())));
                s.queue.update(id, |e| {
                    if e.output.as_ref() == Some(&out) {
                        e.message = saved_message(before, after, &out);
                    }
                });
            }
            (update, _) => s.queue.apply(id, token, update),
        }
    });
    sync(app);
}

fn sync(app: &AppWindow) {
    let g = app.global::<PdfCompressor>();
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        let ready = q.entries().iter().all(|e| e.status != FileStatus::Reading);
        g.set_running(running);
        g.set_can_compress(ready && !running && q.can_start(|e| e.data.0.is_some()));
        g.set_list_summary(q.count_label("PDF", "PDFs").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_finished_row_shows_the_size_before_and_after() {
        let message = saved_message(2_400_000, 600_000, Path::new("a compressed.pdf"));

        assert_eq!(
            message,
            "2.4 MB → 600 KB, 75% smaller · Saved as a compressed.pdf"
        );
    }
}
