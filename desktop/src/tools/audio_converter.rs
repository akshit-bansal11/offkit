//! Audio Converter: owns the tool's state and wires the `AudioConverter` global
//! (ui/tools/audio-converter.slint) to the FFmpeg engine. A batch of audio files, each converted
//! to one chosen format; built like the Video Converter, the reference file-in/file-out tool.
//!
//! State lives on the UI thread. The file list is a `FileQueue`; probing and converting run
//! in the background (`jobs::run_pool`, `file_queue::run`) and come back via `jobs::post`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::ComponentHandle;

use crate::engine::ffmpeg::{Ffmpeg, MediaInfo};
use crate::engine::ffmpeg_audio::{self, AUDIO_EXTENSIONS, BITRATES, Format, bitrate_label};
use crate::file_queue::{self, FileQueue, Update};
use crate::files::{self, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, AudioConverter, FileStatus};

pub const SLUG: &str = "audio-converter";
const CONCURRENT_CONVERSIONS: usize = 2;
const CONCURRENT_PROBES: usize = 4;

/// A row's probe: `None` until it finishes, and for files it refused. Its row summary is the
/// Video Converter's (`impl Summary for Option<MediaInfo>`).
type Probe = Option<MediaInfo>;

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    /// The formats offered: every one until FFmpeg has listed its encoders, then those it has.
    formats: Vec<Format>,
    encoders_known: bool,
    queue: FileQueue<Probe>,
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
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<AudioConverter>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    let bitrates: Vec<String> = BITRATES.into_iter().map(bitrate_label).collect();
    g.set_bitrates(select::options(bitrates.iter().map(String::as_str)));
    with(|s| {
        g.set_files(s.queue.rows());
        s.formats = Format::ALL.to_vec();
        s.ffmpeg = ffmpeg;
    });
    refresh_settings(app);
    sync(app);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Listing encoders runs FFmpeg, so it stays off the UI thread.
        let weak = app.as_weak();
        thread::spawn(move || {
            let formats = ffmpeg_audio::available(&Format::ALL, ff.encoders());
            jobs::post(&weak, move |app| {
                let previous = chosen_format(&app);
                with(|s| {
                    s.formats = formats;
                    s.encoders_known = true;
                });
                select_format(&app, previous);
                refresh_settings(&app);
                sync(&app);
            });
        });
    }

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Audio", Some(("Audio", AUDIO_EXTENSIONS)))
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
    g.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh_settings(&app);
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Converted Audio To") {
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
    g.on_get_ffmpeg(|| {
        if let Err(e) = open::that("https://ffmpeg.org/download.html") {
            eprintln!("couldn't open the FFmpeg download page: {e}");
        }
    });
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<AudioConverter>();
    g.set_drop_hover(hover && g.get_ffmpeg_found());
}

/// Adds files (from a drop, the dialog or the snapshot hook), skipping folders and
/// duplicates, and probes the new ones in the background: any file with an audio track works.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(ff) = with(|s| s.ffmpeg.clone()) else {
        return;
    };
    let added = with(|s| s.queue.add(paths, FileStatus::Reading, || None));
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        let result = ffmpeg_audio::probe(&ff, &path).map_err(|e| e.to_string());
        jobs::post(&weak, move |app| {
            with(|s| {
                s.queue.update(id, |e| match result {
                    Ok(info) => {
                        e.data = Some(info);
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

/// Cancels running conversions and hands back their workers, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

fn chosen_format(app: &AppWindow) -> Option<Format> {
    let i = app.global::<AudioConverter>().get_format_index();
    with(|s| s.formats.get(index(i, s.formats.len())).copied())
}

/// Points the format menu at `format` in the current list, or its first entry.
fn select_format(app: &AppWindow, format: Option<Format>) {
    let i = with(|s| {
        format
            .and_then(|f| s.formats.iter().position(|x| *x == f))
            .unwrap_or(0)
    });
    app.global::<AudioConverter>().set_format_index(i as i32);
}

fn convert_all(app: &AppWindow) {
    let g = app.global::<AudioConverter>();
    let format = chosen_format(app);
    let bitrate = BITRATES[index(g.get_bitrate_index(), BITRATES.len())];
    let queued = with(|s| {
        let (Some(ff), Some(format)) = (s.ffmpeg.clone(), format) else {
            return None;
        };
        if !s.encoders_known {
            return None;
        }
        let jobs = s.queue.start(|e| Some(e.data.as_ref()?.duration));
        Some((ff, format, s.queue.out_dir().map(PathBuf::from), jobs))
    });
    let Some((ff, format, out_dir, jobs)) = queued else {
        return;
    };
    sync(app);

    let workers = file_queue::run(
        app,
        jobs,
        CONCURRENT_CONVERSIONS,
        apply,
        move |job, progress| {
            let output = reserve_output(&job.input, out_dir.as_deref(), format.extension())
                .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
            let args = ffmpeg_audio::encode_args(format, bitrate, ff.encoders())
                .map_err(JobError::failed)?;
            ffmpeg_audio::transcode(&ff, &job.input, &output, job.work, args, &job.cancel, |p| {
                progress(p)
            })?;
            Ok(output)
        },
    );
    with(|s| s.queue.track(workers));
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
}

/// Rebuilds the format menu and whether the bitrate applies to the chosen format.
fn refresh_settings(app: &AppWindow) {
    let g = app.global::<AudioConverter>();
    let labels: Vec<&str> = with(|s| s.formats.iter().map(|f| f.label()).collect());
    g.set_formats(select::options(labels.into_iter()));
    let format = chosen_format(app);
    g.set_bitrate_applies(format.is_some_and(Format::lossy));
}

/// Recomputes the list-level properties.
fn sync(app: &AppWindow) {
    let g = app.global::<AudioConverter>();
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        let probed = q.entries().iter().all(|e| e.status != FileStatus::Reading);
        let convertible = q.can_start(|e| e.data.is_some());
        let blocked = s.encoders_known && s.formats.is_empty();
        g.set_running(running);
        g.set_can_convert(s.encoders_known && probed && !running && convertible && !blocked);
        g.set_blocked_reason(
            if blocked {
                "This FFmpeg build has no audio encoders. Install a full build of FFmpeg."
            } else {
                ""
            }
            .into(),
        );
        g.set_list_summary(q.count_label("file", "files").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}
