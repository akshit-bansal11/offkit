//! Video family (frames-extractor, video-editor, video-merger): what two or more of its tools
//! share on top of `engine::ffmpeg`. The output formats and their encoder options (taken from
//! the Video Converter, so hardware encoding and quality behave the same), the hardware
//! detection they reuse, single-frame extraction for previews and thumbnails, and clock text.
//! Each tool's own filtergraph lives in its tool module. Owner: video family.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Context;

use crate::engine::ffmpeg::{
    AudioCodec, Capabilities, Chain, Container, ConvertJob, Ffmpeg, Filter, FramesRun, Graph,
    HwFamily, Settings, VideoCodec, build_args,
};
use crate::files::reserve_dir;
use crate::jobs::{CancelToken, JobError};

/// How the tools name their inputs in `probe`'s error.
pub const VIDEO: &str = "a video";

/// Video files the family's open dialogs offer: the Video Converter's list.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "ts", "mts", "m2ts",
    "3gp", "ogv", "gif",
];

/// The Video Editor's and Video Merger's output formats: the website's three, with the Video
/// Converter's codecs for them (VP9 and Opus rather than the website's VP8 and Vorbis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp4,
    WebM,
    Mkv,
}

impl Format {
    pub const ALL: [Format; 3] = [Self::Mp4, Self::WebM, Self::Mkv];

    pub fn label(self) -> &'static str {
        self.container().label()
    }

    pub fn extension(self) -> &'static str {
        self.container().extension()
    }

    /// The format a save path's extension names, if it's one of the three.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|f| f.extension() == ext)
    }

    fn container(self) -> Container {
        match self {
            Self::Mp4 => Container::Mp4,
            Self::WebM => Container::WebM,
            Self::Mkv => Container::Mkv,
        }
    }

    fn video(self) -> VideoCodec {
        match self {
            Self::WebM => VideoCodec::Vp9,
            Self::Mp4 | Self::Mkv => VideoCodec::H264,
        }
    }

    fn audio(self) -> AudioCodec {
        match self {
            Self::WebM => AudioCodec::Opus,
            Self::Mp4 | Self::Mkv => AudioCodec::Aac,
        }
    }
}

/// The encoders and verified hardware family, detected once for the whole family. Trial
/// encodes take about a second, so call it off the UI thread.
pub fn capabilities(ff: &Ffmpeg) -> &'static Capabilities {
    static CAPS: OnceLock<Capabilities> = OnceLock::new();
    CAPS.get_or_init(|| ff.capabilities())
}

/// The verified hardware family that encodes `format`'s video, if there is one.
pub fn hw_for(format: Format, caps: &Capabilities) -> Option<HwFamily> {
    caps.hw.filter(|_| caps.hw_encodes(format.video()))
}

/// Why `format` can't be written with this FFmpeg, if it can't.
pub fn missing_encoder(format: Format, caps: &Capabilities) -> Option<String> {
    let missing = match (
        caps.supports_video(format.video()),
        caps.supports_audio(format.audio()),
    ) {
        (false, _) => format.video().label(),
        (_, false) => format.audio().label(),
        _ => return None,
    };
    Some(format!(
        "This FFmpeg build has no {missing} encoder for {}. Choose another format.",
        format.label()
    ))
}

/// The Video Converter's encoder options for `format` at its default quality, without its
/// stream maps and filter: the editor and the merger map and filter the streams themselves.
/// `audio` false drops sound (`-an`); `hw` uses that verified family where it can.
pub fn encoder_args(
    format: Format,
    audio: bool,
    hw: Option<HwFamily>,
    caps: &Capabilities,
) -> anyhow::Result<Vec<OsString>> {
    let job = ConvertJob {
        input: PathBuf::new(),
        settings: Settings {
            container: format.container(),
            video: format.video(),
            audio: if audio {
                format.audio()
            } else {
                AudioCodec::None
            },
            // The converter's default: CRF 23 for H.264, as the website's FFmpeg defaulted to.
            quality: 60,
            hw,
        },
        duration: None,
        source_codec: None,
    };
    let mut args = build_args(&job, caps)?.into_iter();
    let mut kept = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "-map" || arg == "-vf" {
            args.next();
        } else {
            kept.push(arg);
        }
    }
    Ok(kept)
}

/// Rounds the frame down to even dimensions, which 4:2:0 encoders need (as the converter does).
pub fn even_size() -> Filter {
    Filter::new("scale")
        .arg("trunc(iw/2)*2")
        .arg("trunc(ih/2)*2")
}

/// One frame of `input` at `at` seconds, at most `max_side` pixels on its longer side, for a
/// preview or a thumbnail. Written through `run_frames` into a reserved temporary folder, which
/// is deleted once the frame is read.
pub fn extract_frame(
    ff: &Ffmpeg,
    input: &Path,
    at: f64,
    max_side: u32,
    cancel: &CancelToken,
) -> Result<::image::RgbaImage, JobError> {
    let dir = reserve_dir(input, Some(&std::env::temp_dir()), " preview")
        .map_err(|e| JobError::Failed(format!("Couldn't make a preview: {e}")))?;
    let run = FramesRun {
        input,
        media: VIDEO,
        dir: &dir,
        extension: "png",
        seek: Some(at),
        duration: None,
    };
    let fit = Filter::new("scale")
        .opt("w", format!("min(iw,{max_side})"))
        .opt("h", format!("min(ih,{max_side})"))
        .opt("force_original_aspect_ratio", "decrease");
    let args = |_: &_| {
        let mut args: Vec<OsString> = ["-map", "0:v:0", "-an", "-frames:v", "1", "-vf"]
            .map(OsString::from)
            .to_vec();
        args.push(Graph::new().chain(Chain::new().then(fit)).arg());
        args.extend(["-c:v", "png", "-compression_level", "1"].map(OsString::from));
        Ok(args)
    };
    ff.run_frames(&run, cancel, args, |_| {})?;
    let frame = fs::read_dir(dir.path())
        .and_then(|mut entries| entries.next().transpose())
        .map_err(JobError::failed)?
        .ok_or_else(|| JobError::Failed("There's no frame at that time".into()))?;
    ::image::open(frame.path())
        .context("Couldn't read the preview frame")
        .map(|img| img.to_rgba8())
        .map_err(JobError::failed)
}

