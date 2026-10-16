//! Native FFmpeg engine: find the binaries, probe media, detect working hardware encoders,
//! build conversion arguments and run a conversion with progress and cancellation.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

use crate::jobs::CancelToken;

pub struct Ffmpeg {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

impl Ffmpeg {
    /// Looks next to the executable, then in `app/vendor/<os>/` (development builds), then on
    /// PATH. Both `ffmpeg` and `ffprobe` must sit in the same directory.
    pub fn locate() -> Option<Self> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        // The source tree's vendor folder only exists on a developer machine.
        let vendor = cfg!(debug_assertions).then(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("vendor")
                .join(std::env::consts::OS)
        });
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .unwrap_or_default();
        exe_dir
            .into_iter()
            .chain(vendor)
            .chain(path_dirs)
            .find_map(|dir| {
                let bin = |name: &str| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
                let (ffmpeg, ffprobe) = (bin("ffmpeg"), bin("ffprobe"));
                (ffmpeg.is_file() && ffprobe.is_file()).then_some(Self { ffmpeg, ffprobe })
            })
    }

    /// Reads a file's streams and container. Also the gate for untrusted input: anything that
    /// isn't a single local media file (see `INDIRECT_DEMUXERS`) is rejected here, and
    /// `convert` probes again right before running, so a file swapped after it was added
    /// can't slip through.
    pub fn probe(&self, path: &Path) -> anyhow::Result<MediaInfo> {
        let out = command(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_entries",
                "format=duration,format_name:stream=codec_type,codec_name,width,height",
            ])
            .args(input_args(path, None)?)
            .output()
            .context("couldn't run ffprobe")?;
        if !out.status.success() {
            bail!("Not a video FFmpeg can read");
        }
        MediaInfo::from_ffprobe_json(&out.stdout)
    }

    /// Lists compiled-in encoders, then proves each hardware encoder works with a tiny trial
    /// encode: an encoder being listed says nothing about the GPU or driver present. Takes
    /// about a second with a GPU, so call it off the UI thread.
    pub fn capabilities(&self) -> Capabilities {
        let encoders = command(&self.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|out| parse_encoders(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or_default();
        // The first family with any working encoder wins; machines rarely have two.
        let (hw, hw_codecs) = HwFamily::ALL
            .into_iter()
            .map(|family| {
                let working: Vec<VideoCodec> = HW_CODECS
                    .into_iter()
                    .filter(|codec| {
                        family.encoder(*codec).is_some_and(|e| {
                            encoders.contains(&e) && self.trial_encode(&e, family.pix_fmt())
                        })
                    })
                    .collect();
                (family, working)
            })
            .find(|(_, working)| !working.is_empty())
            .map_or((None, Vec::new()), |(family, working)| {
                (Some(family), working)
            });
        Capabilities {
            encoders,
            hw,
            hw_codecs,
        }
    }

    fn trial_encode(&self, encoder: &str, pix_fmt: &str) -> bool {
        command(&self.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
            .arg("color=c=black:s=256x256:r=25:d=0.2")
            .args([
                "-frames:v",
                "5",
                "-c:v",
                encoder,
                "-pix_fmt",
                pix_fmt,
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Runs one conversion into its already-reserved output (see `reserve_output`), reporting
    /// progress as a 0..1 fraction. On failure or cancellation the partial output is removed.
    pub fn convert(
        &self,
        job: &ConvertJob,
        caps: &Capabilities,
        cancel: &CancelToken,
        mut on_progress: impl FnMut(f32),
    ) -> Result<(), ConvertError> {
        let result = self.run(job, caps, cancel, &mut on_progress);
        if result.is_err() {
            // We created this file in `reserve_output`, so removing it can't destroy user data.
            let _ = fs::remove_file(&job.output);
        }
        result
    }

    fn run(
        &self,
        job: &ConvertJob,
        caps: &Capabilities,
        cancel: &CancelToken,
        on_progress: &mut impl FnMut(f32),
    ) -> Result<(), ConvertError> {
        if cancel.is_cancelled() {
            return Err(ConvertError::Cancelled);
        }
        // Re-probe and pin the demuxer that probe accepted: the file may have changed since.
        let info = self.probe(&job.input).map_err(ConvertError::failed)?;
        let args = build_args(job, caps, &info.format).map_err(ConvertError::failed)?;
        let mut child = command(&self.ffmpeg)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ConvertError::Failed(format!("Couldn't start FFmpeg: {e}")))?;
        let (Some(stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return Err(ConvertError::Failed("FFmpeg output unavailable".into()));
        };
        let stderr_reader = thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        });
        cancel.attach(child);

        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match (parse_progress_line(&line), job.duration) {
                (Some(Progress::Time(t)), Some(total)) if total > 0.0 => {
                    on_progress((t / total).clamp(0.0, 1.0) as f32);
                }
                (Some(Progress::End), _) => on_progress(1.0),
                _ => {}
            }
        }

        let status = cancel.detach().map(|mut child| child.wait());
        let stderr_text = stderr_reader.join().unwrap_or_default();
        if cancel.is_cancelled() {
            return Err(ConvertError::Cancelled);
        }
        match status {
            Some(Ok(status)) if status.success() => Ok(()),
            Some(Ok(status)) => {
                let message = summarize_error(&stderr_text)
                    .unwrap_or_else(|| format!("FFmpeg exited with {status}"));
                let uses_hw = job.settings.hw.is_some() && caps.hw_encodes(job.settings.video);
                Err(ConvertError::Failed(if uses_hw {
                    format!("Hardware encoder failed: {message}")
                } else {
                    message
                }))
            }
            Some(Err(e)) => Err(ConvertError::Failed(format!("FFmpeg didn't finish: {e}"))),
            None => Err(ConvertError::Failed("FFmpeg process lost".into())),
        }
    }
}

/// A command for an FFmpeg binary that never opens a console window on Windows.
fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    // SVT-AV1 prints an info banner to stderr whatever `-v` says; 1 = errors only.
    cmd.stdin(Stdio::null()).env("SVT_LOG", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub struct ConvertJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub settings: Settings,
    /// Input duration in seconds, for progress; `None` leaves progress at 0.
    pub duration: Option<f64>,
    /// The input's video codec as ffprobe names it (e.g. "hevc"), for stream copies.
    pub source_codec: Option<String>,
}

#[derive(Debug)]
pub enum ConvertError {
    Cancelled,
    Failed(String),
}

impl ConvertError {
    fn failed(e: anyhow::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

// ---- Probing ------------------------------------------------------------------------------

/// Demuxers that read other files or URLs named inside the input (playlists, file lists).
/// Fed a crafted file they fetch remote URLs or pull unrelated local files into the output,
/// so they are refused outright. Names as ffprobe's `format_name` reports them.
const INDIRECT_DEMUXERS: &[&str] = &["hls", "dash", "concat", "tee", "webm_dash_manifest"];

/// The input half of every ffmpeg/ffprobe command line; nothing else may add `-i`.
/// `-protocol_whitelist file` stops the input (and anything it references) from reaching the
/// network, and the path is made absolute so a name like `http:x` can't parse as a protocol.
/// `format` pins the demuxer, so the file is read exactly as it was probed.
fn input_args(path: &Path, format: Option<&str>) -> io::Result<Vec<OsString>> {
    let mut args: Vec<OsString> = vec!["-protocol_whitelist".into(), "file".into()];
    if let Some(format) = format {
        args.extend(["-f".into(), format.into()]);
    }
    args.extend(["-i".into(), std::path::absolute(path)?.into_os_string()]);
    Ok(args)
}

#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// The demuxer ffprobe chose (first name of its `format_name`, e.g. "mov").
    pub format: String,
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

impl MediaInfo {
    fn from_ffprobe_json(json: &[u8]) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Out {
            #[serde(default)]
            streams: Vec<Stream>,
            format: Option<Format>,
        }
        #[derive(Deserialize)]
        struct Stream {
            codec_type: Option<String>,
            codec_name: Option<String>,
            width: Option<u32>,
            height: Option<u32>,
        }
        #[derive(Deserialize)]
        struct Format {
            duration: Option<String>,
            format_name: Option<String>,
        }
        let out: Out = serde_json::from_slice(json).context("unreadable ffprobe output")?;
        let format_name = out
            .format
            .as_ref()
            .and_then(|f| f.format_name.clone())
            .unwrap_or_default();
        if format_name
            .split(',')
            .any(|name| INDIRECT_DEMUXERS.contains(&name))
        {
            bail!("Playlists and file lists aren't supported");
        }
        let format = match format_name.split(',').next() {
            Some(name) if !name.is_empty() => name.to_owned(),
            _ => bail!("Not a video FFmpeg can read"),
        };
        let of = |kind: &str| {
            out.streams
                .iter()
                .find(|s| s.codec_type.as_deref() == Some(kind))
        };
        let video = of("video").ok_or_else(|| anyhow!("No video stream found"))?;
        Ok(Self {
            format,
            duration: out
                .format
                .and_then(|f| f.duration)
                .and_then(|d| d.parse().ok()),
            width: video.width,
            height: video.height,
            video_codec: video.codec_name.clone(),
            audio_codec: of("audio").and_then(|s| s.codec_name.clone()),
        })
    }

    /// "0:42 · 1920×1080 · H.264 / AAC"
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(d) = self.duration {
            parts.push(format_duration(d));
        }
        if let (Some(w), Some(h)) = (self.width, self.height) {
            parts.push(format!("{w}×{h}"));
        }
        let codecs: Vec<_> = [&self.video_codec, &self.audio_codec]
            .into_iter()
            .flatten()
            .map(|c| codec_display_name(c))
            .collect();
        if !codecs.is_empty() {
            parts.push(codecs.join(" / "));
        }
        parts.join(" · ")
    }
}

fn format_duration(seconds: f64) -> String {
    let s = seconds.round() as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn codec_display_name(name: &str) -> String {
    match name {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "mpeg4" => "MPEG-4".into(),
        "prores" => "ProRes".into(),
        "opus" => "Opus".into(),
        "vorbis" => "Vorbis".into(),
        "flac" => "FLAC".into(),
        n if n.starts_with("pcm_") => "PCM".into(),
        n => n.to_uppercase(),
    }
}

// ---- Capabilities -------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Capabilities {
    encoders: HashSet<String>,
    /// The hardware family with at least one encoder that passed a trial encode.
    pub hw: Option<HwFamily>,
    /// The codecs whose `hw` encoder passed its trial encode.
    hw_codecs: Vec<VideoCodec>,
}

/// Codecs a hardware family can encode; each gets its own trial encode.
pub const HW_CODECS: [VideoCodec; 3] = [VideoCodec::H264, VideoCodec::Hevc, VideoCodec::Av1];

impl Capabilities {
    #[cfg(test)]
    /// Test capabilities: every listed hardware encoder counts as having passed its trial.
    pub fn with(encoders: &[&str], hw: Option<HwFamily>) -> Self {
        let encoders: HashSet<String> = encoders.iter().map(|e| e.to_string()).collect();
        let hw_codecs = HW_CODECS
            .into_iter()
            .filter(|c| {
                hw.and_then(|f| f.encoder(*c))
                    .is_some_and(|e| encoders.contains(&e))
            })
            .collect();
        Self {
            encoders,
            hw,
            hw_codecs,
        }
    }

    pub fn supports_video(&self, codec: VideoCodec) -> bool {
        codec == VideoCodec::Copy || self.software_encoder(codec).is_some()
    }

    pub fn supports_audio(&self, codec: AudioCodec) -> bool {
        codec.encoder().is_none_or(|e| self.encoders.contains(e))
    }

    /// Whether `codec`'s hardware encoder passed its trial encode on this machine.
    pub fn hw_encodes(&self, codec: VideoCodec) -> bool {
        self.hw_codecs.contains(&codec)
    }

    fn software_encoder(&self, codec: VideoCodec) -> Option<&'static str> {
        let candidates: &[&'static str] = match codec {
            VideoCodec::H264 => &["libx264"],
            VideoCodec::Hevc => &["libx265"],
            VideoCodec::Vp9 => &["libvpx-vp9"],
            VideoCodec::Av1 => &["libsvtav1", "libaom-av1"],
            VideoCodec::ProRes => &["prores_ks"],
            VideoCodec::Mpeg4 => &["mpeg4"],
            VideoCodec::Gif => &["gif"],
            VideoCodec::Copy => &[],
        };
        candidates
            .iter()
            .copied()
            .find(|e| self.encoders.contains(*e))
    }
}

fn parse_encoders(listing: &str) -> HashSet<String> {
    // Rows look like " V....D libx264   libx264 H.264 ..."; the legend above uses "=".
    listing
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let flags = cols.next()?;
            let name = cols.next()?;
            (flags.len() == 6 && !name.starts_with('=')).then(|| name.to_string())
        })
        .collect()
}

