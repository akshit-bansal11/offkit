//! Video Editor: owns the tool's state and wires the `VideoEditor` global
//! (ui/tools/video-editor.slint) to FFmpeg. One video at a time: trim, crop, rotate, flip,
//! resize, speed and volume, as on the website, exported through a save dialog.
//!
//! The preview is one frame at the playhead, extracted off the UI thread; while one is being
//! read, only the latest position asked for waits, so scrubbing never queues up frames. The
//! export is one FFmpeg run whose filters apply the edit in the website's order: trim, crop,
//! rotate, flip, scale, speed. Trimming is a filter, not a seek, so the start and end are
//! seconds of the source whatever the speed.

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::{ComponentHandle, SharedPixelBuffer};

use crate::engine::ffmpeg::{
    Capabilities, Chain, Ffmpeg, Filter, Graph, HwFamily, MediaInfo, Run, VideoStream,
};
use crate::engine::ffmpeg_video::{
    self, Format, VIDEO, VIDEO_EXTENSIONS, clock, encoder_args, even_size, extract_frame, hw_for,
    missing_encoder,
};
use crate::files::{self, reserve_replacing};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, VideoEditor, VideoRotation};

pub const SLUG: &str = "video-editor";
/// The preview frame's longer side, in pixels.
const PREVIEW_SIDE: u32 = 1280;
/// The shortest clip a trim may leave, in seconds.
const MIN_CLIP: f64 = 0.1;
/// The smallest crop, in source pixels (as in the crop overlay).
const MIN_CROP: f64 = 16.0;
const SIZES: &[(&str, Size)] = &[
    ("Original", Size::Original),
    ("2160p (4K)", Size::Height(2160)),
    ("1080p", Size::Height(1080)),
    ("720p", Size::Height(720)),
    ("480p", Size::Height(480)),
    ("Custom", Size::Custom),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Size {
    Original,
    /// This height, the width following the aspect ratio (rounded to even).
    Height(u32),
    /// The size typed in Width and Height.
    Custom,
}

/// The output size an `Edit` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scale {
    Original,
    Height(u32),
    Exact(u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turn {
    None,
    Quarter,
    Half,
    ThreeQuarters,
}

/// One export's edit, in source seconds and pixels.
#[derive(Debug, Clone, PartialEq)]
struct Edit {
    start: f64,
    end: f64,
    duration: f64,
    crop: Option<Rect>,
    turn: Turn,
    flip_h: bool,
    flip_v: bool,
    scale: Scale,
    /// 0.25..=4.
    speed: f64,
    /// 0..=2, where 1 leaves the sound as it is.
    volume: f64,
    format: Format,
    hw: Option<HwFamily>,
}

/// The open video.
struct Source {
    path: PathBuf,
    info: MediaInfo,
    stream: VideoStream,
}

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    caps: Option<&'static Capabilities>,
    source: Option<Source>,
    /// Counts opened videos, so a frame or probe for an earlier one is dropped.
    generation: u64,
    preview_busy: bool,
    /// The latest position asked for while a frame was being read.
    preview_wanted: Option<f64>,
    /// Cancels previews and probes at shutdown.
    cancel: Arc<CancelToken>,
    export: Option<Arc<CancelToken>>,
    output: Option<PathBuf>,
    workers: Vec<JoinHandle<()>>,
}

impl State {
    fn track(&mut self, worker: JoinHandle<()>) {
        self.workers.retain(|w| !w.is_finished());
        self.workers.push(worker);
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
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<VideoEditor>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    g.set_formats(select::options(Format::ALL.iter().map(|f| f.label())));
    g.set_sizes(select::options(SIZES.iter().map(|(label, _)| *label)));
    with(|s| s.ffmpeg = ffmpeg);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Hardware detection runs trial encodes, so it stays off the UI thread.
        let weak = app.as_weak();
        let worker = thread::spawn(move || {
            let caps = ffmpeg_video::capabilities(&ff);
            jobs::post(&weak, move |app| {
                with(|s| s.caps = Some(caps));
                refresh(&app);
            });
        });
        with(|s| s.track(worker));
    }

    let weak = app.as_weak();
    g.on_choose_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose a Video", Some(("Video", VIDEO_EXTENSIONS)))
        {
            open(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_close_file(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            s.generation += 1;
            s.source = None;
            s.output = None;
        });
        let g = app.global::<VideoEditor>();
        g.set_loaded(false);
        g.set_load_error("".into());
        g.set_preview(slint::Image::default());
    });
    let weak = app.as_weak();
    g.on_scrubbed(move || {
        let Some(app) = weak.upgrade() else { return };
        let at = f64::from(app.global::<VideoEditor>().get_position());
        refresh(&app);
        request_preview(&app, at);
    });
    let weak = app.as_weak();
    g.on_trimmed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_nudge_start(move |delta| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<VideoEditor>();
        let start = (g.get_trim_start() + delta)
            .min(g.get_trim_end() - MIN_CLIP as f32)
            .max(0.0);
        g.set_trim_start(start);
        g.set_position(start);
        refresh(&app);
        request_preview(&app, f64::from(start));
    });
    let weak = app.as_weak();
    g.on_nudge_end(move |delta| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<VideoEditor>();
        let end = (g.get_trim_end() + delta)
            .max(g.get_trim_start() + MIN_CLIP as f32)
            .min(g.get_duration());
        g.set_trim_end(end);
        g.set_position(end);
        refresh(&app);
        request_preview(&app, f64::from(end));
    });
    let weak = app.as_weak();
    g.on_crop_changed(move || {
        let Some(app) = weak.upgrade() else { return };
        clamp_crop(&app.global::<VideoEditor>());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_export(move || {
        if let Some(app) = weak.upgrade() {
            export(&app);
        }
    });
    g.on_cancel_export(|| {
        if let Some(cancel) = with(|s| s.export.clone()) {
            cancel.cancel();
        }
    });
    g.on_reveal(|| {
        if let Some(output) = with(|s| s.output.clone()) {
            files::reveal(&output);
        }
    });
    g.on_get_ffmpeg(|| {
        if let Err(e) = open::that("https://ffmpeg.org/download.html") {
            eprintln!("couldn't open the FFmpeg download page: {e}");
        }
    });
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<VideoEditor>();
    g.set_drop_hover(hover && g.get_ffmpeg_found() && !g.get_exporting());
}

/// Opens the first of the dropped (or preloaded) files; the editor holds one video.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    if let Some(path) = paths.into_iter().find(|p| p.is_file()) {
        open(app, path);
    }
}

