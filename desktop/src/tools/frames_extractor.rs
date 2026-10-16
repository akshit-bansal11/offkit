//! Frames Extractor: owns the tool's state and wires the `FramesExtractor` global
//! (ui/tools/frames-extractor.slint) to FFmpeg's image-sequence runner (`Ffmpeg::run_frames`).
//! A batch tool like the Video Converter: each video's frames go into a new folder,
//! "<name> frames", next to it or in the chosen folder, or into "<name> frames.zip".
//!
//! Frames are picked as the website picks them (every frame, or a frame rate, from an optional
//! start to an optional end), or every Nth frame, or the first frame at or after each of a list
//! of times. The start is an input seek; every other choice is a filter, so the end, the rate
//! and the times all mean seconds of the source.

use std::cell::RefCell;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;

use slint::ComponentHandle;
use zip::write::SimpleFileOptions;

use crate::engine::ffmpeg::{Chain, Ffmpeg, Filter, FramesRun, Graph, MediaInfo, VideoStream};
use crate::engine::ffmpeg_video::{VIDEO, VIDEO_EXTENSIONS, trim_number};
use crate::file_queue::{FileQueue, Job, Summary, Update};
use crate::files::{self, ReservedDir, reserve_dir, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, FileStatus, FramesExtractor, FramesExtractorImage, FramesExtractorPick};

pub const SLUG: &str = "frames-extractor";
const CONCURRENT_EXTRACTIONS: usize = 2;
const CONCURRENT_PROBES: usize = 4;
/// The website's frame-rate choices.
const RATES: &[(&str, f64)] = &[
    ("30 frames a second", 30.0),
    ("10 frames a second", 10.0),
    ("5 frames a second", 5.0),
    ("1 frame a second", 1.0),
    ("1 frame every 2 seconds", 0.5),
    ("1 frame every 10 seconds", 0.1),
];

/// A row's probe and its video stream: `None` until both finish, and for files refused.
type Probe = Option<(MediaInfo, VideoStream)>;

impl Summary for Probe {
    fn summary(&self) -> String {
        let Some((info, stream)) = self else {
            return String::new();
        };
        match stream.fps() {
            Some(fps) => format!("{} · {} fps", info.summary(), trim_number(fps)),
            None => info.summary(),
        }
    }
}

