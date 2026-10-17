//! The file list behind every file-in/file-out tool: rows with stable ids, per-run
//! cancellation, the output folder, and bounded batches on `jobs::run_pool`.
//!
//! A tool keeps one `FileQueue<T>` in its thread-local state, `T` being what it learns about
//! each file (Video Converter: its probe), and binds `rows()` to its global's `files`. To run a
//! batch it calls `start`, hands the jobs to `run`, and passes `track` the workers it gets
//! back, so `shutdown` can cancel them and hand them to `join`. Workers report through
//! `Update`s, which the tool's `apply` function feeds to `FileQueue::apply` on the UI thread.
//! An update applies only while the row still holds that run's token (`Arc::ptr_eq`), so a
//! stale worker never overwrites a newer run of the same row.
//!
//! Adding files skips duplicates by comparing each new path with every listed one, which is
//! quadratic; fine for the hundreds of files a person drops, not for an automated import.

use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::files::{Reserved, file_name, format_size, same_file};
use crate::jobs::{self, CancelToken, JobError};
use crate::{AppWindow, FileItem, FileStatus};

/// What a tool shows after a file's size in its row, e.g. "0:42 · 1920×1080"; "" for nothing.
pub trait Summary {
    fn summary(&self) -> String;
}

pub struct Entry<T> {
    pub id: u64,
    pub path: PathBuf,
    pub size: u64,
    pub status: FileStatus,
    pub progress: f32,
    /// Replaces the detail line when set: the error when failed, "Saved as …" when done.
    pub message: String,
    pub output: Option<PathBuf>,
    pub data: T,
    cancel: Arc<CancelToken>,
}

/// One row's share of a batch from `FileQueue::start`.
pub struct Job<W> {
    pub id: u64,
    pub input: PathBuf,
    pub cancel: Arc<CancelToken>,
    /// What the tool's worker needs for this row, picked by `start`.
    pub work: W,
}

/// A worker's report on its row, applied by `FileQueue::apply`.
pub enum Update {
    Running,
    Progress(f32),
    /// The output to reveal, or why there is none.
    Finished(Result<PathBuf, JobError>),
}

pub struct FileQueue<T> {
    entries: Vec<Entry<T>>,
    next_id: u64,
    rows: Rc<VecModel<FileItem>>,
    /// `None` saves each output next to its source.
    out_dir: Option<PathBuf>,
    workers: Vec<JoinHandle<()>>,
}

impl<T> Default for FileQueue<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
            rows: Rc::default(),
            out_dir: None,
            workers: Vec::new(),
        }
    }
}

