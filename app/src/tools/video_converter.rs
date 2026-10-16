//! Video Converter: owns the tool's state and wires the `VideoConverter` global
//! (ui/tools/video-converter.slint) to the FFmpeg engine.
//!
//! State lives on the UI thread. Background work (probing, converting) runs through
//! `jobs::run_pool` and comes back via `jobs::post`, following the pattern in `jobs`: stable
//! ids, and conversion results guarded by the run's cancel token.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::engine::ffmpeg::{
    AudioCodec, Capabilities, Container, ConvertError, ConvertJob, Ffmpeg, MediaInfo, Settings,
    VideoCodec, reserve_output,
};
use crate::jobs::{self, CancelToken};
use crate::{AppWindow, FileItem, FileStatus, VideoConverter};

pub const SLUG: &str = "video-converter";
const CONCURRENT_CONVERSIONS: usize = 2;
const CONCURRENT_PROBES: usize = 4;
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "ts", "mts", "m2ts",
    "3gp", "ogv", "gif",
];

struct Entry {
    id: u64,
    path: PathBuf,
    size: u64,
    info: Option<MediaInfo>,
    status: FileStatus,
    progress: f32,
    message: String,
    output: Option<PathBuf>,
    cancel: Arc<CancelToken>,
}

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    caps: Option<Arc<Capabilities>>,
    entries: Vec<Entry>,
    next_id: u64,
    rows: Rc<VecModel<FileItem>>,
    video_options: Vec<VideoCodec>,
    audio_options: Vec<AudioCodec>,
    out_dir: Option<PathBuf>,
    workers: Vec<JoinHandle<()>>,
    convert_when_ready: bool,
    /// Why Convert All is unavailable for the current settings, if it is.
    blocked: Option<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn init(app: &AppWindow, ffmpeg: Option<Ffmpeg>) {
    let g = app.global::<VideoConverter>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    g.set_containers(strings(Container::ALL.iter().map(|c| c.label())));
    with(|s| {
        g.set_files(ModelRc::from(s.rows.clone()));
        s.ffmpeg = ffmpeg.map(Arc::new);
    });
    refresh_settings(app);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Hardware detection runs trial encodes, so it stays off the UI thread.
        let weak = app.as_weak();
        thread::spawn(move || {
            let caps = ff.capabilities();
            jobs::post(&weak, move |app| {
                with(|s| s.caps = Some(Arc::new(caps)));
                refresh_settings(&app);
                sync(&app);
            });
        });
    }

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose Videos")
            .add_filter("Video", VIDEO_EXTENSIONS)
            .pick_files()
        {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let i = i as usize;
            if s.entries.get(i).is_some_and(|e| !is_active(e.status)) {
                s.entries.remove(i);
                s.rows.remove(i);
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_cancel(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            if let Some(e) = s.entries.get_mut(i as usize) {
                cancel_entry(e);
            }
        });
        sync(&app);
    });
    g.on_cancel_all({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            with(|s| s.entries.iter_mut().for_each(cancel_entry));
            sync(&app);
        }
    });
    g.on_reveal(|i| {
        if let Some(output) = with(|s| s.entries.get(i as usize).and_then(|e| e.output.clone())) {
            reveal(&output);
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
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Save Converted Videos To")
            .pick_folder()
        {
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
    let g = app.global::<VideoConverter>();
    g.set_drop_hover(hover && g.get_ffmpeg_found());
}

/// Adds files (from a drop, the dialog or the snapshot hook), skipping folders and
/// duplicates, and probes the new ones in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(ff) = with(|s| s.ffmpeg.clone()) else {
        return;
    };
    let added: Vec<(u64, PathBuf)> = with(|s| {
        let mut added = Vec::new();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() || s.entries.iter().any(|e| same_file(&e.path, &path)) {
                continue;
            }
            s.next_id += 1;
            let entry = Entry {
                id: s.next_id,
                path: path.clone(),
                size: meta.len(),
                info: None,
                status: FileStatus::Reading,
                progress: 0.0,
                message: String::new(),
                output: None,
                cancel: Arc::default(),
            };
            s.rows.push(item(&entry));
            s.entries.push(entry);
            added.push((s.next_id, path));
        }
        added
    });
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        let result = ff.probe(&path).map_err(|e| e.to_string());
        jobs::post(&weak, move |app| {
            update(&app, id, |e| match result {
                Ok(info) => {
                    e.info = Some(info);
                    e.status = FileStatus::Ready;
                }
                Err(message) => {
                    e.status = FileStatus::Failed;
                    e.message = message;
                }
            });
        });
    });
}

