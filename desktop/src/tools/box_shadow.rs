//! Box Shadow: layered CSS `box-shadow`s (outer or inset; offset, blur, spread, colour and
//! opacity), drawn natively and copied as CSS. Pure logic first (the CSS string and a tiny-skia
//! renderer that follows CSS's shadow geometry, tested on rendered pixels), then the `BoxShadow`
//! global (ui/tools/box-shadow.slint) wired to it. Blurring takes longer than a frame, so the
//! preview renders on one worker that always takes the newest request.
//!
//! The css family has no engine file, so this module also holds what its three tools share:
//! colour parsing (`Rgb`), the newest-request worker (`LatestWorker`), `blur` and `slint_image`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use image::{RgbaImage, imageops};
use resvg::tiny_skia::{
    self, FillRule, IntSize, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Transform,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, BoxShadow, BoxShadowColor, BoxShadowRow, clipboard, jobs};

pub const SLUG: &str = "box-shadow";

// ---- Shared with the other css tools ------------------------------------------------------

/// An opaque sRGB colour, as the tools' colour fields hold it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

pub(super) const WHITE: Rgb = rgb(0xffffff);

/// `0xrrggbb` as a colour, for constants.
pub(super) const fn rgb(value: u32) -> Rgb {
    Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

impl Rgb {
    /// `#rgb` or `#rrggbb`, with or without the `#`, in any case.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let digits = text.strip_prefix('#').unwrap_or(text);
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let full: String = match digits.len() {
            3 => digits.chars().flat_map(|c| [c, c]).collect(),
            6 => digits.into(),
            _ => return None,
        };
        u32::from_str_radix(&full, 16).ok().map(rgb)
    }

    /// `#rrggbb` in lower case, as a colour picker reports it.
    pub(super) fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// `rgba(r, g, b, a)` with an opacity percentage as the fraction, printed as JavaScript
    /// prints `percent / 100` (both print the shortest form that reads back the same).
    pub(super) fn rgba(self, percent: u8) -> String {
        let (r, g, b) = (self.r, self.g, self.b);
        format!("rgba({r}, {g}, {b}, {})", f64::from(percent) / 100.0)
    }

    /// The same, without spaces, for a Tailwind arbitrary value.
    pub(super) fn rgba_compact(self, percent: u8) -> String {
        self.rgba(percent).replace(' ', "")
    }

    pub(super) fn slint(self, alpha: u8) -> slint::Color {
        slint::Color::from_argb_u8(alpha, self.r, self.g, self.b)
    }

    pub(super) fn skia(self, alpha: u8) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, alpha)
    }
}

/// An opacity percentage as an 8-bit alpha, rounded as browsers do.
pub(super) fn alpha(percent: u8) -> u8 {
    (f32::from(percent.min(100)) * 2.55).round() as u8
}

/// One background thread that works on the newest request only: requests that arrive while it
/// is busy replace each other, so dragging a slider never queues stale frames.
pub(super) struct LatestWorker<T> {
    sender: Option<mpsc::Sender<T>>,
    thread: Option<JoinHandle<()>>,
    name: &'static str,
}

impl<T: Send + 'static> LatestWorker<T> {
    /// `name` says whose preview it is, for the log line if the worker ever dies.
    pub(super) fn spawn(name: &'static str, mut work: impl FnMut(T) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel::<T>();
        let thread = thread::spawn(move || {
            while let Ok(mut request) = receiver.recv() {
                while let Ok(newer) = receiver.try_recv() {
                    request = newer;
                }
                work(request);
            }
        });
        Self {
            sender: Some(sender),
            thread: Some(thread),
            name,
        }
    }

    pub(super) fn send(&self, request: T) {
        let Some(sender) = &self.sender else { return };
        // Fails only when the thread has died (a panic, already printed).
        if sender.send(request).is_err() {
            eprintln!("the {} preview stopped updating", self.name);
        }
    }

    /// Stops taking requests and hands back the thread, which exits once the request in hand
    /// is done.
    pub(super) fn shutdown(&mut self) -> Option<JoinHandle<()>> {
        self.sender = None;
        self.thread.take()
    }
}

