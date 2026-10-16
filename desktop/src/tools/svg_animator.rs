//! SVG Border Animator: draws each `<path>`'s outline in, one after another or all at once.
//! The exported markup is the website's (src/app/(tools)/svg-animator/page.tsx): every path
//! gets the stroke attributes and a SMIL `<animate>` on `stroke-dashoffset`, with the same
//! attribute order, number formatting, timing and easing. Slint can't play SMIL, so the preview
//! re-renders a frame on every timer tick with each path's offset computed here exactly as SMIL
//! computes it. Frames render off the UI thread; the timer stops when the animation finishes,
//! is paused, or the tool is no longer showing, and resumes when it shows again.
//!
//! The file is untrusted and its markup leaves the app, so it is re-serialised (as the
//! website's XMLSerializer did) without scripts, embedded documents, event handlers and links
//! to anything but its own elements, before anything is measured, shown or exported. The
//! website ran DOMPurify for the same reason.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use resvg::usvg::roxmltree::{self, Node, NodeType};
use resvg::usvg::tiny_skia_path::{PathSegment, Point};
use resvg::usvg::{self, Node as TreeNode};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::engine::svg::{self, Latest};
use crate::{AppWindow, SvgAnimator, SvgAnimatorMode, clipboard, files, jobs};

pub const SLUG: &str = "svg-animator";
const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const DEFAULT_STROKE: &str = "#000000";
const SVG_EXTENSIONS: &[&str] = &["svg"];
/// About 60 frames a second. A frame still rendering at the next tick isn't doubled up.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// Space kept around the drawing at 100% zoom, in logical pixels.
const PREVIEW_INSET: f32 = 24.0;
/// The only elements that survive, compared lower-cased: shapes, containers, gradients and
/// text. Anything else (script, style, foreignObject, SMIL, unknown) goes with its subtree.
const ALLOWED_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "defs",
    "lineargradient",
    "radialgradient",
    "stop",
    "clippath",
    "mask",
    "use",
    "title",
    "desc",
    "a",
    "text",
    "tspan",
    "symbol",
];
/// The only attributes that survive, compared lower-cased: geometry and presentation. No
/// `style`, no `on*`, no `data-*`; `href` is checked for a `#` target on top of this.
const ALLOWED_ATTRIBUTES: &[&str] = &[
    "id",
    "class",
    "d",
    "x",
    "y",
    "x1",
    "x2",
    "y1",
    "y2",
    "cx",
    "cy",
    "fx",
    "fy",
    "r",
    "rx",
    "ry",
    "width",
    "height",
    "points",
    "viewbox",
    "preserveaspectratio",
    "version",
    "transform",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "opacity",
    "color",
    "display",
    "visibility",
    "clip-path",
    "clip-rule",
    "mask",
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientunits",
    "gradienttransform",
    "spreadmethod",
    "clippathunits",
    "maskunits",
    "maskcontentunits",
    "font-family",
    "font-size",
    "font-weight",
    "text-anchor",
    "href",
];
/// The stroke attributes each path gets, so its own are removed first.
const STROKE_ATTRIBUTES: &[&str] = &[
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
];
// ---- Timing -------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Sequential,
    Simultaneous,
}

/// When each path draws: `per_path` seconds each (1 / speed), one after another or together.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Timing {
    per_path: f64,
    mode: Mode,
}

impl Timing {
    fn new(speed: f64, mode: Mode) -> Self {
        Self {
            per_path: 1.0 / speed,
            mode,
        }
    }

    /// The path's `begin`, in seconds.
    fn delay(self, i: usize) -> f64 {
        match self.mode {
            Mode::Sequential => i as f64 * self.per_path,
            Mode::Simultaneous => 0.0,
        }
    }

    fn total(self, paths: usize) -> f64 {
        match self.mode {
            Mode::Sequential => paths as f64 * self.per_path,
            Mode::Simultaneous => self.per_path,
        }
    }

    /// The `stroke-dashoffset` SMIL gives path `i` (of `length`) `t` seconds in: `length`
    /// (hidden) before it begins, eased down to 0 (drawn), then frozen there.
    fn offset(self, length: f64, i: usize, t: f64) -> f64 {
        let progress = ((t - self.delay(i)) / self.per_path).clamp(0.0, 1.0);
        length * (1.0 - ease(progress))
    }
}

/// The animation's one `keySplines`, cubic-bezier(0.25, 0.1, 0.25, 1): how much of the stroke
/// is drawn once a fraction `x` of its time has passed.
fn ease(x: f64) -> f64 {
    const X1: f64 = 0.25;
    const Y1: f64 = 0.1;
    const X2: f64 = 0.25;
    const Y2: f64 = 1.0;
    let bezier = |s: f64, a: f64, b: f64| {
        3.0 * (1.0 - s) * (1.0 - s) * s * a + 3.0 * (1.0 - s) * s * s * b + s * s * s
    };
    let x = x.clamp(0.0, 1.0);
    // x(s) rises monotonically (both x control points lie in 0..1), so bisection finds s.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if bezier(mid, X1, X2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier((lo + hi) / 2.0, Y1, Y2)
}

/// JavaScript's `String(number)`: Rust's shortest round-trip form agrees for every value
/// these sliders produce (no exponents below 1e21).
fn js_number(n: f64) -> String {
    if n == 0.0 { "0".into() } else { n.to_string() }
}

// ---- Path length --------------------------------------------------------------------------

