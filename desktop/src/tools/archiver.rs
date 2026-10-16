//! Archiver: packs files and folders into one ZIP (stored, or compressed at level 3, 6 or 9, the
//! website's choices), TAR.GZ or 7z, saved where the user chooses. Each item keeps its own name
//! and everything below it; links are skipped, never followed (`engine::archive::plan`). Its
//! Slint side is ui/tools/archiver.slint.
//!
//! The list isn't a `FileQueue`: that skips folders and runs one job per row, while here every
//! row feeds one archive. Rows still use `FileItem`, so they show which item is being added.

use std::cell::RefCell;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::engine::archive::{self, MAX_ENTRIES, Plan, Target, echoed, plural};
use crate::files::{self, file_name, format_size, same_file};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, Archiver, FileItem, FileStatus};

pub const SLUG: &str = "archiver";
const CONCURRENT_SCANS: usize = 4;
const LEVELS: [(&str, u32); 4] = [
    ("None (level 0)", 0),
    ("Fast (level 3)", 3),
    ("Default (level 6)", 6),
    ("Best (level 9)", 9),
];
const DEFAULT_LEVEL: usize = 2;

struct Root {
    id: u64,
    path: PathBuf,
    /// What its scan found: "Folder · 12 files · 4.2 MB".
    detail: String,
    bytes: u64,
    status: FileStatus,
    progress: f32,
    message: String,
}

impl Root {
    fn item(&self) -> FileItem {
        FileItem {
            name: file_name(&self.path).into(),
            detail: self.detail.as_str().into(),
            status: self.status,
            progress: self.progress,
            message: self.message.as_str().into(),
        }
    }
}

/// The archive being written.
struct Run {
    cancel: Arc<CancelToken>,
    /// The rows it stores, in the order it writes them.
    ids: Vec<u64>,
}

#[derive(Default)]
struct State {
    roots: Vec<Root>,
    rows: Rc<VecModel<FileItem>>,
    next_id: u64,
    run: Option<Run>,
    worker: Option<JoinHandle<()>>,
    /// The last archive written, for Show in Folder.
    output: Option<PathBuf>,
}

impl State {
    fn update(&mut self, id: u64, f: impl FnOnce(&mut Root)) {
        if let Some(i) = self.roots.iter().position(|r| r.id == id) {
            f(&mut self.roots[i]);
            self.rows.set_row_data(i, self.roots[i].item());
        }
    }
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
    let g = app.global::<Archiver>();
    g.set_formats(select::options(Target::ALL.iter().map(|t| t.label())));
    g.set_levels(select::options(LEVELS.iter().map(|(label, _)| *label)));
    g.set_level_index(DEFAULT_LEVEL as i32);
    with(|s| g.set_files(ModelRc::from(s.rows.clone())));

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Files", None) {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_choose_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Choose a Folder") {
            add_paths(&app, vec![dir]);
        }
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let i = i as usize;
            let idle = s
                .roots
                .get(i)
                .is_some_and(|r| !matches!(r.status, FileStatus::Queued | FileStatus::Running));
            if idle {
                s.roots.remove(i);
                s.rows.remove(i);
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_cancel(move |_| {
        if let Some(app) = weak.upgrade() {
            stop(&app);
        }
    });
    let weak = app.as_weak();
    g.on_cancel_all(move || {
        if let Some(app) = weak.upgrade() {
            stop(&app);
        }
    });
    g.on_reveal(|_| {
        if let Some(output) = with(|s| s.output.clone()) {
            files::reveal(&output);
        }
    });
    let weak = app.as_weak();
    g.on_create(move || {
        if let Some(app) = weak.upgrade() {
            create(&app);
        }
    });
    sync(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<Archiver>().set_drop_hover(hover);
}

/// Lists files and folders (from a drop, a dialog or the snapshot hook) and scans each new one
/// in the background. Refuses links, and a second item with a name already listed: both would
/// be stored under one name.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let mut refused = Vec::new();
    let added = with(|s| {
        let mut added = Vec::new();
        for path in paths {
            let name = file_name(&path);
            match fs::symlink_metadata(&path) {
                Err(e) => refused.push(format!("Couldn't add {name}: {e}")),
                Ok(meta) if meta.file_type().is_symlink() => refused.push(format!(
                    "{name} is a link. Archiver stores files and folders, never links."
                )),
                Ok(_) if s.roots.iter().any(|r| same_file(&r.path, &path)) => {}
                Ok(_)
                    if s.roots
                        .iter()
                        .any(|r| file_name(&r.path).to_lowercase() == name.to_lowercase()) =>
                {
                    refused.push(format!(
                        "Another item is already named “{}”. Rename one to archive both.",
                        echoed(&name)
                    ));
                }
                Ok(_) => {
                    s.next_id += 1;
                    let root = Root {
                        id: s.next_id,
                        path: path.clone(),
                        detail: String::new(),
                        bytes: 0,
                        status: FileStatus::Reading,
                        progress: 0.0,
                        message: String::new(),
                    };
                    s.rows.push(root.item());
                    s.roots.push(root);
                    added.push((s.next_id, path));
                }
            }
        }
        added
    });
    app.global::<Archiver>()
        .set_error(refused.join(" ").into());
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_SCANS, move |(id, path)| {
        let scan = archive::plan(&[path], None, MAX_ENTRIES, &CancelToken::default());
        jobs::post(&weak, move |app| {
            with(|s| {
                s.update(id, |r| match scan {
                    Ok(plan) => {
                        r.detail = describe(&plan);
                        r.bytes = plan.bytes;
                        r.status = FileStatus::Ready;
                    }
                    Err(JobError::Cancelled) => r.status = FileStatus::Cancelled,
                    Err(JobError::Failed(message)) => {
                        r.status = FileStatus::Failed;
                        r.message = message;
                    }
                })
            });
            sync(&app);
        });
    });
}