impl<T: Summary> FileQueue<T> {
    pub fn rows(&self) -> ModelRc<FileItem> {
        ModelRc::from(self.rows.clone())
    }

    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }

    /// Lists files (from a drop, a dialog or the snapshot hook) as `status`, skipping folders,
    /// missing paths and files already listed. Returns the new rows' ids and paths, for the
    /// tool to inspect them in the background.
    pub fn add(
        &mut self,
        paths: Vec<PathBuf>,
        status: FileStatus,
        data: impl Fn() -> T,
    ) -> Vec<(u64, PathBuf)> {
        let mut added = Vec::new();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() || self.entries.iter().any(|e| same_file(&e.path, &path)) {
                continue;
            }
            self.next_id += 1;
            let entry = Entry {
                id: self.next_id,
                path: path.clone(),
                size: meta.len(),
                status,
                progress: 0.0,
                message: String::new(),
                output: None,
                data: data(),
                cancel: Arc::default(),
            };
            self.rows.push(item(&entry));
            self.entries.push(entry);
            added.push((self.next_id, path));
        }
        added
    }

    /// Applies `f` to the row with `id`, if it's still listed, and refreshes it.
    pub fn update(&mut self, id: u64, f: impl FnOnce(&mut Entry<T>)) {
        if let Some(i) = self.entries.iter().position(|e| e.id == id) {
            f(&mut self.entries[i]);
            self.rows.set_row_data(i, item(&self.entries[i]));
        }
    }

    /// Applies a worker's update, unless the row has moved on from the run that owns `token`.
    pub fn apply(&mut self, id: u64, token: &Arc<CancelToken>, update: Update) {
        self.update(id, |e| {
            if !Arc::ptr_eq(&e.cancel, token) {
                return;
            }
            match update {
                Update::Running => e.status = FileStatus::Running,
                Update::Progress(p) => e.progress = p,
                Update::Finished(result) => {
                    e.progress = 0.0;
                    match result {
                        Ok(output) => {
                            e.status = FileStatus::Done;
                            e.message = format!("Saved as {}", file_name(&output));
                            e.output = Some(output);
                        }
                        Err(JobError::Cancelled) => e.status = FileStatus::Cancelled,
                        Err(JobError::Failed(message)) => {
                            e.status = FileStatus::Failed;
                            e.message = message;
                        }
                    }
                }
            }
        });
    }

    /// Removes a row, unless it is queued or running (Cancel takes Remove's place there).
    pub fn remove(&mut self, index: usize) {
        if self
            .entries
            .get(index)
            .is_some_and(|e| !is_active(e.status))
        {
            self.entries.remove(index);
            self.rows.remove(index);
        }
    }

    pub fn cancel(&mut self, index: usize) {
        if let Some(e) = self.entries.get_mut(index) {
            cancel_entry(e);
            self.rows.set_row_data(index, item(e));
        }
    }

    pub fn cancel_all(&mut self) {
        for (i, e) in self.entries.iter_mut().enumerate() {
            cancel_entry(e);
            self.rows.set_row_data(i, item(e));
        }
    }

    /// The finished output of a row, for Show in Folder.
    pub fn output(&self, index: usize) -> Option<PathBuf> {
        self.entries.get(index).and_then(|e| e.output.clone())
    }

    pub fn is_running(&self) -> bool {
        self.entries.iter().any(|e| is_active(e.status))
    }

    /// Whether `start` would pick any row: one that is ready, failed or cancelled, and
    /// `ready` for the tool (e.g. its probe has finished).
    pub fn can_start(&self, ready: impl Fn(&Entry<T>) -> bool) -> bool {
        self.entries
            .iter()
            .any(|e| is_startable(e.status) && ready(e))
    }

    /// Queues every row that is ready, failed or cancelled and for which `pick` returns work,
    /// each with a fresh cancel token and its previous result cleared.
    pub fn start<W>(&mut self, mut pick: impl FnMut(&Entry<T>) -> Option<W>) -> Vec<Job<W>> {
        let mut jobs = Vec::new();
        for (i, e) in self.entries.iter_mut().enumerate() {
            if !is_startable(e.status) {
                continue;
            }
            let Some(work) = pick(e) else { continue };
            e.cancel = Arc::default();
            e.status = FileStatus::Queued;
            e.progress = 0.0;
            e.message.clear();
            e.output = None;
            self.rows.set_row_data(i, item(e));
            jobs.push(Job {
                id: e.id,
                input: e.path.clone(),
                cancel: e.cancel.clone(),
                work,
            });
        }
        jobs
    }

    /// Keeps a batch's workers so `shutdown` can wait for them.
    pub fn track(&mut self, workers: Vec<JoinHandle<()>>) {
        self.workers.retain(|w| !w.is_finished());
        self.workers.extend(workers);
    }

    /// Cancels every queued and running row and hands back the workers, for `join`. Join them
    /// outside the tool's `with()`: a finishing worker posts back to the UI thread.
    pub fn shutdown(&mut self) -> Vec<JoinHandle<()>> {
        self.cancel_all();
        std::mem::take(&mut self.workers)
    }

    pub fn out_dir(&self) -> Option<&Path> {
        self.out_dir.as_deref()
    }

    /// `None` saves outputs next to their sources again.
    pub fn set_out_dir(&mut self, dir: Option<PathBuf>) {
        self.out_dir = dir;
    }

    /// The Save to row's label: the chosen folder's name, or "Same as original".
    pub fn out_dir_label(&self) -> String {
        self.out_dir
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| "Same as original".into())
    }

    /// "1 video", "3 videos".
    pub fn count_label(&self, one: &str, many: &str) -> String {
        let n = self.entries.len();
        format!("{n} {}", if n == 1 { one } else { many })
    }
}