/// `getTotalLength()` of a path's `d`, rounded to 3 places as the website did; 1 when it has
/// no length or can't be read. usvg parses the data (arcs become cubics) and the segments are
/// measured here.
fn path_length(d: &str) -> f64 {
    let doc = format!(
        r##"<svg xmlns="{SVG_NS}" width="1" height="1"><path d="{}" fill="none" stroke="#000"/></svg>"##,
        escape_attribute(d)
    );
    let Ok(tree) = usvg::Tree::from_str(&doc, &svg::options()) else {
        return 1.0;
    };
    let total: f64 = tree
        .root()
        .children()
        .iter()
        .map(|node| match node {
            TreeNode::Path(path) => segments_length(path.data().segments()),
            _ => 0.0,
        })
        .sum();
    let rounded = (total * 1000.0).round() / 1000.0;
    if rounded.is_finite() && rounded > 0.0 {
        rounded
    } else {
        1.0
    }
}

fn segments_length(segments: impl Iterator<Item = PathSegment>) -> f64 {
    let (mut length, mut start, mut at) = (0.0, Point::zero(), Point::zero());
    for segment in segments {
        match segment {
            PathSegment::MoveTo(p) => (start, at) = (p, p),
            PathSegment::LineTo(p) => {
                length += distance(at, p);
                at = p;
            }
            PathSegment::QuadTo(c, p) => {
                // The same curve as a cubic: controls two thirds of the way to `c`.
                let c1 = lerp(at, c, 2.0 / 3.0);
                let c2 = lerp(p, c, 2.0 / 3.0);
                length += cubic_length(at, c1, c2, p, 0);
                at = p;
            }
            PathSegment::CubicTo(c1, c2, p) => {
                length += cubic_length(at, c1, c2, p, 0);
                at = p;
            }
            PathSegment::Close => {
                length += distance(at, start);
                at = start;
            }
        }
    }
    length
}

/// Splits the curve in half until its control polygon is as long as its chord.
fn cubic_length(p0: Point, p1: Point, p2: Point, p3: Point, depth: u32) -> f64 {
    let chord = distance(p0, p3);
    let polygon = distance(p0, p1) + distance(p1, p2) + distance(p2, p3);
    if polygon - chord < 1e-6 || depth >= 20 {
        return (chord + polygon) / 2.0;
    }
    let (a, b, c) = (lerp(p0, p1, 0.5), lerp(p1, p2, 0.5), lerp(p2, p3, 0.5));
    let (d, e) = (lerp(a, b, 0.5), lerp(b, c, 0.5));
    let mid = lerp(d, e, 0.5);
    cubic_length(p0, a, d, mid, depth + 1) + cubic_length(mid, e, c, p3, depth + 1)
}

