//! Audio Extractor: owns the tool's state and wires the `AudioExtractor` global
//! (ui/tools/audio-extractor.slint) to the FFmpeg engine. Saves the sound of a batch of videos,
//! re-encoded to a chosen format or copied as it is; built like the Video Converter.
//!
//! A file is accepted only when its extension names a video and the probe finds an audio
//! track (the website checked only the browser's MIME type).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::ComponentHandle;

use crate::engine::ffmpeg::{Ffmpeg, MediaInfo};
use crate::engine::ffmpeg_audio::{self, BITRATES, Format, bitrate_label, words};
use crate::file_queue::{self, FileQueue, Update};
use crate::files::{self, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, AudioExtractor, FileStatus};

pub const SLUG: &str = "audio-extractor";
const CONCURRENT_EXTRACTIONS: usize = 2;
const CONCURRENT_PROBES: usize = 4;
/// The website's six, then the other common video containers.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "mov", "avi", "webm", "flv", "m4v", "wmv", "mpg", "mpeg", "ts", "mts", "m2ts",
    "3gp", "ogv",
];
const NOT_A_VIDEO: &str = "Not a video file. For audio files, use Audio Converter.";

/// The website's formats, in its order.
const FORMATS: [Format; 6] = [
    Format::Mp3,
    Format::Aac,
    Format::Opus,
    Format::Flac,
    Format::Wav,
    Format::M4a,
];

/// What a video's sound becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Encode(Format),
    /// The audio track as it is, in a container that holds its codec.
    Original,
}

impl Target {
    fn label(self) -> &'static str {
        match self {
            Self::Encode(f) => f.label(),
            Self::Original => "Original (no re-encode)",
        }
    }
}

/// A row's probe: `None` until it finishes, and for files it refused. Its row summary is the
/// Video Converter's (`impl Summary for Option<MediaInfo>`).
type Probe = Option<MediaInfo>;

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    /// The choices offered: every one until FFmpeg has listed its encoders, then those it has.
    targets: Vec<Target>,
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

/// The targets this FFmpeg can write: the formats it has an encoder for, then Original.
fn targets(formats: &[Format]) -> Vec<Target> {
    formats
        .iter()
        .map(|f| Target::Encode(*f))
        .chain([Target::Original])
        .collect()
}

