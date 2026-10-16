//! Image sequences and video stream details, for the video family: the Frames Extractor's
//! frames, the Video Editor's preview and the Video Merger's thumbnails and target size.
//!
//! `run` writes one reserved file and refuses multi-file muxers. An image sequence is the one
//! exception, and it fails closed the same way: the input passes the same gate, the options the
//! same allowlists, and the frames go into a `ReservedDir` made for the run, under a file name
//! pattern built here from that folder and the input's stem. No tool ever names the files.

use std::ffi::OsString;
use std::path::Path;

use anyhow::{Context, bail};
use serde::Deserialize;

use super::{Ffmpeg, MediaInfo, args, command, global_args, input_args, probe_failure, run_ffprobe};
use crate::files::ReservedDir;
use crate::jobs::{CancelToken, JobError};

/// One FFmpeg run that writes a numbered image sequence (see `Ffmpeg::run_frames`).
pub struct FramesRun<'a> {
    pub input: &'a Path,
    /// What the tool expects the input to be, for `probe`'s error (e.g. "a video").
    pub media: &'a str,
    /// Made for this run with `files::reserve_dir`, so `-y` overwrites nothing else.
    pub dir: &'a ReservedDir,
    /// "png", "jpg" or "webp": the frames' extension.
    pub extension: &'a str,
    /// Seconds of the input to skip, as a fast (and still frame-accurate) input seek.
    pub seek: Option<f64>,
    /// Seconds of output, for progress; `None` leaves progress at 0 until FFmpeg finishes.
    pub duration: Option<f64>,
}

impl Ffmpeg {
    /// `run` for an image sequence. Probes `run.input` again, asks `prepare` for the output
    /// options (held to the same allowlists as `run`), and writes `<stem>_000001.<ext>`,
    /// `<stem>_000002.<ext>`, … into `run.dir` and nowhere else. The caller keeps the folder on
    /// success and drops it otherwise, which deletes every frame written so far.
    pub fn run_frames(
        &self,
        run: &FramesRun,
        cancel: &CancelToken,
        prepare: impl FnOnce(&MediaInfo) -> anyhow::Result<Vec<OsString>>,
        on_progress: impl FnMut(f32),
    ) -> Result<(), JobError> {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let info = self
            .probe_cancellable(run.input, run.media, Some(cancel))
            .map_err(|e| probe_failure(e, cancel))?;
        let output_args = prepare(&info).map_err(JobError::failed)?;
        let args = frames_command_line(run, &info.format, output_args).map_err(JobError::failed)?;
        self.execute(args, run.duration, "", cancel, on_progress)
    }

    /// The first video stream's size as displayed and its frame rate. `info` is the file's
    /// `probe`, whose demuxer this pins, as `run` does.
    pub fn probe_video_stream(&self, path: &Path, info: &MediaInfo) -> anyhow::Result<VideoStream> {
        let mut cmd = command(&self.ffprobe);
        cmd.args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate:stream_side_data=rotation",
        ])
        .args(input_args(path, Some(&info.format))?);
        let out = run_ffprobe(cmd, None)?;
        if !out.status.success() {
            bail!("Couldn't read the video stream");
        }
        VideoStream::from_ffprobe_json(&out.stdout)
    }
}

/// A video stream as the video family uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoStream {
    /// The size FFmpeg's filters see: a phone video stored as 1920×1080 with a 90° rotation
    /// is 1080×1920, because FFmpeg applies the rotation when it decodes.
    pub width: u32,
    pub height: u32,
    /// The average frame rate as FFmpeg's fraction (30000/1001); `None` when unknown.
    pub frame_rate: Option<(u32, u32)>,
}

impl VideoStream {
    fn from_ffprobe_json(json: &[u8]) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Out {
            #[serde(default)]
            streams: Vec<Stream>,
        }
        #[derive(Deserialize)]
        struct Stream {
            width: Option<u32>,
            height: Option<u32>,
            avg_frame_rate: Option<String>,
            #[serde(default)]
            side_data_list: Vec<SideData>,
        }
        #[derive(Deserialize)]
        struct SideData {
            rotation: Option<f64>,
        }
        let out: Out = serde_json::from_slice(json).context("unreadable ffprobe output")?;
        let stream = out
            .streams
            .into_iter()
            .next()
            .context("No video stream found")?;
        let (Some(width), Some(height)) = (stream.width, stream.height) else {
            bail!("The video stream has no size");
        };
        let rotation = stream.side_data_list.iter().find_map(|s| s.rotation);
        let quarter_turn = rotation.is_some_and(|r| (r.round() as i64).rem_euclid(180) == 90);
        let (width, height) = if quarter_turn {
            (height, width)
        } else {
            (width, height)
        };
        let frame_rate = stream.avg_frame_rate.as_deref().and_then(|r| {
            let (n, d) = r.split_once('/')?;
            let (n, d) = (n.parse().ok()?, d.parse().ok()?);
            (n > 0 && d > 0).then_some((n, d))
        });
        Ok(Self {
            width,
            height,
            frame_rate,
        })
    }

    /// Frames per second, when known.
    pub fn fps(&self) -> Option<f64> {
        self.frame_rate.map(|(n, d)| f64::from(n) / f64::from(d))
    }
}

