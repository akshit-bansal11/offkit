//! Audio Editor: one file, trimmed and reshaped (speed, pitch, a three-band EQ), then exported.
//! Wires the `AudioEditor` global (ui/tools/audio-editor.slint) to the FFmpeg engine.
//!
//! Opening a file probes it, reads its sample rate (pitch shifting needs it) and decodes its
//! waveform, all on one background thread. Export and Preview run FFmpeg on their own threads.
//! Every result carries the token of the job that made it and applies only while that job is
//! still the current one, so a file replaced mid-load never shows the old one's waveform.
//!
//! The trim is applied inside the filtergraph, before speed and pitch: the website's output
//! `-ss`/`-to` ran after its filters, so at any speed but 1× it cut the wrong stretch.
//!
//! Play renders the edited stretch to a temporary WAV and plays it in the app. The audio
//! output lives on its own thread (`Engine`), which the UI thread only sends commands to and
//! reads a report from, so it never waits on the device. While the user is listening, a change
//! to a setting that then holds still re-renders the preview and carries on from the same spot.

use std::cell::RefCell;
use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::bail;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::engine::ffmpeg::{Chain, Ffmpeg, Filter, Graph};
use crate::engine::ffmpeg_audio::{
    self, AUDIO_EXTENSIONS, Format, decode_wav, format_for_path, format_time, read_wav_header,
    words,
};
use crate::files::{self, Reserved, file_name, format_size, reserve_replacing};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, AudioEditor, AudioEditorStage};

pub const SLUG: &str = "audio-editor";
/// Bars in the waveform; drawn scaled to the view's width.
const BARS: usize = 240;
/// How often the playhead follows the engine and the settings are checked.
const TICK: Duration = Duration::from_millis(50);
/// How long the settings must hold still before a playing preview is re-rendered.
const SETTLE: Duration = Duration::from_millis(400);
/// How often the engine thread refreshes its report between commands.
const POLL: Duration = Duration::from_millis(30);
const ENGINE_GONE: &str = "Audio playback stopped unexpectedly. Press Play to try again.";

// ---- Logic --------------------------------------------------------------------------------

/// Everything an export applies, in the units FFmpeg takes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Edit {
    /// The kept stretch of the source, in seconds.
    start: f64,
    end: f64,
    /// Playback rate: 0.25 to 4.
    speed: f64,
    /// Semitones up (or down when negative): -12 to 12.
    pitch: f64,
    /// Gains in dB: -12 to 12.
    bass: f64,
    mid: f64,
    treble: f64,
}

impl Edit {
    /// Seconds of output: the kept stretch, played at `speed`.
    fn output_seconds(&self) -> f64 {
        (self.end - self.start).max(0.0) / self.speed
    }

    /// The source time heard `output` seconds into this edit's preview. The preview is the kept
    /// stretch at `speed`, so the playhead covers `speed` source seconds per second; it stays
    /// within the kept stretch.
    fn source_time(&self, output: f64) -> f64 {
        (self.start + output * self.speed).max(self.start).min(self.end)
    }

    /// How far into this edit's preview `source` falls: the inverse of `source_time`, so a
    /// seek outside the kept stretch lands on its nearest edge.
    fn output_time(&self, source: f64) -> f64 {
        (source.max(self.start).min(self.end) - self.start) / self.speed
    }
}

/// Where playback is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PlayState {
    /// Nothing loaded in the engine, or it was stopped.
    #[default]
    Idle,
    Playing,
    Paused,
    /// Played to the end; Play starts again from the beginning.
    Ended,
}

impl PlayState {
    /// After Play / Pause (or the space bar): playing pauses, anything else plays.
    fn toggled(self) -> Self {
        match self {
            Self::Playing => Self::Paused,
            Self::Idle | Self::Paused | Self::Ended => Self::Playing,
        }
    }

    /// After the engine reports the end of the preview.
    fn finished(self) -> Self {
        match self {
            Self::Playing => Self::Ended,
            other => other,
        }
    }
}

/// Where to carry on from, and whether to play, once the preview being rendered is ready.
#[derive(Debug, Clone, Copy)]
struct Resume {
    /// A time in the source, in seconds.
    at: f64,
    play: bool,
}

/// The website's tolerances: below these a setting counts as untouched.
const RATE_EPSILON: f64 = 1e-4;
const GAIN_EPSILON: f64 = 0.1;

/// `atempo` takes 0.5 to 2 at a time, so a larger change is a chain: whole doublings or
/// halvings first, then the remainder (the website's `buildAtempoChain`).
fn atempo_chain(speed: f64) -> Vec<Filter> {
    if (speed - 1.0).abs() < RATE_EPSILON {
        return Vec::new();
    }
    let mut chain = Vec::new();
    let mut remaining = speed;
    while remaining > 2.0 + 1e-5 {
        chain.push(Filter::new("atempo").arg("2.0"));
        remaining /= 2.0;
    }
    while remaining < 0.5 - 1e-5 {
        chain.push(Filter::new("atempo").arg("0.5"));
        remaining /= 0.5;
    }
    if (remaining - 1.0).abs() > RATE_EPSILON {
        chain.push(Filter::new("atempo").arg(format!("{remaining:.6}")));
    }
    chain
}

