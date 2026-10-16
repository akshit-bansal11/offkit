//! Audio family (audio-converter, audio-extractor, audio-editor, audio-joiner): FFmpeg
//! arguments and audio-specific parsing, run through `engine::ffmpeg`. Owner: audio family.
//!
//! Shared by the four tools: the output formats and their encoder arguments (the website's
//! codec map, limited to the encoders this FFmpeg has), the probe that accepts a file, the
//! `mm:ss.s` time format, and the waveform peaks the editor and joiner draw, decoded by FFmpeg
//! into a temporary WAV that is deleted once read.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, bail};

use crate::engine::ffmpeg::{Ffmpeg, MediaInfo, Run, require_audio};
use crate::files::{Reserved, reserve_output};
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

    /// The Audio Editor's and Audio Joiner's list, as on the website.
    pub const EDIT: [Format; 5] = [Self::Mp3, Self::Wav, Self::Flac, Self::Aac, Self::Opus];

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

/// The format of `list` a save path names by its extension: "take.flac" is FLAC.
pub fn format_for_path(path: &Path, list: &[Format]) -> Option<Format> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    list.iter().copied().find(|f| f.extension() == ext)
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

/// "01:15.3": minutes and seconds to a tenth, as the website shows times.
pub fn format_time(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    format!(
        "{:02}:{:02}.{}",
        tenths / 600,
        tenths / 10 % 60,
        tenths % 10
    )
}

// ---- Waveforms ----------------------------------------------------------------------------

/// The rate waveforms are decoded at: plenty for a few hundred bars, and an hour of audio is a
/// 29 MB temporary file.
const PEAK_RATE: u32 = 4000;

/// Decodes `input`'s first audio track to a temporary 16-bit WAV with `options` (e.g. a
/// filter, a sample rate), for reading back. Dropping the result deletes it.
pub fn decode_wav(
    ff: &Ffmpeg,
    input: &Path,
    mut options: Vec<OsString>,
    cancel: &CancelToken,
) -> Result<Reserved, JobError> {
    let output = reserve_output(input, Some(&std::env::temp_dir()), "wav")
        .map_err(|e| JobError::Failed(format!("Couldn't create a temporary file: {e}")))?;
    options.extend(words("-c:a pcm_s16le"));
    transcode(ff, input, &output, None, options, cancel, |_| {})?;
    Ok(output)
}

/// The loudest sample in each of `bars` equal slices of `input`'s first audio track, 0..1.
pub fn peaks(
    ff: &Ffmpeg,
    input: &Path,
    bars: usize,
    cancel: &CancelToken,
) -> Result<Vec<f32>, JobError> {
    let options = words(&format!("-ac 1 -ar {PEAK_RATE}"));
    let wav = decode_wav(ff, input, options, cancel)?;
    let read = || -> anyhow::Result<Vec<f32>> {
        let mut file = BufReader::new(File::open(wav.path())?);
        let header = read_wav_header(&mut file)?;
        // ponytail: reads the whole decode into memory (29 MB an hour); stream it if
        // multi-hour files become common.
        let mut data = Vec::new();
        file.take(header.data_len).read_to_end(&mut data)?;
        let samples = data
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]));
        Ok(bucket_peaks(samples, data.len() / 2, bars))
    };
    read().map_err(|e| JobError::Failed(format!("Couldn't read the waveform: {e}")))
}

/// The largest |sample| in each of `bars` equal slices of `total` samples, scaled to 0..1.
fn bucket_peaks(samples: impl Iterator<Item = i16>, total: usize, bars: usize) -> Vec<f32> {
    let mut peaks = vec![0.0f32; bars];
    if total == 0 {
        return peaks;
    }
    for (i, sample) in samples.take(total).enumerate() {
        let bar = i * bars / total;
        peaks[bar] = peaks[bar].max(f32::from(sample).abs() / 32768.0);
    }
    peaks
}

#[derive(Debug)]
pub struct WavHeader {
    pub sample_rate: u32,
    /// Bytes of sample data after the header.
    pub data_len: u64,
}

