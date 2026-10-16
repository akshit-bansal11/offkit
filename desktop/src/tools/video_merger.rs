//! Video Merger: owns the tool's state and wires the `VideoMerger` global
//! (ui/tools/video-merger.slint) to FFmpeg. A list of clips in play order, joined into one
//! video through a save dialog.
//!
//! Every clip is brought to the first clip's size (letterboxed, never stretched), frame rate and
//! 4:2:0 pixels, and to 48 kHz stereo sound, because FFmpeg's `concat` needs identical streams.
//! A clip with no sound gets silence for its length. The state lives on the UI thread; probing,
//! thumbnails and the merge run in the background and come back through `jobs::post`.

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::Context;
use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, VecModel};

use crate::engine::ffmpeg::{
    Capabilities, Chain, Ffmpeg, Filter, Graph, HwFamily, MediaInfo, Run, VideoStream,
};
use crate::engine::ffmpeg_video::{
    self, Format, VIDEO, VIDEO_EXTENSIONS, clock, encoder_args, extract_frame, hw_for,
    missing_encoder, trim_number,
};
use crate::files::{self, reserve_replacing};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, VideoMerger, VideoMergerClip};

pub const SLUG: &str = "video-merger";
const CONCURRENT_PROBES: usize = 4;
/// A thumbnail's longer side, in pixels: twice the row's, for sharp edges on a dense display.
const THUMBNAIL_SIDE: u32 = 128;
/// The sound every clip is converted to.
const SAMPLE_RATE: u32 = 48_000;
/// The frame rate when the first clip doesn't say.
const FALLBACK_FPS: (u32, u32) = (30, 1);

enum Probe {
    Reading,
    Ready {
        info: MediaInfo,
        stream: VideoStream,
    },
    Failed(String),
}

struct Entry {
    id: u64,
    path: PathBuf,
    probe: Probe,
    thumbnail: Option<slint::Image>,
}

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    caps: Option<&'static Capabilities>,
    entries: Vec<Entry>,
    next_id: u64,
    rows: Rc<VecModel<VideoMergerClip>>,
    /// Cancels probes and thumbnails at shutdown.
    cancel: Arc<CancelToken>,
    merge: Option<Arc<CancelToken>>,
    output: Option<PathBuf>,
    workers: Vec<JoinHandle<()>>,
}

impl State {
    fn track(&mut self, workers: Vec<JoinHandle<()>>) {
        self.workers.retain(|w| !w.is_finished());
        self.workers.extend(workers);
    }

    fn entry(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
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

/// The size and frame rate every clip is brought to: the first clip's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    width: u32,
    height: u32,
    fps: (u32, u32),
}

impl Layout {
    /// Even sides, which 4:2:0 encoders need.
    fn of(stream: &VideoStream) -> Self {
        Self {
            width: (stream.width / 2 * 2).max(2),
            height: (stream.height / 2 * 2).max(2),
            fps: stream.frame_rate.unwrap_or(FALLBACK_FPS),
        }
    }

    fn fps_label(&self) -> String {
        trim_number(f64::from(self.fps.0) / f64::from(self.fps.1))
    }
}

/// What the merge needs of a clip's sound.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Input {
    audio: bool,
    seconds: Option<f64>,
}

/// A merge that can start: every clip read, at least two.
struct Plan {
    inputs: Vec<PathBuf>,
    layout: Layout,
    /// Seconds of output, when every clip's length is known.
    seconds: Option<f64>,
}

