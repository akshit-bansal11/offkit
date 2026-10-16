//! Audio Joiner: an ordered list of audio files, joined into one. Wires the `AudioJoiner` global
//! (ui/tools/audio-joiner.slint) to the FFmpeg engine.
//!
//! The website decoded every track to a 48 kHz stereo WAV, then joined the WAVs with the concat
//! demuxer. Here one filtergraph does both: each input is converted to 48 kHz stereo 16-bit and
//! the `concat` filter joins them, so no temporary files are written.
//!
//! Opening files probes them on a background thread; joining runs FFmpeg on its own thread.
//! Results apply only while the job that made them is still the current one.

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::{ComponentHandle, Model, VecModel};

use crate::engine::ffmpeg::{Chain, Ffmpeg, Filter, Graph, MediaInfo, Run, require_audio};
use crate::engine::ffmpeg_audio::{
    self, AUDIO_EXTENSIONS, Format, MEDIA, format_for_path, format_time, probe, words,
};
use crate::files::{self, Reserved, file_name, format_size, reserve_replacing};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, AudioJoiner, FileItem, FileStatus};

pub const SLUG: &str = "audio-joiner";

/// What the website forced every track to before joining.
const SAMPLE_RATE: u32 = 48_000;

// ---- Logic --------------------------------------------------------------------------------

/// Where the track at `from` lands when moved `delta` places, or `None` off either end.
fn moved_index(len: usize, from: usize, delta: i32) -> Option<usize> {
    let to = from.checked_add_signed(delta as isize)?;
    (from < len && to < len).then_some(to)
}

/// The filtergraph for `count` inputs: each one's first audio track as 48 kHz stereo 16-bit,
/// then all of them in order through `concat`, to `[out]`.
fn join_graph(count: usize) -> OsString {
    let mut graph = Graph::new();
    let mut join = Chain::new();
    for i in 0..count {
        let label = format!("a{i}");
        let convert = Filter::new("aformat")
            .opt("sample_fmts", "s16")
            .opt("sample_rates", SAMPLE_RATE)
            .opt("channel_layouts", "stereo");
        graph = graph.chain(
            Chain::new()
                .input(&format!("{i}:a:0"))
                .then(convert)
                .output(&label),
        );
        join = join.input(&label);
    }
    let concat = Filter::new("concat")
        .opt("n", count)
        .opt("v", 0)
        .opt("a", 1);
    graph.chain(join.then(concat).output("out")).arg()
}

/// Joins `inputs`, in order, into `output` as `format`. `seconds` is the joined length, for
/// progress.
fn run_join(
    ff: &Ffmpeg,
    inputs: &[PathBuf],
    output: &Reserved,
    format: Format,
    seconds: f64,
    cancel: &CancelToken,
    on_progress: impl FnMut(f32),
) -> Result<(), JobError> {
    let run = Run {
        inputs,
        media: MEDIA,
        output,
        duration: Some(seconds),
        exit_prefix: "",
    };
    let prepare = |infos: &[MediaInfo]| {
        infos.iter().try_for_each(require_audio)?;
        let mut args = vec!["-filter_complex".into(), join_graph(infos.len())];
        args.extend(words("-map [out]"));
        args.extend(ffmpeg_audio::encode_args(format, None, ff.encoders())?);
        Ok(args)
    };
    ff.run(&run, cancel, prepare, on_progress)
}

// ---- State --------------------------------------------------------------------------------

struct Track {
    path: PathBuf,
    seconds: f64,
}

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    /// The formats offered: every one until FFmpeg has listed its encoders, then those it has.
    formats: Vec<Format>,
    tracks: Vec<Track>,
    /// One row per track, in the same order.
    rows: Rc<VecModel<FileItem>>,
    /// Files still being read.
    probing: usize,
    job: Option<Arc<CancelToken>>,
    saved: Option<PathBuf>,
    workers: Vec<JoinHandle<()>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