fn distance(a: Point, b: Point) -> f64 {
    f64::from(a.x - b.x).hypot(f64::from(a.y - b.y))
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point::from_xy(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

// ---- Markup -------------------------------------------------------------------------------

/// The re-serialised file, with gaps for what the controls decide.
#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    /// Path `i`'s stroke attributes (and `fill="none"` when it had no fill and Fill is off).
    Stroke(usize),
    /// The value of path `i`'s own `fill`: kept, or "none" when Fill is off.
    Fill(usize),
    /// Path `i`'s `<animate>`, in the exported markup only.
    Animate(usize),
}

#[derive(Debug, PartialEq)]
struct PathInfo {
    length: f64,
    /// Its `fill` attribute as written (escaped), if it had one.
    fill: Option<String>,
}

#[derive(Debug, PartialEq)]
struct Template {
    pieces: Vec<Piece>,
    paths: Vec<PathInfo>,
}

/// The stroke the controls describe; `stroke` is a validated `#rrggbb`.
#[derive(Clone, Debug, PartialEq)]
struct Style {
    stroke: String,
    width: f64,
    fill: bool,
}

#[derive(Clone, Copy)]
enum Output {
    /// The export: SMIL that plays wherever the file is opened.
    Smil(Timing),
    /// A still of the animation `t` seconds in, for the preview.
    Frame(Timing, f64),
}

impl Template {
    /// Sanitises and re-serialises `source`, measuring its paths. The errors are the website's.
    fn build(source: &str) -> Result<Self, String> {
        // DTDs stay on: Illustrator exports carry a DOCTYPE and the website accepts them. roxmltree
        // supports only internal entities (no external entity resolution) and caps expansion, so
        // a DOCTYPE can't read files or blow up memory.
        let options = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        };
        let doc = roxmltree::Document::parse_with_options(source, options)
            .map_err(|_| "This SVG could not be parsed.".to_string())?;
        let root = doc.root_element();
        if root.tag_name().name() != "svg" {
            return Err("No <svg> root element was found.".into());
        }
        let mut writer = Writer::default();
        writer.element(root);
        writer.flush();
        if writer.paths.is_empty() {
            return Err("No <path> elements were found in this SVG.".into());
        }
        Ok(Self {
            pieces: writer.pieces,
            paths: writer.paths,
        })
    }

    fn write(&self, style: &Style, output: Output) -> String {
        let mut out = String::new();
        for piece in &self.pieces {
            match *piece {
                Piece::Text(ref text) => out.push_str(text),
                Piece::Stroke(i) => {
                    let path = &self.paths[i];
                    let length = js_number(path.length);
                    let offset = match output {
                        Output::Smil(_) => length.clone(),
                        Output::Frame(timing, t) => js_number(timing.offset(path.length, i, t)),
                    };
                    let _ = write!(
                        out,
                        r#" stroke="{}" stroke-width="{}" stroke-dasharray="{length}" stroke-dashoffset="{offset}""#,
                        style.stroke,
                        js_number(style.width)
                    );
                    if path.fill.is_none() && !style.fill {
                        out.push_str(r#" fill="none""#);
                    }
                }
                Piece::Fill(i) => match &self.paths[i].fill {
                    Some(fill) if style.fill => out.push_str(fill),
                    _ => out.push_str("none"),
                },
                Piece::Animate(i) => {
                    if let Output::Smil(timing) = output {
                        let _ = write!(
                            out,
                            r#"<animate data-svg-animator="true" attributeName="stroke-dashoffset" from="{}" to="0" dur="{}s" begin="{}s" fill="freeze" calcMode="spline" keySplines="0.25 0.1 0.25 1" keyTimes="0;1"/>"#,
                            js_number(self.paths[i].length),
                            js_number(timing.per_path),
                            js_number(timing.delay(i))
                        );
                    }
                }
            }
        }
        out
    }
}

/// Builds a `Template` from the parsed tree, the way XMLSerializer writes a DOM: resolved
/// values re-escaped, double quotes, `<x/>` for an empty element, each namespace declared where
/// it first comes into scope.
#[derive(Default)]
struct Writer {
    pieces: Vec<Piece>,
    text: String,
    paths: Vec<PathInfo>,
}

impl Writer {
    fn piece(&mut self, piece: Piece) {
        self.flush();
        self.pieces.push(piece);
    }

    fn flush(&mut self) {
        if !self.text.is_empty() {
            self.pieces.push(Piece::Text(std::mem::take(&mut self.text)));
        }
    }

    fn node(&mut self, node: Node) {
        match node.node_type() {
            NodeType::Element => self.element(node),
            NodeType::Text => self
                .text
                .push_str(&escape_text(node.text().unwrap_or_default())),
            // Comments and processing instructions can carry `-->` or `<`: never written.
            NodeType::Comment | NodeType::PI => {}
            NodeType::Root => {}
        }
    }

    fn element(&mut self, node: Node) {
        if is_dropped(node) {
            return;
        }
        let name = element_name(node);
        let is_root = node.parent_element().is_none();
        let path = (node.tag_name().name() == "path"
            && node.tag_name().namespace().is_none_or(|ns| ns == SVG_NS))
        .then(|| {
            self.paths.push(PathInfo {
                length: path_length(node.attribute("d").unwrap_or_default()),
                fill: None,
            });
            self.paths.len() - 1
        });

        let _ = write!(self.text, "<{name}");
        let (default, xlink) = self.namespaces(node);
        for attribute in node.attributes() {
            if !is_safe_attribute(&attribute) {
                continue;
            }
            let plain = attribute.namespace().is_none();
            if let Some(i) = path {
                if plain && STROKE_ATTRIBUTES.contains(&attribute.name()) {
                    continue;
                }
                if plain && attribute.name() == "fill" {
                    self.text.push_str(r#" fill=""#);
                    self.piece(Piece::Fill(i));
                    self.text.push('"');
                    self.paths[i].fill = Some(escape_attribute(attribute.value()));
                    continue;
                }
            }
            let _ = write!(
                self.text,
                r#" {}="{}""#,
                attribute_name(node, &attribute),
                escape_attribute(attribute.value())
            );
        }
        // The website set these on the root after its own attributes.
        if is_root && !default {
            let _ = write!(self.text, r#" xmlns="{SVG_NS}""#);
        }
        if is_root && !xlink {
            let _ = write!(self.text, r#" xmlns:xlink="{XLINK_NS}""#);
        }
        if let Some(i) = path {
            self.piece(Piece::Stroke(i));
        }

        let children: Vec<Node> = node
            .children()
            .filter(|c| !c.is_element() || !is_dropped(*c))
            .collect();
        if children.is_empty() && path.is_none() {
            self.text.push_str("/>");
            return;
        }
        self.text.push('>');
        for child in children {
            self.node(child);
        }
        if let Some(i) = path {
            self.piece(Piece::Animate(i));
        }
        let _ = write!(self.text, "</{name}>");
    }

    /// Declares the namespaces that come into scope on `node`, and says whether a default and
    /// an `xlink` namespace are in scope.
    fn namespaces(&mut self, node: Node) -> (bool, bool) {
        let inherited = |name: Option<&str>, uri: &str| {
            node.parent_element()
                .is_some_and(|p| p.namespaces().any(|ns| ns.name() == name && ns.uri() == uri))
        };
        let mut default = false;
        let mut xlink = false;
        for ns in node.namespaces() {
            default |= ns.name().is_none();
            xlink |= ns.name() == Some("xlink");
            if ns.name() == Some("xml") || inherited(ns.name(), ns.uri()) {
                continue;
            }
            match ns.name() {
                Some(prefix) => {
                    let _ = write!(self.text, r#" xmlns:{prefix}="{}""#, escape_attribute(ns.uri()));
                }
                None => {
                    let _ = write!(self.text, r#" xmlns="{}""#, escape_attribute(ns.uri()));
                }
            }
        }
        (default, xlink)
    }
}

/// Elements left out with everything inside them: anything not on the allowlist, or outside
/// the SVG namespace.
fn is_dropped(node: Node) -> bool {
    let tag = node.tag_name();
    let svg = tag.namespace().is_none_or(|ns| ns == SVG_NS);
    !svg || !ALLOWED_ELEMENTS.contains(&tag.name().to_ascii_lowercase().as_str())
}

/// Whether every `url(` in a presentation value points at a `#id` in the file. The value is
/// read as CSS would: escapes decoded, whitespace and quotes ignored, case folded.
fn urls_stay_local(value: &str) -> bool {
    let mut decoded = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            decoded.push(c);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
            hex.extend(chars.next());
        }
        if hex.is_empty() {
            decoded.extend(chars.next());
        } else {
            let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
            decoded.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
            // One whitespace character ends a hex escape.
            if chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
        }
    }
    let flat = decoded
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '"' | '\''))
        .collect::<String>()
        .to_ascii_lowercase();
    flat.split("url(").skip(1).all(|rest| rest.starts_with('#'))
}

/// Only allowlisted attributes, un-namespaced (or `xlink:href`); a link survives only to the
/// file's own elements, and a `url()` only to a `#id` in it.
fn is_safe_attribute(attribute: &roxmltree::Attribute) -> bool {
    let name = attribute.name().to_ascii_lowercase();
    let namespace_ok = match attribute.namespace() {
        None => true,
        Some(ns) => ns == XLINK_NS && name == "href",
    };
    namespace_ok
        && ALLOWED_ATTRIBUTES.contains(&name.as_str())
        && (name != "href" || attribute.value().trim().starts_with('#'))
        && urls_stay_local(attribute.value())
}

fn element_name(node: Node) -> String {
    let local = node.tag_name().name();
    let prefix = node.tag_name().namespace().and_then(|uri| {
        let is_default = node
            .namespaces()
            .any(|ns| ns.name().is_none() && ns.uri() == uri);
        if is_default { None } else { node.lookup_prefix(uri) }
    });
    match prefix {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.into(),
    }
}

fn attribute_name(node: Node, attribute: &roxmltree::Attribute) -> String {
    let prefix = attribute.namespace().and_then(|uri| {
        if uri == roxmltree::NS_XML_URI {
            return Some("xml");
        }
        node.namespaces()
            .find(|ns| ns.uri() == uri && ns.name().is_some())
            .and_then(|ns| ns.name())
    });
    match prefix {
        Some(prefix) => format!("{prefix}:{}", attribute.name()),
        None => attribute.name().into(),
    }
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---- Playback clock -----------------------------------------------------------------------

/// Seconds into the animation, stoppable.
struct Clock {
    origin: Instant,
    /// Where it stopped, while paused.
    paused: Option<f64>,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            paused: None,
        }
    }
}

impl Clock {
    fn elapsed(&self) -> f64 {
        self.paused
            .unwrap_or_else(|| self.origin.elapsed().as_secs_f64())
    }

    fn restart(&mut self) {
        *self = Self::default();
    }

    fn pause(&mut self) {
        self.paused = Some(self.elapsed());
    }

    fn resume(&mut self) {
        if let Some(at) = self.paused.take() {
            self.origin = Instant::now() - Duration::from_secs_f64(at);
        }
    }
}

// ---- State and wiring ---------------------------------------------------------------------

struct Loaded {
    name: String,
    template: Arc<Template>,
    /// The exported markup for the current controls.
    markup: String,
}

#[derive(Default)]
struct State {
    loaded: Option<Loaded>,
    /// The last valid stroke colour: a half-typed hex keeps the animation on it.
    stroke: Option<String>,
    /// The preview's size in logical pixels, from the view.
    canvas: (f32, f32),
    clock: Clock,
    timer: Timer,
    /// Paused because the tool stopped showing; it resumes when the tool shows again.
    hidden: bool,
    frame_in_flight: bool,
    frame_wanted: bool,
    frames: Latest,
    loads: Latest,
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
    let g = app.global::<SvgAnimator>();
    g.set_stroke_hex(DEFAULT_STROKE.into());
    g.set_stroke(slint::Color::from_rgb_u8(0, 0, 0));
    let weak = app.as_weak();
    g.on_choose_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose an SVG", Some(("SVG", SVG_EXTENSIONS))) {
            open(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_remove_file(move || {
        if let Some(app) = weak.upgrade() {
            remove_file(&app);
        }
    });
    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
            restart(&app);
        }
    });
    let weak = app.as_weak();
    g.on_stroke_edited(move |text| {
        let Some(app) = weak.upgrade() else { return };
        let valid = svg::parse_hex(&text);
        let g = app.global::<SvgAnimator>();
        g.set_stroke_invalid(valid.is_none());
        if let Some((hex, [r, gr, b])) = valid {
            g.set_stroke(slint::Color::from_rgb_u8(r, gr, b));
            with(|s| s.stroke = Some(hex));
            refresh(&app);
            restart(&app);
        }
    });
    let weak = app.as_weak();
    g.on_zoom_changed(move || {
        if let Some(app) = weak.upgrade() {
            request_frame(&app);
        }
    });
    let weak = app.as_weak();
    g.on_toggle_play(move || {
        if let Some(app) = weak.upgrade() {
            toggle_play(&app);
        }
    });
    let weak = app.as_weak();
    g.on_replay(move || {
        if let Some(app) = weak.upgrade() {
            restart(&app);
        }
    });
    let weak = app.as_weak();
    g.on_preview_resized(move |width, height| {
        let Some(app) = weak.upgrade() else { return };
        let changed = with(|s| std::mem::replace(&mut s.canvas, (width, height)) != (width, height));
        if changed {
            request_frame(&app);
        }
    });
    let weak = app.as_weak();
    g.on_shown(move || {
        let Some(app) = weak.upgrade() else { return };
        if with(|s| std::mem::take(&mut s.hidden)) {
            play(&app);
        }
    });
    let weak = app.as_weak();
    g.on_copy_markup(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let result = with(|s| s.loaded.as_ref().map(|l| clipboard::copy_text(&l.markup)));
        match result {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                app.global::<SvgAnimator>()
                    .set_error(format!("Couldn't copy: {e}").into());
                false
            }
            None => false,
        }
    });
    let weak = app.as_weak();
    g.on_save(move || {
        if let Some(app) = weak.upgrade() {
            save(&app);
        }
    });
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<SvgAnimator>().set_drop_hover(hover);
}