/// Cancels the export and any preview, and hands back their threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        s.cancel.cancel();
        if let Some(export) = &s.export {
            export.cancel();
        }
        std::mem::take(&mut s.workers)
    })
}

/// Reads `path` in the background, then shows it with every setting back at its default.
fn open(app: &AppWindow, path: PathBuf) {
    let g = app.global::<VideoEditor>();
    if g.get_exporting() {
        return;
    }
    let Some((ff, generation, cancel)) = with(|s| {
        s.generation += 1;
        Some((s.ffmpeg.clone()?, s.generation, s.cancel.clone()))
    }) else {
        return;
    };
    g.set_loading(true);
    g.set_loaded(false);
    g.set_load_error("".into());
    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let probed = ff
            .probe_video(&path)
            .and_then(|info| Ok((ff.probe_video_stream(&path, &info)?, info)));
        if cancel.is_cancelled() {
            return;
        }
        jobs::post(&weak, move |app| match probed {
            Ok((stream, info)) => show(&app, generation, Source { path, info, stream }),
            Err(e) => {
                if with(|s| s.generation) == generation {
                    let g = app.global::<VideoEditor>();
                    g.set_loading(false);
                    g.set_load_error(format!("Couldn't open {}: {e}", files::file_name(&path)).into());
                }
            }
        });
    });
    with(|s| s.track(worker));
}

