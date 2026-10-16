//! SVG Optimizer: SVGO's job done by oxvg_optimiser, configured with the website's own SVGO
//! plugin list (src/app/(tools)/svg-optimizer/page.tsx) and repeated until the output stops
//! shrinking, as SVGO's `multipass` does. oxvg is not a byte-for-byte SVGO port, so output can
//! differ from the website's. Paste mode optimises as you type, with rendered before/after
//! previews; Files mode batch-optimises to `name.min.svg`. All optimising and rendering runs
//! off the UI thread.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use oxvg_ast::parse::roxmltree::{ParsingOptions, parse_with_options};
use oxvg_ast::serialize::Node as _;
use oxvg_ast::visitor::Info;
use oxvg_optimiser::Jobs;
use serde_json::{Value, json};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::engine::svg::{self, Latest};
use crate::file_queue::{self, FileQueue, Summary, Update};
use crate::files::{self, format_size, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::{AppWindow, FileStatus, SvgOptimizer, SvgOptimizerFormat, SvgOptimizerMode, clipboard};

pub const SLUG: &str = "svg-optimizer";
const SVG_EXTENSIONS: &[&str] = &["svg"];
/// SVGO's `multipass` limit.
const MAX_PASSES: usize = 10;
/// Typing pauses this long before the SVG is optimised again.
const DEBOUNCE: Duration = Duration::from_millis(200);
/// The editor's own limit (`TextPane`); a longer file goes to Files mode instead.
const EDITOR_CHARACTERS: usize = 100_000;
const CONCURRENT_FILES: usize = 2;

// ---- Logic --------------------------------------------------------------------------------

/// The website's controls, with its defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    /// Decimal places for numbers and path data, 0 to 5.
    precision: u8,
    comments: bool,
    /// Metadata, title, description and the XML declaration.
    metadata: bool,
    /// Empty attributes and empty groups.
    empty: bool,
    collapse_groups: bool,
    editor_namespaces: bool,
    styles_to_attributes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            precision: 3,
            comments: true,
            metadata: true,
            empty: true,
            collapse_groups: true,
            editor_namespaces: true,
            styles_to_attributes: true,
        }
    }
}

impl Settings {
    /// The website's SVGO `plugins`, in its order. SVGO takes preset-default as an object with
    /// `overrides`; oxvg takes it as a name, so its two overrides follow as their own entries
    /// (later entries win). Its third override, `transformGroupAnimations: false`, names no
    /// SVGO plugin, so there is nothing to carry over.
    fn plugins(&self) -> Vec<Value> {
        let mut plugins = Vec::new();
        let mut add = |on: bool, names: &[&str]| {
            if on {
                plugins.extend(names.iter().map(|n| json!(n)));
            }
        };
        add(self.comments, &["removeComments"]);
        add(
            self.metadata,
            &["removeMetadata", "removeTitle", "removeDesc", "removeXMLProcInst"],
        );
        add(self.empty, &["removeEmptyAttrs", "removeEmptyContainers"]);
        add(self.collapse_groups, &["collapseGroups"]);
        add(self.editor_namespaces, &["removeEditorsNSData"]);
        add(self.styles_to_attributes, &["convertStyleToAttrs"]);
        add(
            true,
            &["cleanupIds", "minifyStyles", "removeUnknownsAndDefaults", "preset-default"],
        );
        let precision = json!({ "floatPrecision": self.precision });
        plugins.push(json!({ "name": "cleanupNumericValues", "params": precision }));
        plugins.push(json!({ "name": "convertPathData", "params": precision }));
        plugins
    }