/// Reads a 16-bit PCM WAV's header, as FFmpeg writes one, leaving `r` at the first sample.
pub fn read_wav_header(r: &mut impl Read) -> anyhow::Result<WavHeader> {
    let mut riff = [0u8; 12];
    r.read_exact(&mut riff).context("not a WAV file")?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        bail!("not a WAV file");
    }
    let mut format = None;
    loop {
        let mut chunk = [0u8; 8];
        r.read_exact(&mut chunk)
            .context("the WAV file has no sample data")?;
        let len = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        match &chunk[0..4] {
            b"fmt " => {
                let mut fmt = vec![0u8; len as usize];
                r.read_exact(&mut fmt)
                    .context("the WAV format is cut short")?;
                if fmt.len() < 16 || u16::from_le_bytes([fmt[14], fmt[15]]) != 16 {
                    bail!("the WAV file isn't 16-bit");
                }
                format = Some(u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]));
            }
            b"data" => {
                let sample_rate = format.context("the WAV format comes too late")?;
                return Ok(WavHeader {
                    sample_rate,
                    // An unfinished header says 0 (or u32::MAX, which reads to the end anyway).
                    data_len: if len == 0 { u64::MAX } else { u64::from(len) },
                });
            }
            // LIST (metadata) and the like; chunks are padded to an even length.
            _ => {
                let skip = u64::from(len) + u64::from(len % 2);
                std::io::copy(&mut r.take(skip), &mut std::io::sink())?;
            }
        }
    }
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

    /// A WAV header: `fmt ` (PCM, 16-bit), an optional LIST chunk, then `data` of `len` bytes.
    fn wav(channels: u16, rate: u32, list: &[u8], len: u32) -> Vec<u8> {
        let mut w = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        w.extend(16u32.to_le_bytes());
        w.extend(1u16.to_le_bytes());
        w.extend(channels.to_le_bytes());
        w.extend(rate.to_le_bytes());
        w.extend((rate * 2 * u32::from(channels)).to_le_bytes());
        w.extend((2 * channels).to_le_bytes());
        w.extend(16u16.to_le_bytes());
        if !list.is_empty() {
            w.extend(b"LIST");
            w.extend((list.len() as u32).to_le_bytes());
            w.extend(list);
            w.extend(&b"\0"[..list.len() % 2]);
        }
        w.extend(b"data");
        w.extend(len.to_le_bytes());
        w
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
    fn a_save_path_names_its_format_by_extension() {
        let format = format_for_path(Path::new("C:/Music/take.FLAC"), &Format::EDIT);

        assert_eq!(format, Some(Format::Flac));
    }

    #[test]
    fn a_save_path_with_another_extension_names_no_format() {
        assert_eq!(format_for_path(Path::new("take.txt"), &Format::EDIT), None);
    }

    #[test]
    fn times_show_minutes_seconds_and_tenths() {
        assert_eq!(format_time(75.25), "01:15.3");
    }

    #[test]
    fn times_round_up_into_the_next_minute() {
        assert_eq!(format_time(59.96), "01:00.0");
    }

    #[test]
    fn bitrates_are_labelled() {
        assert_eq!(bitrate_label(None), "Automatic");
        assert_eq!(bitrate_label(Some(320)), "320 kbps");
    }

    #[test]
    fn peaks_are_the_loudest_sample_of_each_slice() {
        let samples = [100i16, -16384, 0, 8192, i16::MIN, 3];

        let peaks = bucket_peaks(samples.into_iter(), samples.len(), 3);

        assert_eq!(peaks, [0.5, 0.25, 1.0]);
    }

    #[test]
    fn silence_has_no_peaks() {
        let peaks = bucket_peaks(std::iter::empty(), 0, 4);

        assert_eq!(peaks, [0.0; 4]);
    }

    #[test]
    fn wav_header_skips_metadata_chunks() {
        let bytes = wav(2, 44100, b"INFOabc", 400);

        let header = read_wav_header(&mut bytes.as_slice()).unwrap();

        assert_eq!((header.sample_rate, header.data_len), (44100, 400));
    }

    #[test]
    fn wav_header_without_a_length_reads_to_the_end() {
        let bytes = wav(1, 4000, b"", 0);

        let header = read_wav_header(&mut bytes.as_slice()).unwrap();

        assert_eq!(header.data_len, u64::MAX);
    }

    #[test]
    fn a_non_wav_file_is_refused() {
        let error = read_wav_header(&mut &b"ID3\x04\0\0\0\0\0\0\0\0"[..]).unwrap_err();

        assert_eq!(error.to_string(), "not a WAV file");
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
    fn a_steady_tone_has_even_peaks_at_its_amplitude() {
        let Some((ff, dir)) = real_ffmpeg("peaks") else {
            return;
        };
        // lavfi's sine has an amplitude of 1/8.
        let input = fixture(&dir, "tone.flac", "sine=frequency=440:duration=2", "");

        let peaks = peaks(&ff, &input, 10, &CancelToken::default()).unwrap();

        assert!(peaks.iter().all(|p| (0.11..0.14).contains(p)), "{peaks:?}");
        assert_eq!(peaks.len(), 10);
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