/// Shows a freshly read video, unless another was opened since.
fn show(app: &AppWindow, generation: u64, source: Source) {
    let g = app.global::<VideoEditor>();
    let size = std::fs::metadata(&source.path).map(|m| m.len()).unwrap_or_default();
    let duration = source.info.duration.unwrap_or_default();
    let (w, h) = (source.stream.width as f32, source.stream.height as f32);
    let current = with(|s| {
        if s.generation != generation {
            return false;
        }
        s.preview_wanted = None;
        s.output = None;
        true
    });
    if !current {
        return;
    }
    g.set_file_name(files::file_name(&source.path).into());
    g.set_file_detail(format!("{} · {}", files::format_size(size), source.info.summary()).into());
    g.set_source_width(w);
    g.set_source_height(h);
    g.set_duration(duration as f32);
    g.set_position(0.0);
    g.set_trim_start(0.0);
    g.set_trim_end(duration as f32);
    g.set_crop_enabled(false);
    g.set_crop_x(0.0);
    g.set_crop_y(0.0);
    g.set_crop_w(w);
    g.set_crop_h(h);
    g.set_rotation(VideoRotation::None);
    g.set_flip_h(false);
    g.set_flip_v(false);
    g.set_size_index(0);
    g.set_custom_width(w);
    g.set_custom_height(h);
    g.set_speed(100.0);
    g.set_volume(100.0);
    g.set_has_audio(source.info.audio_codec.is_some());
    g.set_preview(slint::Image::default());
    g.set_export_message("".into());
    g.set_export_failed(false);
    g.set_can_reveal(false);
    g.set_loading(false);
    g.set_loaded(true);
    with(|s| s.source = Some(source));
    refresh(app);
    request_preview(app, 0.0);
}

/// Reads the frame at `at` seconds, or remembers `at` if a frame is already being read.
fn request_preview(app: &AppWindow, at: f64) {
    let start = with(|s| {
        let path = s.source.as_ref()?.path.clone();
        if s.preview_busy {
            s.preview_wanted = Some(at);
            return None;
        }
        s.preview_busy = true;
        Some((s.ffmpeg.clone()?, path, s.generation, s.cancel.clone()))
    });
    let Some((ff, path, generation, cancel)) = start else {
        return;
    };
    // There's no frame at the very end to seek to.
    let duration = f64::from(app.global::<VideoEditor>().get_duration());
    let at = at.min(duration - MIN_CLIP).max(0.0);
    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let frame = extract_frame(&ff, &path, at, PREVIEW_SIDE, &cancel);
        jobs::post(&weak, move |app| {
            let (current, next) = with(|s| {
                s.preview_busy = false;
                (s.generation == generation, s.preview_wanted.take())
            });
            match frame {
                Ok(frame) if current => {
                    let (w, h) = frame.dimensions();
                    let buffer = SharedPixelBuffer::clone_from_slice(frame.as_raw(), w, h);
                    app.global::<VideoEditor>()
                        .set_preview(slint::Image::from_rgba8(buffer));
                }
                Ok(_) | Err(JobError::Cancelled) => {}
                Err(JobError::Failed(e)) => eprintln!("couldn't read a preview frame: {e}"),
            }
            if let Some(at) = next {
                request_preview(&app, at);
            }
        });
    });
    with(|s| s.track(worker));
}

/// Keeps the crop inside the frame and at least `MIN_CROP` square, in whole pixels.
fn clamp_crop(g: &VideoEditor) {
    let (sw, sh) = (f64::from(g.get_source_width()), f64::from(g.get_source_height()));
    let side = |value: f32, room: f64| f64::from(value).round().clamp(MIN_CROP.min(room), room);
    let w = side(g.get_crop_w(), sw);
    let h = side(g.get_crop_h(), sh);
    let x = f64::from(g.get_crop_x()).round().clamp(0.0, sw - w);
    let y = f64::from(g.get_crop_y()).round().clamp(0.0, sh - h);
    g.set_crop_x(x as f32);
    g.set_crop_y(y as f32);
    g.set_crop_w(w as f32);
    g.set_crop_h(h as f32);
}