    /// The website's toggles never switched anything off: preset-default re-adds every job
    /// they name. Here a toggle that is off removes its jobs, so it does what it says.
    fn omitted(&self) -> Vec<&'static str> {
        let toggles: [(bool, &[&'static str]); 5] = [
            (self.comments, &["remove_comments"]),
            (
                self.metadata,
                &["remove_metadata", "remove_title", "remove_desc", "remove_x_m_l_proc_inst"],
            ),
            (self.empty, &["remove_empty_attrs", "remove_empty_containers"]),
            (self.collapse_groups, &["collapse_groups"]),
            (self.editor_namespaces, &["remove_editors_n_s_data"]),
        ];
        toggles
            .into_iter()
            .filter(|(on, _)| !on)
            .flat_map(|(_, jobs)| jobs.iter().copied())
            .collect()
    }

    fn jobs(&self) -> Result<Jobs, String> {
        let mut jobs = Jobs::from_svgo_plugin_config(Some(self.plugins()))
            .map_err(|e| format!("The optimizer settings were refused ({e})"))?;
        for job in self.omitted() {
            jobs.omit(job);
        }
        Ok(jobs)
    }
}

/// Optimises `svg`, rerunning the jobs on their own output until it stops getting smaller
/// (at most `MAX_PASSES` runs). The error is a sentence for the person.
fn optimize(svg: &str, settings: &Settings) -> Result<String, String> {
    if svg.len() > svg::MAX_SVG_BYTES {
        return Err(format!(
            "This SVG is over the {} limit.",
            format_size(svg::MAX_SVG_BYTES as u64)
        ));
    }
    let jobs = settings.jobs()?;
    let mut best = pass(svg, &jobs)?;
    for _ in 1..MAX_PASSES {
        let next = pass(&best, &jobs)?;
        if next.len() >= best.len() {
            break;
        }
        best = next;
    }
    Ok(best)
}

fn pass(svg: &str, jobs: &Jobs) -> Result<String, String> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    parse_with_options(svg, options, |dom, allocator| {
        jobs.run(dom, &Info::new(allocator))
            .map_err(|e| format!("This SVG couldn't be optimized ({e})."))?;
        dom.serialize()
            .map_err(|e| format!("The optimized SVG couldn't be written ({e})."))
    })
    .map_err(|e| format!("This isn't valid SVG markup ({e})."))?
}

/// The attributes React spells in camelCase: the website's 19, then the three it missed that
/// make invalid JSX (`xlink:href`, `xmlns:xlink`, `xml:space`).
const JSX_ATTRIBUTES: &[(&str, &str)] = &[
    ("stroke-width", "strokeWidth"),
    ("stroke-linecap", "strokeLinecap"),
    ("stroke-linejoin", "strokeLinejoin"),
    ("stroke-miterlimit", "strokeMiterlimit"),
    ("fill-rule", "fillRule"),
    ("clip-rule", "clipRule"),
    ("fill-opacity", "fillOpacity"),
    ("stroke-opacity", "strokeOpacity"),
    ("stroke-dasharray", "strokeDasharray"),
    ("stroke-dashoffset", "strokeDashoffset"),
    ("font-family", "fontFamily"),
    ("font-size", "fontSize"),
    ("font-weight", "fontWeight"),
    ("text-anchor", "textAnchor"),
    ("alignment-baseline", "alignmentBaseline"),
    ("clip-path", "clipPath"),
    ("stop-color", "stopColor"),
    ("stop-opacity", "stopOpacity"),
    ("vector-effect", "vectorEffect"),
    ("xlink:href", "xlinkHref"),
    ("xmlns:xlink", "xmlnsXlink"),
    ("xml:space", "xmlSpace"),
];

/// The website's React component wrapper: the same text replacements (not a parser), then
/// `{...props}` on the root and the function around it.
fn to_jsx(svg: &str) -> String {
    let mut jsx = svg.replace("class=", "className=").replace("for=", "htmlFor=");
    for (kebab, camel) in JSX_ATTRIBUTES {
        jsx = replace_ignore_ascii_case(&jsx, &format!("{kebab}="), &format!("{camel}="));
    }
    if let Some(start) = jsx.find("<svg") {
        let rest = &jsx[start + 4..];
        let end = start + 4 + (rest.len() - rest.trim_start().len());
        jsx.replace_range(start..end, "<svg {...props} ");
    }
    format!(
        "export default function SvgIcon(props) {{\n  return (\n    {}\n  );\n}}",
        jsx.trim().replace('\n', "\n    ")
    )
}

/// Replaces every ASCII-case-insensitive match of `from` (ASCII) with `to`.
fn replace_ignore_ascii_case(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(from) {
        out.push_str(&text[at..at + found]);
        out.push_str(to);
        at += found + from.len();
    }
    out.push_str(&text[at..]);
    out
}

/// "42% smaller", "No change" or "12% larger", rounded as the website rounded it.
fn savings(before: u64, after: u64) -> String {
    if before == 0 {
        return String::new();
    }
    let percent = ((before as f64 - after as f64) / before as f64 * 100.0).round() as i64;
    match percent {
        0 => "No change".into(),
        p if p > 0 => format!("{p}% smaller"),
        p => format!("{}% larger", -p),
    }
}