/// Verification hook: start converting as soon as hardware detection and every probe finish.
pub fn convert_when_ready(app: &AppWindow) {
    with(|s| s.convert_when_ready = true);
    sync(app);
}

/// Cancels running conversions and waits for them, so no partial output outlives the app.
pub fn shutdown() {
    let workers = with(|s| {
        s.entries.iter_mut().for_each(cancel_entry);
        std::mem::take(&mut s.workers)
    });
    for worker in workers {
        let _ = worker.join();
    }
}

fn convert_all(app: &AppWindow) {
    let g = app.global::<VideoConverter>();
    let queued = with(|s| {
        let (Some(ff), Some(caps)) = (s.ffmpeg.clone(), s.caps.clone()) else {
            return None;
        };
        let settings = Settings {
            container: Container::ALL[index(g.get_container_index(), Container::ALL.len())],
            video: *s
                .video_options
                .get(index(g.get_video_codec_index(), s.video_options.len()))?,
            audio: *s
                .audio_options
                .get(index(g.get_audio_codec_index(), s.audio_options.len()))?,
            quality: g.get_quality().round().clamp(0.0, 100.0) as u8,
            hw: caps.hw.filter(|_| g.get_hw_enabled()),
        };
        let mut items = Vec::new();
        for e in &mut s.entries {
            if matches!(
                e.status,
                FileStatus::Ready | FileStatus::Failed | FileStatus::Cancelled
            ) && e.info.is_some()
            {
                e.cancel = Arc::default();
                e.status = FileStatus::Queued;
                e.progress = 0.0;
                e.message.clear();
                e.output = None;
                let info = e.info.as_ref();
                items.push((
                    e.id,
                    e.path.clone(),
                    info.and_then(|i| i.duration),
                    info.and_then(|i| i.video_codec.clone()),
                    e.cancel.clone(),
                ));
            }
        }
        s.workers.retain(|w| !w.is_finished());
        Some((ff, caps, settings, s.out_dir.clone(), items))
    });
    let Some((ff, caps, settings, out_dir, items)) = queued else {
        return;
    };
    sync(app);

    let weak = app.as_weak();
    let workers = jobs::run_pool(
        items,
        CONCURRENT_CONVERSIONS,
        move |(id, input, duration, source_codec, cancel)| {
            if cancel.is_cancelled() {
                return; // Cancelled while queued; the row already says so.
            }
            // Every post carries this run's token; `update_run` drops it if the row has moved on.
            let post = |f: Box<dyn FnOnce(&mut Entry) + Send>| {
                let token = cancel.clone();
                jobs::post(&weak, move |app| update_run(&app, id, &token, f));
            };
            post(Box::new(|e| e.status = FileStatus::Running));
            let result = reserve_output(&input, out_dir.as_deref(), settings.container.extension())
                .map_err(|e| ConvertError::Failed(format!("Couldn't create the output file: {e}")))
                .and_then(|output| {
                    let job = ConvertJob {
                        input,
                        output,
                        settings,
                        duration,
                        source_codec,
                    };
                    ff.convert(&job, &caps, &cancel, |p| {
                        post(Box::new(move |e| e.progress = p))
                    })
                    .map(|()| job.output)
                });
            post(Box::new(move |e| {
                e.progress = 0.0;
                match result {
                    Ok(output) => {
                        e.status = FileStatus::Done;
                        e.message = format!("Saved as {}", file_name(&output));
                        e.output = Some(output);
                    }
                    Err(ConvertError::Cancelled) => e.status = FileStatus::Cancelled,
                    Err(ConvertError::Failed(message)) => {
                        e.status = FileStatus::Failed;
                        e.message = message;
                    }
                }
            }));
        },
    );
    with(|s| s.workers.extend(workers));
}