fn spawn(work: impl FnOnce() + Send + 'static) {
    let worker = thread::spawn(work);
    with(|s| {
        s.workers.retain(|w| !w.is_finished());
        s.workers.push(worker);
    });
}

/// Runs `f` on the UI thread if the join holding `token` is still the current one.
fn post_current(
    weak: &slint::Weak<AppWindow>,
    token: &Arc<CancelToken>,
    f: impl FnOnce(&AppWindow) + Send + 'static,
) {
    let token = token.clone();
    jobs::post(weak, move |app| {
        let current = with(|s| s.job.as_ref().is_some_and(|t| Arc::ptr_eq(t, &token)));
        if current {
            f(&app);
        }
    });
}

pub fn init(app: &AppWindow) {
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<AudioJoiner>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    with(|s| {
        g.set_files(s.rows.clone().into());
        s.formats = Format::EDIT.to_vec();
        s.ffmpeg = ffmpeg;
    });
    set_formats(app);
    sync(app);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Listing encoders runs FFmpeg, so it stays off the UI thread.
        let weak = app.as_weak();
        thread::spawn(move || {
            let formats = ffmpeg_audio::available(&Format::EDIT, ff.encoders());
            jobs::post(&weak, move |app| {
                let g = app.global::<AudioJoiner>();
                let previous = with(|s| {
                    s.formats
                        .get(index(g.get_format_index(), s.formats.len()))
                        .copied()
                });
                with(|s| s.formats = formats);
                let i = with(|s| s.formats.iter().position(|f| Some(*f) == previous));
                g.set_format_index(i.unwrap_or(0) as i32);
                set_formats(&app);
            });
        });
    }

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Audio", Some(("Audio", AUDIO_EXTENSIONS)))
        {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let i = i as usize;
            if i < s.tracks.len() {
                s.tracks.remove(i);
                s.rows.remove(i);
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_reorder(move |i, delta| {
        let Some(app) = weak.upgrade() else { return };
        let moved = with(|s| {
            let from = i as usize;
            let to = moved_index(s.tracks.len(), from, delta)?;
            s.tracks.swap(from, to);
            let (a, b) = (s.rows.row_data(from)?, s.rows.row_data(to)?);
            s.rows.set_row_data(from, b);
            s.rows.set_row_data(to, a);
            Some(to)
        });
        if let Some(to) = moved {
            // The row now at `to` takes the keyboard focus, so a reorder can be repeated.
            app.global::<AudioJoiner>().set_active(to as i32);
        }
    });
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            s.tracks.clear();
            s.rows.set_vec(Vec::new());
        });
        app.global::<AudioJoiner>().set_error("".into());
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_join(move || {
        if let Some(app) = weak.upgrade() {
            join(&app);
        }
    });
    let weak = app.as_weak();
    g.on_cancel_join(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(token) = with(|s| s.job.take()) {
            token.cancel();
        }
        let g = app.global::<AudioJoiner>();
        g.set_joining(false);
        g.set_result("Join cancelled".into());
        g.set_result_failed(false);
        sync(&app);
    });
    g.on_reveal(|| {
        if let Some(path) = with(|s| s.saved.clone()) {
            files::reveal(&path);
        }
    });
    g.on_get_ffmpeg(|| {
        if let Err(e) = open::that("https://ffmpeg.org/download.html") {
            eprintln!("couldn't open the FFmpeg download page: {e}");
        }
    });
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<AudioJoiner>();
    g.set_drop_hover(hover && g.get_ffmpeg_found());
}

/// One file read by the probe thread: its track and the row to show, or why it was refused.
type Read = Result<(Track, FileItem), String>;