/// A Gaussian blur with standard deviation `sigma` pixels: CSS's `blur(r)` filter has σ = r, and
/// a shadow's blur radius r has σ = r / 2. `fast_blur` expects premultiplied alpha, which is what
/// tiny-skia stores.
pub(super) fn blur(pixmap: Pixmap, sigma: f32) -> Pixmap {
    if sigma <= 0.0 {
        return pixmap;
    }
    let size = IntSize::from_wh(pixmap.width(), pixmap.height()).expect("a pixmap is never empty");
    let image = RgbaImage::from_raw(size.width(), size.height(), pixmap.take())
        .expect("a pixmap holds width × height RGBA pixels");
    let mut data = imageops::fast_blur(&image, sigma).into_raw();
    // Each channel rounds on its own, which can leave a colour a step above its alpha.
    for px in data.as_chunks_mut::<4>().0.iter_mut() {
        let a = px[3];
        px[..3].iter_mut().for_each(|c| *c = (*c).min(a));
    }
    Pixmap::from_vec(data, size).expect("the blur keeps the size")
}

/// A rendered pixmap as a Slint image. Call it on the UI thread.
pub(super) fn slint_image(pixmap: &Pixmap) -> slint::Image {
    slint::Image::from_rgba8_premultiplied(slint::SharedPixelBuffer::clone_from_slice(
        pixmap.data(),
        pixmap.width(),
        pixmap.height(),
    ))
}

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    x: i32,
    y: i32,
    blur: i32,
    spread: i32,
    /// Percent.
    opacity: u8,
    color: Rgb,
    inset: bool,
}

const BLACK: Rgb = rgb(0x000000);
/// What "Add Shadow" adds, as the first layer; also the website's first default layer.
const NEW_LAYER: Layer = Layer {
    x: 0,
    y: 12,
    blur: 24,
    spread: -4,
    opacity: 25,
    color: BLACK,
    inset: false,
};
const SOFT_LAYER: Layer = Layer {
    x: 0,
    y: 4,
    blur: 8,
    spread: -2,
    opacity: 10,
    color: BLACK,
    inset: false,
};
const BOX_COLOR: Rgb = rgb(0x3b82f6);
const BACKGROUND: Rgb = rgb(0xf1f5f9);

/// The website's preview box: 16rem square, `rounded-3xl`, a `border-white/10` border.
const BOX_SIZE: f32 = 256.0;
const BOX_RADIUS: f32 = 24.0;
const BORDER_WIDTH: f32 = 1.0;
const BORDER_OPACITY: u8 = 10;

fn css(layers: &[Layer]) -> String {
    let shadows: Vec<String> = layers
        .iter()
        .map(|l| {
            let inset = if l.inset { "inset " } else { "" };
            let color = l.color.rgba(l.opacity);
            format!(
                "{inset}{}px {}px {}px {}px {color}",
                l.x, l.y, l.blur, l.spread
            )
        })
        .collect();
    format!("box-shadow: {};", shadows.join(",\n  "))
}

/// A rounded rectangle with one radius for all four corners, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RoundedRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
}

/// How far a quarter circle's cubic control points sit along its tangents, as a share of r.
const KAPPA: f32 = 0.552_284_8;