/// Rebuilds the codec lists for the chosen container (keeping the current codec when the
/// new container allows it) and everything that depends on the codec.
fn refresh_settings(app: &AppWindow) {
    let g = app.global::<VideoConverter>();
    with(|s| {
        let container = Container::ALL[index(g.get_container_index(), Container::ALL.len())];
        let caps = s.caps.clone();
        let prev_video = s
            .video_options
            .get(g.get_video_codec_index() as usize)
            .copied();
        let prev_audio = s
            .audio_options
            .get(g.get_audio_codec_index() as usize)
            .copied();
        s.video_options = container
            .video_codecs()
            .iter()
            .copied()
            .filter(|c| caps.as_ref().is_none_or(|k| k.supports_video(*c)))
            .collect();
        s.audio_options = container
            .audio_codecs()
            .iter()
            .copied()
            .filter(|c| caps.as_ref().is_none_or(|k| k.supports_audio(*c)))
            .collect();
        let vi = prev_video
            .and_then(|p| s.video_options.iter().position(|c| *c == p))
            .unwrap_or(0);
        let ai = prev_audio
            .and_then(|p| s.audio_options.iter().position(|c| *c == p))
            .unwrap_or(0);
        g.set_video_codecs(strings(s.video_options.iter().map(|c| c.label())));
        g.set_audio_codecs(strings(s.audio_options.iter().map(|c| c.label())));
        g.set_video_codec_index(vi as i32);
        g.set_audio_codec_index(ai as i32);
        let missing = match (s.video_options.is_empty(), s.audio_options.is_empty()) {
            (true, _) => Some("video"),
            (_, true) => Some("audio"),
            _ => None,
        };
        s.blocked = missing.map(|kind| {
            format!(
                "This FFmpeg build has no {kind} encoder for {}. Choose another container.",
                container.label()
            )
        });

        let video = s.video_options.get(vi).copied();
        g.set_quality_applies(video.is_some_and(VideoCodec::has_quality));
        let hw = caps.as_ref().and_then(|k| k.hw);
        g.set_hw_name(hw.map(|f| f.label()).unwrap_or_default().into());
        let detail = match (video, caps) {
            (Some(v), Some(k)) if g.get_hw_enabled() && !k.hw_encodes(v) => {
                format!("{} always encodes on the CPU.", v.label())
            }
            (_, Some(k)) => {
                let gpu: Vec<_> = [VideoCodec::H264, VideoCodec::Hevc, VideoCodec::Av1]
                    .into_iter()
                    .filter(|c| k.hw_encodes(*c))
                    .map(VideoCodec::label)
                    .collect();
                let list = match gpu.split_last() {
                    Some((last, rest)) if !rest.is_empty() => {
                        format!("{} and {last}", rest.join(", "))
                    }
                    _ => gpu.join(""),
                };
                format!("Encodes {list} on the GPU: faster, with slightly larger files.")
            }
            (_, None) => String::new(),
        };
        g.set_hw_detail(detail.into());
    });
}

/// Applies `f` to the entry with `id` (if it's still listed) and refreshes its row.
fn update(app: &AppWindow, id: u64, f: impl FnOnce(&mut Entry)) {
    with(|s| {
        if let Some(i) = s.entries.iter().position(|e| e.id == id) {
            f(&mut s.entries[i]);
            s.rows.set_row_data(i, item(&s.entries[i]));
        }
    });
    sync(app);
}