/// Adds `paths` to the end of the list, once FFmpeg has read each. A file FFmpeg can't read
/// as audio is named in the error line and left out; the rest are added.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(ff) = with(|s| s.ffmpeg.clone()) else {
        return;
    };
    let paths: Vec<PathBuf> = paths.into_iter().filter(|p| p.is_file()).collect();
    if paths.is_empty() {
        return;
    }
    with(|s| s.probing += paths.len());
    app.global::<AudioJoiner>().set_error("".into());
    sync(app);

    let weak = app.as_weak();
    spawn(move || {
        let count = paths.len();
        let reads: Vec<Read> = paths
            .into_iter()
            .map(|path| {
                let info = probe(&ff, &path)
                    .map_err(|e| format!("Couldn't add {}: {e}", file_name(&path)))?;
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or_default();
                let row = FileItem {
                    name: file_name(&path).into(),
                    detail: format!("{} · {}", format_size(size), info.summary()).into(),
                    status: FileStatus::Ready,
                    progress: 0.0,
                    message: "".into(),
                };
                let seconds = info.duration.unwrap_or_default();
                Ok((Track { path, seconds }, row))
            })
            .collect();
        jobs::post(&weak, move |app| {
            let mut refused = Vec::new();
            with(|s| {
                s.probing -= count;
                for read in reads {
                    match read {
                        Ok((track, row)) => {
                            s.tracks.push(track);
                            s.rows.push(row);
                        }
                        Err(message) => refused.push(message),
                    }
                }
            });
            app.global::<AudioJoiner>()
                .set_error(refused.join("\n").into());
            sync(&app);
        });
    });
}

/// Refreshes what depends on the list: the summary line, whether Join is possible and why not.
fn sync(app: &AppWindow) {
    let g = app.global::<AudioJoiner>();
    let (count, seconds, probing, joining) = with(|s| {
        let seconds: f64 = s.tracks.iter().map(|t| t.seconds).sum();
        (s.tracks.len(), seconds, s.probing, s.job.is_some())
    });
    let noun = if count == 1 { "track" } else { "tracks" };
    g.set_summary(format!("{count} {noun} · {} in total", format_time(seconds)).into());
    g.set_reading(probing > 0);
    g.set_can_join(count >= 2 && probing == 0 && !joining);
    g.set_note(
        if count < 2 {
            "Add at least 2 audio files to join."
        } else {
            ""
        }
        .into(),
    );
}

fn join(app: &AppWindow) {
    let g = app.global::<AudioJoiner>();
    let prepared = with(|s| {
        let format = *s.formats.get(index(g.get_format_index(), s.formats.len()))?;
        let paths: Vec<PathBuf> = s.tracks.iter().map(|t| t.path.clone()).collect();
        let seconds = s.tracks.iter().map(|t| t.seconds).sum::<f64>();
        Some((s.ffmpeg.clone()?, paths, seconds, s.formats.clone(), format))
    });
    let Some((ff, inputs, seconds, offered, format)) = prepared else {
        return;
    };
    // Outside `with`: the dialog runs a nested event loop.
    let suggested = format!("joined_audio.{}", format.extension());
    let Some(target) = files::choose_save_path("Save Joined Audio", &suggested) else {
        return;
    };
    let fail = |message: String| {
        g.set_result_failed(true);
        g.set_result(message.into());
        g.set_can_reveal(false);
    };
    // The name typed in the dialog decides the format, so "mix.wav" is a WAV file.
    let Some(format) = format_for_path(&target, &offered) else {
        let names: Vec<_> = offered
            .iter()
            .map(|f| format!(".{}", f.extension()))
            .collect();
        return fail(format!("Choose a name ending in {}.", names.join(", ")));
    };
    let output = match reserve_replacing(&target) {
        Ok(output) => output,
        Err(e) => return fail(format!("Couldn't create {}: {e}", file_name(&target))),
    };

    let token = Arc::new(CancelToken::default());
    if let Some(old) = with(|s| s.job.replace(token.clone())) {
        old.cancel();
    }
    g.set_joining(true);
    g.set_progress(0.0);
    g.set_result_failed(false);
    g.set_result("".into());
    g.set_can_reveal(false);
    sync(app);
    let weak = app.as_weak();
    spawn(move || {
        let progress = |p: f32| {
            post_current(&weak, &token, move |app| {
                app.global::<AudioJoiner>().set_progress(p);
            });
        };
        let result = run_join(&ff, &inputs, &output, format, seconds, &token, progress)
            .and_then(|()| {
                output.keep().map_err(|e| {
                    JobError::Failed(format!("Couldn't save {}: {e}", file_name(&target)))
                })
            });
        post_current(&weak, &token, move |app| {
            let g = app.global::<AudioJoiner>();
            g.set_joining(false);
            with(|s| s.job = None);
            match result {
                Ok(saved) => {
                    let size = std::fs::metadata(&saved)
                        .map(|m| m.len())
                        .unwrap_or_default();
                    g.set_result(
                        format!("Saved as {} · {}", file_name(&saved), format_size(size)).into(),
                    );
                    g.set_can_reveal(true);
                    with(|s| s.saved = Some(saved));
                }
                Err(JobError::Cancelled) => g.set_result("Join cancelled".into()),
                Err(JobError::Failed(message)) => {
                    g.set_result_failed(true);
                    g.set_result(message.into());
                }
            }
            sync(app);
        });
    });
}

