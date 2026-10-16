//! Audio family (audio-converter, audio-extractor, audio-editor, audio-joiner): FFmpeg
//! arguments and audio-specific parsing, run through `engine::ffmpeg`. Owner: audio family.
//!
//! Shared by the four tools: the output formats and their encoder arguments (the website's
//! codec map, limited to the encoders this FFmpeg has), and the probe that accepts a file.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::Path;

use anyhow::bail;

use crate::engine::ffmpeg::{Ffmpeg, MediaInfo, Run, require_audio};
use crate::files::Reserved;
use crate::jobs::{CancelToken, JobError};

/// How the audio tools name what they accept, in `probe`'s error: "Not an audio file FFmpeg
/// can read".
pub const MEDIA: &str = "an audio file";

/// What the open dialogs offer. A drop isn't limited to these: `probe` decides.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "aac", "opus", "ogg", "oga", "flac", "wav", "aif", "aiff", "wma", "mp2", "mpeg",
    "mpg", "ac3", "mka", "wv", "ape", "caf", "amr",
];

/// The first audio track: every audio tool reads and writes exactly one.
pub const FIRST_AUDIO: &str = "-map 0:a:0";

/// Reads a file's streams, as every audio tool accepts files: FFmpeg must read it (see
/// `Ffmpeg::probe`) and it must hold an audio track.
pub fn probe(ff: &Ffmpeg, path: &Path) -> anyhow::Result<MediaInfo> {
    let info = ff.probe(path, MEDIA)?;
    require_audio(&info)?;
    Ok(info)
}

/// Whitespace-separated words as arguments; no word holds a space.
pub fn words(text: &str) -> Vec<OsString> {
    text.split_whitespace().map(OsString::from).collect()
}

// ---- Formats ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp3,
    Opus,
    M4a,
    Aac,
    Mpeg,
    Wma,
    Flac,
    Wav,
    Aiff,
    Ogg,
}

impl Format {
    /// The Audio Converter's list: the website's, in its order, then OGG Vorbis.
    pub const ALL: [Format; 10] = [
        Self::Mp3,
        Self::Opus,
        Self::M4a,
        Self::Aac,
        Self::Mpeg,
        Self::Wma,
        Self::Flac,
        Self::Wav,
        Self::Aiff,
        Self::Ogg,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Opus => "Opus",
            Self::M4a => "M4A (AAC)",
            Self::Aac => "AAC",
            Self::Mpeg => "MPEG (MP2)",
            Self::Wma => "WMA",
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
            Self::Aiff => "AIFF",
            Self::Ogg => "OGG (Vorbis)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::M4a => "m4a",
            Self::Aac => "aac",
            Self::Mpeg => "mpeg",
            Self::Wma => "wma",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Ogg => "ogg",
        }
    }

    /// Whether a bitrate means anything for this format.
    pub fn lossy(self) -> bool {
        !matches!(self, Self::Flac | Self::Wav | Self::Aiff)
    }

    /// The website's codec map, first choice first, as (encoder, its arguments). Opus prefers
    /// libopus; the website couldn't use it (the wasm build crashed on 44.1 kHz stereo), so its
    /// workaround, FFmpeg's experimental native encoder at 48 kHz stereo, is the fallback. OGG
    /// falls back to the native Vorbis encoder the same way.
    fn encoders(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Mp3 => &[("libmp3lame", "-c:a libmp3lame")],
            Self::Opus => &[
                ("libopus", "-c:a libopus"),
                ("opus", "-c:a opus -strict -2 -ar 48000 -ac 2"),
            ],
            Self::M4a | Self::Aac => &[("aac", "-c:a aac")],
            Self::Mpeg => &[("mp2", "-c:a mp2"), ("libmp3lame", "-c:a libmp3lame")],
            Self::Wma => &[("wmav2", "-c:a wmav2")],
            Self::Flac => &[("flac", "-c:a flac")],
            Self::Wav => &[("pcm_s16le", "-c:a pcm_s16le")],
            Self::Aiff => &[("pcm_s16be", "-c:a pcm_s16be")],
            Self::Ogg => &[
                ("libvorbis", "-c:a libvorbis"),
                ("vorbis", "-c:a vorbis -strict -2 -ac 2"),
            ],
        }
    }

    fn codec_args(self, encoders: &HashSet<String>) -> Option<&'static str> {
        self.encoders()
            .iter()
            .find(|(encoder, _)| encoders.contains(*encoder))
            .map(|(_, args)| *args)
    }

    /// Whether this FFmpeg has an encoder for the format.
    pub fn available(self, encoders: &HashSet<String>) -> bool {
        self.codec_args(encoders).is_some()
    }
}