pub fn init(app: &AppWindow) {
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<VideoMerger>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    g.set_formats(select::options(Format::ALL.iter().map(|f| f.label())));
    with(|s| {
        g.set_clips(ModelRc::from(s.rows.clone()));
        s.ffmpeg = ffmpeg;
    });

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
        with(|s| s.track(vec![worker]));
    }
    refresh(app);

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Videos", Some(("Video", VIDEO_EXTENSIONS)))
        {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        if let Some(app) = weak.upgrade() {
            edit_list(&app, |entries| {
                entries.remove(i as usize);
            });
        }
    });
    let weak = app.as_weak();
    g.on_move_up(move |i| {
        if let Some(app) = weak.upgrade() {
            move_clip(&app, i as usize, -1);
        }
    });
    let weak = app.as_weak();
    g.on_move_down(move |i| {
        if let Some(app) = weak.upgrade() {
            move_clip(&app, i as usize, 1);
        }
    });
    let weak = app.as_weak();
    g.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_merge(move || {
        if let Some(app) = weak.upgrade() {
            merge(&app);
        }
    });
    g.on_cancel_merge(|| {
        if let Some(cancel) = with(|s| s.merge.clone()) {
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
    let g = app.global::<VideoMerger>();
    g.set_drop_hover(hover && g.get_ffmpeg_found() && !g.get_merging());
}

/// Adds the files to the end of the list, skipping folders, and reads them in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let started = with(|s| {
        if s.merge.is_some() {
            return None;
        }
        Some((s.ffmpeg.clone()?, s.cancel.clone()))
    });
    let Some((ff, cancel)) = started else { return };
    let added: Vec<(u64, PathBuf)> = with(|s| {
        paths
            .into_iter()
            .filter(|p| p.is_file())
            .map(|path| {
                s.next_id += 1;
                s.entries.push(Entry {
                    id: s.next_id,
                    path: path.clone(),
                    probe: Probe::Reading,
                    thumbnail: None,
                });
                (s.next_id, path)
            })
            .collect()
    });
    refresh(app);

    let weak = app.as_weak();
    let workers = jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        if cancel.is_cancelled() {
            return;
        }
        let probed = ff
            .probe_video(&path)
            .and_then(|info| Ok((ff.probe_video_stream(&path, &info)?, info)))
            .map_err(|e| format!("Couldn't read this file: {e}"));
        let at = probed
            .as_ref()
            .ok()
            .and_then(|(_, info)| info.duration)
            .map_or(0.0, |d| (d / 2.0).min(1.0));
        let readable = probed.is_ok();
        jobs::post(&weak, move |app| {
            with(|s| {
                if let Some(entry) = s.entry(id) {
                    entry.probe = match probed {
                        Ok((stream, info)) => Probe::Ready { info, stream },
                        Err(message) => Probe::Failed(message),
                    };
                }
            });
            refresh(&app);
        });
        if !readable {
            return;
        }
        match extract_frame(&ff, &path, at, THUMBNAIL_SIDE, &cancel) {
            Ok(frame) => jobs::post(&weak, move |app| {
                let (w, h) = frame.dimensions();
                let image = slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(
                    frame.as_raw(),
                    w,
                    h,
                ));
                with(|s| {
                    if let Some(entry) = s.entry(id) {
                        entry.thumbnail = Some(image);
                    }
                });
                refresh(&app);
            }),
            Err(JobError::Cancelled) => {}
            Err(JobError::Failed(e)) => {
                eprintln!("couldn't read a thumbnail of {}: {e}", path.display());
            }
        }
    });
    with(|s| s.track(workers));
}

/// Cancels the merge, the probes and the thumbnails, and hands back their threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        s.cancel.cancel();
        if let Some(merge) = &s.merge {
            merge.cancel();
        }
        std::mem::take(&mut s.workers)
    })
}

/// Changes the list unless a merge is reading it.
fn edit_list(app: &AppWindow, change: impl FnOnce(&mut Vec<Entry>)) {
    let editable = with(|s| {
        let editable = s.merge.is_none();
        if editable {
            change(&mut s.entries);
        }
        editable
    });
    if editable {
        refresh(app);
    }
}