/// A number with at most three decimals and no trailing zeros: 29.97, 0.5, 30.
pub fn trim_number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// "1:02.5" (tenths) or "1:02"; hours appear when needed ("1:00:02").
pub fn clock(seconds: f64, tenths: bool) -> String {
    let tenths_total = (seconds.max(0.0) * 10.0).round() as u64;
    let whole = if tenths {
        tenths_total / 10
    } else {
        seconds.max(0.0).round() as u64
    };
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    let mut text = match h {
        0 => format!("{m}:{s:02}"),
        _ => format!("{h}:{m:02}:{s:02}"),
    };
    if tenths {
        text.push_str(&format!(".{}", tenths_total % 10));
    }
    text
}

/// Fixtures for the family's tests, made with the FFmpeg on PATH.
#[cfg(test)]
pub mod test_media {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;

    use crate::engine::ffmpeg::Ffmpeg;

    /// A fresh folder for one test.
    pub fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("open-tools-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The shared FFmpeg and a fresh folder; `None`, with a note, when FFmpeg isn't installed.
    pub fn real_ffmpeg(test: &str) -> Option<(Arc<Ffmpeg>, PathBuf)> {
        let Some(ff) = Ffmpeg::shared() else {
            eprintln!("note: FFmpeg not found; skipping a test that runs it");
            return None;
        };
        Some((ff, temp_dir(test)))
    }

    /// A `seconds`-long test pattern at `size` and 25 fps, with a 1 kHz tone when `audio`.
    pub fn make_clip(path: &Path, seconds: u32, size: &str, audio: bool) {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc=duration={seconds}:size={size}:rate=25"));
        if audio {
            cmd.args(["-f", "lavfi", "-i"])
                .arg(format!("sine=duration={seconds}"))
                .args(["-c:a", "aac"]);
        }
        let status = cmd
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

#[cfg(test)]
mod tests {
    use super::test_media::{make_clip, real_ffmpeg};
    use super::*;

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn caps() -> Capabilities {
        Capabilities::with(
            &["libx264", "libvpx-vp9", "aac", "libopus", "h264_nvenc"],
            Some(HwFamily::Nvenc),
        )
    }

    #[test]
    fn mp4_encoder_args_are_the_converters_without_maps_or_filters() {
        let args = strings(encoder_args(Format::Mp4, true, None, &caps()).unwrap());

        assert_eq!(
            args.join(" "),
            "-c:v libx264 -preset medium -crf 23 -pix_fmt yuv420p -c:a aac -b:a 192k -sn -dn -movflags +faststart"
        );
    }

    #[test]
    fn webm_without_sound_is_vp9_and_no_audio() {
        let args = strings(encoder_args(Format::WebM, false, None, &caps()).unwrap());

        assert_eq!(
            args.join(" "),
            "-c:v libvpx-vp9 -crf 30 -b:v 0 -row-mt 1 -deadline good -cpu-used 4 -pix_fmt yuv420p -an -sn -dn"
        );
    }

    #[test]
    fn a_verified_hardware_family_encodes_h264() {
        let caps = caps();

        let args =
            strings(encoder_args(Format::Mkv, true, hw_for(Format::Mkv, &caps), &caps).unwrap());

        assert_eq!(args[..2], ["-c:v", "h264_nvenc"]);
    }

    #[test]
    fn webm_has_no_hardware_encoder() {
        assert_eq!(hw_for(Format::WebM, &caps()), None);
    }

    #[test]
    fn a_missing_encoder_names_the_codec() {
        let caps = Capabilities::with(&["libx264", "aac"], None);

        assert_eq!(
            missing_encoder(Format::WebM, &caps).as_deref(),
            Some("This FFmpeg build has no VP9 encoder for WebM. Choose another format.")
        );
        assert_eq!(missing_encoder(Format::Mp4, &caps), None);
    }

    #[test]
    fn a_save_path_extension_picks_its_format() {
        assert_eq!(Format::from_path(Path::new("a.WEBM")), Some(Format::WebM));
        assert_eq!(Format::from_path(Path::new("a.avi")), None);
    }

    #[test]
    fn numbers_have_no_trailing_zeros() {
        assert_eq!(trim_number(30.0), "30");
        assert_eq!(trim_number(29.97003), "29.97");
    }

    #[test]
    fn clock_text() {
        assert_eq!(clock(62.46, true), "1:02.5");
        assert_eq!(clock(62.46, false), "1:02");
        assert_eq!(clock(3602.0, false), "1:00:02");
        assert_eq!(clock(59.96, true), "1:00.0");
    }

    #[test]
    fn extracts_one_frame_and_leaves_no_temporary_folder() {
        let Some((ff, dir)) = real_ffmpeg("extract-frame") else {
            return;
        };
        let input = dir.join("extract-me.mp4");
        make_clip(&input, 2, "640x360", false);

        let frame = extract_frame(&ff, &input, 1.0, 320, &CancelToken::default()).unwrap();

        assert_eq!(frame.dimensions(), (320, 180));
        assert!(!std::env::temp_dir().join("extract-me preview").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
