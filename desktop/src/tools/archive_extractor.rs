//! Archive Extractor: unpacks ZIP, TAR, TAR.GZ, GZ and 7z archives into new folders, beside each
//! archive or in a chosen one. The file list is a `FileQueue`; probing and extracting run in the
//! background. Every guard against hostile archives lives in `engine::archive`. Its Slint side
//! is ui/tools/archive-extractor.slint.

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use slint::ComponentHandle;

use crate::engine::archive::{self, Limits, Probe};
use crate::file_queue::{FileQueue, Summary, Update};
use crate::files;
use crate::jobs::{self, CancelToken, JobError};
use crate::{AppWindow, ArchiveExtractor, FileStatus};

pub const SLUG: &str = "archive-extractor";
const CONCURRENT_EXTRACTIONS: usize = 2;
const CONCURRENT_PROBES: usize = 4;
/// For the open dialog only: the format is read from each file's content.
const ARCHIVE_EXTENSIONS: &[&str] = &["zip", "tar", "gz", "tgz", "7z"];

/// A row's probe: `None` until it finishes, and for files it refused.
type Probed = Option<Probe>;

impl Summary for Probed {
    fn summary(&self) -> String {
        self.as_ref().map(Probe::summary).unwrap_or_default()
    }
}

#[derive(Default)]
struct State {
    queue: FileQueue<Probed>,
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
    let g = app.global::<ArchiveExtractor>();
    with(|s| g.set_files(s.queue.rows()));
    g.set_size_limit(Limits::describe().into());

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) =
            files::choose_files("Choose Archives", Some(("Archives", ARCHIVE_EXTENSIONS)))
        {
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
        if let Some(dir) = files::choose_folder("Extract Into") {
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
    g.on_extract_all(move || {
        if let Some(app) = weak.upgrade() {
            extract_all(&app);
        }
    });
    sync(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<ArchiveExtractor>().set_drop_hover(hover);
}

/// Lists archives (from a drop, the dialog or the snapshot hook), skipping folders and
/// duplicates, and identifies each new one in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let added = with(|s| s.queue.add(paths, FileStatus::Reading, || None));
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        let result = archive::probe(&path);
        jobs::post(&weak, move |app| {
            with(|s| {
                s.queue.update(id, |e| match result {
                    Ok(probe) => {
                        e.data = Some(probe);
                        e.status = FileStatus::Ready;
                    }
                    Err(message) => {
                        e.status = FileStatus::Failed;
                        e.message = message;
                    }
                })
            });
            sync(&app);
        });
    });
}

/// Cancels running extractions and hands back their workers, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

fn extract_all(app: &AppWindow) {
    let (jobs, out_dir) = with(|s| {
        let jobs = s.queue.start(|e| e.data.as_ref().map(|p| p.format));
        (jobs, s.queue.out_dir().map(PathBuf::from))
    });
    sync(app);

    // `file_queue::run` keeps a `Reserved` file; an extraction keeps a folder, so its batch
    // runs here on the same pool, with the same cancel and panic handling.
    let weak = app.as_weak();
    let workers = jobs::run_pool(jobs, CONCURRENT_EXTRACTIONS, move |job| {
        if job.cancel.is_cancelled() {
            return; // Cancelled while queued; the row already says so.
        }
        let post = |report: Report| {
            let (id, token) = (job.id, job.cancel.clone());
            jobs::post(&weak, move |app| apply(&app, id, &token, report));
        };
        post(Report::Update(Update::Running));
        let run = || {
            let len = std::fs::metadata(&job.input).map_or(0, |m| m.len());
            let progress = |p| post(Report::Update(Update::Progress(p)));
            let extracted = archive::extract(
                &job.input,
                job.work,
                out_dir.as_deref(),
                Limits::for_archive(len),
                &job.cancel,
                &progress,
            )?;
            extracted
                .keep()
                .map_err(|e| JobError::Failed(format!("Couldn't keep what was extracted: {e}")))
        };
        let result = panic::catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|panic| {
            let reason = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            eprintln!("extracting {} panicked: {reason}", job.input.display());
            Err(JobError::Failed(
                "Something went wrong with this archive. Try again, or remove it.".into(),
            ))
        });
        match result {
            Ok((output, line)) => post(Report::Extracted(output, line)),
            Err(e) => post(Report::Update(Update::Finished(Err(e)))),
        }
    });
    with(|s| s.queue.track(workers));
}

/// What a worker reports on its row.
enum Report {
    Update(Update),
    /// A kept extraction: its output, and the row's line, which says more than "Saved as …".
    Extracted(PathBuf, String),
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, report: Report) {
    with(|s| match report {
        Report::Update(update) => s.queue.apply(id, token, update),
        Report::Extracted(output, line) => {
            s.queue.apply(id, token, Update::Finished(Ok(output.clone())));
            s.queue.update(id, |e| {
                if e.status == FileStatus::Done && e.output.as_ref() == Some(&output) {
                    e.message = line;
                }
            });
        }
    });
    sync(app);
}

/// Recomputes the list-level properties.
fn sync(app: &AppWindow) {
    let g = app.global::<ArchiveExtractor>();
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        g.set_running(running);
        g.set_can_extract(!running && q.can_start(|e| e.data.is_some()));
        g.set_list_summary(q.count_label("archive", "archives").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}