/// Swaps clip `i` with its neighbour `step` places away, and sends keyboard focus after it.
fn move_clip(app: &AppWindow, i: usize, step: isize) {
    let Some(to) = i.checked_add_signed(step) else {
        return;
    };
    let moved = with(|s| {
        let movable = s.merge.is_none() && i < s.entries.len() && to < s.entries.len();
        if movable {
            s.entries.swap(i, to);
        }
        movable
    });
    if moved {
        refresh(app);
        app.global::<VideoMerger>().set_focus_clip(to as i32);
    }
}

/// The row a clip shows as.
fn row(entry: &Entry) -> VideoMergerClip {
    let (detail, failed) = match &entry.probe {
        Probe::Reading => ("Reading…".to_string(), false),
        Probe::Ready { info, .. } => (info.summary(), false),
        Probe::Failed(message) => (message.clone(), true),
    };
    VideoMergerClip {
        name: files::file_name(&entry.path).into(),
        detail: detail.into(),
        thumbnail: entry.thumbnail.clone().unwrap_or_default(),
        has_thumbnail: entry.thumbnail.is_some(),
        failed,
    }
}

/// Writes `rows` into the model in place, so the rows already on screen keep their focus.
fn show_rows(model: &VecModel<VideoMergerClip>, rows: Vec<VideoMergerClip>) {
    let kept = model.row_count().min(rows.len());
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
    for (i, row) in rows.into_iter().enumerate() {
        if i < kept {
            model.set_row_data(i, row);
        } else {
            model.push(row);
        }
    }
}

/// The merge the list describes, or `None` until every clip is read and there are two.
fn plan(entries: &[Entry]) -> Option<Plan> {
    let mut inputs = Vec::new();
    let mut layout = None;
    let mut seconds = Some(0.0);
    for entry in entries {
        let Probe::Ready { info, stream } = &entry.probe else {
            return None;
        };
        layout.get_or_insert_with(|| Layout::of(stream));
        seconds = seconds.zip(info.duration).map(|(sum, d)| sum + d);
        inputs.push(entry.path.clone());
    }
    (inputs.len() >= 2).then_some(Plan {
        inputs,
        layout: layout?,
        seconds,
    })
}

/// Why a merge can't start, if it can't.
fn blocked(entries: &[Entry], caps: Option<&Capabilities>, format: Format) -> Option<String> {
    let Some(caps) = caps else {
        return Some("Checking this FFmpeg's encoders…".into());
    };
    if entries.iter().any(|e| matches!(e.probe, Probe::Reading)) {
        return Some("Reading the clips…".into());
    }
    if entries.iter().any(|e| matches!(e.probe, Probe::Failed(_))) {
        return Some("Remove the clips that can't be read.".into());
    }
    if entries.len() < 2 {
        return Some("Add at least two videos to merge.".into());
    }
    missing_encoder(format, caps)
}

fn selected_format(g: &VideoMerger) -> Format {
    Format::ALL[index(g.get_format_index(), Format::ALL.len())]
}

/// Recomputes everything derived from the list and the settings.
fn refresh(app: &AppWindow) {
    let g = app.global::<VideoMerger>();
    let format = selected_format(&g);
    with(|s| {
        show_rows(&s.rows, s.entries.iter().map(row).collect());
        let merging = s.merge.is_some();
        let blocked = blocked(&s.entries, s.caps, format);
        g.set_list_summary(
            match s.entries.len() {
                0 => String::new(),
                1 => "1 video".to_string(),
                n => format!("{n} videos"),
            }
            .into(),
        );
        g.set_output_summary(
            plan(&s.entries)
                .map(|p| {
                    let total = p.seconds.map(|t| format!(" · {}", clock(t, false)));
                    format!(
                        "Joins {} clips into {}×{} · {} fps{}",
                        p.inputs.len(),
                        p.layout.width,
                        p.layout.height,
                        p.layout.fps_label(),
                        total.unwrap_or_default()
                    )
                })
                .unwrap_or_default()
                .into(),
        );
        let hw = s.caps.and_then(|c| c.hw);
        g.set_hw_name(hw.map(|f| f.label()).unwrap_or_default().into());
        g.set_hw_detail(match (hw, s.caps.and_then(|c| hw_for(format, c))) {
            (Some(_), Some(_)) => "Encodes on the GPU: faster, with slightly larger files.".into(),
            (Some(_), None) => format!("{} always encodes on the CPU.", format.label()).into(),
            _ => "".into(),
        });
        g.set_blocked_reason(blocked.clone().unwrap_or_default().into());
        g.set_can_merge(!merging && blocked.is_none());
    });
}