/// A dropped SVG replaces the current one; the animator takes one file at a time.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) => open(app, path),
        Err(paths) if paths.is_empty() => {}
        Err(_) => app
            .global::<SvgAnimator>()
            .set_error("Drop one SVG file at a time.".into()),
    }
}

/// Stops the preview and any load, and hands back their threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        s.timer.stop();
        let mut workers = s.frames.shutdown();
        workers.extend(s.loads.shutdown());
        workers
    })
}

/// Reads and prepares `path` in the background (measuring thousands of paths takes a moment).
fn open(app: &AppWindow, path: PathBuf) {
    let g = app.global::<SvgAnimator>();
    let name = files::file_name(&path);
    let is_svg = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    if !is_svg {
        g.set_error(format!("“{name}” isn't an SVG file. Choose an .svg file.").into());
        return;
    }
    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > svg::MAX_SVG_BYTES as u64 => {
            let message = format!(
                "{name} is {}. The animator takes SVGs up to {}.",
                files::format_size(meta.len()),
                files::format_size(svg::MAX_SVG_BYTES as u64)
            );
            return g.set_error(message.into());
        }
        Ok(_) => {}
        Err(e) => return g.set_error(format!("Couldn't open {name}: {e}").into()),
    }
    g.set_error("".into());
    g.set_note(format!("Reading {name}…").into());
    let weak = app.as_weak();
    with(|s| {
        s.loads.spawn(move |ticket| {
            let result = std::fs::read_to_string(&path)
                .map_err(|e| format!("Couldn't read {name}: {e}"))
                .and_then(|text| Template::build(&text));
            jobs::post(&weak, move |app| {
                if !ticket.is_current() {
                    return;
                }
                let g = app.global::<SvgAnimator>();
                g.set_note("".into());
                match result {
                    Ok(template) => {
                        with(|s| {
                            s.loaded = Some(Loaded {
                                name,
                                template: Arc::new(template),
                                markup: String::new(),
                            })
                        });
                        g.set_loaded(true);
                        refresh(&app);
                        restart(&app);
                    }
                    Err(message) => g.set_error(message.into()),
                }
            });
        });
    });
}