/// Runs a batch from `FileQueue::start` on at most `concurrency` threads, returning the
/// workers for `FileQueue::track`. Each job is skipped if it was cancelled while queued;
/// otherwise its row turns Running and `work` runs with a progress callback (0..1).
///
/// `work` returns its output as the `Reserved` it wrote (see `files`): the queue keeps it and
/// the row shows "Saved as …". Returning an error, or panicking, drops the `Reserved`, which
/// deletes the partial file; a panic is logged and the row says something went wrong. `work`
/// must poll `job.cancel.is_cancelled()` between steps and return `Err(JobError::Cancelled)`
/// (FFmpeg runs are killed by the token themselves). Every report reaches the tool's queue
/// through `apply`, which is `FileQueue::apply` on the tool's state followed by its refresh.
pub fn run<W: Send + 'static>(
    app: &AppWindow,
    jobs: Vec<Job<W>>,
    concurrency: usize,
    apply: fn(&AppWindow, u64, &Arc<CancelToken>, Update),
    work: impl Fn(&Job<W>, &dyn Fn(f32)) -> Result<Reserved, JobError> + Send + Sync + 'static,
) -> Vec<JoinHandle<()>> {
    let weak = app.as_weak();
    jobs::run_pool(jobs, concurrency, move |job| {
        let post = |update: Update| {
            let (id, token) = (job.id, job.cancel.clone());
            jobs::post(&weak, move |app| apply(&app, id, &token, update));
        };
        run_one(&job, &work, post);
    })
}

/// One job of `run`, with the posting left to the caller so it can be tested without a window.
fn run_one<W>(
    job: &Job<W>,
    work: &impl Fn(&Job<W>, &dyn Fn(f32)) -> Result<Reserved, JobError>,
    post: impl Fn(Update),
) {
    if job.cancel.is_cancelled() {
        return; // Cancelled while queued; the row already says so.
    }
    post(Update::Running);
    let progress = |p| post(Update::Progress(p));
    let result = match panic::catch_unwind(AssertUnwindSafe(|| work(job, &progress))) {
        Ok(Ok(output)) => output
            .keep()
            .map_err(|e| JobError::Failed(format!("Couldn't save {}: {e}", file_name(&job.input)))),
        Ok(Err(e)) => Err(e),
        Err(panic) => {
            let reason = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            eprintln!("processing {} panicked: {reason}", job.input.display());
            Err(JobError::Failed(
                "Something went wrong with this file. Try again, or remove it.".into(),
            ))
        }
    };
    post(Update::Finished(result));
}

/// Waits for workers handed back by `FileQueue::shutdown`, logging any that panicked.
pub fn join(workers: Vec<JoinHandle<()>>) {
    for worker in workers {
        if worker.join().is_err() {
            eprintln!("a background job panicked while the app was closing");
        }
    }
}

fn is_active(status: FileStatus) -> bool {
    matches!(status, FileStatus::Queued | FileStatus::Running)
}

fn is_startable(status: FileStatus) -> bool {
    matches!(
        status,
        FileStatus::Ready | FileStatus::Failed | FileStatus::Cancelled
    )
}

fn cancel_entry<T>(e: &mut Entry<T>) {
    match e.status {
        FileStatus::Queued => {
            e.cancel.cancel();
            e.status = FileStatus::Cancelled;
        }
        // The worker reports Cancelled once its process is gone and the partial file removed.
        FileStatus::Running => e.cancel.cancel(),
        _ => {}
    }
}