fn set_formats(app: &AppWindow) {
    let labels: Vec<&str> = with(|s| s.formats.iter().map(|f| f.label()).collect());
    app.global::<AudioJoiner>()
        .set_formats(select::options(labels.into_iter()));
}

/// Cancels the join and hands back the threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        if let Some(token) = s.job.take() {
            token.cancel();
        }
        std::mem::take(&mut s.workers)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ffmpeg_audio::tests::{fixture, real_ffmpeg};
    use crate::files::reserve_output;
    use std::fs;

    #[test]
    fn a_track_moves_down_one_place() {
        assert_eq!(moved_index(3, 0, 1), Some(1));
    }

    #[test]
    fn a_track_moves_up_one_place() {
        assert_eq!(moved_index(3, 2, -1), Some(1));
    }

    #[test]
    fn the_first_track_cannot_move_up() {
        assert_eq!(moved_index(3, 0, -1), None);
    }

    #[test]
    fn the_last_track_cannot_move_down() {
        assert_eq!(moved_index(3, 2, 1), None);
    }

    #[test]
    fn a_track_past_the_end_cannot_move() {
        assert_eq!(moved_index(2, 5, -1), None);
    }

    #[test]
    fn two_inputs_are_converted_then_concatenated() {
        assert_eq!(
            join_graph(2).to_string_lossy(),
            "[0:a:0]aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo[a0];\
             [1:a:0]aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo[a1];\
             [a0][a1]concat=n=2:v=0:a=1[out]"
        );
    }

    #[test]
    fn mixed_files_join_into_one_file_as_long_as_their_sum() {
        let Some((ff, dir)) = real_ffmpeg("joiner") else {
            return;
        };
        let mono = fixture(&dir, "mono.mp3", "sine=duration=1:sample_rate=22050", "-ac 1");
        let stereo = fixture(&dir, "stereo.flac", "sine=duration=2", "-ac 2");
        let inputs = [mono, stereo];
        let output = reserve_output(&inputs[0], None, "wav").unwrap();

        run_join(&ff, &inputs, &output, Format::Wav, 3.0, &CancelToken::default(), |_| {})
            .unwrap();

        let info = probe(&ff, &output.keep().unwrap()).unwrap();
        let seconds = info.duration.unwrap();
        assert!((seconds - 3.0).abs() < 0.1, "{seconds}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_video_without_sound_stops_the_join() {
        let Some((ff, dir)) = real_ffmpeg("joiner-silent") else {
            return;
        };
        let tone = fixture(&dir, "tone.wav", "sine=duration=1", "");
        let silent = fixture(
            &dir,
            "silent.mp4",
            "testsrc=duration=1:size=64x64",
            "-c:v libx264 -pix_fmt yuv420p",
        );
        let inputs = [tone, silent];
        let output = reserve_output(&inputs[0], None, "wav").unwrap();

        let result =
            run_join(&ff, &inputs, &output, Format::Wav, 2.0, &CancelToken::default(), |_| {});

        assert!(matches!(result, Err(JobError::Failed(m)) if m == "No audio track found"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