/// A row's detail from its scan: "4.2 MB" for a file, "Folder · 12 files · 4.2 MB" for a folder.
fn describe(plan: &Plan) -> String {
    let mut parts = Vec::new();
    if plan.folders > 0 {
        parts.push("Folder".to_string());
        parts.push(plural(plan.files, "file"));
    }
    parts.push(format_size(plan.bytes));
    if !plan.skipped.is_empty() {
        parts.push(format!("{} skipped", plural(plan.skipped.len(), "link")));
    }
    parts.join(" · ")
}

/// Cancels the archive being written and hands back its thread.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        if let Some(run) = &s.run {
            run.cancel.cancel();
        }
        s.worker.take().into_iter().collect()
    })
}

fn stop(app: &AppWindow) {
    with(|s| {
        if let Some(run) = &s.run {
            run.cancel.cancel();
        }
    });
    sync(app);
}

/// Asks where to save, then writes every listed item into one archive in the background.
fn create(app: &AppWindow) {
    let g = app.global::<Archiver>();
    let target = Target::ALL[index(g.get_format_index(), Target::ALL.len())];
    let level = LEVELS[index(g.get_level_index(), LEVELS.len())].1;
    let Some(roots) = with(|s| {
        let idle = s.run.is_none() && !s.roots.is_empty();
        idle.then(|| s.roots.iter().map(|r| r.path.clone()).collect::<Vec<_>>())
    }) else {
        return;
    };
    let suggested = match roots.as_slice() {
        [one] => format!("{}.{}", file_name(one), target.extension()),
        _ => format!("Archive.{}", target.extension()),
    };
    let Some(path) = files::choose_save_path("Save Archive", &suggested) else {
        return;
    };
    // Replacing an item with the archive of itself would destroy it.
    if let Some(root) = roots.iter().find(|r| same_file(r, &path)) {
        let message = format!(
            "{} is one of the items being archived. Save the archive under another name.",
            file_name(root)
        );
        return g.set_error(message.into());
    }
    let out = match files::reserve_replacing(&path) {
        Ok(out) => out,
        Err(e) => return g.set_error(format!("Couldn't create the archive: {e}").into()),
    };
    let cancel = Arc::new(CancelToken::default());
    with(|s| {
        s.output = None;
        s.run = Some(Run {
            cancel: cancel.clone(),
            ids: s.roots.iter().map(|r| r.id).collect(),
        });
        for (i, root) in s.roots.iter_mut().enumerate() {
            root.status = FileStatus::Queued;
            root.progress = 0.0;
            root.message.clear();
            s.rows.set_row_data(i, root.item());
        }
    });
    g.set_error("".into());
    g.set_note("".into());
    sync(app);

    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let post = |report: Report| {
            let token = cancel.clone();
            jobs::post(&weak, move |app| apply(&app, &token, report));
        };
        let progress = |root: usize, done: f32| post(Report::Progress(root, done));
        // `out` is dropped, deleting the partial archive, on any error or panic.
        let run = || -> Result<(PathBuf, Vec<String>), JobError> {
            let plan = archive::plan(&roots, Some(out.path()), MAX_ENTRIES, &cancel)?;
            // NOTE: `Reserved` doesn't keep the handle from its `create_new`, so this opens the
            // reserved (empty) file once, never creating it, and `write` uses only that handle.
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(out.path())
                .map_err(|e| JobError::Failed(format!("Couldn't open the archive file: {e}")))?;
            let changed = archive::write(&plan, target, level, file, &cancel, &progress)?;
            let mut left_out = plan.skipped;
            left_out.extend(changed);
            let saved = out
                .keep()
                .map_err(|e| JobError::Failed(format!("Couldn't save the archive: {e}")))?;
            Ok((saved, left_out))
        };
        let result = panic::catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|panic| {
            let reason = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            eprintln!("writing an archive panicked: {reason}");
            Err(JobError::Failed(
                "Something went wrong writing the archive. Try again.".into(),
            ))
        });
        post(Report::Done(result));
    });
    with(|s| s.worker = Some(worker));
}