/// The `-af` graph for `edit`: the trim, then the website's chain. A pitch shift plays the
/// audio at `2^(st/12)` times its `sample_rate` (higher and faster), corrects the tempo with
/// `atempo`, and resamples back; the EQ follows.
fn graph(edit: &Edit, sample_rate: Option<u32>) -> anyhow::Result<OsString> {
    let mut chain = Chain::new()
        .then(
            Filter::new("atrim")
                .opt("start", format!("{:.3}", edit.start))
                .opt("end", format!("{:.3}", edit.end)),
        )
        .then(Filter::new("asetpts").arg("PTS-STARTPTS"));
    if edit.pitch.abs() > RATE_EPSILON {
        let Some(rate) = sample_rate else {
            bail!("The file's sample rate isn't known yet, so its pitch can't change");
        };
        let factor = 2f64.powf(edit.pitch / 12.0);
        chain = chain.then(Filter::new("asetrate").arg((f64::from(rate) * factor).round()));
        for filter in atempo_chain(edit.speed / factor) {
            chain = chain.then(filter);
        }
        chain = chain.then(Filter::new("aresample").arg(rate));
    } else {
        for filter in atempo_chain(edit.speed) {
            chain = chain.then(filter);
        }
    }
    if edit.bass.abs() > GAIN_EPSILON {
        chain = chain.then(Filter::new("bass").opt("g", edit.bass).opt("f", 100));
    }
    if edit.mid.abs() > GAIN_EPSILON {
        let eq = Filter::new("equalizer")
            .opt("f", 1000)
            .opt("t", "q")
            .opt("w", 2)
            .opt("g", edit.mid);
        chain = chain.then(eq);
    }
    if edit.treble.abs() > GAIN_EPSILON {
        chain = chain.then(Filter::new("treble").opt("g", edit.treble).opt("f", 8000));
    }
    Ok(Graph::new().chain(chain).arg())
}

/// "1.25×", as the website shows a speed.
fn speed_label(speed: f64) -> String {
    format!("{speed:.2}×")
}

/// The sample rate of `input`'s first audio track, read from the header of a one-frame decode
/// (the probe doesn't report it).
fn sample_rate(
    ff: &Ffmpeg,
    input: &std::path::Path,
    cancel: &CancelToken,
) -> Result<u32, JobError> {
    let wav = decode_wav(ff, input, words("-frames:a 1"), cancel)?;
    let header = std::fs::File::open(wav.path())
        .map_err(anyhow::Error::from)
        .and_then(|mut f| read_wav_header(&mut f))
        .map_err(|e| JobError::Failed(format!("Couldn't read the sample rate: {e}")))?;
    Ok(header.sample_rate)
}

// ---- Sound --------------------------------------------------------------------------------

/// What the UI thread asks of the engine thread.
enum Command {
    /// Plays a preview from `at` seconds in: the new `wav`, or (`None`) the one it still holds.
    /// `serial` names this load in the engine's reports.
    Open {
        wav: Option<Reserved>,
        at: f64,
        play: bool,
        serial: u64,
    },
    Play,
    Pause,
    /// Jumps within the loaded preview, in seconds.
    Seek(f64),
    /// Stops playing; with `release`, also lets go of (and so deletes) the preview file.
    Stop { release: bool },
}

/// What the engine thread tells the UI thread, refreshed every `POLL`.
#[derive(Default)]
struct Report {
    /// The `Open` this describes; a report for an older one is stale and ignored.
    serial: u64,
    /// Seconds into the preview.
    position: f64,
    /// The preview has played to its end.
    ended: bool,
    /// What went wrong since the UI last looked; taken when read.
    error: Option<String>,
}

fn lock(report: &Mutex<Report>) -> MutexGuard<'_, Report> {
    // A poisoned lock only means the engine thread panicked mid-update; its fields stay valid.
    report.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The UI thread's end of a running engine.
struct Audio {
    commands: Sender<Command>,
    report: Arc<Mutex<Report>>,
}

/// Owns the audio output and the player on the engine thread: the output stream is `!Send` on
/// some platforms, so it is created there, lazily, on the first Play. No device (a server, CI,
/// headphones unplugged) is an error in the report rather than a crash, and the next Play tries
/// again.
struct Engine {
    report: Arc<Mutex<Report>>,
    sink: Option<MixerDeviceSink>,
    player: Option<Player>,
    /// The preview file; kept open by the player only while it plays.
    wav: Option<Reserved>,
    serial: u64,
}

impl Engine {
    fn new(report: Arc<Mutex<Report>>) -> Self {
        Self {
            report,
            sink: None,
            player: None,
            wav: None,
            serial: 0,
        }
    }