// ---- Settings -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mkv,
    WebM,
    Mov,
    Avi,
    Gif,
}

impl Container {
    pub const ALL: [Container; 6] = [
        Self::Mp4,
        Self::Mkv,
        Self::WebM,
        Self::Mov,
        Self::Avi,
        Self::Gif,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Mkv => "MKV",
            Self::WebM => "WebM",
            Self::Mov => "MOV",
            Self::Avi => "AVI",
            Self::Gif => "GIF",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::WebM => "webm",
            Self::Mov => "mov",
            Self::Avi => "avi",
            Self::Gif => "gif",
        }
    }

    /// Video codecs this container can hold, most useful first.
    pub fn video_codecs(self) -> &'static [VideoCodec] {
        use VideoCodec::*;
        match self {
            Self::Mp4 => &[H264, Hevc, Av1, Mpeg4, Copy],
            Self::Mkv => &[H264, Hevc, Vp9, Av1, ProRes, Mpeg4, Copy],
            Self::WebM => &[Vp9, Av1],
            Self::Mov => &[H264, Hevc, ProRes, Mpeg4, Copy],
            Self::Avi => &[Mpeg4, H264, Copy],
            Self::Gif => &[Gif],
        }
    }

    pub fn audio_codecs(self) -> &'static [AudioCodec] {
        use AudioCodec::*;
        match self {
            Self::Mp4 => &[Aac, Mp3, Opus, Copy, None],
            Self::Mkv => &[Aac, Opus, Mp3, Flac, Vorbis, Pcm, Copy, None],
            Self::WebM => &[Opus, Vorbis, None],
            Self::Mov => &[Aac, Pcm, Copy, None],
            Self::Avi => &[Mp3, Pcm, Copy, None],
            Self::Gif => &[None],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Hevc,
    Vp9,
    Av1,
    ProRes,
    Mpeg4,
    Gif,
    Copy,
}