/// The output allowlist `run` and `run_frames` apply, for the tools' tests of their arguments.
#[cfg(test)]
pub fn check_output_args(output_args: &[OsString]) -> anyhow::Result<()> {
    args::check_output_args(output_args)
}

/// The whole command line for `run_frames`: as `command_line`'s, with the optional seek before
/// the input and the image2 muxer writing to `frame_pattern`.
fn frames_command_line(
    run: &FramesRun,
    format: &str,
    output_args: Vec<OsString>,
) -> anyhow::Result<Vec<OsString>> {
    args::check_output_args(&output_args)?;
    let muxer = args::sequence_muxer(run.extension)?;
    let mut line = global_args();
    if let Some(seek) = run.seek {
        // A number this function formats; NaN and negatives become 0.
        line.extend(["-ss".into(), format!("{:.3}", seek.max(0.0)).into()]);
    }
    line.extend(input_args(run.input, Some(format))?);
    line.extend(output_args);
    line.extend(["-f".into(), muxer.into()]);
    line.push(frame_pattern(run.dir.path(), run.input, run.extension)?);
    Ok(line)
}

/// `<dir>/<input stem>_%06d.<extension>`, the only file name image2 is ever given. `%` is its
/// pattern character, so each `%` in the folder or the stem is doubled, which FFmpeg reads as
/// one literal `%`: a folder called `100%%` or a file called `a%d.mp4` can neither add a
/// counter nor point the frames at another folder. Separators in the stem become `_`, so the
/// frames always land directly in `dir`.
fn frame_pattern(dir: &Path, input: &Path, extension: &str) -> anyhow::Result<OsString> {
    let dir = std::path::absolute(dir)?;
    let dir = dir
        .to_str()
        .context("Frames can only be saved in a folder whose path is valid Unicode")?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("frame")
        .replace(['/', '\\'], "_");
    let escape = |s: &str| s.replace('%', "%%");
    let name = format!("{}_%06d.{extension}", escape(&stem));
    Ok(Path::new(&escape(dir)).join(name).into_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ffmpeg_video::test_media::{make_clip, real_ffmpeg, temp_dir};
    use crate::files::reserve_dir;
    use std::fs;
    use std::path::PathBuf;

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn words(text: &str) -> Vec<OsString> {
        text.split_whitespace().map(OsString::from).collect()
    }

    /// Every file under `dir`, relative to it, sorted.
    fn tree(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in fs::read_dir(&next).unwrap() {
                let path = entry.unwrap().path();
                match path.is_dir() {
                    true => pending.push(path),
                    false => found.push(
                        path.strip_prefix(dir)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    ),
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn every_percent_in_the_folder_and_the_stem_is_doubled() {
        let dir = Path::new("out").join("100%").join("a%d frames");

        let pattern = frame_pattern(&dir, Path::new("a%d.mp4"), "png").unwrap();

        let pattern = Path::new(&pattern);
        assert_eq!(pattern.file_name().unwrap(), "a%%d_%06d.png");
        assert!(pattern.parent().unwrap().ends_with("out/100%%/a%%d frames"));
    }

    #[test]
    fn separators_in_the_input_never_move_the_frames_out_of_the_folder() {
        let dir = std::path::absolute("frames").unwrap();

        let pattern = frame_pattern(&dir, Path::new("../../evil.mp4"), "jpg").unwrap();

        assert_eq!(Path::new(&pattern).parent().unwrap(), dir);
        assert_eq!(Path::new(&pattern).file_name().unwrap(), "evil_%06d.jpg");
    }

    #[test]
    fn the_sequence_command_seeks_before_its_only_input_and_ends_with_the_pattern() {
        let dir = temp_dir("frames-line");
        let reserved = reserve_dir(&dir.join("clip.mp4"), None, " frames").unwrap();
        let run = FramesRun {
            input: &dir.join("clip.mp4"),
            media: "a video",
            dir: &reserved,
            extension: "png",
            seek: Some(1.5),
            duration: None,
        };

        let line = strings(frames_command_line(&run, "mov", words("-c:v png")).unwrap());

        let ss = line.iter().position(|a| a == "-ss").unwrap();
        let i = line.iter().position(|a| a == "-i").unwrap();
        assert_eq!(line[ss + 1], "1.500");
        assert!(ss < i);
        assert_eq!(line.iter().filter(|a| *a == "-i").count(), 1);
        assert_eq!(line[line.len() - 3..line.len() - 1], ["-f", "image2"]);
        assert!(line[line.len() - 1].ends_with("clip_%06d.png"));
        drop(reserved);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_sequence_command_refuses_options_the_allowlist_refuses() {
        let dir = temp_dir("frames-refused");
        let reserved = reserve_dir(&dir.join("clip.mp4"), None, " frames").unwrap();
        let run = FramesRun {
            input: &dir.join("clip.mp4"),
            media: "a video",
            dir: &reserved,
            extension: "png",
            seek: None,
            duration: None,
        };

        let result = frames_command_line(&run, "mov", words("-f image2 -strftime 1"));

        assert_eq!(
            result.unwrap_err().to_string(),
            "FFmpeg option “-f” isn't allowed"
        );
        drop(reserved);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_quarter_turn_swaps_the_displayed_size() {
        let json = br#"{"streams":[{"width":1920,"height":1080,"avg_frame_rate":"30000/1001","side_data_list":[{"rotation":-90}]}]}"#;

        let stream = VideoStream::from_ffprobe_json(json).unwrap();

        assert_eq!((stream.width, stream.height), (1080, 1920));
        assert_eq!(stream.frame_rate, Some((30000, 1001)));
    }

    #[test]
    fn an_unknown_frame_rate_is_none() {
        let json = br#"{"streams":[{"width":640,"height":480,"avg_frame_rate":"0/0"}]}"#;

        let stream = VideoStream::from_ffprobe_json(json).unwrap();

        assert_eq!(stream.frame_rate, None);
        assert_eq!((stream.width, stream.height), (640, 480));
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    /// The folder holds `%%`: unescaped, FFmpeg would read it as `%` and write into the decoy.
    #[test]
    fn frames_land_only_in_the_reserved_folder_even_when_its_path_holds_percent_signs() {
        let Some((ff, dir)) = real_ffmpeg("frames-percent") else {
            return;
        };
        let source = dir.join("p%%q");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(dir.join("p%q").join("clip frames")).unwrap();
        let input = source.join("clip.mp4");
        make_clip(&input, 1, "160x120", false);
        let reserved = reserve_dir(&input, None, " frames").unwrap();
        let run = FramesRun {
            input: &input,
            media: "a video",
            dir: &reserved,
            extension: "png",
            seek: None,
            duration: Some(1.0),
        };

        let result = ff.run_frames(
            &run,
            &CancelToken::default(),
            |_| Ok(words("-map 0:v:0 -an -vf fps=2 -c:v png")),
            |_| {},
        );

        assert!(result.is_ok(), "{result:?}");
        reserved.keep();
        assert_eq!(
            tree(&dir),
            [
                "p%%q/clip frames/clip_000001.png",
                "p%%q/clip frames/clip_000002.png",
                "p%%q/clip.mp4",
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_sequence_leaves_no_frames_behind() {
        let Some((ff, dir)) = real_ffmpeg("frames-failed") else {
            return;
        };
        let input = dir.join("clip.mp4");
        make_clip(&input, 1, "160x120", false);
        let reserved = reserve_dir(&input, None, " frames").unwrap();
        let folder: PathBuf = reserved.path().to_path_buf();
        let run = FramesRun {
            input: &input,
            media: "a video",
            dir: &reserved,
            extension: "png",
            seek: None,
            duration: None,
        };

        let result = ff.run_frames(
            &run,
            &CancelToken::default(),
            |_| Ok(words("-c:v no_such_encoder")),
            |_| {},
        );
        drop(reserved);

        assert!(matches!(result, Err(JobError::Failed(_))), "{result:?}");
        assert!(!folder.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_rotated_video_reports_its_displayed_size() {
        let Some((ff, dir)) = real_ffmpeg("frames-rotated") else {
            return;
        };
        let plain = dir.join("plain.mp4");
        make_clip(&plain, 1, "320x240", false);
        let rotated = dir.join("rotated.mp4");
        let status = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-display_rotation", "90", "-i"])
            .arg(&plain)
            .args(["-c", "copy"])
            .arg(&rotated)
            .status()
            .unwrap();
        assert!(status.success());
        let info = ff.probe_video(&rotated).unwrap();

        let stream = ff.probe_video_stream(&rotated, &info).unwrap();

        assert_eq!(
            stream,
            VideoStream {
                width: 240,
                height: 320,
                frame_rate: Some((25, 1))
            }
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