fn remove_file(app: &AppWindow) {
    with(|s| {
        s.timer.stop();
        s.loaded = None;
        s.frames.ticket();
        s.loads.ticket();
    });
    let g = app.global::<SvgAnimator>();
    g.set_loaded(false);
    g.set_frame(slint::Image::default());
    g.set_markup("".into());
    g.set_markup_note("".into());
    g.set_error("".into());
    g.set_note("".into());
    g.set_playing(false);
}

fn style(app: &AppWindow) -> Style {
    let g = app.global::<SvgAnimator>();
    Style {
        stroke: with(|s| s.stroke.clone()).unwrap_or_else(|| DEFAULT_STROKE.into()),
        // Tenths, so 1.5 is the same double the website's slider gives.
        width: f64::from(g.get_stroke_width().round()) / 10.0,
        fill: g.get_fill(),
    }
}

fn timing(app: &AppWindow) -> Timing {
    let g = app.global::<SvgAnimator>();
    let mode = match g.get_mode() {
        SvgAnimatorMode::Sequential => Mode::Sequential,
        SvgAnimatorMode::Simultaneous => Mode::Simultaneous,
    };
    // Hundredths, so 0.3 is the same double the website's slider gives.
    Timing::new(f64::from(g.get_speed().round().max(1.0)) / 100.0, mode)
}

/// Rebuilds the exported markup and the summary for the current controls.
fn refresh(app: &AppWindow) {
    let g = app.global::<SvgAnimator>();
    let (style, timing) = (style(app), timing(app));
    let shown = with(|s| {
        let loaded = s.loaded.as_mut()?;
        loaded.markup = loaded.template.write(&style, Output::Smil(timing));
        let paths = loaded.template.paths.len();
        let summary = format!(
            "{paths} {} · {} · {:.2}× · {:.2} s in all",
            if paths == 1 { "path" } else { "paths" },
            match timing.mode {
                Mode::Sequential => "Sequential",
                Mode::Simultaneous => "Simultaneous",
            },
            1.0 / timing.per_path,
            timing.total(paths)
        );
        Some((
            format!("{} · {summary}", loaded.name),
            svg::preview(&loaded.markup).to_owned(),
            svg::preview_note(&loaded.markup),
        ))
    });
    if let Some((summary, markup, note)) = shown {
        g.set_summary(summary.into());
        g.set_markup(markup.into());
        g.set_markup_note(note.into());
    }
}