/// The formats of `list` this FFmpeg can write, in order.
pub fn available(list: &[Format], encoders: &HashSet<String>) -> Vec<Format> {
    list.iter()
        .copied()
        .filter(|f| f.available(encoders))
        .collect()
}

/// Bitrate choices for lossy formats, in kbps; `None` leaves it to the encoder, as the
/// website did.
pub const BITRATES: [Option<u32>; 5] = [None, Some(128), Some(192), Some(256), Some(320)];

pub fn bitrate_label(bitrate: Option<u32>) -> String {
    bitrate.map_or_else(|| "Automatic".into(), |k| format!("{k} kbps"))
}

/// The encoder options for `format`, plus `-b:a` when it is lossy and a bitrate is chosen.
pub fn encode_args(
    format: Format,
    bitrate: Option<u32>,
    encoders: &HashSet<String>,
) -> anyhow::Result<Vec<OsString>> {
    let Some(codec) = format.codec_args(encoders) else {
        bail!("This FFmpeg build has no {} encoder", format.label());
    };
    let mut args = words(codec);
    if let Some(k) = bitrate.filter(|_| format.lossy()) {
        args.extend(words(&format!("-b:a {k}k")));
    }
    Ok(args)
}

/// Writes `input`'s first audio track into `output` with `options` (encoder options, and any
/// filter), for one-file tools. `duration` is the output's length in seconds, for progress.
pub fn transcode(
    ff: &Ffmpeg,
    input: &Path,
    output: &Reserved,
    duration: Option<f64>,
    options: Vec<OsString>,
    cancel: &CancelToken,
    on_progress: impl FnMut(f32),
) -> Result<(), JobError> {
    let inputs = [input.to_path_buf()];
    let run = Run {
        inputs: &inputs,
        media: MEDIA,
        output,
        duration,
        exit_prefix: "",
    };
    let prepare = |infos: &[MediaInfo]| {
        infos.iter().try_for_each(require_audio)?;
        let mut args = words(FIRST_AUDIO);
        args.extend(options);
        Ok(args)
    };
    ff.run(&run, cancel, prepare, on_progress)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    fn all_encoders() -> HashSet<String> {
        [
            "libmp3lame",
            "libopus",
            "aac",
            "mp2",
            "wmav2",
            "flac",
            "pcm_s16le",
            "pcm_s16be",
            "libvorbis",
        ]
        .map(String::from)
        .into()
    }

    fn text(args: Vec<OsString>) -> String {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn opus_uses_libopus_when_present() {
        let args = encode_args(Format::Opus, None, &all_encoders()).unwrap();

        assert_eq!(text(args), "-c:a libopus");
    }

    #[test]
    fn opus_falls_back_to_the_websites_native_encoder_workaround() {
        let encoders: HashSet<String> = ["opus".to_string()].into();

        let args = encode_args(Format::Opus, None, &encoders).unwrap();

        assert_eq!(text(args), "-c:a opus -strict -2 -ar 48000 -ac 2");
    }

    #[test]
    fn mpeg_prefers_mp2_like_the_website() {
        let args = encode_args(Format::Mpeg, None, &all_encoders()).unwrap();

        assert_eq!(text(args), "-c:a mp2");
    }

    #[test]
    fn a_bitrate_applies_to_lossy_formats() {
        let args = encode_args(Format::Mp3, Some(192), &all_encoders()).unwrap();

        assert_eq!(text(args), "-c:a libmp3lame -b:a 192k");
    }

    #[test]
    fn a_bitrate_is_ignored_for_lossless_formats() {
        let args = encode_args(Format::Flac, Some(320), &all_encoders()).unwrap();

        assert_eq!(text(args), "-c:a flac");
    }

    #[test]
    fn a_format_without_an_encoder_is_an_error() {
        let encoders: HashSet<String> = ["aac".to_string()].into();

        let error = encode_args(Format::Wma, None, &encoders).unwrap_err();

        assert_eq!(error.to_string(), "This FFmpeg build has no WMA encoder");
    }

    #[test]
    fn only_formats_with_an_encoder_are_offered() {
        let encoders: HashSet<String> = ["aac", "flac"].map(String::from).into();

        let offered = available(&[Format::Wma, Format::Flac, Format::Aac], &encoders);

        assert_eq!(offered, [Format::Flac, Format::Aac]);
    }

    #[test]
    fn bitrates_are_labelled() {
        assert_eq!(bitrate_label(None), "Automatic");
        assert_eq!(bitrate_label(Some(320)), "320 kbps");
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    use crate::files::reserve_output;
    use std::fs;
    use std::sync::Arc;

    pub(crate) fn real_ffmpeg(test: &str) -> Option<(Arc<Ffmpeg>, PathBuf)> {
        let Some(ff) = Ffmpeg::shared() else {
            eprintln!("note: FFmpeg not found; skipping a test that runs it");
            return None;
        };
        let dir =
            std::env::temp_dir().join(format!("open-tools-audio-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Some((ff, dir))
    }

    /// Makes `dir/name` with the `ffmpeg` on PATH from lavfi `source` and `options`.
    pub(crate) fn fixture(dir: &Path, name: &str, source: &str, options: &str) -> PathBuf {
        let path = dir.join(name);
        let status = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", source])
            .args(options.split_whitespace())
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "{name}");
        path
    }

    /// Converts `input` to `format` at `bitrate` through the allowlisted runner, returning the
    /// output's audio codec, or the error.
    fn convert(ff: &Ffmpeg, input: &Path, format: Format, bitrate: Option<u32>) -> String {
        let output = reserve_output(input, None, format.extension()).unwrap();
        let args = encode_args(format, bitrate, ff.encoders()).unwrap();
        let result = transcode(
            ff,
            input,
            &output,
            Some(1.0),
            args,
            &CancelToken::default(),
            |_| {},
        );
        match result {
            Ok(()) => {
                let path = output.keep().unwrap();
                probe(ff, &path)
                    .map(|i| i.audio_codec.unwrap_or_default())
                    .unwrap_or_else(|e| e.to_string())
            }
            Err(e) => format!("{format:?}: {e:?}"),
        }
    }

    #[test]
    fn every_format_converts_at_its_highest_bitrate() {
        let Some((ff, dir)) = real_ffmpeg("formats") else {
            return;
        };
        let input = fixture(&dir, "tone.wav", "sine=duration=1", "-ac 2");
        let offered = available(&Format::ALL, ff.encoders());

        let codecs: Vec<(Format, String)> = offered
            .iter()
            .map(|f| (*f, convert(&ff, &input, *f, Some(320))))
            .collect();

        let expected = [
            (Format::Mp3, "mp3"),
            (Format::Opus, "opus"),
            (Format::M4a, "aac"),
            (Format::Aac, "aac"),
            (Format::Mpeg, "mp2"),
            (Format::Wma, "wmav2"),
            (Format::Flac, "flac"),
            (Format::Wav, "pcm_s16le"),
            (Format::Aiff, "pcm_s16be"),
            (Format::Ogg, "vorbis"),
        ]
        .into_iter()
        .filter(|(f, _)| offered.contains(f))
        .map(|(f, c)| (f, c.to_string()))
        .collect::<Vec<_>>();
        assert_eq!(codecs, expected);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_video_without_sound_is_refused_by_the_probe() {
        let Some((ff, dir)) = real_ffmpeg("silent-video") else {
            return;
        };
        let input = fixture(
            &dir,
            "silent.mp4",
            "testsrc=duration=1:size=64x64",
            "-c:v libx264 -pix_fmt yuv420p",
        );

        let error = probe(&ff, &input).unwrap_err();

        assert_eq!(error.to_string(), "No audio track found");
        fs::remove_dir_all(&dir).unwrap();
    }
}