/// What the worker reports.
enum Report {
    /// The run's `n`th root is being added, this far.
    Progress(usize, f32),
    /// The saved archive and the links left out, or why there's none.
    Done(Result<(PathBuf, Vec<String>), JobError>),
}

fn apply(app: &AppWindow, token: &Arc<CancelToken>, report: Report) {
    let g = app.global::<Archiver>();
    with(|s| {
        let Some(run) = s.run.as_ref().filter(|r| Arc::ptr_eq(&r.cancel, token)) else {
            return;
        };
        let ids = run.ids.clone();
        match report {
            Report::Progress(current, done) => {
                for (i, id) in ids.into_iter().enumerate() {
                    s.update(id, |r| {
                        (r.status, r.progress) = match i.cmp(&current) {
                            std::cmp::Ordering::Less => (FileStatus::Done, 0.0),
                            std::cmp::Ordering::Equal => (FileStatus::Running, done),
                            std::cmp::Ordering::Greater => (FileStatus::Queued, 0.0),
                        };
                        if r.status == FileStatus::Done {
                            r.message = "Added".into();
                        }
                    });
                }
            }
            Report::Done(result) => {
                s.run = None;
                let (status, message) = match &result {
                    Ok((saved, _)) => (FileStatus::Done, format!("Added to {}", file_name(saved))),
                    Err(JobError::Cancelled) => (FileStatus::Cancelled, String::new()),
                    Err(JobError::Failed(_)) => (FileStatus::Ready, String::new()),
                };
                for id in ids {
                    s.update(id, |r| {
                        r.status = status;
                        r.progress = 0.0;
                        r.message = message.clone();
                    });
                }
                match result {
                    Ok((saved, skipped)) => {
                        g.set_note(saved_note(&saved, &skipped).into());
                        s.output = Some(saved);
                    }
                    Err(JobError::Cancelled) => g.set_note("Stopped. Nothing was saved.".into()),
                    Err(JobError::Failed(message)) => g.set_error(message.into()),
                }
            }
        }
    });
    sync(app);
}

/// "Saved as photos.zip · left out 1 item: “photos/latest”": links, and files that changed.
fn saved_note(saved: &std::path::Path, skipped: &[String]) -> String {
    let saved = format!("Saved as {}", file_name(saved));
    match skipped {
        [] => saved,
        [one] => format!("{saved} · left out 1 item: “{}”", echoed(one)),
        [first, rest @ ..] => format!(
            "{saved} · left out {}: “{}” and {} more",
            plural(skipped.len(), "item"),
            echoed(first),
            rest.len()
        ),
    }
}

/// Recomputes the list-level properties.
fn sync(app: &AppWindow) {
    let g = app.global::<Archiver>();
    with(|s| {
        let running = s.run.is_some();
        let failed = s.roots.iter().any(|r| r.status == FileStatus::Failed);
        g.set_running(running);
        g.set_can_create(!running && !failed && s.rows.row_count() > 0);
        let blocked = if failed {
            "Remove the items that couldn't be read."
        } else {
            ""
        };
        g.set_blocked_reason(blocked.into());
        let bytes: u64 = s.roots.iter().map(|r| r.bytes).sum();
        let summary = format!("{} · {}", plural(s.roots.len(), "item"), format_size(bytes));
        g.set_list_summary(summary.into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saved_note_names_one_left_out_item() {
        let note = saved_note(std::path::Path::new("out/photos.zip"), &["photos/latest".into()]);

        assert_eq!(note, "Saved as photos.zip · left out 1 item: “photos/latest”");
    }

    #[test]
    fn the_saved_note_counts_further_left_out_items() {
        let skipped = ["a/x".to_string(), "a/y".into(), "a/z".into()];

        let note = saved_note(std::path::Path::new("a.7z"), &skipped);

        assert_eq!(note, "Saved as a.7z · left out 3 items: “a/x” and 2 more");
    }

    #[test]
    fn levels_match_the_website_choices() {
        let levels: Vec<u32> = LEVELS.iter().map(|(_, level)| *level).collect();

        assert_eq!(levels, [0, 3, 6, 9]);
        assert_eq!(LEVELS[DEFAULT_LEVEL].0, "Default (level 6)");
    }
}