// ---- State and wiring ---------------------------------------------------------------------

/// A queued file: nothing to show before it runs.
#[derive(Default)]
struct Row;

impl Summary for Row {
    fn summary(&self) -> String {
        String::new()
    }
}

/// The optimised editor text, in both formats.
struct Optimized {
    svg: String,
    jsx: String,
}

#[derive(Default)]
struct State {
    output: Option<Optimized>,
    /// The file the editor text came from, for the Save name.
    source: Option<PathBuf>,
    /// Each preview's size in logical pixels, from the view.
    canvas: (f32, f32),
    debounce: Timer,
    runs: Latest,
    queue: FileQueue<Row>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn init(app: &AppWindow) {
    let g = app.global::<SvgOptimizer>();
    with(|s| g.set_files(s.queue.rows()));
    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            schedule(&app);
        }
    });
    let weak = app.as_weak();
    g.on_format_changed(move || {
        if let Some(app) = weak.upgrade() {
            show_output(&app);
        }
    });
    let weak = app.as_weak();
    g.on_open_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose an SVG", Some(("SVG", SVG_EXTENSIONS))) {
            open_in_editor(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.source = None);
        app.global::<SvgOptimizer>().set_input("".into());
        optimize_input(&app);
    });
    let weak = app.as_weak();
    g.on_copy_output(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let jsx = app.global::<SvgOptimizer>().get_format() == SvgOptimizerFormat::Jsx;
        let result = with(|s| {
            s.output
                .as_ref()
                .map(|o| clipboard::copy_text(if jsx { &o.jsx } else { &o.svg }))
        });
        match result {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                app.global::<SvgOptimizer>()
                    .set_error(format!("Couldn't copy: {e}").into());
                false
            }
            None => false,
        }
    });
    let weak = app.as_weak();
    g.on_save_output(move || {
        if let Some(app) = weak.upgrade() {
            save_output(&app);
        }
    });
    let weak = app.as_weak();
    g.on_preview_resized(move |width, height| {
        let Some(app) = weak.upgrade() else { return };
        let changed = with(|s| std::mem::replace(&mut s.canvas, (width, height)) != (width, height));
        if changed {
            optimize_input(&app);
        }
    });

    // Files mode, as Video Converter.
    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose SVGs", Some(("SVG", SVG_EXTENSIONS))) {
            add_to_queue(&app, paths);
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
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Optimized SVGs To") {
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
    g.on_optimize_all(move || {
        if let Some(app) = weak.upgrade() {
            optimize_all(&app);
        }
    });
    sync(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<SvgOptimizer>().set_drop_hover(hover);
}

/// In Paste mode one dropped SVG opens in the editor; several go to Files mode, as does any
/// drop while Files mode shows.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let g = app.global::<SvgOptimizer>();
    if g.get_mode() == SvgOptimizerMode::Paste {
        match <[PathBuf; 1]>::try_from(paths) {
            Ok([path]) => open_in_editor(app, path),
            Err(paths) if paths.is_empty() => {}
            Err(paths) => {
                g.set_mode(SvgOptimizerMode::Files);
                add_to_queue(app, paths);
            }
        }
    } else {
        add_to_queue(app, paths);
    }
}

/// Cancels the batch and any optimisation of the editor text, and hands back their threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        s.debounce.stop();
        let mut workers = s.queue.shutdown();
        workers.extend(s.runs.shutdown());
        workers
    })
}

fn settings(app: &AppWindow) -> Settings {
    let g = app.global::<SvgOptimizer>();
    Settings {
        precision: g.get_precision().round().clamp(0.0, 5.0) as u8,
        comments: g.get_remove_comments(),
        metadata: g.get_remove_metadata(),
        empty: g.get_remove_empty(),
        collapse_groups: g.get_collapse_groups(),
        editor_namespaces: g.get_remove_editor_data(),
        styles_to_attributes: g.get_styles_to_attributes(),
    }
}