/// `scale` and `pad` fit the clip inside the first clip's frame without stretching it; the
/// frame rate and pixel format are the first clip's, so `concat` sees identical streams.
fn video_chain(layout: &Layout, i: usize) -> Chain {
    let scale = Filter::new("scale")
        .opt("w", layout.width)
        .opt("h", layout.height)
        .opt("force_original_aspect_ratio", "decrease");
    let pad = Filter::new("pad")
        .arg(layout.width)
        .arg(layout.height)
        .arg("(ow-iw)/2")
        .arg("(oh-ih)/2");
    let fps = Filter::new("fps").arg(format!("{}/{}", layout.fps.0, layout.fps.1));
    Chain::new()
        .input(&format!("{i}:v"))
        .then(scale)
        .then(pad)
        .then(Filter::new("setsar").arg(1))
        .then(fps)
        .then(Filter::new("format").arg("yuv420p"))
        .output(&format!("v{i}"))
}

/// The clip's sound as 48 kHz stereo, or silence as long as the clip.
fn audio_chain(input: &Input, i: usize) -> anyhow::Result<Chain> {
    let chain = if input.audio {
        Chain::new()
            .input(&format!("{i}:a"))
            .then(Filter::new("aresample").arg(SAMPLE_RATE))
            .then(
                Filter::new("aformat")
                    .opt("sample_fmts", "fltp")
                    .opt("channel_layouts", "stereo"),
            )
    } else {
        let seconds = input
            .seconds
            .with_context(|| format!("Couldn't read how long clip {} is", i + 1))?;
        Chain::new()
            .then(
                Filter::new("anullsrc")
                    .opt("r", SAMPLE_RATE)
                    .opt("cl", "stereo"),
            )
            .then(Filter::new("atrim").opt("duration", format!("{seconds:.3}")))
    };
    Ok(chain.output(&format!("a{i}")))
}

/// Every clip conformed, then joined: `[v0][a0][v1][a1]…concat=n=N:v=1:a=1[v][a]`. Sound is
/// joined only when some clip has it.
fn filter_graph(layout: &Layout, inputs: &[Input]) -> anyhow::Result<OsString> {
    let any_audio = inputs.iter().any(|i| i.audio);
    let mut graph = Graph::new();
    let mut joined = Chain::new();
    for (i, input) in inputs.iter().enumerate() {
        graph = graph.chain(video_chain(layout, i));
        joined = joined.input(&format!("v{i}"));
        if any_audio {
            graph = graph.chain(audio_chain(input, i)?);
            joined = joined.input(&format!("a{i}"));
        }
    }
    let concat = Filter::new("concat")
        .opt("n", inputs.len())
        .opt("v", 1)
        .opt("a", u8::from(any_audio));
    joined = joined.then(concat).output("v");
    if any_audio {
        joined = joined.output("a");
    }
    Ok(graph.chain(joined).arg())
}

/// The output options for the merge of `inputs`.
fn output_args(
    layout: &Layout,
    inputs: &[Input],
    format: Format,
    hw: Option<HwFamily>,
    caps: &Capabilities,
) -> anyhow::Result<Vec<OsString>> {
    let any_audio = inputs.iter().any(|i| i.audio);
    let mut args: Vec<OsString> = vec![
        "-filter_complex".into(),
        filter_graph(layout, inputs)?,
        "-map".into(),
        "[v]".into(),
    ];
    if any_audio {
        args.extend(["-map".into(), "[a]".into()]);
    }
    args.extend(encoder_args(format, any_audio, hw, caps)?);
    Ok(args)
}