/// The edit the inspector describes, for the open video.
fn read_edit(g: &VideoEditor, caps: Option<&Capabilities>) -> Edit {
    let format = Format::ALL[index(g.get_format_index(), Format::ALL.len())];
    let px = |v: f32| v.round().max(0.0) as u32;
    let crop = g.get_crop_enabled().then(|| Rect {
        x: px(g.get_crop_x()),
        y: px(g.get_crop_y()),
        w: px(g.get_crop_w()),
        h: px(g.get_crop_h()),
    });
    let scale = match SIZES[index(g.get_size_index(), SIZES.len())].1 {
        Size::Original => Scale::Original,
        Size::Height(h) => Scale::Height(h),
        Size::Custom => Scale::Exact(px(g.get_custom_width()), px(g.get_custom_height())),
    };
    Edit {
        start: f64::from(g.get_trim_start()),
        end: f64::from(g.get_trim_end()),
        duration: f64::from(g.get_duration()),
        crop,
        turn: match g.get_rotation() {
            VideoRotation::None => Turn::None,
            VideoRotation::Quarter => Turn::Quarter,
            VideoRotation::Half => Turn::Half,
            VideoRotation::ThreeQuarters => Turn::ThreeQuarters,
        },
        flip_h: g.get_flip_h(),
        flip_v: g.get_flip_v(),
        scale,
        speed: (f64::from(g.get_speed()) / 100.0).clamp(0.25, 4.0),
        volume: (f64::from(g.get_volume()) / 100.0).clamp(0.0, 2.0),
        format,
        hw: caps
            .and_then(|c| hw_for(format, c))
            .filter(|_| g.get_hw_enabled()),
    }
}

/// Recomputes everything derived from the settings: labels, the output summary, the hardware
/// row and whether Export is available.
fn refresh(app: &AppWindow) {
    let g = app.global::<VideoEditor>();
    let (caps, has_source, exporting) =
        with(|s| (s.caps, s.source.is_some(), s.export.is_some()));
    let edit = read_edit(&g, caps);
    g.set_custom_size(SIZES[index(g.get_size_index(), SIZES.len())].1 == Size::Custom);
    g.set_position_label(clock(f64::from(g.get_position()), true).into());
    g.set_start_label(clock(edit.start, true).into());
    g.set_end_label(clock(edit.end, true).into());
    g.set_clip_label(clock(edit.end - edit.start, true).into());
    let source = (
        f64::from(g.get_source_width()) as u32,
        f64::from(g.get_source_height()) as u32,
    );
    let (w, h) = output_size(&edit, source);
    g.set_output_summary(
        format!(
            "Exports {w}×{h} · {} · {}",
            clock((edit.end - edit.start) / edit.speed, true),
            edit.format.label()
        )
        .into(),
    );
    let blocked = match caps {
        None => Some("Checking this FFmpeg's encoders…".to_string()),
        Some(c) => missing_encoder(edit.format, c),
    };
    let hw = caps.and_then(|c| c.hw);
    g.set_hw_name(hw.map(|f| f.label()).unwrap_or_default().into());
    g.set_hw_detail(match (hw, caps.and_then(|c| hw_for(edit.format, c))) {
        (Some(_), Some(_)) => "Encodes on the GPU: faster, with slightly larger files.".into(),
        (Some(_), None) => format!("{} always encodes on the CPU.", edit.format.label()).into(),
        _ => "".into(),
    });
    g.set_blocked_reason(blocked.clone().unwrap_or_default().into());
    g.set_can_export(has_source && !exporting && blocked.is_none());
}