fn item<T: Summary>(e: &Entry<T>) -> FileItem {
    let mut detail = format_size(e.size);
    let summary = e.data.summary();
    if !summary.is_empty() {
        detail = format!("{detail} · {summary}");
    }
    FileItem {
        name: file_name(&e.path).into(),
        detail: detail.into(),
        status: e.status,
        progress: e.progress,
        message: e.message.as_str().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Probe(Option<&'static str>);

    impl Summary for Probe {
        fn summary(&self) -> String {
            self.0.unwrap_or_default().into()
        }
    }

    /// A queue over `names`, created as files in a fresh temp folder, all Ready.
    fn queue(test: &str, names: &[&str]) -> (FileQueue<Probe>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("offkit-queue-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
        for p in &paths {
            fs::write(p, b"12345").unwrap();
        }
        let mut q = FileQueue::default();
        q.add(paths, FileStatus::Ready, || Probe(Some("0:42")));
        (q, dir)
    }

    fn statuses(q: &FileQueue<Probe>) -> Vec<FileStatus> {
        q.entries().iter().map(|e| e.status).collect()
    }

    #[test]
    fn add_skips_folders_missing_paths_and_duplicates() {
        let (mut q, dir) = queue("add", &["a.mp4"]);

        let added = q.add(
            vec![dir.join("a.mp4"), dir.clone(), dir.join("missing.mp4")],
            FileStatus::Reading,
            || Probe(None),
        );

        assert!(added.is_empty());
        assert_eq!(q.rows().row_count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rows_show_size_then_summary() {
        let (q, dir) = queue("rows", &["a.mp4"]);

        let row = q.rows().row_data(0).unwrap();

        assert_eq!(row.name, "a.mp4");
        assert_eq!(row.detail, "5 bytes · 0:42");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ids_stay_with_their_rows_after_a_removal() {
        let (mut q, dir) = queue("ids", &["a.mp4", "b.mp4"]);
        let b = q.entries()[1].id;

        q.remove(0);
        q.update(b, |e| e.message = "probed".into());

        assert_eq!(q.entries()[0].message, "probed");
        assert_eq!(q.rows().row_data(0).unwrap().message, "probed");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn start_queues_only_startable_rows_the_tool_picks() {
        let (mut q, dir) = queue("start", &["a.mp4", "b.mp4", "c.mp4"]);
        q.update(q.entries()[2].id, |e| e.status = FileStatus::Reading);

        let jobs = q.start(|e| (!e.path.ends_with("b.mp4")).then_some("work"));

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].work, "work");
        assert_eq!(
            statuses(&q),
            [FileStatus::Queued, FileStatus::Ready, FileStatus::Reading]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finished_runs_mark_rows_done_failed_or_cancelled() {
        let (mut q, dir) = queue("finish", &["a.mp4", "b.mp4", "c.mp4"]);
        let jobs = q.start(|_| Some(()));
        let out = dir.join("a.webm");

        q.apply(
            jobs[0].id,
            &jobs[0].cancel,
            Update::Finished(Ok(out.clone())),
        );
        q.apply(
            jobs[1].id,
            &jobs[1].cancel,
            Update::Finished(Err(JobError::Failed("Bad file".into()))),
        );
        q.apply(
            jobs[2].id,
            &jobs[2].cancel,
            Update::Finished(Err(JobError::Cancelled)),
        );

        assert_eq!(
            statuses(&q),
            [FileStatus::Done, FileStatus::Failed, FileStatus::Cancelled]
        );
        assert_eq!(q.entries()[0].message, "Saved as a.webm");
        assert_eq!(q.output(0), Some(out));
        assert_eq!(q.entries()[1].message, "Bad file");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn updates_from_an_earlier_run_are_ignored() {
        let (mut q, dir) = queue("stale", &["a.mp4"]);
        let first = q.start(|_| Some(()));
        q.apply(
            first[0].id,
            &first[0].cancel,
            Update::Finished(Err(JobError::Cancelled)),
        );
        let second = q.start(|_| Some(()));

        q.apply(first[0].id, &first[0].cancel, Update::Running);

        assert_eq!(statuses(&q), [FileStatus::Queued]);
        assert!(!Arc::ptr_eq(&first[0].cancel, &second[0].cancel));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancelling_a_queued_row_cancels_it_at_once() {
        let (mut q, dir) = queue("cancel-queued", &["a.mp4"]);
        let jobs = q.start(|_| Some(()));

        q.cancel(0);

        assert_eq!(statuses(&q), [FileStatus::Cancelled]);
        // The row must say so too: no worker will report on a job cancelled while queued.
        assert_eq!(q.rows().row_data(0).unwrap().status, FileStatus::Cancelled);
        assert!(jobs[0].cancel.is_cancelled());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancelling_a_running_row_waits_for_its_worker() {
        let (mut q, dir) = queue("cancel-running", &["a.mp4"]);
        let jobs = q.start(|_| Some(()));
        q.apply(jobs[0].id, &jobs[0].cancel, Update::Running);

        q.cancel_all();

        assert_eq!(statuses(&q), [FileStatus::Running]);
        assert!(jobs[0].cancel.is_cancelled());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn active_rows_cannot_be_removed() {
        let (mut q, dir) = queue("remove-active", &["a.mp4"]);
        q.start(|_| Some(()));

        q.remove(0);

        assert_eq!(q.entries().len(), 1);
        assert!(q.is_running());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Runs one job of the queue in `dir` with `work`, returning the updates it posted.
    fn run_job(
        q: &mut FileQueue<Probe>,
        work: impl Fn(&Job<()>, &dyn Fn(f32)) -> Result<Reserved, JobError>,
    ) -> Vec<String> {
        let jobs = q.start(|_| Some(()));
        let posted = std::sync::Mutex::new(Vec::new());

        run_one(&jobs[0], &work, |u: Update| {
            let label = match &u {
                Update::Running => "running".to_string(),
                Update::Progress(p) => format!("progress {p}"),
                Update::Finished(Ok(path)) => format!("saved {}", file_name(path)),
                Update::Finished(Err(JobError::Cancelled)) => "cancelled".into(),
                Update::Finished(Err(JobError::Failed(m))) => format!("failed: {m}"),
            };
            posted.lock().unwrap().push(label);
        });

        posted.into_inner().unwrap()
    }

    #[test]
    fn a_successful_job_keeps_its_output() {
        let (mut q, dir) = queue("run-ok", &["a.wav"]);

        let posted = run_job(&mut q, |job, progress| {
            progress(0.5);
            crate::files::reserve_output(&job.input, None, "mp3").map_err(JobError::failed)
        });

        assert_eq!(posted, ["running", "progress 0.5", "saved a.mp3"]);
        assert!(dir.join("a.mp3").is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_job_leaves_no_partial_output() {
        let (mut q, dir) = queue("run-failed", &["a.wav"]);

        let posted = run_job(&mut q, |job, _| {
            let _output = crate::files::reserve_output(&job.input, None, "mp3").unwrap();
            Err(JobError::Failed("Bad file".into()))
        });

        assert_eq!(posted, ["running", "failed: Bad file"]);
        assert!(!dir.join("a.mp3").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_panicking_job_fails_its_row_instead_of_hanging_it() {
        let (mut q, dir) = queue("run-panic", &["a.wav"]);

        let posted = run_job(&mut q, |job, _| {
            let _output = crate::files::reserve_output(&job.input, None, "mp3").unwrap();
            panic!("decoder bug");
        });

        assert_eq!(
            posted,
            [
                "running",
                "failed: Something went wrong with this file. Try again, or remove it."
            ]
        );
        assert!(!dir.join("a.mp3").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_job_cancelled_while_queued_never_runs() {
        let (mut q, dir) = queue("run-cancelled", &["a.wav"]);
        let jobs = q.start(|_| Some(()));
        q.cancel(0);
        let posted = std::sync::Mutex::new(Vec::new());

        run_one(
            &jobs[0],
            &|_: &Job<()>, _: &dyn Fn(f32)| -> Result<Reserved, JobError> {
                panic!("must not run")
            },
            |_| posted.lock().unwrap().push(()),
        );

        assert!(posted.into_inner().unwrap().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn labels_name_the_output_folder_and_count_files() {
        let (mut q, dir) = queue("labels", &["a.mp4"]);
        assert_eq!(q.out_dir_label(), "Same as original");
        assert_eq!(q.count_label("video", "videos"), "1 video");

        q.set_out_dir(Some(dir.join("Exports")));

        assert_eq!(q.out_dir_label(), "Exports");
        fs::remove_dir_all(&dir).unwrap();
    }
}