fn merge(app: &AppWindow) {
    let g = app.global::<VideoMerger>();
    let prepared = with(|s| {
        if s.merge.is_some() {
            return None;
        }
        Some((s.ffmpeg.clone()?, s.caps?, plan(&s.entries)?))
    });
    let Some((ff, caps, plan)) = prepared else {
        return;
    };
    let mut format = selected_format(&g);
    let first = plan.inputs[0]
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let suggested = format!("{first} merged.{}", format.extension());
    // Outside `with`: the dialog runs a nested event loop.
    let Some(target) = files::choose_save_path("Save Merged Video", &suggested) else {
        return;
    };
    // A name typed with another of the three extensions picks that format.
    let Some(typed) = Format::from_path(&target) else {
        return finish(
            app,
            Err(JobError::Failed(format!(
                "Save it as .{}: that's the format chosen.",
                format.extension()
            ))),
        );
    };
    format = typed;
    let hw = hw_for(format, caps).filter(|_| g.get_hw_enabled());
    let output = match reserve_replacing(&target) {
        Ok(output) => output,
        Err(e) => {
            let name = files::file_name(&target);
            return finish(
                app,
                Err(JobError::Failed(format!("Couldn't create {name}: {e}"))),
            );
        }
    };
    let cancel = Arc::new(CancelToken::default());
    with(|s| {
        s.merge = Some(cancel.clone());
        s.output = None;
    });
    g.set_merge_progress(0.0);
    g.set_merge_message("".into());
    g.set_merge_failed(false);
    g.set_can_reveal(false);
    g.set_merging(true);
    refresh(app);

    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let run = Run {
            inputs: &plan.inputs,
            media: VIDEO,
            output: &output,
            duration: plan.seconds,
            exit_prefix: if hw.is_some() {
                "Hardware encoder failed: "
            } else {
                ""
            },
        };
        let progress = |p: f32| {
            jobs::post(&weak, move |app| {
                app.global::<VideoMerger>().set_merge_progress(p);
            });
        };
        // The clips are read again here, so what the merge builds is what FFmpeg will open.
        let prepare = |infos: &[MediaInfo]| {
            let inputs: Vec<Input> = infos
                .iter()
                .map(|info| Input {
                    audio: info.audio_codec.is_some(),
                    seconds: info.duration,
                })
                .collect();
            output_args(&plan.layout, &inputs, format, hw, caps)
        };
        let result = ff.run(&run, &cancel, prepare, progress).and_then(|()| {
            output
                .keep()
                .map_err(|e| JobError::Failed(format!("Couldn't save the video: {e}")))
        });
        jobs::post(&weak, move |app| finish(&app, result));
    });
    with(|s| s.track(vec![worker]));
}