/// Which frames to keep.
#[derive(Debug, Clone, PartialEq)]
enum Pick {
    Every,
    Rate(f64),
    Nth(u32),
    /// Seconds, sorted, without duplicates.
    Times(Vec<f64>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageKind {
    Png,
    Jpeg,
    Webp,
}

impl ImageKind {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    /// The encoder and its quality: JPEG at FFmpeg's best scale step, WebP at 90 of 100.
    fn codec_args(self) -> &'static str {
        match self {
            Self::Png => "-c:v png",
            Self::Jpeg => "-c:v mjpeg -q:v 2",
            Self::Webp => "-c:v libwebp -q:v 90",
        }
    }
}

/// Everything one extraction needs besides the file.
#[derive(Debug, Clone, PartialEq)]
struct Extraction {
    pick: Pick,
    /// Seconds into the source; ignored for `Pick::Times`.
    start: Option<f64>,
    end: Option<f64>,
    image: ImageKind,
    zip: bool,
}

struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    /// Whether this FFmpeg has the WebP encoder; `None` until its encoders are listed.
    webp: Option<bool>,
    queue: FileQueue<Probe>,
    /// The settings as last read, or why they can't run.
    extraction: Result<Extraction, String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        ffmpeg: None,
        webp: None,
        queue: FileQueue::default(),
        // `init` reads the settings before anything can run.
        extraction: Err(String::new()),
    });
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn init(app: &AppWindow) {
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<FramesExtractor>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    g.set_rates(select::options(RATES.iter().map(|(label, _)| *label)));
    with(|s| {
        g.set_files(s.queue.rows());
        s.ffmpeg = ffmpeg;
    });
    refresh_settings(app);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Listing the encoders runs FFmpeg, so it stays off the UI thread.
        let weak = app.as_weak();
        std::thread::spawn(move || {
            let webp = ff.encoders().contains("libwebp");
            jobs::post(&weak, move |app| {
                with(|s| s.webp = Some(webp));
                refresh_settings(&app);
            });
        });
    }

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
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Frames To") {
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
    g.on_get_ffmpeg(|| {
        if let Err(e) = open::that("https://ffmpeg.org/download.html") {
            eprintln!("couldn't open the FFmpeg download page: {e}");
        }
    });
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<FramesExtractor>();
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
        let result = ff
            .probe_video(&path)
            .and_then(|info| Ok((ff.probe_video_stream(&path, &info)?, info)))
            .map_err(|e| e.to_string());
        jobs::post(&weak, move |app| {
            with(|s| {
                s.queue.update(id, |e| match result {
                    Ok((stream, info)) => {
                        e.data = Some((info, stream));
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

/// Cancels running extractions and hands back their workers, for `tools::shutdown_all`.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

/// Reads the settings into an `Extraction`, or says what to fix.
fn read_settings(g: &FramesExtractor, webp: Option<bool>) -> Result<Extraction, String> {
    let image = match g.get_image() {
        FramesExtractorImage::Png => ImageKind::Png,
        FramesExtractorImage::Jpeg => ImageKind::Jpeg,
        FramesExtractorImage::Webp => ImageKind::Webp,
    };
    if image == ImageKind::Webp && webp == Some(false) {
        return Err("This FFmpeg build has no WebP encoder. Choose PNG or JPEG.".into());
    }
    let pick = match g.get_pick() {
        FramesExtractorPick::Every => Pick::Every,
        FramesExtractorPick::Rate => Pick::Rate(RATES[index(g.get_rate_index(), RATES.len())].1),
        FramesExtractorPick::Nth => Pick::Nth(g.get_nth().round().clamp(2.0, 10_000.0) as u32),
        FramesExtractorPick::Times => Pick::Times(parse_times(&g.get_times())?),
    };
    let time = |text: &str, which: &str| match text.trim() {
        "" => Ok(None),
        t => parse_time(t)
            .map(Some)
            .ok_or_else(|| format!("The {which} time “{t}” isn't seconds or m:ss.")),
    };
    let (start, end) = (time(&g.get_start(), "start")?, time(&g.get_end(), "end")?);
    if let (Some(s), Some(e)) = (start, end)
        && e <= s
    {
        return Err("The end time must come after the start time.".into());
    }
    Ok(Extraction {
        pick,
        start,
        end,
        image,
        zip: g.get_zip(),
    })
}

fn refresh_settings(app: &AppWindow) {
    let g = app.global::<FramesExtractor>();
    let unreadable = |text: &str| !text.trim().is_empty() && parse_time(text).is_none();
    g.set_start_invalid(unreadable(&g.get_start()));
    g.set_end_invalid(unreadable(&g.get_end()));
    let times = g.get_times();
    g.set_times_invalid(!times.trim().is_empty() && parse_times(&times).is_err());
    with(|s| s.extraction = read_settings(&g, s.webp));
    sync(app);
}

/// Recomputes the list-level properties and the frame estimate.
fn sync(app: &AppWindow) {
    let g = app.global::<FramesExtractor>();
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        let ready = q.entries().iter().all(|e| e.status != FileStatus::Reading);
        let blocked = s.extraction.as_ref().err().cloned().unwrap_or_default();
        g.set_running(running);
        g.set_can_extract(
            ready && !running && blocked.is_empty() && q.can_start(|e| e.data.is_some()),
        );
        g.set_blocked_reason(blocked.into());
        g.set_list_summary(q.count_label("video", "videos").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
        let total: Option<u64> = match &s.extraction {
            Ok(x) => q
                .entries()
                .iter()
                .filter_map(|e| e.data.as_ref())
                .map(|(info, stream)| estimate(x, info.duration, stream.fps()))
                .sum(),
            Err(_) => None,
        };
        g.set_estimate(
            match total {
                Some(0) | None => String::new(),
                Some(1) => "About 1 frame.".into(),
                Some(n) => format!("About {} frames.", group_thousands(n)),
            }
            .into(),
        );
    });
}

fn extract_all(app: &AppWindow) {
    let queued = with(|s| {
        let ff = s.ffmpeg.clone()?;
        let extraction = s.extraction.clone().ok()?;
        let jobs = s
            .queue
            .start(|e| e.data.as_ref().map(|(info, _)| info.duration));
        Some((ff, extraction, s.queue.out_dir().map(PathBuf::from), jobs))
    });
    let Some((ff, extraction, out_dir, jobs)) = queued else {
        return;
    };
    sync(app);

    // `file_queue::run` takes a `Reserved` file; a folder of frames is a `ReservedDir`, so the
    // same loop is spelled out here: skip cancelled jobs, report, catch a panic.
    let weak = app.as_weak();
    let workers = jobs::run_pool(
        jobs,
        CONCURRENT_EXTRACTIONS,
        move |job: Job<Option<f64>>| {
            if job.cancel.is_cancelled() {
                return; // Cancelled while queued; the row already says so.
            }
            let post = |update: Update| {
                let (id, token) = (job.id, job.cancel.clone());
                jobs::post(&weak, move |app| apply(&app, id, &token, update));
            };
            post(Update::Running);
            let progress = |p| post(Update::Progress(p));
            let work = || {
                extract(
                    &ff,
                    &job.input,
                    job.work,
                    &extraction,
                    out_dir.as_deref(),
                    &job.cancel,
                    &progress,
                )
            };
            let result = panic::catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|panic| {
                let reason = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                eprintln!(
                    "extracting frames from {} panicked: {reason}",
                    job.input.display()
                );
                Err(JobError::Failed(
                    "Something went wrong with this file. Try again, or remove it.".into(),
                ))
            });
            post(Update::Finished(result));
        },
    );
    with(|s| s.queue.track(workers));
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
}

/// Extracts `input`'s frames into a new folder (or ZIP) in `out_dir` or beside it, returning
/// where they went. Nothing is left behind on failure or cancellation.
fn extract(
    ff: &Ffmpeg,
    input: &Path,
    duration: Option<f64>,
    x: &Extraction,
    out_dir: Option<&Path>,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<PathBuf, JobError> {
    let frames = reserve_dir(input, out_dir, " frames")
        .map_err(|e| JobError::Failed(format!("Couldn't create the frames folder: {e}")))?;
    let run = FramesRun {
        input,
        media: VIDEO,
        dir: &frames,
        extension: x.image.extension(),
        seek: x.start.filter(|_| !matches!(x.pick, Pick::Times(_))),
        duration: output_seconds(x, duration),
    };
    ff.run_frames(&run, cancel, |_| Ok(output_args(x)), progress)?;
    let count = fs::read_dir(frames.path())
        .map_err(JobError::failed)?
        .count();
    if count == 0 {
        return Err(JobError::Failed(match x.pick {
            Pick::Times(_) => "None of those times is in this video".into(),
            _ => "There are no frames in that range".into(),
        }));
    }
    if !x.zip {
        return Ok(frames.keep());
    }
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let named = input.with_file_name(format!("{stem} frames.zip"));
    let archive = reserve_output(&named, out_dir, "zip")
        .map_err(|e| JobError::Failed(format!("Couldn't create the ZIP file: {e}")))?;
    zip_folder(&frames, archive.path(), cancel)?;
    // Dropping `frames` deletes the folder now that the archive holds them.
    archive
        .keep()
        .map_err(|e| JobError::Failed(format!("Couldn't save the ZIP file: {e}")))
}

/// Stores every frame in `frames` in a ZIP at `out`, by name order. Frames are already
/// compressed images, so they're stored as they are.
fn zip_folder(frames: &ReservedDir, out: &Path, cancel: &CancelToken) -> Result<(), JobError> {
    let failed =
        |e: &dyn std::fmt::Display| JobError::Failed(format!("Couldn't write the ZIP file: {e}"));
    let mut names = fs::read_dir(frames.path())
        .and_then(|entries| {
            entries
                .map(|e| e.map(|e| e.file_name()))
                .collect::<io::Result<Vec<_>>>()
        })
        .map_err(|e| failed(&e))?;
    names.sort();
    let file = File::create(out).map_err(|e| failed(&e))?;
    let mut zip = zip::ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        zip.start_file(name.to_string_lossy(), options)
            .map_err(|e| failed(&e))?;
        let mut frame = File::open(frames.path().join(&name)).map_err(|e| failed(&e))?;
        io::copy(&mut frame, &mut zip).map_err(|e| failed(&e))?;
    }
    zip.finish()
        .map_err(|e| failed(&e))?
        .flush()
        .map_err(|e| failed(&e))
}

/// The output options for `x`: the first video stream only, the frames picked by filters, and
/// the image encoder.
fn output_args(x: &Extraction) -> Vec<OsString> {
    let mut args: Vec<OsString> = words("-map 0:v:0 -an -sn -dn");
    let mut filters = Vec::new();
    match &x.pick {
        Pick::Times(times) => {
            // Decoding stops a second after the last time; the expression keeps the first
            // frame at or after each time.
            let last = times.last().copied().unwrap_or_default();
            filters.push(Filter::new("trim").opt("end", trim_number(last + 1.0)));
            filters.push(Filter::new("select").arg(times_expression(times)));
        }
        pick => {
            // After the input seek, time 0 is the start, so the end becomes a duration.
            if let Some(end) = x.end {
                let length = end - x.start.unwrap_or_default();
                filters.push(Filter::new("trim").opt("duration", trim_number(length)));
            }
            match pick {
                Pick::Rate(fps) => filters.push(Filter::new("fps").arg(trim_number(*fps))),
                Pick::Nth(n) => {
                    filters.push(Filter::new("select").arg(format!("not(mod(n,{n}))")));
                }
                Pick::Every | Pick::Times(_) => {}
            }
        }
    }
    if !filters.is_empty() {
        let chain = filters.into_iter().fold(Chain::new(), Chain::then);
        args.push("-vf".into());
        args.push(Graph::new().chain(chain).arg());
    }
    // Every picked frame is written once, with its own time, never duplicated to a rate.
    if !matches!(x.pick, Pick::Rate(_)) {
        args.extend(words("-fps_mode passthrough"));
    }
    args.extend(words(x.image.codec_args()));
    args
}

/// Keeps a frame when, for some time T, it is at or after T and no frame has been kept at or
/// after T yet. Times must be sorted.
fn times_expression(times: &[f64]) -> String {
    let prev = "if(isnan(prev_selected_t),-1,prev_selected_t)";
    times
        .iter()
        .map(|t| {
            let t = trim_number(*t);
            format!("gte(t,{t})*lt({prev},{t})")
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Seconds of output, for progress.
fn output_seconds(x: &Extraction, duration: Option<f64>) -> Option<f64> {
    match &x.pick {
        Pick::Times(times) => times.last().map(|t| t + 1.0),
        _ => {
            let end = x.end.or(duration)?;
            Some((end - x.start.unwrap_or_default()).max(0.0))
        }
    }
    .map(|s| duration.map_or(s, |d| s.min(d)))
}

/// Roughly how many frames `x` takes from a video of `duration` at `fps`.
fn estimate(x: &Extraction, duration: Option<f64>, fps: Option<f64>) -> Option<u64> {
    let duration = duration?;
    let length = (x.end.unwrap_or(duration).min(duration) - x.start.unwrap_or_default()).max(0.0);
    let frames = match &x.pick {
        Pick::Every => length * fps?,
        Pick::Rate(rate) => length * rate,
        Pick::Nth(n) => length * fps? / f64::from(*n),
        Pick::Times(times) => times.iter().filter(|t| **t < duration).count() as f64,
    };
    Some(frames.ceil() as u64)
}

/// "1.5", "90" or "0:01.5", "1:02:03": seconds, optionally after minutes and hours.
fn parse_time(text: &str) -> Option<f64> {
    let mut seconds = 0.0;
    for part in text.trim().split(':') {
        let value: f64 = part.trim().parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        seconds = seconds * 60.0 + value;
    }
    Some(seconds)
}

/// A comma- or space-separated list of times, sorted and without duplicates.
fn parse_times(text: &str) -> Result<Vec<f64>, String> {
    let mut times = text
        .split([',', ';', ' ', '\n'])
        .filter(|t| !t.trim().is_empty())
        .map(|t| {
            parse_time(t)
                .ok_or_else(|| format!("“{}” isn't a time. Use seconds or m:ss.", t.trim()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if times.is_empty() {
        return Err("Type the times to take frames at.".into());
    }
    times.sort_by(f64::total_cmp);
    times.dedup();
    Ok(times)
}

/// 12,345.
fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn words(text: &str) -> Vec<OsString> {
    text.split_whitespace().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ffmpeg::check_output_args;
    use crate::engine::ffmpeg_video::test_media::{make_clip, real_ffmpeg};

    fn line(args: Vec<OsString>) -> String {
        args.iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extraction(pick: Pick) -> Extraction {
        Extraction {
            pick,
            start: None,
            end: None,
            image: ImageKind::Png,
            zip: false,
        }
    }

    #[test]
    fn every_frame_is_written_once_each() {
        let args = output_args(&extraction(Pick::Every));

        assert_eq!(
            line(args),
            "-map 0:v:0 -an -sn -dn -fps_mode passthrough -c:v png"
        );
    }

    #[test]
    fn a_frame_rate_is_an_fps_filter_after_the_end_trim() {
        let mut x = extraction(Pick::Rate(0.5));
        x.start = Some(2.0);
        x.end = Some(7.5);
        x.image = ImageKind::Jpeg;

        assert_eq!(
            line(output_args(&x)),
            "-map 0:v:0 -an -sn -dn -vf trim=duration=5.5,fps=0.5 -c:v mjpeg -q:v 2"
        );
    }

    #[test]
    fn every_nth_frame_selects_by_frame_number() {
        let args = output_args(&extraction(Pick::Nth(5)));

        assert_eq!(
            line(args),
            "-map 0:v:0 -an -sn -dn -vf select=not(mod(n\\,5)) -fps_mode passthrough -c:v png"
        );
    }

    #[test]
    fn chosen_times_select_the_first_frame_at_or_after_each() {
        let mut x = extraction(Pick::Times(vec![1.0, 2.5]));
        x.image = ImageKind::Webp;

        let args = line(output_args(&x));

        let prev = "if(isnan(prev_selected_t)\\,-1\\,prev_selected_t)";
        assert_eq!(
            args,
            format!(
                "-map 0:v:0 -an -sn -dn -vf trim=end=3.5,select=gte(t\\,1)*lt({prev}\\,1)+gte(t\\,2.5)*lt({prev}\\,2.5) -fps_mode passthrough -c:v libwebp -q:v 90"
            )
        );
    }

    #[test]
    fn every_pick_and_format_passes_the_output_allowlist() {
        let picks = [
            Pick::Every,
            Pick::Rate(29.97),
            Pick::Nth(3),
            Pick::Times(vec![0.5, 61.25]),
        ];
        let refused: Vec<_> = picks
            .into_iter()
            .flat_map(|pick| {
                [ImageKind::Png, ImageKind::Jpeg, ImageKind::Webp].map(|image| Extraction {
                    image,
                    start: Some(1.0),
                    end: Some(4.0),
                    ..extraction(pick.clone())
                })
            })
            .filter(|x| check_output_args(&output_args(x)).is_err())
            .collect();

        assert!(refused.is_empty(), "{refused:?}");
    }

    #[test]
    fn times_parse_as_seconds_or_minutes_and_seconds() {
        assert_eq!(parse_time("1.5"), Some(1.5));
        assert_eq!(parse_time(" 1:02.5 "), Some(62.5));
        assert_eq!(parse_time("1:00:02"), Some(3602.0));
        assert_eq!(parse_time("-1"), None);
        assert_eq!(parse_time("1:x"), None);
    }

    #[test]
    fn a_list_of_times_is_sorted_and_deduplicated() {
        assert_eq!(parse_times("5, 0:01, 1 2.5;5"), Ok(vec![1.0, 2.5, 5.0]));
    }

    #[test]
    fn an_unreadable_time_is_named() {
        assert_eq!(
            parse_times("1, soon"),
            Err("“soon” isn't a time. Use seconds or m:ss.".into())
        );
    }

    #[test]
    fn an_empty_list_of_times_asks_for_some() {
        assert_eq!(
            parse_times(" , "),
            Err("Type the times to take frames at.".into())
        );
    }

    #[test]
    fn estimates_count_frames_in_the_range() {
        let mut x = extraction(Pick::Every);
        x.start = Some(2.0);

        assert_eq!(estimate(&x, Some(10.0), Some(25.0)), Some(200));
        assert_eq!(
            estimate(&extraction(Pick::Nth(10)), Some(10.0), Some(30.0)),
            Some(30)
        );
        assert_eq!(
            estimate(&extraction(Pick::Times(vec![1.0, 20.0])), Some(10.0), None),
            Some(1)
        );
        assert_eq!(estimate(&extraction(Pick::Every), Some(10.0), None), None);
    }

    #[test]
    fn counts_read_naturally() {
        assert_eq!(group_thousands(1_234_567), "1,234,567");
        assert_eq!(group_thousands(999), "999");
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn extracts_a_range_at_a_frame_rate_into_a_new_folder() {
        let Some((ff, dir)) = real_ffmpeg("frames-rate") else {
            return;
        };
        let input = dir.join("clip.mp4");
        make_clip(&input, 4, "320x240", true);
        let mut x = extraction(Pick::Rate(1.0));
        x.start = Some(1.0);
        x.end = Some(3.0);

        let out = extract(
            &ff,
            &input,
            Some(4.0),
            &x,
            None,
            &CancelToken::default(),
            &|_| {},
        );

        assert_eq!(out.as_ref().unwrap(), &dir.join("clip frames"));
        assert_eq!(
            names(&dir.join("clip frames")),
            ["clip_000001.png", "clip_000002.png"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extracts_frames_at_chosen_times_into_a_zip_and_removes_the_folder() {
        let Some((ff, dir)) = real_ffmpeg("frames-zip") else {
            return;
        };
        let input = dir.join("clip.mp4");
        make_clip(&input, 3, "320x240", false);
        let mut x = extraction(Pick::Times(vec![0.5, 1.0, 2.0, 9.0]));
        x.image = ImageKind::Jpeg;
        x.zip = true;

        let out = extract(
            &ff,
            &input,
            Some(3.0),
            &x,
            None,
            &CancelToken::default(),
            &|_| {},
        )
        .unwrap();

        assert_eq!(out, dir.join("clip frames.zip"));
        assert_eq!(names(&dir), ["clip frames.zip", "clip.mp4"]);
        let archive = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let mut entries: Vec<_> = archive
            .file_names()
            .map(|n| n.unwrap().into_owned())
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            ["clip_000001.jpg", "clip_000002.jpg", "clip_000003.jpg"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_nth_frame_as_webp() {
        let Some((ff, dir)) = real_ffmpeg("frames-nth") else {
            return;
        };
        let input = dir.join("clip.mp4");
        make_clip(&input, 2, "160x120", false);
        let mut x = extraction(Pick::Nth(10));
        x.image = ImageKind::Webp;

        extract(
            &ff,
            &input,
            Some(2.0),
            &x,
            None,
            &CancelToken::default(),
            &|_| {},
        )
        .unwrap();

        assert_eq!(names(&dir.join("clip frames")).len(), 5);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_range_past_the_end_fails_and_leaves_nothing() {
        let Some((ff, dir)) = real_ffmpeg("frames-empty") else {
            return;
        };
        let input = dir.join("clip.mp4");
        make_clip(&input, 1, "160x120", false);
        let mut x = extraction(Pick::Every);
        x.start = Some(5.0);

        let result = extract(
            &ff,
            &input,
            Some(1.0),
            &x,
            None,
            &CancelToken::default(),
            &|_| {},
        );

        assert!(
            matches!(&result, Err(JobError::Failed(m)) if m == "There are no frames in that range"),
            "{result:?}"
        );
        assert_eq!(names(&dir), ["clip.mp4"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
