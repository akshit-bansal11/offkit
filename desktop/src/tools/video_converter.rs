//! Video Converter: owns the tool's state and wires the `VideoConverter` global
//! (ui/tools/video-converter.slint) to the FFmpeg engine. The reference file-in/file-out tool.
//!
//! State lives on the UI thread. The file list is a `FileQueue`; probing and converting run
//! in the background (`jobs::run_pool`, `file_queue::run`) and come back via `jobs::post`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::ComponentHandle;

use crate::engine::ffmpeg::{
    AudioCodec, Capabilities, Container, ConvertJob, Ffmpeg, MediaInfo, Settings, VideoCodec,
};
use crate::file_queue::{self, FileQueue, Summary, Update};
use crate::files::{self, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, FileStatus, VideoConverter};

pub const SLUG: &str = "video-converter";
const CONCURRENT_CONVERSIONS: usize = 2;
const CONCURRENT_PROBES: usize = 4;
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "ts", "mts", "m2ts",
    "3gp", "ogv", "gif",
];

/// A row's probe: `None` until it finishes, and for files it refused.
type Probe = Option<MediaInfo>;

impl Summary for Probe {
    fn summary(&self) -> String {
        self.as_ref().map(MediaInfo::summary).unwrap_or_default()
    }
}

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    caps: Option<Arc<Capabilities>>,
    queue: FileQueue<Probe>,
    video_options: Vec<VideoCodec>,
    audio_options: Vec<AudioCodec>,
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

pub fn init(app: &AppWindow) {
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<VideoConverter>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    g.set_containers(select::options(Container::ALL.iter().map(|c| c.label())));
    with(|s| {
        g.set_files(s.queue.rows());
        s.ffmpeg = ffmpeg;
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
        if let Some(paths) =
            files::choose_files("Choose Videos", Some(("Video", VIDEO_EXTENSIONS)))
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
        if let Some(dir) = files::choose_folder("Save Converted Videos To") {
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
    let g = app.global::<VideoConverter>();
    g.set_drop_hover(hover && g.get_ffmpeg_found());
}

/// Adds files (from a drop, the dialog or the snapshot hook), skipping folders and
/// duplicates, and probes the new ones in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(ff) = with(|s| s.ffmpeg.clone()) else {
        return;
    };
    let added = with(|s| s.queue.add(paths, FileStatus::Reading, || None));
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        let result = ff.probe_video(&path).map_err(|e| e.to_string());
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

/// Verification hook: start converting as soon as hardware detection and every probe finish.
pub fn convert_when_ready(app: &AppWindow) {
    with(|s| s.convert_when_ready = true);
    sync(app);
}

/// Cancels running conversions and hands back their workers, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

/// The source's duration and video codec, for progress and stream copies.
type Work = (Option<f64>, Option<String>);

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
        let jobs = s.queue.start(|e| {
            let info = e.data.as_ref()?;
            Some((info.duration, info.video_codec.clone()))
        });
        Some((ff, caps, settings, s.queue.out_dir().map(PathBuf::from), jobs))
    });
    let Some((ff, caps, settings, out_dir, jobs)) = queued else {
        return;
    };
    sync(app);

    let workers = file_queue::run(app, jobs, CONCURRENT_CONVERSIONS, apply, move |job, progress| {
        let (duration, source_codec): &Work = &job.work;
        let output = reserve_output(&job.input, out_dir.as_deref(), settings.container.extension())
            .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
        let convert = ConvertJob {
            input: job.input.clone(),
            settings,
            duration: *duration,
            source_codec: source_codec.clone(),
        };
        ff.convert(&convert, &output, &caps, &job.cancel, progress)?;
        Ok(output)
    });
    with(|s| s.queue.track(workers));
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
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
        g.set_video_codecs(select::options(s.video_options.iter().map(|c| c.label())));
        g.set_audio_codecs(select::options(s.audio_options.iter().map(|c| c.label())));
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

/// Recomputes the list-level properties, and starts the pending verification conversion.
fn sync(app: &AppWindow) {
    let g = app.global::<VideoConverter>();
    let autostart = with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        let ready = s.caps.is_some() && q.entries().iter().all(|e| e.status != FileStatus::Reading);
        let convertible = q.can_start(|e| e.data.is_some());
        g.set_running(running);
        g.set_can_convert(ready && !running && convertible && s.blocked.is_none());
        g.set_blocked_reason(s.blocked.clone().unwrap_or_default().into());
        g.set_list_summary(q.count_label("video", "videos").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
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