/// The size the export will be: the crop (or the frame), turned, then scaled, then rounded
/// down to even sides as the encoders need.
fn output_size(edit: &Edit, (source_w, source_h): (u32, u32)) -> (u32, u32) {
    let (w, h) = edit.crop.map_or((source_w, source_h), |c| (c.w, c.h));
    let (w, h) = match edit.turn {
        Turn::Quarter | Turn::ThreeQuarters => (h, w),
        Turn::None | Turn::Half => (w, h),
    };
    let (w, h) = match edit.scale {
        Scale::Original => (w, h),
        // FFmpeg's -2: the width that keeps the aspect ratio, rounded to even.
        Scale::Height(target) => {
            let scaled = f64::from(w) * f64::from(target) / f64::from(h.max(1));
            ((scaled / 2.0).round() as u32 * 2, target)
        }
        Scale::Exact(w, h) => (w, h),
    };
    (w / 2 * 2, h / 2 * 2)
}

/// The website's chain, in its order: trim, crop, rotate, flip, scale, speed; then even sides.
fn video_graph(edit: &Edit) -> OsString {
    let mut chain = Chain::new();
    if let Some(trim) = trim(edit, "trim") {
        chain = chain
            .then(trim)
            .then(Filter::new("setpts").arg("PTS-STARTPTS"));
    }
    if let Some(c) = edit.crop {
        chain = chain.then(Filter::new("crop").arg(c.w).arg(c.h).arg(c.x).arg(c.y));
    }
    chain = match edit.turn {
        Turn::None => chain,
        Turn::Quarter => chain.then(Filter::new("transpose").arg(1)),
        Turn::Half => chain
            .then(Filter::new("transpose").arg(1))
            .then(Filter::new("transpose").arg(1)),
        Turn::ThreeQuarters => chain.then(Filter::new("transpose").arg(2)),
    };
    if edit.flip_h {
        chain = chain.then(Filter::new("hflip"));
    }
    if edit.flip_v {
        chain = chain.then(Filter::new("vflip"));
    }
    chain = match edit.scale {
        Scale::Original => chain,
        Scale::Height(h) => chain.then(Filter::new("scale").arg(-2).arg(h)),
        Scale::Exact(w, h) => chain.then(Filter::new("scale").arg(w).arg(h)),
    };
    if (edit.speed - 1.0).abs() > 0.01 {
        let factor = format!("{:.6}*PTS", 1.0 / edit.speed);
        chain = chain.then(Filter::new("setpts").arg(factor));
    }
    Graph::new().chain(chain.then(even_size())).arg()
}

/// The sound: the same trim, the volume, then the speed as an atempo chain. `None` when the
/// sound passes through unchanged.
fn audio_graph(edit: &Edit) -> Option<OsString> {
    let mut filters = Vec::new();
    if let Some(trim) = trim(edit, "atrim") {
        filters.push(trim);
        filters.push(Filter::new("asetpts").arg("PTS-STARTPTS"));
    }
    if (edit.volume - 1.0).abs() > 0.01 {
        filters.push(Filter::new("volume").arg(format!("{:.2}", edit.volume)));
    }
    if (edit.speed - 1.0).abs() > 0.01 {
        filters.extend(atempo_chain(edit.speed));
    }
    if filters.is_empty() {
        return None;
    }
    let chain = filters.into_iter().fold(Chain::new(), Chain::then);
    Some(Graph::new().chain(chain).arg())
}

/// `trim` or `atrim` for the kept part, as the website decides it: a start past 0.01 s, an end
/// more than 0.05 s before the end. `None` keeps everything.
fn trim(edit: &Edit, name: &str) -> Option<Filter> {
    let cut_start = edit.start > 0.01;
    let cut_end = edit.end < edit.duration - 0.05;
    let mut filter = Filter::new(name);
    if cut_start {
        filter = filter.opt("start", format!("{:.3}", edit.start));
    }
    if cut_end {
        filter = filter.opt("end", format!("{:.3}", edit.end));
    }
    (cut_start || cut_end).then_some(filter)
}