impl VideoCodec {
    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Hevc => "HEVC (H.265)",
            Self::Vp9 => "VP9",
            Self::Av1 => "AV1",
            Self::ProRes => "ProRes 422",
            Self::Mpeg4 => "MPEG-4",
            Self::Gif => "GIF",
            Self::Copy => "Copy stream",
        }
    }

    /// Whether the quality slider means anything for this codec.
    pub fn has_quality(self) -> bool {
        !matches!(self, Self::ProRes | Self::Gif | Self::Copy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Aac,
    Opus,
    Mp3,
    Flac,
    Vorbis,
    Pcm,
    Copy,
    None,
}

impl AudioCodec {
    pub fn label(self) -> &'static str {
        match self {
            Self::Aac => "AAC",
            Self::Opus => "Opus",
            Self::Mp3 => "MP3",
            Self::Flac => "FLAC",
            Self::Vorbis => "Vorbis",
            Self::Pcm => "PCM",
            Self::Copy => "Copy stream",
            Self::None => "None",
        }
    }

    fn encoder(self) -> Option<&'static str> {
        match self {
            Self::Aac => Some("aac"),
            Self::Opus => Some("libopus"),
            Self::Mp3 => Some("libmp3lame"),
            Self::Flac => Some("flac"),
            Self::Vorbis => Some("libvorbis"),
            Self::Pcm => Some("pcm_s16le"),
            Self::Copy | Self::None => None,
        }
    }

    fn args(self) -> &'static str {
        match self {
            Self::Aac => "-c:a aac -b:a 192k",
            Self::Opus => "-c:a libopus -b:a 160k",
            Self::Mp3 => "-c:a libmp3lame -q:a 2",
            Self::Flac => "-c:a flac",
            Self::Vorbis => "-c:a libvorbis -q:a 5",
            Self::Pcm => "-c:a pcm_s16le",
            Self::Copy => "-c:a copy",
            Self::None => "-an",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwFamily {
    Nvenc,
    Qsv,
    Amf,
    VideoToolbox,
}

impl HwFamily {
    pub const ALL: [HwFamily; 4] = [Self::Nvenc, Self::Qsv, Self::Amf, Self::VideoToolbox];

    pub fn label(self) -> &'static str {
        match self {
            Self::Nvenc => "NVIDIA NVENC",
            Self::Qsv => "Intel Quick Sync",
            Self::Amf => "AMD AMF",
            Self::VideoToolbox => "Apple VideoToolbox",
        }
    }

    /// The encoder name for `codec`, if this family has one at all.
    pub fn encoder(self, codec: VideoCodec) -> Option<String> {
        let base = match codec {
            VideoCodec::H264 => "h264",
            VideoCodec::Hevc => "hevc",
            VideoCodec::Av1 if self != Self::VideoToolbox => "av1",
            _ => return None,
        };
        let suffix = match self {
            Self::Nvenc => "nvenc",
            Self::Qsv => "qsv",
            Self::Amf => "amf",
            Self::VideoToolbox => "videotoolbox",
        };
        Some(format!("{base}_{suffix}"))
    }

    fn pix_fmt(self) -> &'static str {
        if self == Self::Qsv { "nv12" } else { "yuv420p" }
    }

    fn quality_args(self, q: u8) -> String {
        match self {
            Self::Nvenc => format!("-preset p5 -rc vbr -cq {} -b:v 0", lerp(q, 38, 18)),
            Self::Qsv => format!("-global_quality {}", lerp(q, 36, 18)),
            Self::Amf => {
                let qp = lerp(q, 36, 18);
                format!("-rc cqp -qp_i {qp} -qp_p {qp}")
            }
            Self::VideoToolbox => format!("-q:v {}", lerp(q, 40, 85)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub container: Container,
    pub video: VideoCodec,
    pub audio: AudioCodec,
    /// 0 = smallest file, 100 = best quality; mapped per encoder.
    pub quality: u8,
    /// Use this verified hardware family where it has an encoder for `video`.
    pub hw: Option<HwFamily>,
}

/// 4:2:0 encoders reject odd widths and heights (e.g. 641×481), so every re-encode rounds
/// the frame down to even dimensions. A no-op for even sizes.
const EVEN_SIZE: &str = "-vf scale=trunc(iw/2)*2:trunc(ih/2)*2";

/// Linear map of quality 0..=100 onto an encoder's own scale.
fn lerp(q: u8, at_0: i32, at_100: i32) -> i32 {
    let q = i32::from(q.min(100));
    at_0 + ((at_100 - at_0) * q + 50 * (at_100 - at_0).signum()) / 100
}

/// `input_format` is the demuxer `probe` reported for `job.input`.
pub fn build_args(
    job: &ConvertJob,
    caps: &Capabilities,
    input_format: &str,
) -> anyhow::Result<Vec<OsString>> {
    let s = &job.settings;
    // Output options as whitespace-separated words; no word below contains a space.
    let mut o: Vec<String> = Vec::new();
    let mut push = |words: &str| o.extend(words.split_whitespace().map(str::to_owned));
    push("-map 0:v:0");
    if s.audio != AudioCodec::None {
        push("-map 0:a:0?");
    }

    let q = s.quality;
    // Only codecs whose hardware encoder passed its trial run on this GPU.
    let hw_encoder =
        s.hw.filter(|_| caps.hw_encodes(s.video))
            .and_then(|f| f.encoder(s.video).map(|e| (f, e)));
    match (s.video, hw_encoder) {
        (VideoCodec::Copy, _) => push("-c:v copy"),
        (VideoCodec::Gif, _) => push(
            "-vf fps=12,scale='min(640,iw)':-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse -loop 0",
        ),
        (_, Some((family, encoder))) => {
            push(&format!(
                "-c:v {encoder} {} -pix_fmt {} {EVEN_SIZE}",
                family.quality_args(q),
                family.pix_fmt()
            ));
        }
        (codec, None) => {
            let encoder = caps
                .software_encoder(codec)
                .ok_or_else(|| anyhow!("This FFmpeg build has no {} encoder", codec.label()))?;
            push(&format!("-c:v {encoder}"));
            push(&match encoder {
                "libx264" => format!("-preset medium -crf {} -pix_fmt yuv420p", lerp(q, 34, 16)),
                "libx265" => format!(
                    "-preset medium -crf {} -pix_fmt yuv420p -x265-params log-level=error",
                    lerp(q, 38, 18)
                ),
                "libvpx-vp9" => format!(
                    "-crf {} -b:v 0 -row-mt 1 -deadline good -cpu-used 4 -pix_fmt yuv420p",
                    lerp(q, 48, 18)
                ),
                "libsvtav1" => format!("-preset 8 -crf {} -pix_fmt yuv420p", lerp(q, 50, 20)),
                "libaom-av1" => format!(
                    "-crf {} -b:v 0 -cpu-used 6 -row-mt 1 -pix_fmt yuv420p",
                    lerp(q, 50, 20)
                ),
                "prores_ks" => "-profile:v 2".into(),
                "mpeg4" => format!("-q:v {}", lerp(q, 20, 2)),
                _ => String::new(),
            });
            push(EVEN_SIZE);
        }
    }
    let hevc_out = match s.video {
        VideoCodec::Hevc => true,
        VideoCodec::Copy => job.source_codec.as_deref() == Some("hevc"),
        _ => false,
    };
    if hevc_out && matches!(s.container, Container::Mp4 | Container::Mov) {
        push("-tag:v hvc1"); // Apple players only recognise HEVC tagged as hvc1.
    }
    push(s.audio.args());
    push("-sn -dn");
    if matches!(s.container, Container::Mp4 | Container::Mov) {
        push("-movflags +faststart");
    }

    // `-y` is safe: `reserve_output` created the output file itself, so nothing else is overwritten.
    let mut args: Vec<OsString> = "-hide_banner -nostdin -v error -nostats -progress pipe:1 -y"
        .split_whitespace()
        .map(OsString::from)
        .collect();
    args.extend(input_args(&job.input, Some(input_format))?);
    args.extend(o.into_iter().map(OsString::from));
    args.push(job.output.as_os_str().into());
    Ok(args)
}

// ---- Output naming ------------------------------------------------------------------------

/// `<stem>.<ext>`, then `<stem> (1).<ext>`, `<stem> (2).<ext>`, …
pub fn candidate_name(stem: &str, ext: &str, n: u32) -> String {
    if n == 0 {
        format!("{stem}.{ext}")
    } else {
        format!("{stem} ({n}).{ext}")
    }
}

/// Picks the first free output name in `dir` (default: the input's folder) and creates the
/// file atomically, so the input and existing files are never overwritten, even when two
/// jobs race for the same name.
pub fn reserve_output(input: &Path, dir: Option<&Path>, ext: &str) -> io::Result<PathBuf> {
    let dir = dir.or_else(|| input.parent()).unwrap_or(Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    for n in 0..10_000 {
        let candidate = dir.join(candidate_name(&stem, ext, n));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free output name"))
}

// ---- Progress and errors ------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum Progress {
    /// Output time reached, in seconds.
    Time(f64),
    End,
}

/// Parses one line of `-progress` output. Only `out_time_us` and `progress=end` matter;
/// `out_time_ms` is also microseconds (a long-standing FFmpeg quirk) and is ignored.
pub fn parse_progress_line(line: &str) -> Option<Progress> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        "out_time_us" => value
            .parse::<i64>()
            .ok()
            .filter(|us| *us >= 0)
            .map(|us| Progress::Time(us as f64 / 1e6)),
        "progress" if value == "end" => Some(Progress::End),
        _ => None,
    }
}

/// FFmpeg's generic follow-up lines after the real error, and encoder banners that some
/// libraries print whatever `-v` says.
const STDERR_NOISE: &[&str] = &[
    "[info]",
    "[warning]",
    "Conversion failed!",
    "Error while opening encoder",
    "Error sending frames to consumers",
    "Task finished with error code",
    "Terminating thread with return code",
    "Could not open encoder before EOF",
    "Nothing was written into output file",
    "Error opening output file",
];

/// The first stderr line that states the actual problem, without "[encoder @ 0x…] " or
/// "x265 [error]: " prefixes, capped for a row.
pub fn summarize_error(stderr: &str) -> Option<String> {
    let mut line = stderr.lines().map(str::trim).find(|l| {
        !l.is_empty() && !l.starts_with("encoded ") && !STDERR_NOISE.iter().any(|n| l.contains(n))
    })?;
    while let (true, Some(end)) = (line.starts_with('['), line.find("] ")) {
        line = &line[end + 2..];
    }
    if let Some((_, rest)) = line.split_once("[error]: ") {
        line = rest;
    }
    let mut text: String = line.chars().take(140).collect();
    if line.chars().count() > 140 {
        text.push('…');
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: Settings, caps: &Capabilities) -> Vec<String> {
        build_args(&job(s), caps, "mov")
            .unwrap()
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn job(settings: Settings) -> ConvertJob {
        ConvertJob {
            input: "in.mov".into(),
            output: "out.x".into(),
            settings,
            duration: None,
            source_codec: None,
        }
    }

    fn has(args: &[String], seq: &[&str]) -> bool {
        args.windows(seq.len())
            .any(|w| w.iter().map(String::as_str).eq(seq.iter().copied()))
    }

    fn full_caps() -> Capabilities {
        Capabilities::with(
            &[
                "libx264",
                "libx265",
                "libvpx-vp9",
                "libsvtav1",
                "libaom-av1",
                "prores_ks",
                "mpeg4",
                "gif",
                "aac",
                "libopus",
                "libmp3lame",
                "flac",
                "libvorbis",
                "pcm_s16le",
                "h264_nvenc",
                "hevc_nvenc",
                "av1_nvenc",
            ],
            Some(HwFamily::Nvenc),
        )
    }

    fn settings(container: Container, video: VideoCodec, audio: AudioCodec) -> Settings {
        Settings {
            container,
            video,
            audio,
            quality: 60,
            hw: None,
        }
    }

    #[test]
    fn mp4_h264_aac() {
        let a = args(
            settings(Container::Mp4, VideoCodec::H264, AudioCodec::Aac),
            &full_caps(),
        );
        assert!(has(
            &a,
            &["-c:v", "libx264", "-preset", "medium", "-crf", "23"]
        ));
        assert!(has(&a, &["-pix_fmt", "yuv420p"]));
        assert!(has(&a, &["-c:a", "aac", "-b:a", "192k"]));
        assert!(has(&a, &["-movflags", "+faststart"]));
        assert!(has(&a, &["-progress", "pipe:1"]));
        let input = std::path::absolute("in.mov").unwrap();
        assert_eq!(a.last().map(String::as_str), Some("out.x"));
        assert!(a.iter().any(|x| *x == input.to_string_lossy()));
        assert_eq!(a.last().map(String::as_str), Some("out.x"));
    }

    #[test]
    fn webm_vp9_opus_without_faststart() {
        let a = args(
            settings(Container::WebM, VideoCodec::Vp9, AudioCodec::Opus),
            &full_caps(),
        );
        assert!(has(&a, &["-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0"]));
        assert!(has(&a, &["-c:a", "libopus"]));
        assert!(!a.iter().any(|x| x == "-movflags"));
    }

    #[test]
    fn copy_stream_has_no_quality_or_pix_fmt() {
        let a = args(
            settings(Container::Mkv, VideoCodec::Copy, AudioCodec::Copy),
            &full_caps(),
        );
        assert!(has(&a, &["-c:v", "copy"]));
        assert!(has(&a, &["-c:a", "copy"]));
        assert!(!a.iter().any(|x| x == "-crf" || x == "-pix_fmt"));
    }

    #[test]
    fn none_audio_drops_audio_and_its_map() {
        let a = args(
            settings(Container::Mp4, VideoCodec::H264, AudioCodec::None),
            &full_caps(),
        );
        assert!(a.iter().any(|x| x == "-an"));
        assert!(!a.iter().any(|x| x == "0:a:0?"));
    }

    #[test]
    fn hevc_in_mp4_is_tagged_hvc1() {
        let a = args(
            settings(Container::Mp4, VideoCodec::Hevc, AudioCodec::Aac),
            &full_caps(),
        );
        assert!(has(&a, &["-c:v", "libx265"]));
        assert!(has(&a, &["-tag:v", "hvc1"]));
    }

    #[test]
    fn hw_encoder_used_only_where_the_family_has_one() {
        let mut s = settings(Container::Mp4, VideoCodec::H264, AudioCodec::Aac);
        s.hw = Some(HwFamily::Nvenc);
        let a = args(s, &full_caps());
        assert!(has(
            &a,
            &[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-rc",
                "vbr",
                "-cq",
                "26"
            ]
        ));
        assert!(!a.iter().any(|x| x == "-crf"));

        let mut vp9 = settings(Container::WebM, VideoCodec::Vp9, AudioCodec::Opus);
        vp9.hw = Some(HwFamily::Nvenc);
        assert!(has(&args(vp9, &full_caps()), &["-c:v", "libvpx-vp9"]));

        let mut qsv = settings(Container::Mkv, VideoCodec::Hevc, AudioCodec::Aac);
        qsv.hw = Some(HwFamily::Qsv);
        let a = args(qsv, &full_caps());
        assert!(has(&a, &["-c:v", "hevc_qsv", "-global_quality"]));
        assert!(has(&a, &["-pix_fmt", "nv12"]));
    }

    #[test]
    fn input_is_hardened_before_dash_i() {
        let a = args(
            settings(Container::Mp4, VideoCodec::H264, AudioCodec::Aac),
            &full_caps(),
        );
        let input = std::path::absolute("in.mov").unwrap();
        let input = input.to_string_lossy();
        assert!(has(
            &a,
            &["-protocol_whitelist", "file", "-f", "mov", "-i", &input]
        ));
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 1);
        // A relative name that looks like a URL becomes an absolute file path.
        let probe: Vec<_> = input_args(Path::new("http:evil"), None).unwrap();
        assert_eq!(
            probe[..3],
            ["-protocol_whitelist", "file", "-i"].map(OsString::from)
        );
        assert!(Path::new(&probe[3]).is_absolute());
    }

    #[test]
    fn playlist_and_list_demuxers_are_refused() {
        for name in ["hls", "dash", "concat", "tee", "webm_dash_manifest"] {
            let json = format!(
                r#"{{"streams":[{{"codec_type":"video"}}],"format":{{"format_name":"{name}"}}}}"#
            );
            let err = MediaInfo::from_ffprobe_json(json.as_bytes()).unwrap_err();
            assert_eq!(err.to_string(), "Playlists and file lists aren't supported");
        }
    }

    #[test]
    fn hw_is_skipped_for_codecs_that_failed_their_trial() {
        let caps = Capabilities::with(
            &["libx264", "libsvtav1", "aac", "h264_nvenc"],
            Some(HwFamily::Nvenc),
        );
        let mut s = settings(Container::Mp4, VideoCodec::Av1, AudioCodec::Aac);
        s.hw = Some(HwFamily::Nvenc);
        assert!(has(&args(s, &caps), &["-c:v", "libsvtav1"]));
        s.video = VideoCodec::H264;
        assert!(has(&args(s, &caps), &["-c:v", "h264_nvenc"]));
    }

    #[test]
    fn reencodes_round_to_even_sizes_but_copies_and_gifs_do_not() {
        let even = ["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2"];
        let caps = full_caps();
        assert!(has(
            &args(
                settings(Container::Mp4, VideoCodec::H264, AudioCodec::Aac),
                &caps
            ),
            &even
        ));
        let mut hw = settings(Container::Mp4, VideoCodec::Hevc, AudioCodec::Aac);
        hw.hw = Some(HwFamily::Nvenc);
        assert!(has(&args(hw, &caps), &even));
        assert!(!has(
            &args(
                settings(Container::Mkv, VideoCodec::Copy, AudioCodec::Copy),
                &caps
            ),
            &even
        ));
        let gif = args(
            settings(Container::Gif, VideoCodec::Gif, AudioCodec::None),
            &caps,
        );
        assert_eq!(gif.iter().filter(|a| *a == "-vf").count(), 1);
    }

    #[test]
    fn copied_hevc_into_mp4_is_tagged_hvc1() {
        let mut j = job(settings(Container::Mp4, VideoCodec::Copy, AudioCodec::Copy));
        let words = |j: &ConvertJob| -> Vec<String> {
            build_args(j, &full_caps(), "mov")
                .unwrap()
                .into_iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        assert!(!has(&words(&j), &["-tag:v", "hvc1"]));
        j.source_codec = Some("hevc".into());
        assert!(has(&words(&j), &["-tag:v", "hvc1"]));
    }

    #[test]
    fn av1_prefers_svt_and_falls_back_to_aom() {
        let a = args(
            settings(Container::Mkv, VideoCodec::Av1, AudioCodec::Opus),
            &full_caps(),
        );
        assert!(has(&a, &["-c:v", "libsvtav1"]));
        let aom_only = Capabilities::with(&["libaom-av1", "libopus"], None);
        let a = args(
            settings(Container::Mkv, VideoCodec::Av1, AudioCodec::Opus),
            &aom_only,
        );
        assert!(has(&a, &["-c:v", "libaom-av1"]));
    }

    #[test]
    fn missing_encoder_is_an_error() {
        let caps = Capabilities::with(&["aac"], None);
        let s = settings(Container::Mp4, VideoCodec::H264, AudioCodec::Aac);
        assert!(build_args(&job(s), &caps, "mov").is_err());
    }

    #[test]
    fn gif_uses_palette_filter_and_no_audio() {
        let a = args(
            settings(Container::Gif, VideoCodec::Gif, AudioCodec::None),
            &full_caps(),
        );
        assert!(a.iter().any(|x| x.contains("palettegen")));
        assert!(a.iter().any(|x| x == "-an"));
    }

    #[test]
    fn every_container_offers_codecs() {
        for c in Container::ALL {
            assert!(
                !c.video_codecs().is_empty() && !c.audio_codecs().is_empty(),
                "{c:?}"
            );
        }
    }

    #[test]
    fn quality_maps_to_each_scale() {
        assert_eq!(lerp(0, 34, 16), 34);
        assert_eq!(lerp(100, 34, 16), 16);
        assert_eq!(lerp(60, 34, 16), 23);
        assert_eq!(lerp(60, 40, 85), 67);
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress_line("out_time_us=1500000"),
            Some(Progress::Time(1.5))
        );
        assert_eq!(parse_progress_line("progress=end"), Some(Progress::End));
        assert_eq!(parse_progress_line("progress=continue"), None);
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(
            parse_progress_line("out_time_us=-9223372036854775807"),
            None
        );
        assert_eq!(parse_progress_line("out_time_ms=1500000"), None);
        assert_eq!(parse_progress_line("frame=12"), None);
        assert_eq!(parse_progress_line("garbage"), None);
        assert_eq!(parse_progress_line(""), None);
    }

    #[test]
    fn output_names_never_collide() {
        assert_eq!(candidate_name("clip", "mp4", 0), "clip.mp4");
        assert_eq!(candidate_name("clip", "mp4", 2), "clip (2).mp4");

        let dir = std::env::temp_dir().join(format!("open-tools-names-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("clip.mp4");
        fs::write(&input, b"source").unwrap();
        fs::write(dir.join("clip (1).mp4"), b"existing").unwrap();

        // Same container in the same folder: the input itself is taken, so is (1).
        let first = reserve_output(&input, None, "mp4").unwrap();
        assert_eq!(first, dir.join("clip (2).mp4"));
        let second = reserve_output(&input, None, "mp4").unwrap();
        assert_eq!(second, dir.join("clip (3).mp4"));
        assert_eq!(fs::read(&input).unwrap(), b"source");
        assert_eq!(fs::read(dir.join("clip (1).mp4")).unwrap(), b"existing");
        assert_eq!(
            reserve_output(&input, None, "webm").unwrap(),
            dir.join("clip.webm")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn error_summary_picks_the_real_reason() {
        // Captured from FFmpeg 2025-05 (BtbN build) on a 641×481 input.
        let x264 = "[libx264 @ 000001d5deb19a00] width not divisible by 2 (641x481)
[vost#0:0/libx264 @ 000001d5deb19680] [enc:libx264 @ 000001d5dcc39bc0] Error while opening encoder - maybe incorrect parameters such as bit_rate, rate, width or height.
[vf#0:0 @ 000001d5decaf6c0] Error sending frames to consumers: Generic error in an external library
[vost#0:0/libx264 @ 000001d5deb19680] [enc:libx264 @ 000001d5dcc39bc0] Could not open encoder before EOF
[out#0/matroska @ 000001d5dcbe8180] Nothing was written into output file, because at least one of its streams received no packets.";
        assert_eq!(
            summarize_error(x264).as_deref(),
            Some("width not divisible by 2 (641x481)")
        );

        let x265 = "x265 [info]: HEVC encoder version 4.1+157-78e5ac35
x265 [info]: build info [Windows][GCC 15.1.0][64 bit] 8bit+10bit+12bit
x265 [info]: using cpu capabilities: MMX2 SSE2Fast LZCNT SSSE3 SSE4.2 AVX FMA3 BMI2 AVX2
x265 [error]: Picture width must be an integer multiple of the specified chroma subsampling
[libx265 @ 0000028250749640] Cannot open libx265 encoder.
[vost#0:0/libx265 @ 00000282507492c0] [enc:libx265 @ 000002824eb4bac0] Error while opening encoder - maybe incorrect parameters such as bit_rate, rate, width or height.";
        assert_eq!(
            summarize_error(x265).as_deref(),
            Some("Picture width must be an integer multiple of the specified chroma subsampling")
        );

        let svt = "Svt[info]: -------------------------------------------
Svt[info]: SVT [version]:\tSVT-AV1 Encoder Lib v3.0.2-49-g5def505f
[libsvtav1 @ 000001daee7c9840] Value 99.000000 for parameter 'preset' out of range [-2 - 13]
[libsvtav1 @ 000001daee7c9840] Error setting option preset to value 99.
Error opening output file o4.mkv.";
        assert_eq!(
            summarize_error(svt).as_deref(),
            Some("Value 99.000000 for parameter 'preset' out of range [-2 - 13]")
        );

        assert_eq!(summarize_error("\nConversion failed!\n"), None);
        assert!(summarize_error(&"x".repeat(300)).unwrap().ends_with('…'));
    }

    #[test]
    fn parses_encoder_listing() {
        let listing = "Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264\n A....D aac                  AAC\n";
        let e = parse_encoders(listing);
        assert!(e.contains("libx264") && e.contains("aac"));
        assert!(!e.contains("="));
    }

    #[test]
    fn media_summary_formats() {
        let json = br#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080},{"codec_type":"audio","codec_name":"aac"}],"format":{"duration":"42.4","format_name":"mov,mp4,m4a,3gp,3g2,mj2"}}"#;
        let info = MediaInfo::from_ffprobe_json(json).unwrap();
        assert_eq!(info.format, "mov");
        assert_eq!(info.summary(), "0:42 · 1920×1080 · H.264 / AAC");
        assert!(MediaInfo::from_ffprobe_json(br#"{"streams":[{"codec_type":"audio"}]}"#).is_err());
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    fn real_ffmpeg(test: &str) -> Option<(Ffmpeg, PathBuf)> {
        let Some(ff) = Ffmpeg::locate() else {
            eprintln!("note: FFmpeg not found; skipping a test that runs it");
            return None;
        };
        let dir = std::env::temp_dir().join(format!("open-tools-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Some((ff, dir))
    }

    fn make_clip(ff: &Ffmpeg, path: &Path, seconds: u32, size: &str) {
        let status = command(&ff.ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc=duration={seconds}:size={size}:rate=25"))
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=duration={seconds}"))
            // 4:4:4 so odd sizes can be stored at all; the converter must produce 4:2:0.
            .args(["-c:v", "libx264", "-pix_fmt", "yuv444p", "-c:a", "aac"])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Converts `input` with `settings` into a fresh `<stem>.<ext>` and returns its probe.
    fn convert(
        ff: &Ffmpeg,
        caps: &Capabilities,
        input: &Path,
        ext: &str,
        settings: Settings,
    ) -> MediaInfo {
        let source = ff.probe(input).unwrap();
        let job = ConvertJob {
            input: input.to_path_buf(),
            output: reserve_output(input, None, ext).unwrap(),
            settings,
            duration: source.duration,
            source_codec: source.video_codec,
        };
        let mut last = 0.0;
        ff.convert(&job, caps, &CancelToken::default(), |p| last = p)
            .unwrap_or_else(|e| panic!("{settings:?}: {e:?}"));
        assert!(last > 0.99, "progress ended at {last}");
        ff.probe(&job.output).unwrap()
    }

    #[test]
    fn converts_mp4_to_webm_vp9_opus() {
        let Some((ff, dir)) = real_ffmpeg("convert") else {
            return;
        };
        let input = dir.join("clip.mp4");
        make_clip(&ff, &input, 2, "320x240");
        let caps = ff.capabilities();
        let s = settings(Container::WebM, VideoCodec::Vp9, AudioCodec::Opus);
        let info = convert(&ff, &caps, &input, "webm", s);
        assert_eq!(info.video_codec.as_deref(), Some("vp9"));
        assert_eq!(info.audio_codec.as_deref(), Some("opus"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn odd_sized_input_encodes_to_h264_and_hevc() {
        let Some((ff, dir)) = real_ffmpeg("odd") else {
            return;
        };
        let input = dir.join("odd.mkv");
        make_clip(&ff, &input, 1, "641x481");
        let caps = ff.capabilities();
        for (codec, name) in [(VideoCodec::H264, "h264"), (VideoCodec::Hevc, "hevc")] {
            let info = convert(
                &ff,
                &caps,
                &input,
                "mp4",
                settings(Container::Mp4, codec, AudioCodec::Aac),
            );
            assert_eq!(info.video_codec.as_deref(), Some(name));
            assert_eq!((info.width, info.height), (Some(640), Some(480)));
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancelling_kills_ffmpeg_and_removes_the_partial_file() {
        let Some((ff, dir)) = real_ffmpeg("cancel") else {
            return;
        };
        let input = dir.join("long.mp4");
        make_clip(&ff, &input, 30, "320x240");
        let caps = ff.capabilities();
        let output = reserve_output(&input, None, "mkv").unwrap();
        let job = ConvertJob {
            input,
            output: output.clone(),
            settings: settings(Container::Mkv, VideoCodec::Av1, AudioCodec::Opus),
            duration: Some(30.0),
            source_codec: None,
        };
        let cancel = std::sync::Arc::new(CancelToken::default());
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(std::time::Duration::from_millis(400));
                cancel.cancel();
            })
        };
        let result = ff.convert(&job, &caps, &cancel, |_| {});
        canceller.join().unwrap();
        assert!(matches!(result, Err(ConvertError::Cancelled)), "{result:?}");
        assert!(!output.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_verified_hardware_encoder_converts() {
        let Some((ff, dir)) = real_ffmpeg("hw") else {
            return;
        };
        let caps = ff.capabilities();
        let Some(family) = caps.hw else {
            eprintln!("note: no working hardware encoder here; skipping");
            return fs::remove_dir_all(&dir).unwrap();
        };
        let input = dir.join("clip.mkv");
        make_clip(&ff, &input, 1, "641x481");
        let names = [
            (VideoCodec::H264, "h264"),
            (VideoCodec::Hevc, "hevc"),
            (VideoCodec::Av1, "av1"),
        ];
        for (codec, name) in names.into_iter().filter(|(c, _)| caps.hw_encodes(*c)) {
            let mut s = settings(Container::Mkv, codec, AudioCodec::Opus);
            s.hw = Some(family);
            assert_eq!(
                convert(&ff, &caps, &input, "mkv", s).video_codec.as_deref(),
                Some(name)
            );
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A crafted "video" that is really an HLS playlist or a concat list, saved as `.mp4`.
    #[test]
    fn playlists_disguised_as_mp4_are_rejected_and_never_converted() {
        let Some((ff, dir)) = real_ffmpeg("playlists") else {
            return;
        };
        let secret = dir.join("secret.mp4");
        make_clip(&ff, &secret, 1, "320x240");
        let hls = dir.join("hls.mp4");
        // Port 9 (discard): nothing listens, and the whitelist must stop the attempt anyway.
        fs::write(
            &hls,
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nhttp://127.0.0.1:9/x.ts\n#EXT-X-ENDLIST\n",
        )
        .unwrap();
        // Relative names pass the concat demuxer's own "safe" check, so this is the real threat.
        let relative = dir.join("relative.mp4");
        fs::write(&relative, "ffconcat version 1.0\nfile 'secret.mp4'\n").unwrap();
        let absolute = dir.join("absolute.mp4");
        let listed = secret.to_string_lossy().replace('\\', "/");
        fs::write(
            &absolute,
            format!("ffconcat version 1.0\nfile '{listed}'\n"),
        )
        .unwrap();

        let caps = ff.capabilities();
        for input in [&hls, &relative, &absolute] {
            assert!(
                ff.probe(input).is_err(),
                "probe accepted {}",
                input.display()
            );
            let job = ConvertJob {
                input: input.clone(),
                output: reserve_output(input, None, "mkv").unwrap(),
                settings: settings(Container::Mkv, VideoCodec::H264, AudioCodec::Aac),
                duration: None,
                source_codec: None,
            };
            let started = std::time::Instant::now();
            let result = ff.convert(&job, &caps, &CancelToken::default(), |_| {});
            assert!(matches!(result, Err(ConvertError::Failed(_))), "{result:?}");
            assert!(!job.output.exists(), "{} left output", input.display());
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
        }
        let message = ff.probe(&relative).unwrap_err().to_string();
        assert_eq!(message, "Playlists and file lists aren't supported");

        // Even if probe were bypassed, ffmpeg itself refuses the network for the playlist.
        let out = command(&ff.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-f", "hls"])
            .args(["-protocol_whitelist", "file", "-i"])
            .arg(&hls)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("not on whitelist"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