pub fn init(app: &AppWindow) {
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<AudioExtractor>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    let bitrates: Vec<String> = BITRATES.into_iter().map(bitrate_label).collect();
    g.set_bitrates(select::options(bitrates.iter().map(String::as_str)));
    with(|s| {
        g.set_files(s.queue.rows());
        s.targets = targets(&FORMATS);
        s.ffmpeg = ffmpeg;
    });
    refresh_settings(app);
    sync(app);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Listing encoders runs FFmpeg, so it stays off the UI thread.
        let weak = app.as_weak();
        thread::spawn(move || {
            let formats = ffmpeg_audio::available(&FORMATS, ff.encoders());
            jobs::post(&weak, move |app| {
                let previous = chosen_target(&app);
                with(|s| {
                    s.targets = targets(&formats);
                    s.encoders_known = true;
                });
                let i = with(|s| s.targets.iter().position(|t| Some(*t) == previous));
                app.global::<AudioExtractor>()
                    .set_target_index(i.unwrap_or(0) as i32);
                refresh_settings(&app);
                sync(&app);
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
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Extracted Audio To") {
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
    let g = app.global::<AudioExtractor>();
    g.set_drop_hover(hover && g.get_ffmpeg_found());
}

fn is_video_name(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Adds files (from a drop, the dialog or the snapshot hook), skipping folders and duplicates.
/// A file not named as a video is listed as failed, saying why; the rest are probed in the
/// background and must hold an audio track.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(ff) = with(|s| s.ffmpeg.clone()) else {
        return;
    };
    let (videos, others): (Vec<_>, Vec<_>) =
        with(|s| s.queue.add(paths, FileStatus::Reading, || None))
            .into_iter()
            .partition(|(_, path)| is_video_name(path));
    with(|s| {
        for (id, _) in others {
            s.queue.update(id, |e| {
                e.status = FileStatus::Failed;
                e.message = NOT_A_VIDEO.into();
            });
        }
    });
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(videos, CONCURRENT_PROBES, move |(id, path)| {
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

/// Cancels running extractions and hands back their workers, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

fn chosen_target(app: &AppWindow) -> Option<Target> {
    let i = app.global::<AudioExtractor>().get_target_index();
    with(|s| s.targets.get(index(i, s.targets.len())).copied())
}

/// The file extension that holds an audio track of `codec` (as ffprobe names it) unchanged.
/// Matroska audio holds any codec, so it is the fallback.
fn copy_extension(codec: &str) -> &'static str {
    match codec {
        "aac" | "alac" => "m4a",
        "mp3" => "mp3",
        "mp2" => "mp2",
        "opus" => "opus",
        "vorbis" => "ogg",
        "flac" => "flac",
        "ac3" => "ac3",
        c if c.starts_with("pcm_") && (c.ends_with("le") || c == "pcm_u8") => "wav",
        _ => "mka",
    }
}

/// The output's extension and encoder options for one probed video.
fn plan(
    target: Target,
    bitrate: Option<u32>,
    info: &MediaInfo,
    ff: &Ffmpeg,
) -> anyhow::Result<(&'static str, Vec<std::ffi::OsString>)> {
    Ok(match target {
        Target::Encode(format) => (
            format.extension(),
            ffmpeg_audio::encode_args(format, bitrate, ff.encoders())?,
        ),
        Target::Original => (
            copy_extension(info.audio_codec.as_deref().unwrap_or_default()),
            words("-c:a copy"),
        ),
    })
}

fn extract_all(app: &AppWindow) {
    let g = app.global::<AudioExtractor>();
    let target = chosen_target(app);
    let bitrate = BITRATES[index(g.get_bitrate_index(), BITRATES.len())];
    let queued = with(|s| {
        let (Some(ff), Some(target)) = (s.ffmpeg.clone(), target) else {
            return None;
        };
        if !s.encoders_known {
            return None;
        }
        let jobs = s.queue.start(|e| e.data.clone());
        Some((ff, target, s.queue.out_dir().map(PathBuf::from), jobs))
    });
    let Some((ff, target, out_dir, jobs)) = queued else {
        return;
    };
    sync(app);

    let workers = file_queue::run(
        app,
        jobs,
        CONCURRENT_EXTRACTIONS,
        apply,
        move |job, progress| {
            let info: &MediaInfo = &job.work;
            let (extension, args) = plan(target, bitrate, info, &ff).map_err(JobError::failed)?;
            let output = reserve_output(&job.input, out_dir.as_deref(), extension)
                .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
            ffmpeg_audio::transcode(
                &ff,
                &job.input,
                &output,
                info.duration,
                args,
                &job.cancel,
                progress,
            )?;
            Ok(output)
        },
    );
    with(|s| s.queue.track(workers));
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
}

/// Rebuilds the format menu and whether a bitrate applies to the chosen one.
fn refresh_settings(app: &AppWindow) {
    let g = app.global::<AudioExtractor>();
    let labels: Vec<&str> = with(|s| s.targets.iter().map(|t| t.label()).collect());
    g.set_targets(select::options(labels.into_iter()));
    let target = chosen_target(app);
    g.set_bitrate_applies(matches!(target, Some(Target::Encode(f)) if f.lossy()));
    g.set_copying(target == Some(Target::Original));
}

/// Recomputes the list-level properties.
fn sync(app: &AppWindow) {
    let g = app.global::<AudioExtractor>();
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        let probed = q.entries().iter().all(|e| e.status != FileStatus::Reading);
        let extractable = q.can_start(|e| e.data.is_some());
        g.set_running(running);
        g.set_can_extract(s.encoders_known && probed && !running && extractable);
        g.set_list_summary(q.count_label("video", "videos").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aac_is_copied_into_m4a() {
        assert_eq!(copy_extension("aac"), "m4a");
    }

    #[test]
    fn little_endian_pcm_is_copied_into_wav() {
        assert_eq!(copy_extension("pcm_s24le"), "wav");
    }

    #[test]
    fn big_endian_pcm_falls_back_to_matroska() {
        assert_eq!(copy_extension("pcm_s16be"), "mka");
    }

    #[test]
    fn an_unknown_codec_falls_back_to_matroska() {
        assert_eq!(copy_extension("dts"), "mka");
    }

    #[test]
    fn video_names_are_matched_without_case() {
        assert!(is_video_name(Path::new("Holiday.MOV")));
    }

    #[test]
    fn audio_names_are_not_videos() {
        assert!(!is_video_name(Path::new("song.mp3")));
    }

    #[test]
    fn original_is_offered_after_the_encodable_formats() {
        let offered = targets(&[Format::Mp3]);

        assert_eq!(offered, [Target::Encode(Format::Mp3), Target::Original]);
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    use crate::engine::ffmpeg_audio::tests::{fixture, real_ffmpeg};
    use std::fs;

    /// Extracts `video`'s sound as `target` and returns the output's container and codec.
    fn extract(ff: &Ffmpeg, video: &Path, target: Target) -> (String, Option<String>) {
        let info = ffmpeg_audio::probe(ff, video).unwrap();
        let (extension, args) = plan(target, None, &info, ff).unwrap();
        let output = reserve_output(video, None, extension).unwrap();
        ffmpeg_audio::transcode(
            ff,
            video,
            &output,
            info.duration,
            args,
            &CancelToken::default(),
            |_| {},
        )
        .unwrap();
        let out = ffmpeg_audio::probe(ff, &output.keep().unwrap()).unwrap();
        (out.format, out.audio_codec)
    }

    #[test]
    fn original_copies_a_videos_aac_into_m4a() {
        let Some((ff, dir)) = real_ffmpeg("extract-copy") else {
            return;
        };
        let video = fixture(
            &dir,
            "clip.mp4",
            "testsrc=duration=1:size=64x64[out0];sine=duration=1[out1]",
            "-c:v libx264 -pix_fmt yuv420p -c:a aac",
        );

        let extracted = extract(&ff, &video, Target::Original);

        assert_eq!(extracted, ("mov".into(), Some("aac".into())));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_videos_sound_is_encoded_to_mp3() {
        let Some((ff, dir)) = real_ffmpeg("extract-mp3") else {
            return;
        };
        let video = fixture(
            &dir,
            "clip.mkv",
            "testsrc=duration=1:size=64x64[out0];sine=duration=1[out1]",
            "-c:v libx264 -pix_fmt yuv420p -c:a flac",
        );

        let extracted = extract(&ff, &video, Target::Encode(Format::Mp3));

        assert_eq!(extracted, ("mp3".into(), Some("mp3".into())));
        fs::remove_dir_all(&dir).unwrap();
    }
}