/// `update` for a conversion run: applies only while the row still belongs to the run that
/// owns `token`, so a stale worker can't overwrite a newer run (or a re-added file).
fn update_run(app: &AppWindow, id: u64, token: &Arc<CancelToken>, f: impl FnOnce(&mut Entry)) {
    update(app, id, |e| {
        if Arc::ptr_eq(&e.cancel, token) {
            f(e);
        }
    });
}

/// Recomputes the list-level properties, and starts the pending verification conversion.
fn sync(app: &AppWindow) {
    let g = app.global::<VideoConverter>();
    let autostart = with(|s| {
        let running = s.entries.iter().any(|e| is_active(e.status));
        let ready = s.caps.is_some() && s.entries.iter().all(|e| e.status != FileStatus::Reading);
        let convertible = s.entries.iter().any(|e| {
            matches!(
                e.status,
                FileStatus::Ready | FileStatus::Failed | FileStatus::Cancelled
            ) && e.info.is_some()
        });
        g.set_running(running);
        g.set_can_convert(ready && !running && convertible && s.blocked.is_none());
        g.set_blocked_reason(s.blocked.clone().unwrap_or_default().into());
        let n = s.entries.len();
        g.set_list_summary(format!("{n} {}", if n == 1 { "video" } else { "videos" }).into());
        g.set_custom_output(s.out_dir.is_some());
        g.set_output_folder(
            s.out_dir
                .as_deref()
                .map(file_name)
                .unwrap_or_else(|| "Same as original".into())
                .into(),
        );
        let start = s.convert_when_ready && ready && convertible && s.blocked.is_none();
        if start {
            s.convert_when_ready = false;
        }
        start
    });
    if autostart {
        convert_all(app);
    }
}

fn cancel_entry(e: &mut Entry) {
    match e.status {
        FileStatus::Queued => {
            e.cancel.cancel();
            e.status = FileStatus::Cancelled;
        }
        // The worker reports Cancelled once FFmpeg is gone and the partial file removed.
        FileStatus::Running => e.cancel.cancel(),
        _ => {}
    }
}

fn is_active(status: FileStatus) -> bool {
    matches!(status, FileStatus::Queued | FileStatus::Running)
}

fn item(e: &Entry) -> FileItem {
    let mut detail = format_size(e.size);
    if let Some(info) = &e.info {
        let summary = info.summary();
        if !summary.is_empty() {
            detail = format!("{detail} · {summary}");
        }
    }
    FileItem {
        name: file_name(&e.path).into(),
        detail: detail.into(),
        status: e.status,
        progress: e.progress,
        message: e.message.as_str().into(),
    }
}

fn index(i: i32, len: usize) -> usize {
    usize::try_from(i).unwrap_or(0).min(len.saturating_sub(1))
}

fn strings<'a>(items: impl Iterator<Item = &'a str>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items.map(SharedString::from).collect::<Vec<_>>(),
    ))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Decimal units, as Finder and Explorer's details show them.
fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} bytes"),
        1_000..1_000_000 => format!("{:.0} KB", b / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b / 1e6),
        _ => format!("{:.2} GB", b / 1e9),
    }
}

/// Shows the file selected in the platform file manager.
fn reveal(path: &Path) {
    #[cfg(windows)]
    let result = {
        use std::os::windows::process::CommandExt;
        // `raw_arg`: Explorer parses `/select,"<path>"` itself and rejects Rust's quoting.
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(drop)
    };
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(drop);
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = open::that(path.parent().unwrap_or(path));
    if let Err(e) = result {
        eprintln!("couldn't show {} in the file manager: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(format_size(999), "999 bytes");
        assert_eq!(format_size(12_400_000), "12.4 MB");
        assert_eq!(format_size(3_210_000_000), "3.21 GB");
    }

    #[test]
    fn index_is_clamped() {
        assert_eq!(index(-1, 3), 0);
        assert_eq!(index(7, 3), 2);
        assert_eq!(index(0, 0), 0);
    }
}