impl RoundedRect {
    fn offset(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    /// Grows each side by `by` (shrinks when negative), with the corner radii CSS gives a
    /// spread shadow: shrinking takes `by` off the radius; growing adds it, scaled by
    /// 1 + (ratio − 1)³ when radius / spread is under 1, so a sharp corner stays sharp.
    /// `None` once the shape has no area left.
    fn spread(self, by: f32) -> Option<Self> {
        let (w, h) = (self.w + 2.0 * by, self.h + 2.0 * by);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let r = if by > 0.0 {
            let ratio = self.r / by;
            let factor = if ratio < 1.0 {
                1.0 + (ratio - 1.0).powi(3)
            } else {
                1.0
            };
            self.r + by * factor
        } else {
            (self.r + by).max(0.0)
        };
        Some(Self {
            x: self.x - by,
            y: self.y - by,
            w,
            h,
            r: r.min(w / 2.0).min(h / 2.0),
        })
    }

    fn path(self) -> Option<Path> {
        let Self { x, y, w, h, r } = self;
        let k = r * (1.0 - KAPPA);
        let (right, bottom) = (x + w, y + h);
        let mut p = PathBuilder::new();
        p.move_to(x + r, y);
        p.line_to(right - r, y);
        p.cubic_to(right - k, y, right, y + k, right, y + r);
        p.line_to(right, bottom - r);
        p.cubic_to(right, bottom - k, right - k, bottom, right - r, bottom);
        p.line_to(x + r, bottom);
        p.cubic_to(x + k, bottom, x, bottom - k, x, bottom - r);
        p.line_to(x, y + r);
        p.cubic_to(x, y + k, x + k, y, x + r, y);
        p.close();
        p.finish()
    }
}

fn fill(pixmap: &mut Pixmap, path: &Path, color: tiny_skia::Color, rule: FillRule) {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    pixmap.fill_path(path, &paint, rule, Transform::identity(), None);
}

/// Everything the preview shows, in physical pixels (`scale` per CSS pixel).
#[derive(Clone, Debug, PartialEq)]
struct Scene {
    width: u32,
    height: u32,
    scale: f32,
    layers: Vec<Layer>,
    box_color: Rgb,
    background: Rgb,
}

/// The box centred on the background with every shadow, painted in CSS's order: outer shadows
/// under the box, the box, inset shadows over it, then the border. Within each kind the first
/// layer is on top, so they paint from the last.
fn render(scene: &Scene) -> Option<Pixmap> {
    let s = scene.scale;
    let mut canvas = Pixmap::new(scene.width, scene.height)?;
    canvas.fill(scene.background.skia(u8::MAX));
    let size = (BOX_SIZE * s).round();
    let border_box = RoundedRect {
        x: ((scene.width as f32 - size) / 2.0).round(),
        y: ((scene.height as f32 - size) / 2.0).round(),
        w: size,
        h: size,
        r: BOX_RADIUS * s,
    };
    let padding_box = border_box.spread(-BORDER_WIDTH * s)?;

    for layer in scene.layers.iter().rev().filter(|l| !l.inset) {
        outer_shadow(&mut canvas, border_box, layer, s);
    }
    fill(
        &mut canvas,
        &border_box.path()?,
        scene.box_color.skia(u8::MAX),
        FillRule::Winding,
    );
    let mut clip = Mask::new(scene.width, scene.height)?;
    clip.fill_path(
        &padding_box.path()?,
        FillRule::Winding,
        true,
        Transform::identity(),
    );
    for layer in scene.layers.iter().rev().filter(|l| l.inset) {
        inset_shadow(&mut canvas, padding_box, layer, s, &clip);
    }
    let mut ring = PathBuilder::new();
    ring.push_path(&border_box.path()?);
    ring.push_path(&padding_box.path()?);
    fill(
        &mut canvas,
        &ring.finish()?,
        WHITE.skia(alpha(BORDER_OPACITY)),
        FillRule::EvenOdd,
    );
    Some(canvas)
}

/// σ and the margin a blur needs around a shape (beyond 3σ it adds nothing visible).
fn blur_extent(layer: &Layer, scale: f32) -> (f32, f32) {
    let sigma = layer.blur as f32 / 2.0 * scale;
    (sigma, (3.0 * sigma).ceil() + 1.0)
}

/// The border box moved by the offset and grown by the spread, blurred, behind the box. It is
/// drawn into a layer just big enough for the shape and its blur, cut to what can reach the
/// canvas.
fn outer_shadow(canvas: &mut Pixmap, border_box: RoundedRect, layer: &Layer, s: f32) {
    let Some(shape) = border_box
        .offset(layer.x as f32 * s, layer.y as f32 * s)
        .spread(layer.spread as f32 * s)
    else {
        return;
    };
    let (sigma, margin) = blur_extent(layer, s);
    let left = (shape.x - margin).max(-margin).floor();
    let top = (shape.y - margin).max(-margin).floor();
    let right = (shape.x + shape.w + margin).min(canvas.width() as f32 + margin);
    let bottom = (shape.y + shape.h + margin).min(canvas.height() as f32 + margin);
    if right <= left || bottom <= top {
        return;
    }
    let Some(mut px) = Pixmap::new((right - left).ceil() as u32, (bottom - top).ceil() as u32)
    else {
        return;
    };
    let Some(path) = shape.offset(-left, -top).path() else {
        return;
    };
    fill(
        &mut px,
        &path,
        layer.color.skia(alpha(layer.opacity)),
        FillRule::Winding,
    );
    canvas.draw_pixmap(
        left as i32,
        top as i32,
        blur(px, sigma).as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

/// The shadow colour everywhere outside a hole (the padding box moved by the offset and shrunk
/// by the spread), blurred, then clipped to the padding box.
fn inset_shadow(canvas: &mut Pixmap, padding_box: RoundedRect, layer: &Layer, s: f32, clip: &Mask) {
    let (sigma, margin) = blur_extent(layer, s);
    let left = (padding_box.x - margin).floor();
    let top = (padding_box.y - margin).floor();
    let Some(mut px) = Pixmap::new(
        (padding_box.w + 2.0 * margin).ceil() as u32 + 1,
        (padding_box.h + 2.0 * margin).ceil() as u32 + 1,
    ) else {
        return;
    };
    px.fill(layer.color.skia(alpha(layer.opacity)));
    let hole = padding_box
        .offset(layer.x as f32 * s - left, layer.y as f32 * s - top)
        .spread(-layer.spread as f32 * s)
        .and_then(RoundedRect::path);
    if let Some(hole) = hole {
        let paint = Paint {
            blend_mode: tiny_skia::BlendMode::Clear,
            anti_alias: true,
            ..Paint::default()
        };
        px.fill_path(
            &hole,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    canvas.draw_pixmap(
        left as i32,
        top as i32,
        blur(px, sigma).as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        Some(clip),
    );
}

// ---- State and wiring ---------------------------------------------------------------------

struct State {
    layers: Vec<Layer>,
    active: usize,
    box_color: Rgb,
    background: Rgb,
    /// The preview area, in logical pixels.
    area: (f32, f32),
    worker: Option<LatestWorker<Scene>>,
    /// The layer list's rows, one model for the app's life: replacing it on every change would
    /// rebuild the rows and drop the keyboard focus from the one just selected.
    layer_rows: Rc<VecModel<BoxShadowRow>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            layers: vec![NEW_LAYER, SOFT_LAYER],
            active: 0,
            box_color: BOX_COLOR,
            background: BACKGROUND,
            area: (0.0, 0.0),
            worker: None,
            layer_rows: Rc::default(),
        }
    }
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
    let weak = app.as_weak();
    let worker = LatestWorker::spawn("box shadow", move |scene: Scene| match render(&scene) {
        Some(pixmap) => jobs::post(&weak, move |app| {
            app.global::<BoxShadow>().set_preview(slint_image(&pixmap));
        }),
        None => eprintln!(
            "couldn't draw the box shadow preview at {}×{}",
            scene.width, scene.height
        ),
    });
    with(|s| s.worker = Some(worker));

    let g = app.global::<BoxShadow>();
    let weak = app.as_weak();
    g.on_select(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.active = index(i, s.layers.len()));
        load_active(&app);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_add(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            s.layers.insert(0, NEW_LAYER);
            s.active = 0;
        });
        load_active(&app);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let i = index(i, s.layers.len());
            if s.layers.len() > 1 {
                s.layers.remove(i);
                // As on the website: removing the active layer selects the first one left.
                s.active = match s.active {
                    a if a == i => 0,
                    a if a > i => a - 1,
                    a => a,
                };
            }
        });
        load_active(&app);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_move_layer(move |i, delta| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let i = index(i, s.layers.len());
            let Some(j) = i.checked_add_signed(delta as isize) else {
                return;
            };
            if j < s.layers.len() {
                s.layers.swap(i, j);
                // The selection moves with its layer.
                s.active = match s.active {
                    a if a == i => j,
                    a if a == j => i,
                    a => a,
                };
            }
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_layer_changed(move || {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<BoxShadow>();
        with(|s| {
            let layer = &mut s.layers[s.active];
            layer.x = g.get_x().round() as i32;
            layer.y = g.get_y().round() as i32;
            layer.blur = g.get_blur().round() as i32;
            layer.spread = g.get_spread().round() as i32;
            layer.opacity = g.get_opacity().round() as u8;
            layer.inset = g.get_inset();
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_color_edited(move |target, text| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<BoxShadow>();
        // A half-typed colour marks its field and leaves the last valid one in place.
        let parsed = Rgb::parse(&text);
        let invalid = parsed.is_none();
        match target {
            BoxShadowColor::Layer => g.set_layer_invalid(invalid),
            BoxShadowColor::Box => g.set_box_invalid(invalid),
            BoxShadowColor::Background => g.set_background_invalid(invalid),
        }
        let Some(color) = parsed else { return };
        let shown = color.slint(u8::MAX);
        match target {
            BoxShadowColor::Layer => {
                g.set_layer_color(shown);
                with(|s| s.layers[s.active].color = color);
            }
            BoxShadowColor::Box => {
                g.set_box_color(shown);
                with(|s| s.box_color = color);
            }
            BoxShadowColor::Background => {
                g.set_background_color(shown);
                with(|s| s.background = color);
            }
        }
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_color_accepted(move |target, text| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<BoxShadow>();
        // Tidies a valid entry ("3B8" → "#33bb88"); an invalid one stays for fixing.
        let Some(hex) = Rgb::parse(&text).map(Rgb::hex) else {
            return;
        };
        match target {
            BoxShadowColor::Layer => g.set_layer_hex(hex.into()),
            BoxShadowColor::Box => g.set_box_hex(hex.into()),
            BoxShadowColor::Background => g.set_background_hex(hex.into()),
        }
    });
    let weak = app.as_weak();
    g.on_resized(move |w, h| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.area = (w, h));
        request_render(&app);
    });
    let weak = app.as_weak();
    g.on_copy(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let g = app.global::<BoxShadow>();
        match clipboard::copy_text(&g.get_css()) {
            Ok(()) => {
                g.set_error("".into());
                true
            }
            Err(e) => {
                g.set_error(format!("Couldn't copy: {e}").into());
                false
            }
        }
    });

    with(|s| {
        g.set_box_hex(s.box_color.hex().into());
        g.set_box_color(s.box_color.slint(u8::MAX));
        g.set_background_hex(s.background.hex().into());
        g.set_background_color(s.background.slint(u8::MAX));
    });
    with(|s| g.set_rows(ModelRc::from(s.layer_rows.clone())));
    load_active(app);
    refresh(app);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.worker.as_mut().and_then(LatestWorker::shutdown))
        .into_iter()
        .collect()
}

/// A row index from Slint, clamped into the list.
fn index(i: i32, len: usize) -> usize {
    usize::try_from(i).unwrap_or(0).min(len.saturating_sub(1))
}

/// Shows the active layer's settings, after the selection or the list changed. Not called
/// while editing, so a colour being typed is never rewritten under the caret.
fn load_active(app: &AppWindow) {
    let g = app.global::<BoxShadow>();
    let layer = with(|s| s.layers[s.active]);
    g.set_x(layer.x as f32);
    g.set_y(layer.y as f32);
    g.set_blur(layer.blur as f32);
    g.set_spread(layer.spread as f32);
    g.set_opacity(f32::from(layer.opacity));
    g.set_inset(layer.inset);
    g.set_layer_hex(layer.color.hex().into());
    g.set_layer_color(layer.color.slint(u8::MAX));
    g.set_layer_invalid(false);
}

/// Shows the list and the CSS, and asks for a new preview.
fn refresh(app: &AppWindow) {
    let g = app.global::<BoxShadow>();
    with(|s| {
        let count = s.layers.len();
        let rows: Vec<BoxShadowRow> = s
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| BoxShadowRow {
                // Counts down, as on the website: the newest layer has the highest number.
                label: format!("Layer {}", count - i).into(),
                swatch: l.color.slint(alpha(l.opacity)),
                inset: l.inset,
            })
            .collect();
        g.set_active_label(rows[s.active].label.clone());
        if s.layer_rows.row_count() == rows.len() {
            for (i, row) in rows.into_iter().enumerate() {
                s.layer_rows.set_row_data(i, row);
            }
        } else {
            s.layer_rows.set_vec(rows);
        }
        g.set_active(s.active as i32);
        g.set_css(css(&s.layers).into());
    });
    request_render(app);
}