fn is_svg(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// Puts a small SVG's text in the editor; one too long to edit goes to Files mode.
fn open_in_editor(app: &AppWindow, path: PathBuf) {
    let g = app.global::<SvgOptimizer>();
    let name = files::file_name(&path);
    if !is_svg(&path) {
        g.set_error(format!("“{name}” isn't an SVG file. Choose an .svg file.").into());
        return;
    }
    let size = match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(_) => return g.set_error(format!("{name} is a folder. Choose a file.").into()),
        Err(e) => return g.set_error(format!("Couldn't open {name}: {e}").into()),
    };
    // Four bytes per character at most, so a file this small always fits the editor.
    let text = (size <= (EDITOR_CHARACTERS * 4) as u64)
        .then(|| std::fs::read_to_string(&path))
        .transpose();
    match text {
        Ok(Some(text)) if text.chars().count() <= EDITOR_CHARACTERS => {
            with(|s| s.source = Some(path));
            g.set_input(text.into());
            optimize_input(app);
        }
        Ok(_) => {
            g.set_mode(SvgOptimizerMode::Files);
            add_to_queue(app, vec![path]);
            g.set_note(format!("{name} is too long to edit here, so it was added to Files.").into());
        }
        Err(e) => g.set_error(format!("Couldn't read {name}: {e}").into()),
    }
}

/// Optimises the editor text once typing pauses.
fn schedule(app: &AppWindow) {
    let weak = app.as_weak();
    with(|s| {
        s.debounce.start(TimerMode::SingleShot, DEBOUNCE, move || {
            if let Some(app) = weak.upgrade() {
                optimize_input(&app);
            }
        })
    });
}

/// What one run of the editor text produced, for the UI thread.
struct Run {
    output: Result<Optimized, String>,
    before: Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
    after: Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
    before_size: u64,
    after_size: u64,
}

fn optimize_input(app: &AppWindow) {
    let g = app.global::<SvgOptimizer>();
    let input = g.get_input().to_string();
    let settings = settings(app);
    let scale = app.window().scale_factor();
    if input.trim().is_empty() {
        with(|s| {
            s.output = None;
            s.runs.ticket();
        });
        g.set_before(slint::Image::default());
        g.set_after(slint::Image::default());
        g.set_before_size("".into());
        g.set_after_size("".into());
        g.set_savings("".into());
        g.set_error("".into());
        show_output(app);
        return;
    }
    g.set_busy(true);
    let weak = app.as_weak();
    with(|s| {
        let (width, height) = s.canvas;
        s.runs.spawn(move |ticket| {
            if !ticket.is_current() {
                return;
            }
            let output = optimize(&input, &settings).map(|svg| Optimized {
                jsx: to_jsx(&svg),
                svg,
            });
            let preview = |text: &str| {
                let tree = svg::parse(text).ok()?;
                let fit = svg::fit_scale(&tree, width, height);
                svg::render_on_checker(&tree, fit * scale, scale).map(|p| svg::pixels(&p))
            };
            let run = Run {
                before: preview(&input),
                after: output.as_ref().ok().and_then(|o| preview(&o.svg)),
                before_size: input.len() as u64,
                after_size: output.as_ref().map_or(0, |o| o.svg.len() as u64),
                output,
            };
            jobs::post(&weak, move |app| {
                if ticket.is_current() {
                    show_run(&app, run);
                }
            });
        });
    });
}

fn show_run(app: &AppWindow, run: Run) {
    let g = app.global::<SvgOptimizer>();
    g.set_busy(false);
    let image = |pixels: Option<_>| {
        pixels
            .map(slint::Image::from_rgba8_premultiplied)
            .unwrap_or_default()
    };
    g.set_before(image(run.before));
    g.set_before_size(format_size(run.before_size).into());
    match run.output {
        Ok(output) => {
            g.set_after(image(run.after));
            g.set_after_size(format_size(run.after_size).into());
            g.set_savings(savings(run.before_size, run.after_size).into());
            g.set_larger(run.after_size > run.before_size);
            g.set_error("".into());
            with(|s| s.output = Some(output));
        }
        Err(message) => {
            g.set_after(slint::Image::default());
            g.set_after_size("".into());
            g.set_savings("".into());
            g.set_error(message.into());
            with(|s| s.output = None);
        }
    }
    show_output(app);
}

/// Shows the chosen format of the output (its first part, when it's long).
fn show_output(app: &AppWindow) {
    let g = app.global::<SvgOptimizer>();
    let jsx = g.get_format() == SvgOptimizerFormat::Jsx;
    let (text, note) = with(|s| match &s.output {
        Some(o) => {
            let full = if jsx { &o.jsx } else { &o.svg };
            (svg::preview(full).to_owned(), svg::preview_note(full))
        }
        None => (String::new(), String::new()),
    });
    g.set_can_copy(!text.is_empty());
    g.set_output(text.into());
    g.set_output_note(note.into());
}