/// atempo takes 0.5..=2, so larger changes are steps of 2 or 0.5 and then the rest, as the
/// website builds it.
fn atempo_chain(speed: f64) -> Vec<Filter> {
    let mut chain = Vec::new();
    let mut rest = speed;
    while rest > 2.0 {
        chain.push(Filter::new("atempo").arg("2.0"));
        rest /= 2.0;
    }
    while rest < 0.5 {
        chain.push(Filter::new("atempo").arg("0.5"));
        rest /= 0.5;
    }
    chain.push(Filter::new("atempo").arg(format!("{rest:.4}")));
    chain
}

/// The output options for `edit` on a video with or without sound.
fn output_args(
    edit: &Edit,
    has_audio: bool,
    caps: &Capabilities,
) -> anyhow::Result<Vec<OsString>> {
    let mut args: Vec<OsString> = vec!["-map".into(), "0:v:0".into()];
    if has_audio {
        args.extend(["-map".into(), "0:a:0".into()]);
    }
    args.extend(["-vf".into(), video_graph(edit)]);
    if let Some(audio) = audio_graph(edit).filter(|_| has_audio) {
        args.extend(["-af".into(), audio]);
    }
    args.extend(encoder_args(edit.format, has_audio, edit.hw, caps)?);
    Ok(args)
}

fn export(app: &AppWindow) {
    let g = app.global::<VideoEditor>();
    let prepared = with(|s| {
        let source = s.source.as_ref()?;
        if s.export.is_some() {
            return None;
        }
        Some((s.ffmpeg.clone()?, s.caps?, source.path.clone()))
    });
    let Some((ff, caps, input)) = prepared else {
        return;
    };
    let mut edit = read_edit(&g, Some(caps));
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let suggested = format!("{stem} edited.{}", edit.format.extension());
    // Outside `with`: the dialog runs a nested event loop.
    let Some(target) = files::choose_save_path("Export Video", &suggested) else {
        return;
    };
    // A name typed with another of the three extensions picks that format.
    if let Some(format) = Format::from_path(&target) {
        edit.format = format;
        edit.hw = hw_for(format, caps).filter(|_| g.get_hw_enabled());
    } else {
        return finish_export(
            app,
            Err(JobError::Failed(format!(
                "Save it as .{}: that's the format chosen.",
                edit.format.extension()
            ))),
        );
    }
    let output = match reserve_replacing(&target) {
        Ok(output) => output,
        Err(e) => {
            let name = files::file_name(&target);
            return finish_export(app, Err(JobError::Failed(format!("Couldn't create {name}: {e}"))));
        }
    };
    let cancel = Arc::new(CancelToken::default());
    with(|s| {
        s.export = Some(cancel.clone());
        s.output = None;
    });
    g.set_exporting(true);
    g.set_export_progress(0.0);
    g.set_export_message("".into());
    g.set_export_failed(false);
    g.set_can_reveal(false);
    refresh(app);

    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let run = Run {
            inputs: std::slice::from_ref(&input),
            media: VIDEO,
            output: &output,
            duration: Some((edit.end - edit.start) / edit.speed),
            exit_prefix: if edit.hw.is_some() {
                "Hardware encoder failed: "
            } else {
                ""
            },
        };
        let progress = |p: f32| {
            jobs::post(&weak, move |app| {
                app.global::<VideoEditor>().set_export_progress(p);
            });
        };
        let prepare = |infos: &[MediaInfo]| {
            let has_audio = infos.first().is_some_and(|i| i.audio_codec.is_some());
            output_args(&edit, has_audio, caps)
        };
        let result = ff.run(&run, &cancel, prepare, progress).and_then(|()| {
            output
                .keep()
                .map_err(|e| JobError::Failed(format!("Couldn't save the video: {e}")))
        });
        jobs::post(&weak, move |app| finish_export(&app, result));
    });
    with(|s| s.track(worker));
}