    /// Serves commands until the UI thread drops its sender.
    fn run(mut self, commands: mpsc::Receiver<Command>) {
        loop {
            match commands.recv_timeout(POLL) {
                Ok(command) => self.apply(command),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            self.publish();
        }
    }

    fn apply(&mut self, command: Command) {
        let result = match command {
            Command::Open {
                wav,
                at,
                play,
                serial,
            } => {
                self.serial = serial;
                self.open(wav, at, play)
            }
            Command::Play => {
                if let Some(player) = &self.player {
                    player.play();
                }
                Ok(())
            }
            Command::Pause => {
                if let Some(player) = &self.player {
                    player.pause();
                }
                Ok(())
            }
            Command::Seek(at) => self.seek(at),
            Command::Stop { release } => {
                // The player holds the file open, so it goes first.
                self.player = None;
                if release {
                    self.wav = None;
                }
                Ok(())
            }
        };
        if let Err(message) = result {
            lock(&self.report).error = Some(message);
        }
    }

    fn open(&mut self, wav: Option<Reserved>, at: f64, play: bool) -> Result<(), String> {
        // Release the old file before a new one replaces it: Windows won't delete an open file.
        self.player = None;
        if wav.is_some() {
            self.wav = wav;
        }
        let Some(wav) = &self.wav else {
            return Err("There is no preview to play".into());
        };
        if self.sink.is_none() {
            let mut sink = DeviceSinkBuilder::open_default_sink()
                .map_err(|e| format!("No audio output is available: {e}"))?;
            sink.log_on_drop(false);
            self.sink = Some(sink);
        }
        let Some(sink) = &self.sink else {
            return Err("No audio output is available".into());
        };
        let file = File::open(wav.path())
            .map_err(|e| format!("Couldn't open the preview: {e}"))?;
        let decoder = Decoder::try_from(file)
            .map_err(|e| format!("Couldn't read the preview: {e}"))?;
        let player = Player::connect_new(sink.mixer());
        // Paused until it is in place, so a jump doesn't play the first moments.
        player.pause();
        player.append(decoder);
        if at > 0.0 {
            player
                .try_seek(Duration::from_secs_f64(at))
                .map_err(|e| format!("Couldn't jump to that point: {e}"))?;
        }
        if play {
            player.play();
        }
        self.player = Some(player);
        Ok(())
    }

    fn seek(&self, at: f64) -> Result<(), String> {
        match &self.player {
            Some(player) => player
                .try_seek(Duration::from_secs_f64(at.max(0.0)))
                .map_err(|e| format!("Couldn't jump to that point: {e}")),
            None => Ok(()),
        }
    }

    fn publish(&self) {
        let mut report = lock(&self.report);
        report.serial = self.serial;
        match &self.player {
            Some(player) => {
                report.position = player.get_pos().as_secs_f64();
                report.ended = player.empty();
            }
            None => report.ended = false,
        }
    }
}

// ---- State --------------------------------------------------------------------------------

#[derive(Default)]
struct State {
    ffmpeg: Option<Arc<Ffmpeg>>,
    /// The formats offered: every one until FFmpeg has listed its encoders, then those it has.
    formats: Vec<Format>,
    file: Option<PathBuf>,
    duration: f64,
    sample_rate: Option<u32>,
    /// The current load, export and preview; a result applies only while its token is here.
    load: Option<Arc<CancelToken>>,
    export: Option<Arc<CancelToken>>,
    preview: Option<Arc<CancelToken>>,
    /// The engine thread, started by the first Play.
    audio: Option<Audio>,
    play: PlayState,
    /// The `Open` the UI expects reports for.
    serial: u64,
    /// The settings the engine's preview was rendered with: positions map through these.
    rendered: Option<Edit>,
    /// The settings a preview was last asked for, so a failed one isn't retried until they change.
    requested: Option<Edit>,
    /// Set while a preview is being rendered, taken when it lands.
    resume: Option<Resume>,
    /// The settings last seen and since when, to tell when they have held still.
    settling: Option<(Edit, Instant)>,
    ticker: slint::Timer,
    exported: Option<PathBuf>,
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

/// Which job slot a result belongs to.
#[derive(Clone, Copy)]
enum Slot {
    Load,
    Export,
    Preview,
}

fn slot(s: &mut State, slot: Slot) -> &mut Option<Arc<CancelToken>> {
    match slot {
        Slot::Load => &mut s.load,
        Slot::Export => &mut s.export,
        Slot::Preview => &mut s.preview,
    }
}

/// Starts a job in `slot`, cancelling the one it replaces, and returns its token.
fn begin(slot_of: Slot) -> Arc<CancelToken> {
    let token = Arc::new(CancelToken::default());
    if let Some(old) = with(|s| slot(s, slot_of).replace(token.clone())) {
        old.cancel();
    }
    token
}

/// Runs `f` on the UI thread if the job holding `token` is still the current one in `slot_of`.
fn post_current(
    weak: &slint::Weak<AppWindow>,
    slot_of: Slot,
    token: &Arc<CancelToken>,
    f: impl FnOnce(&AppWindow) + Send + 'static,
) {
    let token = token.clone();
    jobs::post(weak, move |app| {
        let current = with(|s| slot(s, slot_of).as_ref().is_some_and(|t| Arc::ptr_eq(t, &token)));
        if current {
            f(&app);
        }
    });
}

fn spawn(work: impl FnOnce() + Send + 'static) {
    let worker = thread::spawn(work);
    with(|s| {
        s.workers.retain(|w| !w.is_finished());
        s.workers.push(worker);
    });
}

pub fn init(app: &AppWindow) {
    let ffmpeg = Ffmpeg::shared();
    let g = app.global::<AudioEditor>();
    g.set_ffmpeg_found(ffmpeg.is_some());
    with(|s| {
        s.formats = Format::EDIT.to_vec();
        s.ffmpeg = ffmpeg;
    });
    set_formats(app);

    if let Some(ff) = with(|s| s.ffmpeg.clone()) {
        // Listing encoders runs FFmpeg, so it stays off the UI thread.
        let weak = app.as_weak();
        thread::spawn(move || {
            let formats = ffmpeg_audio::available(&Format::EDIT, ff.encoders());
            jobs::post(&weak, move |app| {
                let g = app.global::<AudioEditor>();
                let previous = with(|s| s.formats.get(index(g.get_format_index(), s.formats.len())).copied());
                with(|s| s.formats = formats);
                let i = with(|s| s.formats.iter().position(|f| Some(*f) == previous));
                g.set_format_index(i.unwrap_or(0) as i32);
                set_formats(&app);
            });
        });
    }

    g.on_format_time(|seconds| format_time(f64::from(seconds)).into());
    g.on_format_speed(|percent| speed_label(f64::from(percent) / 100.0).into());

    let weak = app.as_weak();
    g.on_choose_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose Audio", Some(("Audio", AUDIO_EXTENSIONS))) {
            add_paths(&app, vec![path]);
        }
    });
    let weak = app.as_weak();
    g.on_close_file(move || {
        if let Some(app) = weak.upgrade() {
            close(&app);
        }
    });
    let weak = app.as_weak();
    g.on_export(move || {
        if let Some(app) = weak.upgrade() {
            export(&app);
        }
    });
    let weak = app.as_weak();
    g.on_cancel_export(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(token) = with(|s| s.export.take()) {
            token.cancel();
        }
        let g = app.global::<AudioEditor>();
        g.set_exporting(false);
        g.set_export_message("Export cancelled".into());
    });
    let weak = app.as_weak();
    g.on_toggle_playback(move || {
        if let Some(app) = weak.upgrade() {
            toggle(&app);
        }
    });
    let weak = app.as_weak();
    g.on_seek(move |seconds| {
        if let Some(app) = weak.upgrade() {
            seek(&app, f64::from(seconds));
        }
    });
    let weak = app.as_weak();
    with(|s| {
        s.ticker.start(slint::TimerMode::Repeated, TICK, move || {
            if let Some(app) = weak.upgrade() {
                tick(&app);
            }
        });
    });
    g.on_reveal(|| {
        if let Some(path) = with(|s| s.exported.clone()) {
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
    let g = app.global::<AudioEditor>();
    g.set_drop_hover(hover && g.get_ffmpeg_found());
}

/// Opens the first of `paths` (one file at a time, from a drop, the dialog or the snapshot
/// hook) in place of the current one, resetting every setting as the website did.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let Some(ff) = with(|s| s.ffmpeg.clone()) else {
        return;
    };
    let Some(path) = paths.into_iter().find(|p| p.is_file()) else {
        return;
    };
    for slot_of in [Slot::Export, Slot::Preview] {
        if let Some(token) = with(|s| slot(s, slot_of).take()) {
            token.cancel();
        }
    }
    halt(app, true);
    let token = begin(Slot::Load);
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or_default();
    with(|s| {
        s.file = Some(path.clone());
        s.duration = 0.0;
        s.sample_rate = None;
        s.exported = None;
    });
    let g = app.global::<AudioEditor>();
    g.set_stage(AudioEditorStage::Reading);
    g.set_file_name(file_name(&path).into());
    g.set_file_detail(format_size(size).into());
    g.set_error("".into());
    g.set_peaks(ModelRc::default());
    g.set_waveform_failed(false);
    g.set_duration(0.0);
    g.set_start(0.0);
    g.set_end(0.0);
    g.set_speed(100.0);
    g.set_pitch(0.0);
    g.set_bass(0.0);
    g.set_mid(0.0);
    g.set_treble(0.0);
    g.set_exporting(false);
    g.set_previewing(false);
    g.set_export_message("".into());
    g.set_export_failed(false);
    g.set_can_reveal(false);

    let weak = app.as_weak();
    spawn(move || {
        let probed = ffmpeg_audio::probe(&ff, &path).and_then(|info| {
            let rate = sample_rate(&ff, &path, &token).map_err(|e| match e {
                JobError::Failed(m) => anyhow::anyhow!(m),
                JobError::Cancelled => anyhow::anyhow!("Cancelled"),
            })?;
            Ok((info, rate))
        });
        let (info, rate) = match probed {
            Ok(found) => found,
            Err(e) => {
                let message = format!("Couldn't open {}: {e}", file_name(&path));
                return post_current(&weak, Slot::Load, &token, move |app| {
                    with(|s| s.file = None);
                    let g = app.global::<AudioEditor>();
                    g.set_stage(AudioEditorStage::Empty);
                    g.set_error(message.into());
                });
            }
        };
        let duration = info.duration.unwrap_or_default();
        let summary = info.summary();
        post_current(&weak, Slot::Load, &token, move |app| {
            with(|s| {
                s.duration = duration;
                s.sample_rate = Some(rate);
            });
            let g = app.global::<AudioEditor>();
            g.set_file_detail(format!("{} · {summary}", g.get_file_detail()).into());
            g.set_duration(duration as f32);
            g.set_end(duration as f32);
            g.set_stage(AudioEditorStage::Ready);
        });

        let peaks = ffmpeg_audio::peaks(&ff, &path, BARS, &token);
        post_current(&weak, Slot::Load, &token, move |app| {
            let g = app.global::<AudioEditor>();
            match peaks {
                Ok(peaks) => g.set_peaks(ModelRc::from(Rc::new(VecModel::from(peaks)))),
                Err(e) => {
                    eprintln!("couldn't draw the waveform: {e:?}");
                    g.set_waveform_failed(true);
                }
            }
        });
    });
}