fn save_output(app: &AppWindow) {
    let g = app.global::<SvgOptimizer>();
    let jsx = g.get_format() == SvgOptimizerFormat::Jsx;
    let Some(suggested) = with(|s| {
        s.output.as_ref()?;
        Some(match (&s.source, jsx) {
            (_, true) => "SvgIcon.jsx".to_string(),
            (Some(source), false) => format!(
                "{}.min.svg",
                source.file_stem().unwrap_or_default().to_string_lossy()
            ),
            (None, false) => "optimized.svg".into(),
        })
    }) else {
        return;
    };
    let title = if jsx { "Save JSX" } else { "Save Optimized SVG" };
    let Some(path) = files::choose_save_path(title, &suggested) else {
        return;
    };
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        with(|s| match &s.output {
            Some(o) => std::fs::write(output.path(), if jsx { &o.jsx } else { &o.svg }),
            None => Ok(()),
        })?;
        output.keep()
    });
    let name = files::file_name(&path);
    match result {
        Ok(_) => {
            g.set_error("".into());
            g.set_note(format!("Saved as {name}").into());
        }
        Err(e) => g.set_error(format!("Couldn't save {name}: {e}").into()),
    }
}

/// Lists the SVGs among `paths` for Files mode, and says how many others were skipped.
fn add_to_queue(app: &AppWindow, paths: Vec<PathBuf>) {
    let (svgs, others): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| is_svg(p));
    with(|s| s.queue.add(svgs, FileStatus::Ready, Row::default));
    let g = app.global::<SvgOptimizer>();
    match others.len() {
        0 => g.set_note("".into()),
        1 => g.set_note(
            format!("Skipped {}: it isn't an SVG.", files::file_name(&others[0])).into(),
        ),
        n => g.set_note(format!("Skipped {n} files that aren't SVGs.").into()),
    }
    sync(app);
}

fn optimize_all(app: &AppWindow) {
    let settings = settings(app);
    let (jobs, out_dir) = with(|s| {
        (
            s.queue.start(|_| Some(())),
            s.queue.out_dir().map(PathBuf::from),
        )
    });
    sync(app);
    let workers = file_queue::run(app, jobs, CONCURRENT_FILES, apply, move |job, progress| {
        let name = files::file_name(&job.input);
        let size = std::fs::metadata(&job.input).map_err(JobError::failed)?.len();
        if size > svg::MAX_SVG_BYTES as u64 {
            return Err(JobError::Failed(format!(
                "{} is over the {} limit.",
                format_size(size),
                format_size(svg::MAX_SVG_BYTES as u64)
            )));
        }
        let text = std::fs::read_to_string(&job.input)
            .map_err(|e| JobError::Failed(format!("Couldn't read {name}: {e}")))?;
        if job.cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let optimized = optimize(&text, &settings).map_err(JobError::Failed)?;
        progress(0.9);
        if job.cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let output = reserve_output(&job.input, out_dir.as_deref(), "min.svg")
            .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
        std::fs::write(output.path(), optimized)
            .map_err(|e| JobError::Failed(format!("Couldn't write the output file: {e}")))?;
        Ok(output)
    });
    with(|s| s.queue.track(workers));
}

/// Applies a worker's update; a finished file's row also says how much it saved.
fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    let saved = matches!(update, Update::Finished(Ok(_)));
    with(|s| {
        s.queue.apply(id, token, update);
        if saved {
            s.queue.update(id, |e| {
                if let Some(output) = &e.output {
                    let after = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
                    e.message = format!(
                        "Saved as {} · {} → {} · {}",
                        files::file_name(output),
                        format_size(e.size),
                        format_size(after),
                        savings(e.size, after)
                    );
                }
            });
        }
    });
    sync(app);
}