fn finish(app: &AppWindow, result: Result<PathBuf, JobError>) {
    let g = app.global::<VideoMerger>();
    with(|s| s.merge = None);
    g.set_merging(false);
    match result {
        Ok(path) => {
            g.set_merge_message(format!("Saved as {}", files::file_name(&path)).into());
            g.set_merge_failed(false);
            g.set_can_reveal(true);
            with(|s| s.output = Some(path));
        }
        Err(JobError::Cancelled) => {
            g.set_merge_message("".into());
            g.set_merge_failed(false);
        }
        Err(JobError::Failed(message)) => {
            g.set_merge_message(message.into());
            g.set_merge_failed(true);
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

    fn layout() -> Layout {
        Layout {
            width: 1280,
            height: 720,
            fps: (30000, 1001),
        }
    }

    fn sound(seconds: f64) -> Input {
        Input {
            audio: true,
            seconds: Some(seconds),
        }
    }

    fn silent(seconds: Option<f64>) -> Input {
        Input {
            audio: false,
            seconds,
        }
    }

    fn caps() -> Capabilities {
        Capabilities::with(&["libx264", "libvpx-vp9", "aac", "libopus"], None)
    }

    fn text(arg: OsString) -> String {
        arg.to_string_lossy().into_owned()
    }

    fn entry(probe: Probe) -> Entry {
        Entry {
            id: 0,
            path: PathBuf::from("clip.mp4"),
            probe,
            thumbnail: None,
        }
    }

    fn ready(width: u32, height: u32) -> Probe {
        Probe::Ready {
            info: MediaInfo {
                format: "mov".into(),
                duration: Some(2.5),
                width: Some(width),
                height: Some(height),
                video_codec: Some("h264".into()),
                audio_codec: None,
            },
            stream: VideoStream {
                width,
                height,
                frame_rate: Some((25, 1)),
            },
        }
    }

    #[test]
    fn every_clip_is_fitted_padded_and_given_the_first_clips_rate() {
        let graph = text(filter_graph(&layout(), &[sound(1.0), sound(2.0)]).unwrap());

        assert!(
            graph.starts_with(
                "[0:v]scale=w=1280:h=720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30000/1001,format=yuv420p[v0];"
            ),
            "{graph}"
        );
    }

    #[test]
    fn sound_is_conformed_and_everything_is_joined_in_order() {
        let graph = text(filter_graph(&layout(), &[sound(1.0), sound(2.0)]).unwrap());

        assert!(
            graph.contains(
                "[0:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a0];"
            )
        );
        assert!(
            graph.ends_with("[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]"),
            "{graph}"
        );
    }

    #[test]
    fn a_clip_without_sound_gets_silence_as_long_as_itself() {
        let graph = text(filter_graph(&layout(), &[sound(1.0), silent(Some(2.5))]).unwrap());

        assert!(
            graph.contains("anullsrc=r=48000:cl=stereo,atrim=duration=2.500[a1];"),
            "{graph}"
        );
    }

    #[test]
    fn a_silent_clip_of_unknown_length_is_named() {
        let result = filter_graph(&layout(), &[sound(1.0), silent(None)]);

        assert_eq!(
            result.unwrap_err().to_string(),
            "Couldn't read how long clip 2 is"
        );
    }

    #[test]
    fn clips_without_any_sound_are_joined_as_video_only() {
        let graph = text(filter_graph(&layout(), &[silent(None), silent(None)]).unwrap());

        assert!(!graph.contains("anullsrc"), "{graph}");
        assert!(graph.ends_with("[v0][v1]concat=n=2:v=1:a=0[v]"), "{graph}");
    }

    #[test]
    fn the_merge_passes_the_output_allowlist() {
        let cases = [
            vec![sound(1.0), sound(2.0), silent(Some(3.0))],
            vec![silent(None), silent(None)],
        ];

        let refused: Vec<_> = cases
            .iter()
            .flat_map(|inputs| Format::ALL.map(|format| (inputs, format)))
            .filter(|(inputs, format)| {
                let args = output_args(&layout(), inputs, *format, None, &caps()).unwrap();
                check_output_args(&args).is_err()
            })
            .collect();

        assert!(refused.is_empty(), "{refused:?}");
    }

    #[test]
    fn only_the_joined_streams_are_mapped() {
        let with_sound = output_args(
            &layout(),
            &[sound(1.0), sound(1.0)],
            Format::Mp4,
            None,
            &caps(),
        )
        .unwrap();
        let without = output_args(
            &layout(),
            &[silent(None), silent(None)],
            Format::Mp4,
            None,
            &caps(),
        )
        .unwrap();

        let maps = |args: &[OsString]| {
            args.windows(2)
                .filter(|w| w[0] == "-map")
                .map(|w| text(w[1].clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(maps(&with_sound), ["[v]", "[a]"]);
        assert_eq!(maps(&without), ["[v]"]);
        assert!(without.iter().any(|a| a == "-an"));
    }

    #[test]
    fn the_layout_is_the_first_clips_with_even_sides() {
        let stream = VideoStream {
            width: 1081,
            height: 1919,
            frame_rate: None,
        };

        let layout = Layout::of(&stream);

        assert_eq!(
            layout,
            Layout {
                width: 1080,
                height: 1918,
                fps: (30, 1)
            }
        );
        assert_eq!(
            Layout {
                fps: (30000, 1001),
                ..layout
            }
            .fps_label(),
            "29.97"
        );
    }

    #[test]
    fn a_plan_needs_two_clips_all_read() {
        let one = [entry(ready(640, 360))];
        let two = [entry(ready(640, 360)), entry(ready(320, 240))];
        let reading = [entry(ready(640, 360)), entry(Probe::Reading)];

        assert!(plan(&one).is_none());
        assert!(plan(&reading).is_none());
        let planned = plan(&two).unwrap();
        assert_eq!((planned.layout.width, planned.layout.height), (640, 360));
        assert_eq!(planned.seconds, Some(5.0));
    }

    #[test]
    fn the_reason_a_merge_cant_start_is_named() {
        let caps = caps();
        let failed = [entry(ready(640, 360)), entry(Probe::Failed("x".into()))];
        let reading = [entry(ready(640, 360)), entry(Probe::Reading)];
        let alone = [entry(ready(640, 360))];
        let two = [entry(ready(640, 360)), entry(ready(640, 360))];

        assert_eq!(
            blocked(&failed, Some(&caps), Format::Mp4).as_deref(),
            Some("Remove the clips that can't be read.")
        );
        assert_eq!(
            blocked(&reading, Some(&caps), Format::Mp4).as_deref(),
            Some("Reading the clips…")
        );
        assert_eq!(
            blocked(&alone, Some(&caps), Format::Mp4).as_deref(),
            Some("Add at least two videos to merge.")
        );
        assert_eq!(blocked(&two, Some(&caps), Format::Mp4), None);
        assert_eq!(
            blocked(&two, None, Format::Mp4).as_deref(),
            Some("Checking this FFmpeg's encoders…")
        );
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    #[test]
    fn merges_clips_of_different_sizes_with_and_without_sound() {
        let Some((ff, dir)) = real_ffmpeg("merger") else {
            return;
        };
        let (big, small, mute) = (
            dir.join("big.mp4"),
            dir.join("small.mp4"),
            dir.join("mute.mp4"),
        );
        make_clip(&big, 2, "320x240", true);
        make_clip(&small, 1, "160x120", true);
        make_clip(&mute, 1, "200x100", false);
        let inputs = [big, small, mute];
        let output = reserve_replacing(&dir.join("merged.mp4")).unwrap();
        let run = Run {
            inputs: &inputs,
            media: VIDEO,
            output: &output,
            duration: Some(4.0),
            exit_prefix: "",
        };
        let caps = ffmpeg_video::capabilities(&ff);
        let mut last = 0.0;

        let result = ff.run(
            &run,
            &CancelToken::default(),
            |infos| {
                let layout = Layout {
                    width: 320,
                    height: 240,
                    fps: (25, 1),
                };
                let inputs: Vec<Input> = infos
                    .iter()
                    .map(|i| Input {
                        audio: i.audio_codec.is_some(),
                        seconds: i.duration,
                    })
                    .collect();
                output_args(&layout, &inputs, Format::Mp4, None, caps)
            },
            |p| last = p,
        );

        assert!(result.is_ok(), "{result:?}");
        assert!(last > 0.9, "progress ended at {last}");
        let info = ff.probe_video(&output.keep().unwrap()).unwrap();
        assert_eq!((info.width, info.height), (Some(320), Some(240)));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert!(
            (info.duration.unwrap() - 4.0).abs() < 0.3,
            "{:?}",
            info.duration
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