/// Back to the drop zone; cancels whatever was running for the file.
fn close(app: &AppWindow) {
    for slot_of in [Slot::Load, Slot::Export, Slot::Preview] {
        if let Some(token) = with(|s| slot(s, slot_of).take()) {
            token.cancel();
        }
    }
    halt(app, true);
    with(|s| {
        s.file = None;
        s.exported = None;
    });
    let g = app.global::<AudioEditor>();
    g.set_stage(AudioEditorStage::Empty);
    g.set_error("".into());
    g.set_peaks(ModelRc::default());
    g.set_exporting(false);
    g.set_previewing(false);
}

/// The settings as the view holds them: sliders keep whole steps (percent, half semitones,
/// half decibels), so they are converted here.
fn current_edit(app: &AppWindow) -> Edit {
    let g = app.global::<AudioEditor>();
    let half = |v: f32| f64::from(v) / 2.0;
    Edit {
        start: f64::from(g.get_start()),
        end: f64::from(g.get_end()),
        speed: f64::from(g.get_speed()) / 100.0,
        pitch: half(g.get_pitch()),
        bass: half(g.get_bass()),
        mid: half(g.get_mid()),
        treble: half(g.get_treble()),
    }
}

fn export(app: &AppWindow) {
    let g = app.global::<AudioEditor>();
    let edit = current_edit(app);
    let prepared = with(|s| {
        let format = *s.formats.get(index(g.get_format_index(), s.formats.len()))?;
        Some((s.ffmpeg.clone()?, s.file.clone()?, s.sample_rate, s.formats.clone(), format))
    });
    let Some((ff, input, rate, offered, format)) = prepared else {
        return;
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".into());
    // Outside `with`: the dialog runs a nested event loop.
    let suggested = format!("{stem}_edited.{}", format.extension());
    let Some(target) = files::choose_save_path("Export Edited Audio", &suggested) else {
        return;
    };
    let fail = |message: String| {
        g.set_export_failed(true);
        g.set_export_message(message.into());
        g.set_can_reveal(false);
    };
    // The name typed in the dialog decides the format, so "take.wav" is a WAV file.
    let Some(format) = format_for_path(&target, &offered) else {
        let names: Vec<_> = offered.iter().map(|f| format!(".{}", f.extension())).collect();
        return fail(format!("Choose a name ending in {}.", names.join(", ")));
    };
    let mut options = vec!["-af".into(), match graph(&edit, rate) {
        Ok(graph) => graph,
        Err(e) => return fail(e.to_string()),
    }];
    match ffmpeg_audio::encode_args(format, None, ff.encoders()) {
        Ok(codec) => options.extend(codec),
        Err(e) => return fail(e.to_string()),
    }
    let output = match reserve_replacing(&target) {
        Ok(output) => output,
        Err(e) => return fail(format!("Couldn't create {}: {e}", file_name(&target))),
    };

    let token = begin(Slot::Export);
    g.set_exporting(true);
    g.set_export_progress(0.0);
    g.set_export_failed(false);
    g.set_export_message("".into());
    g.set_can_reveal(false);
    let weak = app.as_weak();
    spawn(move || {
        let progress = |p: f32| {
            post_current(&weak, Slot::Export, &token, move |app| {
                app.global::<AudioEditor>().set_export_progress(p);
            });
        };
        let result = ffmpeg_audio::transcode(
            &ff,
            &input,
            &output,
            Some(edit.output_seconds()),
            options,
            &token,
            progress,
        )
        .and_then(|()| {
            output
                .keep()
                .map_err(|e| JobError::Failed(format!("Couldn't save {}: {e}", file_name(&target))))
        });
        post_current(&weak, Slot::Export, &token, move |app| {
            let g = app.global::<AudioEditor>();
            g.set_exporting(false);
            with(|s| s.export = None);
            match result {
                Ok(saved) => {
                    let size = std::fs::metadata(&saved).map(|m| m.len()).unwrap_or_default();
                    g.set_export_message(
                        format!("Saved as {} · {}", file_name(&saved), format_size(size)).into(),
                    );
                    g.set_can_reveal(true);
                    with(|s| s.exported = Some(saved));
                }
                Err(JobError::Cancelled) => g.set_export_message("Export cancelled".into()),
                Err(JobError::Failed(message)) => {
                    g.set_export_failed(true);
                    g.set_export_message(message.into());
                }
            }
        });
    });
}

// ---- Playback -----------------------------------------------------------------------------

/// Records `play` and shows it: the buttons, and whether the playhead is drawn.
fn set_play_state(app: &AppWindow, play: PlayState) {
    with(|s| s.play = play);
    let g = app.global::<AudioEditor>();
    g.set_playing(play == PlayState::Playing);
    g.set_ended(play == PlayState::Ended);
    g.set_playhead_visible(play != PlayState::Idle);
}

/// Sends `command` to the engine, if one is running. If its thread has died, the next Play
/// starts a fresh one.
fn send(app: &AppWindow, command: Command) {
    let Some(commands) = with(|s| s.audio.as_ref().map(|a| a.commands.clone())) else {
        return;
    };
    if commands.send(command).is_err() {
        with(|s| s.audio = None);
        set_play_state(app, PlayState::Idle);
        let g = app.global::<AudioEditor>();
        g.set_playback_message(ENGINE_GONE.into());
    }
}

/// Starts the engine thread if it isn't running.
fn ensure_engine() {
    if with(|s| s.audio.is_some()) {
        return;
    }
    let (commands, receiver) = mpsc::channel();
    let report = Arc::new(Mutex::new(Report::default()));
    let shared = report.clone();
    // The engine is built inside the thread: its output stream can't be sent to one.
    spawn(move || Engine::new(shared).run(receiver));
    with(|s| s.audio = Some(Audio { commands, report }));
}

/// Stops playing and hides the playhead. With `release`, the engine also lets go of the
/// preview, for when the file closes or another opens.
fn halt(app: &AppWindow, release: bool) {
    with(|s| {
        s.serial += 1;
        s.resume = None;
        s.settling = None;
        if release {
            s.rendered = None;
            s.requested = None;
        }
    });
    send(app, Command::Stop { release });
    set_play_state(app, PlayState::Idle);
    if release {
        app.global::<AudioEditor>().set_playback_message("".into());
    }
}

/// Plays the engine's preview, rendered with `edit`, from the source time `at`: `wav` is a
/// freshly rendered one, `None` is the one the engine still holds. `play` false leaves it paused.
fn open_preview(app: &AppWindow, wav: Option<Reserved>, edit: Edit, at: f64, play: bool) {
    ensure_engine();
    let serial = with(|s| {
        s.serial += 1;
        s.rendered = Some(edit);
        s.settling = None;
        s.serial
    });
    let seconds = edit.output_time(at);
    let g = app.global::<AudioEditor>();
    g.set_playback_message("".into());
    g.set_playhead(edit.source_time(seconds) as f32);
    let state = if play {
        PlayState::Playing
    } else {
        PlayState::Paused
    };
    set_play_state(app, state);
    let command = Command::Open {
        wav,
        at: seconds,
        play,
        serial,
    };
    send(app, command);
}

/// Plays from the source time `at`, or loads the preview there paused: straight away when the
/// engine holds one for the current settings, else once one has been rendered.
fn start_at(app: &AppWindow, at: f64, play: bool) {
    let edit = current_edit(app);
    if with(|s| s.rendered) == Some(edit) {
        open_preview(app, None, edit, at, play);
    } else {
        with(|s| s.resume = Some(Resume { at, play }));
        render(app);
    }
}

/// Play, Pause, Replay and the space bar.
fn toggle(app: &AppWindow) {
    let state = with(|s| s.play);
    match state {
        PlayState::Playing => send(app, Command::Pause),
        PlayState::Paused => send(app, Command::Play),
        PlayState::Idle | PlayState::Ended => {
            start_at(app, current_edit(app).start, true);
            return;
        }
    }
    set_play_state(app, state.toggled());
}

/// A click or drag on the waveform: jump to the source time `at`, clamped to the kept stretch.
fn seek(app: &AppWindow, at: f64) {
    let edit = current_edit(app);
    let at = at.max(edit.start).min(edit.end);
    let (state, rendered, rendering) = with(|s| {
        if let Some(resume) = &mut s.resume {
            resume.at = at;
        }
        (s.play, s.rendered, s.preview.is_some())
    });
    match (state, rendered) {
        (PlayState::Playing | PlayState::Paused, Some(basis)) => {
            let seconds = basis.output_time(at);
            send(app, Command::Seek(seconds));
            app.global::<AudioEditor>()
                .set_playhead(basis.source_time(seconds) as f32);
        }
        // The engine's player is spent: load the preview again, paused at the new spot.
        (PlayState::Ended, Some(basis)) => open_preview(app, None, basis, at, false),
        // A drag on a preview still rendering only moves where it will start.
        _ if rendering => {}
        _ => start_at(app, at, true),
    }
}

/// Once a tick: follow the engine, and re-render if the settings have moved.
fn tick(app: &AppWindow) {
    let g = app.global::<AudioEditor>();
    if !matches!(g.get_stage(), AudioEditorStage::Ready) {
        return;
    }
    if app.get_current_slug().as_str() != SLUG {
        // Another tool is showing; whatever was playing stops.
        if with(|s| s.play) != PlayState::Idle {
            halt(app, false);
        }
        return;
    }
    follow(app);
    refresh_settled(app);
}

/// Moves the playhead to where the engine is, and notices the preview ending or failing.
fn follow(app: &AppWindow) {
    let heard = with(|s| {
        if s.play == PlayState::Idle {
            return None;
        }
        let shared = s.audio.as_ref()?.report.clone();
        let mut report = lock(&shared);
        if report.serial != s.serial {
            return None;
        }
        Some((report.position, report.ended, report.error.take(), s.rendered))
    });
    let Some((position, ended, error, rendered)) = heard else {
        return;
    };
    let g = app.global::<AudioEditor>();
    if let Some(message) = error {
        // A failed jump leaves the player running, so stop it rather than only hide it.
        halt(app, false);
        g.set_playback_message(message.into());
        return;
    }
    let Some(edit) = rendered else {
        return;
    };
    if ended {
        let finished = with(|s| s.play).finished();
        set_play_state(app, finished);
        g.set_playhead(edit.end as f32);
    } else {
        g.set_playhead(edit.source_time(position) as f32);
    }
}

/// While the user is listening, re-renders the preview once the settings have held still, so
/// dragging a slider doesn't start a render per step, and carries on from the same spot.
fn refresh_settled(app: &AppWindow) {
    let edit = current_edit(app);
    let due = with(|s| {
        if s.play == PlayState::Idle || s.requested == Some(edit) {
            return false;
        }
        match s.settling {
            Some((seen, since)) if seen == edit => since.elapsed() >= SETTLE,
            _ => {
                s.settling = Some((edit, Instant::now()));
                false
            }
        }
    });
    if due {
        let play = with(|s| s.play) == PlayState::Playing;
        let at = f64::from(app.global::<AudioEditor>().get_playhead());
        with(|s| s.resume = Some(Resume { at, play }));
        render(app);
    }
}

/// Renders the edited stretch to a temporary WAV and hands it to the engine, to play from
/// wherever `resume` says.
fn render(app: &AppWindow) {
    let g = app.global::<AudioEditor>();
    let edit = current_edit(app);
    with(|s| s.requested = Some(edit));
    let prepared = with(|s| Some((s.ffmpeg.clone()?, s.file.clone()?, s.sample_rate)));
    let Some((ff, input, rate)) = prepared else {
        return;
    };
    let graph = match graph(&edit, rate) {
        Ok(graph) => graph,
        Err(e) => {
            with(|s| s.resume = None);
            g.set_playback_message(e.to_string().into());
            return;
        }
    };
    let token = begin(Slot::Preview);
    g.set_previewing(true);
    let weak = app.as_weak();
    spawn(move || {
        let result = decode_wav(&ff, &input, vec!["-af".into(), graph], &token);
        post_current(&weak, Slot::Preview, &token, move |app| {
            let g = app.global::<AudioEditor>();
            g.set_previewing(false);
            let resume = with(|s| {
                s.preview = None;
                s.resume.take()
            });
            match result {
                // The user moved to another tool while it rendered: nothing should start
                // playing, and dropping the file deletes it.
                Ok(_) if app.get_current_slug().as_str() != SLUG => {}
                Ok(wav) => {
                    let resume = resume.unwrap_or(Resume {
                        at: edit.start,
                        play: false,
                    });
                    open_preview(app, Some(wav), edit, resume.at, resume.play);
                }
                Err(JobError::Cancelled) => {}
                Err(JobError::Failed(message)) => {
                    g.set_playback_message(format!("Preview failed: {message}").into());
                }
            }
        });
    });
}

fn set_formats(app: &AppWindow) {
    let labels: Vec<&str> = with(|s| s.formats.iter().map(|f| f.label()).collect());
    app.global::<AudioEditor>()
        .set_formats(select::options(labels.into_iter()));
}

/// Cancels the load, export and preview, stops playback (the engine thread ends and deletes the
/// preview file once its sender is dropped), and hands back the threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        for token in [s.load.take(), s.export.take(), s.preview.take()]
            .into_iter()
            .flatten()
        {
            token.cancel();
        }
        s.ticker.stop();
        s.audio = None;
        std::mem::take(&mut s.workers)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit() -> Edit {
        Edit {
            start: 1.0,
            end: 5.5,
            speed: 1.0,
            pitch: 0.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
        }
    }

    fn graph_text(edit: &Edit, rate: Option<u32>) -> String {
        graph(edit, rate).unwrap().to_string_lossy().into_owned()
    }

    fn tempo_text(speed: f64) -> String {
        let graph = Graph::new().chain(atempo_chain(speed).into_iter().fold(Chain::new(), Chain::then));
        graph.arg().to_string_lossy().into_owned()
    }

    #[test]
    fn an_untouched_edit_only_trims() {
        assert_eq!(
            graph_text(&edit(), None),
            "atrim=start=1.000:end=5.500,asetpts=PTS-STARTPTS"
        );
    }

    #[test]
    fn speeds_within_atempos_range_are_one_filter() {
        assert_eq!(tempo_text(1.25), "atempo=1.250000");
    }

    #[test]
    fn four_times_faster_is_two_doublings() {
        assert_eq!(tempo_text(4.0), "atempo=2.0,atempo=2.000000");
    }

    #[test]
    fn a_quarter_speed_is_two_halvings() {
        assert_eq!(tempo_text(0.25), "atempo=0.5,atempo=0.500000");
    }

    #[test]
    fn three_times_faster_doubles_then_makes_up_the_rest() {
        assert_eq!(tempo_text(3.0), "atempo=2.0,atempo=1.500000");
    }

    #[test]
    fn normal_speed_needs_no_tempo_filter() {
        assert_eq!(tempo_text(1.0), "");
    }

    #[test]
    fn a_pitch_shift_resamples_around_a_tempo_correction() {
        let shifted = Edit {
            pitch: 12.0,
            ..edit()
        };

        assert_eq!(
            graph_text(&shifted, Some(44100)),
            "atrim=start=1.000:end=5.500,asetpts=PTS-STARTPTS,asetrate=88200,atempo=0.500000,aresample=44100"
        );
    }

    #[test]
    fn a_pitch_shift_without_a_sample_rate_is_an_error() {
        let shifted = Edit {
            pitch: 5.0,
            ..edit()
        };

        assert!(graph(&shifted, None).is_err());
    }

    #[test]
    fn the_equalizer_follows_in_the_websites_order() {
        let eq = Edit {
            bass: 3.0,
            mid: -2.5,
            treble: 1.5,
            ..edit()
        };

        assert_eq!(
            graph_text(&eq, None),
            "atrim=start=1.000:end=5.500,asetpts=PTS-STARTPTS,bass=g=3:f=100,equalizer=f=1000:t=q:w=2:g=-2.5,treble=g=1.5:f=8000"
        );
    }

    #[test]
    fn gains_too_small_to_hear_are_left_out() {
        let eq = Edit {
            mid: 0.05,
            ..edit()
        };

        assert_eq!(
            graph_text(&eq, None),
            "atrim=start=1.000:end=5.500,asetpts=PTS-STARTPTS"
        );
    }

    #[test]
    fn output_length_is_the_trimmed_stretch_at_the_new_speed() {
        let faster = Edit {
            speed: 2.0,
            ..edit()
        };

        assert_eq!(faster.output_seconds(), 2.25);
    }

    #[test]
    fn speeds_show_two_decimals() {
        assert_eq!(speed_label(1.0), "1.00×");
    }

    #[test]
    fn the_playhead_starts_at_the_trim_start() {
        assert_eq!(edit().source_time(0.0), 1.0);
    }

    #[test]
    fn the_playhead_moves_at_the_playback_speed() {
        let faster = Edit {
            speed: 2.0,
            ..edit()
        };
        let slower = Edit {
            speed: 0.5,
            ..edit()
        };

        assert_eq!(faster.source_time(1.0), 3.0);
        assert_eq!(slower.source_time(1.0), 1.5);
    }

    #[test]
    fn the_playhead_stops_at_the_trim_end() {
        assert_eq!(edit().source_time(60.0), 5.5);
    }

    #[test]
    fn a_seek_maps_back_through_the_speed() {
        let faster = Edit {
            speed: 2.0,
            ..edit()
        };

        assert_eq!(faster.output_time(3.0), 1.0);
        assert_eq!(faster.source_time(faster.output_time(3.0)), 3.0);
    }

    #[test]
    fn a_seek_outside_the_trim_lands_on_its_nearest_edge() {
        let faster = Edit {
            speed: 2.0,
            ..edit()
        };

        assert_eq!(faster.output_time(0.0), 0.0);
        assert_eq!(faster.output_time(99.0), faster.output_seconds());
    }

    #[test]
    fn play_pauses_and_resumes() {
        assert_eq!(PlayState::Idle.toggled(), PlayState::Playing);
        assert_eq!(PlayState::Playing.toggled(), PlayState::Paused);
        assert_eq!(PlayState::Paused.toggled(), PlayState::Playing);
    }

    #[test]
    fn play_after_the_end_starts_again() {
        assert_eq!(PlayState::Ended.toggled(), PlayState::Playing);
    }

    #[test]
    fn only_a_playing_preview_ends() {
        assert_eq!(PlayState::Playing.finished(), PlayState::Ended);
        assert_eq!(PlayState::Paused.finished(), PlayState::Paused);
        assert_eq!(PlayState::Idle.finished(), PlayState::Idle);
    }

    #[test]
    fn nothing_plays_until_asked() {
        assert_eq!(PlayState::default(), PlayState::Idle);
    }

    // ---- Runs the real FFmpeg; skipped (with a note) when it isn't installed. ----

    use crate::engine::ffmpeg_audio::tests::{fixture, real_ffmpeg};
    use crate::files::reserve_output;
    use std::fs;

    /// Exports `input` with `edit` to `ext` through the allowlisted runner; returns the
    /// output's duration in seconds.
    fn export_seconds(ff: &Ffmpeg, input: &std::path::Path, edit: &Edit, format: Format) -> f64 {
        let rate = sample_rate(ff, input, &CancelToken::default()).unwrap();
        let mut options = vec![OsString::from("-af"), graph(edit, Some(rate)).unwrap()];
        options.extend(ffmpeg_audio::encode_args(format, None, ff.encoders()).unwrap());
        let output = reserve_output(input, None, format.extension()).unwrap();
        ffmpeg_audio::transcode(
            ff,
            input,
            &output,
            Some(edit.output_seconds()),
            options,
            &CancelToken::default(),
            |_| {},
        )
        .unwrap();
        let info = ffmpeg_audio::probe(ff, &output.keep().unwrap()).unwrap();
        info.duration.unwrap()
    }

    #[test]
    fn the_sample_rate_is_read_from_a_real_file() {
        let Some((ff, dir)) = real_ffmpeg("editor-rate") else {
            return;
        };
        let input = fixture(&dir, "tone.mp3", "sine=duration=1:sample_rate=22050", "");

        let rate = sample_rate(&ff, &input, &CancelToken::default()).unwrap();

        assert_eq!(rate, 22050);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The website's bug, fixed: 2× speed over 1.0–5.5 s keeps 4.5 s of source, so 2.25 s.
    #[test]
    fn a_trim_at_double_speed_keeps_the_chosen_stretch() {
        let Some((ff, dir)) = real_ffmpeg("editor-trim") else {
            return;
        };
        let input = fixture(&dir, "tone.wav", "sine=duration=8", "");
        let faster = Edit {
            speed: 2.0,
            ..edit()
        };

        let seconds = export_seconds(&ff, &input, &faster, Format::Wav);

        assert!((seconds - 2.25).abs() < 0.02, "{seconds}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_effect_together_exports_to_mp3() {
        let Some((ff, dir)) = real_ffmpeg("editor-all") else {
            return;
        };
        let input = fixture(&dir, "tone.flac", "sine=duration=8", "");
        let everything = Edit {
            speed: 0.5,
            pitch: -7.0,
            bass: 6.0,
            mid: -3.0,
            treble: 4.5,
            ..edit()
        };

        let seconds = export_seconds(&ff, &input, &everything, Format::Mp3);

        assert!((seconds - 9.0).abs() < 0.1, "{seconds}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