fn restart(app: &AppWindow) {
    with(|s| {
        s.clock.restart();
        s.hidden = false;
    });
    play(app);
}

fn play(app: &AppWindow) {
    if with(|s| s.loaded.is_none()) {
        return;
    }
    with(|s| s.clock.resume());
    let weak = app.as_weak();
    with(|s| {
        s.timer.start(TimerMode::Repeated, FRAME_INTERVAL, move || {
            if let Some(app) = weak.upgrade() {
                tick(&app);
            }
        })
    });
    app.global::<SvgAnimator>().set_playing(true);
    tick(app);
}

fn pause(app: &AppWindow) {
    with(|s| {
        s.timer.stop();
        s.clock.pause();
    });
    app.global::<SvgAnimator>().set_playing(false);
}

fn toggle_play(app: &AppWindow) {
    let (running, finished) = with(|s| {
        let total = s
            .loaded
            .as_ref()
            .map_or(0.0, |l| timing_total(app, l));
        (s.timer.running(), s.clock.elapsed() >= total)
    });
    match (running, finished) {
        (true, _) => pause(app),
        (false, true) => restart(app),
        (false, false) => play(app),
    }
}

fn timing_total(app: &AppWindow, loaded: &Loaded) -> f64 {
    timing(app).total(loaded.template.paths.len())
}

/// One timer tick: stops when the tool isn't showing or the animation is over, and asks for
/// the frame for now.
fn tick(app: &AppWindow) {
    if app.get_current_slug() != SLUG {
        pause(app);
        with(|s| s.hidden = true);
        return;
    }
    let total = timing(app).total(with(|s| s.loaded.as_ref().map_or(0, |l| l.template.paths.len())));
    if with(|s| s.clock.elapsed()) >= total {
        with(|s| {
            s.timer.stop();
            s.clock.paused = Some(total);
        });
        app.global::<SvgAnimator>().set_playing(false);
    }
    request_frame(app);
}

/// Renders the frame for the clock's current time in the background; while one is rendering,
/// the newest request waits for it instead of piling up.
fn request_frame(app: &AppWindow) {
    with(|s| s.frame_wanted = true);
    let busy = with(|s| s.frame_in_flight || s.loaded.is_none());
    if busy {
        return;
    }
    let (style, timing, zoom) = (
        style(app),
        timing(app),
        app.global::<SvgAnimator>().get_zoom().round() / 100.0,
    );
    let scale = app.window().scale_factor();
    let weak = app.as_weak();
    with(|s| {
        let Some(loaded) = s.loaded.as_ref() else { return };
        let template = loaded.template.clone();
        let t = s.clock.elapsed();
        let (width, height) = s.canvas;
        s.frame_wanted = false;
        s.frame_in_flight = true;
        s.frames.spawn(move |ticket| {
            let frame = template.write(&style, Output::Frame(timing, t));
            let pixels = svg::parse(&frame).ok().and_then(|tree| {
                let fit = svg::fit_scale(
                    &tree,
                    (width - 2.0 * PREVIEW_INSET).max(1.0),
                    (height - 2.0 * PREVIEW_INSET).max(1.0),
                );
                svg::render_on_checker(&tree, fit * zoom * scale, scale)
                    .map(|pixmap| svg::pixels(&pixmap))
            });
            jobs::post(&weak, move |app| {
                let again = with(|s| {
                    s.frame_in_flight = false;
                    s.frame_wanted
                });
                if ticket.is_current() {
                    let g = app.global::<SvgAnimator>();
                    match pixels {
                        Some(pixels) => {
                            g.set_frame_width(pixels.width() as f32 / scale);
                            g.set_frame_height(pixels.height() as f32 / scale);
                            g.set_frame(slint::Image::from_rgba8_premultiplied(pixels));
                        }
                        None => g.set_frame(slint::Image::default()),
                    }
                }
                if again {
                    request_frame(&app);
                }
            });
        });
    });
}