fn finish_export(app: &AppWindow, result: Result<PathBuf, JobError>) {
    let g = app.global::<VideoEditor>();
    with(|s| s.export = None);
    g.set_exporting(false);
    match result {
        Ok(path) => {
            g.set_export_message(format!("Saved as {}", files::file_name(&path)).into());
            g.set_export_failed(false);
            g.set_can_reveal(true);
            with(|s| s.output = Some(path));
        }
        Err(JobError::Cancelled) => {
            g.set_export_message("".into());
            g.set_export_failed(false);
        }
        Err(JobError::Failed(message)) => {
            g.set_export_message(message.into());
            g.set_export_failed(true);
        }
    }
    refresh(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ffmpeg::check_output_args;
    use crate::engine::ffmpeg_video::test_media::{make_clip, real_ffmpeg};
    use std::fs;

    fn edit() -> Edit {
        Edit {
            start: 0.0,
            end: 10.0,
            duration: 10.0,
            crop: None,
            turn: Turn::None,
            flip_h: false,
            flip_v: false,
            scale: Scale::Original,
            speed: 1.0,
            volume: 1.0,
            format: Format::Mp4,
            hw: None,
        }
    }

    fn text(arg: Option<OsString>) -> String {
        arg.map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn caps() -> Capabilities {
        Capabilities::with(&["libx264", "libvpx-vp9", "aac", "libopus"], None)
    }

    #[test]
    fn an_untouched_edit_only_rounds_to_even_sides() {
        assert_eq!(
            text(Some(video_graph(&edit()))),
            "scale=trunc(iw/2)*2:trunc(ih/2)*2"
        );
        assert_eq!(audio_graph(&edit()), None);
    }

    #[test]
    fn the_video_chain_follows_the_websites_order() {
        let e = Edit {
            start: 1.5,
            end: 8.0,
            crop: Some(Rect {
                x: 10,
                y: 20,
                w: 300,
                h: 200,
            }),
            turn: Turn::Quarter,
            flip_h: true,
            flip_v: true,
            scale: Scale::Height(720),
            speed: 2.0,
            ..edit()
        };

        assert_eq!(
            text(Some(video_graph(&e))),
            "trim=start=1.500:end=8.000,setpts=PTS-STARTPTS,crop=300:200:10:20,transpose=1,hflip,vflip,scale=-2:720,setpts=0.500000*PTS,scale=trunc(iw/2)*2:trunc(ih/2)*2"
        );
    }

    #[test]
    fn half_and_three_quarter_turns_use_transpose_like_the_website() {
        let half = Edit {
            turn: Turn::Half,
            ..edit()
        };
        let three = Edit {
            turn: Turn::ThreeQuarters,
            scale: Scale::Exact(640, 480),
            ..edit()
        };

        assert!(text(Some(video_graph(&half))).starts_with("transpose=1,transpose=1,"));
        assert!(text(Some(video_graph(&three))).starts_with("transpose=2,scale=640:480,"));
    }

    #[test]
    fn the_sound_is_trimmed_then_turned_up_then_sped_up() {
        let e = Edit {
            end: 6.0,
            volume: 1.5,
            speed: 4.0,
            ..edit()
        };

        assert_eq!(
            text(audio_graph(&e)),
            "atrim=end=6.000,asetpts=PTS-STARTPTS,volume=1.50,atempo=2.0,atempo=2.0000"
        );
    }

    #[test]
    fn slow_speeds_chain_half_steps() {
        let e = Edit {
            speed: 0.25,
            ..edit()
        };

        assert_eq!(text(audio_graph(&e)), "atempo=0.5,atempo=0.5000");
    }

    #[test]
    fn a_video_without_sound_gets_no_audio_options() {
        let e = Edit {
            volume: 0.5,
            ..edit()
        };

        let args = output_args(&e, false, &caps()).unwrap();

        let words: Vec<_> = args.iter().map(|a| a.to_string_lossy()).collect();
        assert!(!words.iter().any(|w| w == "-af" || w == "0:a:0"));
        assert!(words.iter().any(|w| w == "-an"));
    }

    #[test]
    fn every_kind_of_edit_passes_the_output_allowlist() {
        let edits = [
            edit(),
            Edit {
                start: 2.0,
                end: 5.0,
                crop: Some(Rect {
                    x: 1,
                    y: 2,
                    w: 33,
                    h: 45,
                }),
                turn: Turn::Half,
                flip_h: true,
                scale: Scale::Exact(641, 479),
                speed: 0.25,
                volume: 0.0,
                format: Format::WebM,
                ..edit()
            },
            Edit {
                turn: Turn::ThreeQuarters,
                scale: Scale::Height(2160),
                speed: 4.0,
                format: Format::Mkv,
                ..edit()
            },
        ];

        let refused: Vec<_> = edits
            .iter()
            .filter(|e| check_output_args(&output_args(e, true, &caps()).unwrap()).is_err())
            .collect();

        assert!(refused.is_empty(), "{refused:?}");
    }

    #[test]
    fn the_output_size_is_cropped_turned_scaled_and_even() {
        let cropped = Edit {
            crop: Some(Rect {
                x: 0,
                y: 0,
                w: 641,
                h: 361,
            }),
            turn: Turn::Quarter,
            ..edit()
        };
        let scaled = Edit {
            scale: Scale::Height(720),
            ..edit()
        };

        assert_eq!(output_size(&cropped, (1920, 1080)), (360, 640));
        assert_eq!(output_size(&scaled, (1440, 1080)), (960, 720));
        assert_eq!(output_size(&edit(), (1280, 720)), (1280, 720));
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    /// Exports `edit` of a 4 s 320×240 clip with sound and returns the output's probe.
    fn export_clip(test: &str, edit: &Edit) -> Option<MediaInfo> {
        let (ff, dir) = real_ffmpeg(test)?;
        let input = dir.join("clip.mp4");
        make_clip(&input, 4, "320x240", true);
        let caps = ffmpeg_video::capabilities(&ff);
        let output = reserve_replacing(&dir.join(format!("out.{}", edit.format.extension()))).unwrap();
        let run = Run {
            inputs: std::slice::from_ref(&input),
            media: VIDEO,
            output: &output,
            duration: Some((edit.end - edit.start) / edit.speed),
            exit_prefix: "",
        };
        let mut last = 0.0;

        ff.run(
            &run,
            &CancelToken::default(),
            |infos| output_args(edit, infos[0].audio_codec.is_some(), caps),
            |p| last = p,
        )
        .unwrap();

        assert!(last > 0.99, "progress ended at {last}");
        let info = ff.probe_video(&output.keep().unwrap()).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        Some(info)
    }

    #[test]
    fn exports_a_trimmed_cropped_turned_and_faster_clip() {
        let e = Edit {
            start: 1.0,
            end: 3.0,
            duration: 4.0,
            crop: Some(Rect {
                x: 10,
                y: 10,
                w: 201,
                h: 151,
            }),
            turn: Turn::Quarter,
            flip_h: true,
            speed: 2.0,
            volume: 0.5,
            ..edit()
        };

        let Some(info) = export_clip("editor-mp4", &e) else {
            return;
        };

        assert_eq!((info.width, info.height), (Some(150), Some(200)));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert!((info.duration.unwrap() - 1.0).abs() < 0.15, "{:?}", info.duration);
    }

    #[test]
    fn exports_webm_at_a_new_height_and_half_speed() {
        let e = Edit {
            end: 1.0,
            duration: 4.0,
            scale: Scale::Height(120),
            speed: 0.5,
            format: Format::WebM,
            ..edit()
        };

        let Some(info) = export_clip("editor-webm", &e) else {
            return;
        };

        assert_eq!((info.width, info.height), (Some(160), Some(120)));
        assert_eq!(info.video_codec.as_deref(), Some("vp9"));
        assert!((info.duration.unwrap() - 2.0).abs() < 0.15, "{:?}", info.duration);
    }
}