fn request_render(app: &AppWindow) {
    let scale = app.window().scale_factor();
    with(|s| {
        let (w, h) = s.area;
        if w < 1.0 || h < 1.0 {
            return;
        }
        let scene = Scene {
            width: (w * scale).round() as u32,
            height: (h * scale).round() as u32,
            scale,
            layers: s.layers.clone(),
            box_color: s.box_color,
            background: s.background,
        };
        if let Some(worker) = &s.worker {
            worker.send(scene);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = rgb(0xff0000);
    /// A red outer shadow with no offset and no blur.
    const SHARP: Layer = Layer {
        x: 0,
        y: 0,
        blur: 0,
        spread: 10,
        opacity: 100,
        color: RED,
        inset: false,
    };

    /// A 400 × 400 scene at 1×: white background, a black box from 72 to 328 on both axes.
    fn scene(layers: Vec<Layer>) -> Scene {
        Scene {
            width: 400,
            height: 400,
            scale: 1.0,
            layers,
            box_color: BLACK,
            background: WHITE,
        }
    }

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let p = pixmap.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn rgb_parses_long_and_short_hex_with_or_without_the_hash() {
        assert_eq!(Rgb::parse("#3B82F6"), Some(rgb(0x3b82f6)));
        assert_eq!(Rgb::parse("3b82f6"), Some(rgb(0x3b82f6)));
        assert_eq!(Rgb::parse(" #abc "), Some(rgb(0xaabbcc)));
    }

    #[test]
    fn rgb_refuses_what_isnt_hex() {
        assert_eq!(Rgb::parse("#12345"), None);
        assert_eq!(Rgb::parse("#ggg"), None);
        assert_eq!(Rgb::parse(""), None);
    }

    #[test]
    fn rgba_prints_the_opacity_as_javascript_does() {
        assert_eq!(rgb(0x3b82f6).rgba(10), "rgba(59, 130, 246, 0.1)");
        assert_eq!(BLACK.rgba(100), "rgba(0, 0, 0, 1)");
        assert_eq!(BLACK.rgba(0), "rgba(0, 0, 0, 0)");
    }

    #[test]
    fn css_matches_the_website_for_its_two_default_layers() {
        assert_eq!(
            css(&[NEW_LAYER, SOFT_LAYER]),
            "box-shadow: 0px 12px 24px -4px rgba(0, 0, 0, 0.25),\n  \
             0px 4px 8px -2px rgba(0, 0, 0, 0.1);"
        );
    }

    #[test]
    fn css_marks_inset_layers() {
        let layer = Layer {
            x: -3,
            y: 4,
            blur: 5,
            spread: 6,
            opacity: 50,
            color: rgb(0x3b82f6),
            inset: true,
        };

        assert_eq!(
            css(&[layer]),
            "box-shadow: inset -3px 4px 5px 6px rgba(59, 130, 246, 0.5);"
        );
    }

    #[test]
    fn spread_grows_the_radius_by_the_spread_when_it_is_larger_than_the_spread() {
        let rect = RoundedRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            r: 24.0,
        };

        assert_eq!(
            rect.spread(10.0),
            Some(RoundedRect {
                x: -10.0,
                y: -10.0,
                w: 120.0,
                h: 120.0,
                r: 34.0
            })
        );
    }

    #[test]
    fn spread_keeps_a_sharp_corner_sharp() {
        let rect = RoundedRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            r: 0.0,
        };

        assert_eq!(rect.spread(20.0).map(|r| r.r), Some(0.0));
    }

    #[test]
    fn spread_shrinks_the_radius_and_vanishes_past_the_middle() {
        let rect = RoundedRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            r: 24.0,
        };

        assert_eq!(rect.spread(-30.0).map(|r| r.r), Some(0.0));
        assert_eq!(rect.spread(-50.0), None);
    }

    #[test]
    fn an_outer_shadow_covers_its_spread_outside_the_box() {
        let pixmap = render(&scene(vec![SHARP])).unwrap();

        assert_eq!(pixel(&pixmap, 333, 200), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 343, 200), [255, 255, 255, 255]);
    }

    #[test]
    fn an_outer_shadow_follows_its_offset() {
        let moved = Layer {
            x: 30,
            spread: 0,
            ..SHARP
        };

        let pixmap = render(&scene(vec![moved])).unwrap();

        assert_eq!(pixel(&pixmap, 350, 200), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 60, 200), [255, 255, 255, 255]);
    }

    #[test]
    fn an_inset_shadow_covers_its_spread_inside_the_box() {
        let inset = Layer {
            inset: true,
            ..SHARP
        };

        let pixmap = render(&scene(vec![inset])).unwrap();

        assert_eq!(pixel(&pixmap, 77, 200), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 87, 200), [0, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 60, 200), [255, 255, 255, 255]);
    }

    #[test]
    fn a_blurred_shadow_is_half_strength_at_its_edge() {
        // Blur 40 is σ 20; the shape's edge is the box's right edge, x = 328.
        let soft = Layer {
            blur: 40,
            spread: 0,
            ..SHARP
        };

        let pixmap = render(&scene(vec![soft])).unwrap();

        let [_, green, _, _] = pixel(&pixmap, 328, 200);
        assert!((116..=140).contains(&green), "{green}");
    }

    #[test]
    fn the_first_layer_paints_on_top() {
        let blue = Layer {
            color: rgb(0x0000ff),
            ..SHARP
        };

        let pixmap = render(&scene(vec![SHARP, blue])).unwrap();

        assert_eq!(pixel(&pixmap, 333, 200), [255, 0, 0, 255]);
    }
}