fn save(app: &AppWindow) {
    let Some(name) = with(|s| s.loaded.as_ref().map(|l| l.name.clone())) else {
        return;
    };
    let stem = name
        .strip_suffix(".svg")
        .or_else(|| name.strip_suffix(".SVG"))
        .unwrap_or(&name);
    let g = app.global::<SvgAnimator>();
    let Some(path) = files::choose_save_path("Save Animated SVG", &format!("{stem}-animated.svg"))
    else {
        return;
    };
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        with(|s| match &s.loaded {
            Some(l) => std::fs::write(output.path(), &l.markup),
            None => Ok(()),
        })?;
        output.keep()
    });
    let saved = files::file_name(&path);
    match result {
        Ok(_) => {
            g.set_error("".into());
            g.set_note(format!("Saved as {saved}").into());
        }
        Err(e) => g.set_error(format!("Couldn't save {saved}: {e}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The website's defaults, with per-test overrides.
    fn style() -> Style {
        Style {
            stroke: DEFAULT_STROKE.into(),
            width: 1.5,
            fill: true,
        }
    }

    const TWO_LINES: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0 0H10"/><path d="M0 0V5" fill="red" stroke="blue" stroke-width="3"/></svg>"#;

    #[test]
    fn smil_markup_matches_the_website() {
        let template = Template::build(TWO_LINES).unwrap();

        let markup = template.write(&style(), Output::Smil(Timing::new(0.5, Mode::Sequential)));

        assert_eq!(
            markup,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" xmlns:xlink="http://www.w3.org/1999/xlink"><path d="M0 0H10" stroke="#000000" stroke-width="1.5" stroke-dasharray="10" stroke-dashoffset="10"><animate data-svg-animator="true" attributeName="stroke-dashoffset" from="10" to="0" dur="2s" begin="0s" fill="freeze" calcMode="spline" keySplines="0.25 0.1 0.25 1" keyTimes="0;1"/></path><path d="M0 0V5" fill="red" stroke="#000000" stroke-width="1.5" stroke-dasharray="5" stroke-dashoffset="5"><animate data-svg-animator="true" attributeName="stroke-dashoffset" from="5" to="0" dur="2s" begin="2s" fill="freeze" calcMode="spline" keySplines="0.25 0.1 0.25 1" keyTimes="0;1"/></path></svg>"##
        );
    }

    #[test]
    fn simultaneous_paths_all_begin_at_zero() {
        let template = Template::build(TWO_LINES).unwrap();

        let markup = template.write(&style(), Output::Smil(Timing::new(0.3, Mode::Simultaneous)));

        assert_eq!(markup.matches(r#"dur="3.3333333333333335s" begin="0s""#).count(), 2);
    }

    #[test]
    fn fill_off_sets_none_in_place_or_at_the_end() {
        let template = Template::build(TWO_LINES).unwrap();
        let no_fill = Style {
            fill: false,
            ..style()
        };

        let frame = template.write(&no_fill, Output::Frame(Timing::new(1.0, Mode::Sequential), 0.0));

        assert_eq!(
            frame,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" xmlns:xlink="http://www.w3.org/1999/xlink"><path d="M0 0H10" stroke="#000000" stroke-width="1.5" stroke-dasharray="10" stroke-dashoffset="10" fill="none"></path><path d="M0 0V5" fill="none" stroke="#000000" stroke-width="1.5" stroke-dasharray="5" stroke-dashoffset="5"></path></svg>"##
        );
    }

    #[test]
    fn a_second_run_replaces_the_first_animation() {
        let first = Template::build(TWO_LINES)
            .unwrap()
            .write(&style(), Output::Smil(Timing::new(0.5, Mode::Sequential)));

        let second = Template::build(&first)
            .unwrap()
            .write(&style(), Output::Smil(Timing::new(0.5, Mode::Sequential)));

        // Attribute order differs once the namespaces are declared on the root, so count.
        assert_eq!(second.matches("<animate").count(), 2);
        assert_eq!(first.matches("<animate").count(), 2);
    }

    /// The markup for an `<svg>` holding a path and `inner`, after sanitising.
    fn sanitised(inner: &str) -> String {
        let source = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:svg="http://www.w3.org/2000/svg"><path d="M0 0H1"/>{inner}</svg>"##
        );
        Template::build(&source).unwrap().write(
            &style(),
            Output::Frame(Timing::new(1.0, Mode::Sequential), 9.0),
        )
    }

    fn assert_inert(markup: &str) {
        let lower = markup.to_ascii_lowercase();
        for bad in ["script", "alert", "javascript", "data:", "onload", "onclick", "evil"] {
            assert!(!lower.contains(bad), "{bad} survived in {markup}");
        }
    }

    #[test]
    fn script_elements_are_dropped_in_any_case_or_prefix() {
        for tag in ["script", "Script", "SCRIPT", "svg:script"] {
            assert_inert(&sanitised(&format!("<{tag}>alert(1)</{tag}>")));
        }
    }

    #[test]
    fn event_handlers_are_dropped_in_any_case() {
        let markup = sanitised(r#"<g onLoad="alert(1)" ONCLICK="alert(2)"><rect width="1"/></g>"#);

        assert_inert(&markup);
        assert!(markup.contains("<rect"));
    }

    #[test]
    fn hostile_hrefs_are_dropped_in_any_case_or_namespace() {
        for link in [
            r#"HREF="javascript:alert(1)""#,
            r#"xlink:href=" JaVaScRiPt:alert(1)""#,
            r#"href="data:text/html,alert(1)""#,
        ] {
            assert_inert(&sanitised(&format!(r#"<a {link}><rect width="1"/></a>"#)));
        }
    }

    #[test]
    fn url_references_stay_inside_the_file() {
        for value in [
            "url(https://e/x.svg#a)",
            "URL( 'http://e' )",
            r"u\72 l(http://e/x.svg#a)",
            r"\55RL(http://e)",
            "url(#g) url(//e/x.svg#h)",
        ] {
            let markup = sanitised(&format!(r#"<rect width="1" fill="{value}" mask="{value}"/>"#));

            assert!(!markup.contains("//e"), "{markup}");
            assert!(!markup.contains("mask="), "{markup}");
        }
        let kept = sanitised(r##"<rect width="1" fill="url(#g)"/>"##);
        assert!(kept.contains(r##"fill="url(#g)""##), "{kept}");
    }

    #[test]
    fn a_comment_cannot_close_itself_into_a_script() {
        assert_inert(&sanitised("<!-- --><script>alert(1)</script> -->"));
        assert!(!sanitised("<!-- x -->").contains("<!--"));
    }

    #[test]
    fn processing_instructions_are_dropped() {
        assert!(!sanitised("<?php alert(1) ?>").contains("<?"));
    }

    #[test]
    fn foreign_object_style_and_outside_use_are_dropped() {
        for inner in [
            r#"<foreignObject><div onclick="alert(1)"/></foreignObject>"#,
            "<style>*{background:url(javascript:alert(1))}</style>",
            r#"<use href="http://evil/x.svg#a"/>"#,
            r#"<set attributeName="href" to="javascript:alert(1)"/>"#,
            r#"<g style="background:url(javascript:alert(1))"/>"#,
        ] {
            let markup = sanitised(inner);

            assert_inert(&markup);
            assert!(!markup.contains("style"), "{markup}");
        }
        assert!(sanitised(r##"<use xlink:href="#p"/>"##).contains(r##"xlink:href="#p""##));
    }

    #[test]
    fn text_and_attributes_are_re_escaped() {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg"><title>a &amp; b &lt;c&gt;</title><path d="M0 0H1" id="&quot;q&quot;"/></svg>"#;

        let markup = Template::build(source)
            .unwrap()
            .write(&style(), Output::Frame(Timing::new(1.0, Mode::Sequential), 9.0));

        assert!(markup.contains("<title>a &amp; b &lt;c&gt;</title>"));
        assert!(markup.contains(r#"id="&quot;q&quot;""#));
    }

    #[test]
    fn an_svg_without_paths_is_refused_like_the_website() {
        let error = Template::build(r#"<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#);

        assert_eq!(error, Err("No <path> elements were found in this SVG.".into()));
    }

    #[test]
    fn a_non_svg_root_is_refused_like_the_website() {
        assert_eq!(
            Template::build("<html/>"),
            Err("No <svg> root element was found.".into())
        );
    }

    #[test]
    fn broken_xml_is_refused_like_the_website() {
        assert_eq!(
            Template::build("<svg><path></svg>"),
            Err("This SVG could not be parsed.".into())
        );
    }

    #[test]
    fn path_length_measures_lines_and_closes() {
        assert_eq!(path_length("M0 0H3V4Z"), 12.0);
    }

    #[test]
    fn path_length_of_a_circle_is_its_circumference_within_a_tenth_of_a_percent() {
        let length = path_length("M10 0A10 10 0 1 1 -10 0A10 10 0 1 1 10 0");

        assert!((length - 62.832).abs() < 0.063, "{length}");
    }

    #[test]
    fn path_length_of_nothing_is_one() {
        assert_eq!(path_length("M5 5"), 1.0);
    }

    #[test]
    fn sequential_delays_stack_and_simultaneous_ones_dont() {
        let sequential = Timing::new(0.5, Mode::Sequential);
        let simultaneous = Timing::new(0.5, Mode::Simultaneous);

        assert_eq!((sequential.delay(3), sequential.total(4)), (6.0, 8.0));
        assert_eq!((simultaneous.delay(3), simultaneous.total(4)), (0.0, 2.0));
    }

    #[test]
    fn a_path_is_hidden_until_it_begins() {
        let timing = Timing::new(1.0, Mode::Sequential);

        assert_eq!(timing.offset(40.0, 2, 1.5), 40.0);
    }

    #[test]
    fn a_path_is_drawn_and_frozen_once_it_ends() {
        let timing = Timing::new(1.0, Mode::Sequential);

        assert_eq!(timing.offset(40.0, 0, 5.0), 0.0);
    }

    #[test]
    fn mid_animation_offset_follows_the_ease_curve() {
        let timing = Timing::new(1.0, Mode::Sequential);

        let offset = timing.offset(100.0, 1, 1.5);

        // cubic-bezier(0.25, 0.1, 0.25, 1) at x = 0.5 is y = 0.8024033877399112.
        assert!((offset - 19.759661226008878).abs() < 1e-6, "{offset}");
    }

    #[test]
    fn ease_runs_from_zero_to_one() {
        assert!(ease(0.0).abs() < 1e-9);
        assert!((ease(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn numbers_print_like_javascript() {
        assert_eq!(
            [js_number(2.0), js_number(1.0 / 0.3), js_number(-0.0), js_number(141.421)],
            ["2", "3.3333333333333335", "0", "141.421"]
        );
    }

    #[test]
    fn the_clock_holds_still_while_paused() {
        let mut clock = Clock {
            paused: Some(1.25),
            ..Clock::default()
        };

        clock.resume();
        clock.pause();

        assert!(clock.elapsed() >= 1.25);
        assert!(clock.elapsed() < 1.5);
    }

    #[test]
    fn a_doctype_with_an_internal_entity_is_accepted() {
        let svg = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY d "M0 0H10">]><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="&d;"/></svg>"#;

        let template = Template::build(svg).unwrap();

        assert!(
            template
                .write(&style(), Output::Smil(Timing::new(0.5, Mode::Sequential)))
                .contains(r#"d="M0 0H10""#)
        );
    }

    #[test]
    fn a_billion_laughs_document_is_refused() {
        let mut dtd = String::from(r#"<!ENTITY a0 "lol">"#);
        for n in 1..10 {
            let refs = format!("&a{};", n - 1).repeat(10);
            dtd.push_str(&format!(r#"<!ENTITY a{n} "{refs}">"#));
        }
        let svg = format!(
            r#"<!DOCTYPE svg [{dtd}]><svg xmlns="http://www.w3.org/2000/svg"><text>&a9;</text><path d="M0 0H1"/></svg>"#
        );

        assert!(Template::build(&svg).is_err());
    }
}