fn sync(app: &AppWindow) {
    let g = app.global::<SvgOptimizer>();
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        g.set_running(running);
        g.set_can_optimize(!running && q.can_start(|_| true));
        g.set_list_summary(q.count_label("SVG", "SVGs").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const INKSCAPE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Created with Inkscape -->
<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="100" height="100" viewBox="0 0 100 100" inkscape:version="1.3">
  <metadata>made by hand</metadata>
  <title>Square</title>
  <g>
    <g>
      <rect x="10.123456" y="10.000000" width="50.000000" height="50.000000" style="fill:#ff0000;stroke:#000000;stroke-width:2"/>
    </g>
  </g>
</svg>
"#;

    #[test]
    fn the_website_plugin_list_is_accepted() {
        let jobs = Settings::default().jobs();

        assert!(jobs.is_ok(), "{:?}", jobs.err());
    }

    #[test]
    fn plugins_follow_the_website_order_with_its_overrides() {
        let settings = Settings {
            precision: 2,
            ..Settings::default()
        };

        assert_eq!(
            Value::Array(settings.plugins()).to_string(),
            r#"["removeComments","removeMetadata","removeTitle","removeDesc","removeXMLProcInst","removeEmptyAttrs","removeEmptyContainers","collapseGroups","removeEditorsNSData","convertStyleToAttrs","cleanupIds","minifyStyles","removeUnknownsAndDefaults","preset-default",{"name":"cleanupNumericValues","params":{"floatPrecision":2}},{"name":"convertPathData","params":{"floatPrecision":2}}]"#
        );
    }

    #[test]
    fn optimizing_shrinks_the_file_and_keeps_the_view_box() {
        let optimized = optimize(INKSCAPE, &Settings::default()).unwrap();

        assert!(optimized.len() < INKSCAPE.len() / 2, "{optimized}");
        assert!(optimized.contains(r#"viewBox="0 0 100 100""#), "{optimized}");
    }

    #[test]
    fn optimizing_strips_comments_metadata_and_editor_data() {
        let optimized = optimize(INKSCAPE, &Settings::default()).unwrap();

        assert!(!optimized.contains("<!--"), "{optimized}");
        assert!(!optimized.contains("metadata"), "{optimized}");
        assert!(!optimized.contains("inkscape"), "{optimized}");
    }

    #[test]
    fn a_comment_survives_when_its_toggle_is_off() {
        let settings = Settings {
            comments: false,
            ..Settings::default()
        };

        let optimized = optimize(INKSCAPE, &settings).unwrap();

        assert!(optimized.contains("<!--Created with Inkscape-->"), "{optimized}");
    }

    #[test]
    fn the_title_survives_when_metadata_removal_is_off() {
        let settings = Settings {
            metadata: false,
            ..Settings::default()
        };

        let optimized = optimize(INKSCAPE, &settings).unwrap();

        assert!(optimized.contains("<title>Square</title>"), "{optimized}");
    }

    #[test]
    fn precision_rounds_coordinates() {
        let settings = Settings {
            precision: 1,
            ..Settings::default()
        };

        let optimized = optimize(INKSCAPE, &settings).unwrap();

        assert!(!optimized.contains("10.12"), "{optimized}");
        assert!(optimized.contains("10.1"), "{optimized}");
    }

    #[test]
    fn a_second_run_gains_nothing() {
        let once = optimize(INKSCAPE, &Settings::default()).unwrap();

        let twice = optimize(&once, &Settings::default()).unwrap();

        assert_eq!(twice.len(), once.len());
    }

    #[test]
    fn broken_markup_is_reported() {
        let error = optimize("<svg><g></svg>", &Settings::default()).unwrap_err();

        assert!(error.starts_with("This isn't valid SVG markup ("), "{error}");
    }

    #[test]
    fn jsx_matches_the_website_wrapper() {
        let svg = r#"<svg viewBox="0 0 2 2" class="icon"><path stroke-width="2" Fill-Rule="evenodd" d="M0 0"/></svg>"#;

        assert_eq!(
            to_jsx(svg),
            "export default function SvgIcon(props) {\n  return (\n    <svg {...props} viewBox=\"0 0 2 2\" className=\"icon\"><path strokeWidth=\"2\" fillRule=\"evenodd\" d=\"M0 0\"/></svg>\n  );\n}"
        );
    }

    #[test]
    fn jsx_indents_every_line_and_camel_cases_xlink() {
        let svg = "<svg xmlns:xlink=\"x\">\n<use xlink:href=\"#a\"/>\n</svg>";

        assert_eq!(
            to_jsx(svg),
            "export default function SvgIcon(props) {\n  return (\n    <svg {...props} xmlnsXlink=\"x\">\n    <use xlinkHref=\"#a\"/>\n    </svg>\n  );\n}"
        );
    }

    #[test]
    fn savings_read_like_the_website_badge() {
        assert_eq!(
            [savings(100, 58), savings(100, 100), savings(100, 112), savings(0, 0)],
            ["42% smaller", "No change", "12% larger", ""]
        );
    }
}
