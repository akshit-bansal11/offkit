//! Glassmorphism: a frosted-glass card (tint, blur, saturation, border) over a background,
//! copied as CSS or Tailwind. Slint has no backdrop filter, so the preview paints the background
//! itself, then blurs and saturates the part behind the card on a worker
//! (`image::imageops::fast_blur`); Slint lays the card's tint and border over that, so those
//! change at once. Pure logic first (the two outputs, the backgrounds and the frost, tested
//! without a window), then the `Glassmorphism` global (ui/tools/glassmorphism.slint) wired to it.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use image::imageops::{self, FilterType};
use image::{ImageReader, Limits, RgbaImage};
use resvg::tiny_skia::{
    self, BlendMode, FillRule, GradientStop, IntRect, IntSize, LinearGradient, Paint, PathBuilder,
    Pixmap, PixmapPaint, Point, RadialGradient, Rect, SpreadMode, Transform,
};
use slint::ComponentHandle;

use super::box_shadow::{LatestWorker, Rgb, WHITE, alpha, blur, rgb, slint_image};
use crate::{AppWindow, Glassmorphism, clipboard, files, jobs, select};

pub const SLUG: &str = "glassmorphism";

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Glass {
    /// CSS pixels.
    blur: u32,
    /// The tint's opacity, percent.
    opacity: u8,
    /// Percent; 100 leaves colours as they are.
    saturation: u32,
    /// The border's opacity, percent.
    border: u8,
    tint: Rgb,
}

const DEFAULT: Glass = Glass {
    blur: 10,
    opacity: 15,
    saturation: 150,
    border: 20,
    tint: WHITE,
};

fn css(g: &Glass) -> String {
    let filter = format!("blur({}px) saturate({}%)", g.blur, g.saturation);
    [
        format!("background: {};", g.tint.rgba(g.opacity)),
        format!("backdrop-filter: {filter};"),
        format!("-webkit-backdrop-filter: {filter};"),
        format!("border: 1px solid {};", WHITE.rgba(g.border)),
        "border-radius: 1rem;".into(),
    ]
    .join("\n")
}

/// The website's Tailwind classes, which name blurs on the Tailwind v3 scale.
fn tailwind(g: &Glass) -> String {
    let blur = match g.blur {
        0 => "backdrop-blur-none".into(),
        4 => "backdrop-blur-sm".into(),
        8 => "backdrop-blur-md".into(),
        12 => "backdrop-blur-lg".into(),
        16 => "backdrop-blur-xl".into(),
        24 => "backdrop-blur-2xl".into(),
        40 => "backdrop-blur-3xl".into(),
        px => format!("backdrop-blur-[{px}px]"),
    };
    format!(
        "bg-[{}] {blur} backdrop-saturate-[{}%] border border-white/{} rounded-2xl",
        g.tint.rgba_compact(g.opacity),
        g.saturation,
        g.border
    )
}

/// The card, in logical pixels: the website's `max-w-md`, 4:3, centred with room around it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Card {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

const CARD_MAX_WIDTH: f32 = 448.0;
const CARD_MARGIN: f32 = 32.0;

fn card_layout(w: f32, h: f32) -> Card {
    let width = (w - 2.0 * CARD_MARGIN)
        .min(CARD_MAX_WIDTH)
        .min((h - 2.0 * CARD_MARGIN) * 4.0 / 3.0)
        .max(0.0)
        .floor();
    let height = (width * 3.0 / 4.0).floor();
    Card {
        x: ((w - width) / 2.0).floor(),
        y: ((h - height) / 2.0).floor(),
        w: width,
        h: height,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Background {
    Abstract,
    Gradient,
    Solid,
    Image(PathBuf),
}

/// The backgrounds the Select offers, in order; "Image" appears once one is open.
const BACKGROUND_NAMES: [&str; 4] = ["Abstract", "Gradient", "Solid", "Image"];

/// What the worker draws, in physical pixels.
#[derive(Clone, Debug, PartialEq)]
struct Request {
    background: Background,
    width: u32,
    height: u32,
    scale: f32,
    card: IntRect,
    blur: u32,
    saturation: u32,
}

/// The website's "Abstract" scene: three radial glows on slate, and two blurred discs.
fn paint_abstract(w: u32, h: u32, s: f32) -> Option<Pixmap> {
    let mut p = Pixmap::new(w, h)?;
    p.fill(rgb(0x0f172a).skia(u8::MAX));
    let (fw, fh) = (w as f32, h as f32);
    // `background-image` paints its first layer on top, so these go bottom first:
    // (centre as a share of the scene, colour, where it has faded out).
    let glows = [
        (0.5, 0.5, rgb(0x32c896), 0.6),
        (0.9, 0.8, rgb(0x3296fa), 0.4),
        (0.1, 0.2, rgb(0xb43296), 0.4),
    ];
    for (cx, cy, color, fade) in glows {
        let centre = Point::from_xy(cx * fw, cy * fh);
        // `circle at …` sizes to the farthest corner.
        let radius = [(0.0, 0.0), (fw, 0.0), (0.0, fh), (fw, fh)]
            .iter()
            .map(|&(x, y)| centre.distance(Point::from_xy(x, y)))
            .fold(0.0, f32::max);
        // Fading to the same colour at zero alpha, as CSS does with `transparent`, rather
        // than through black.
        let stops = vec![
            GradientStop::new(0.0, color.skia(u8::MAX)),
            GradientStop::new(fade, color.skia(0)),
        ];
        let shader = RadialGradient::new(
            centre,
            0.0,
            centre,
            radius,
            stops,
            SpreadMode::Pad,
            Transform::identity(),
        )?;
        cover(&mut p, shader);
    }
    // Rose and blue, `mix-blend-overlay`, 8rem and 12rem, 20% in from the top left and the
    // bottom right. Their pulse animation is left out: it would mean a new blur every frame.
    let small = 128.0 * s;
    disc(&mut p, 0.2 * fw, 0.2 * fh, small, rgb(0xf43f5e), 4.0 * s);
    let large = 192.0 * s;
    disc(&mut p, 0.8 * fw - large, 0.8 * fh - large, large, rgb(0x3b82f6), 8.0 * s);
    Some(p)
}

fn cover(p: &mut Pixmap, shader: tiny_skia::Shader) {
    let paint = Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    };
    if let Some(rect) = Rect::from_xywh(0.0, 0.0, p.width() as f32, p.height() as f32) {
        p.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// A disc of `diameter` with its box's top-left at (x, y), blurred by σ = `sigma`, laid over
/// the scene with the overlay blend.
fn disc(p: &mut Pixmap, x: f32, y: f32, diameter: f32, color: Rgb, sigma: f32) {
    let margin = (3.0 * sigma).ceil();
    let side = (diameter + 2.0 * margin).ceil() as u32;
    let Some(mut layer) = Pixmap::new(side, side) else {
        return;
    };
    let r = diameter / 2.0;
    let Some(circle) = PathBuilder::from_circle(margin + r, margin + r, r) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(color.skia(u8::MAX));
    paint.anti_alias = true;
    layer.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
    let overlay = PixmapPaint {
        blend_mode: BlendMode::Overlay,
        ..PixmapPaint::default()
    };
    p.draw_pixmap(
        (x - margin).round() as i32,
        (y - margin).round() as i32,
        blur(layer, sigma).as_ref(),
        &overlay,
        Transform::identity(),
        None,
    );
}

/// `linear-gradient(45deg, #f43f5e, #8b5cf6, #3b82f6)`: the line runs bottom-left to top-right
/// through the centre, long enough that its ends touch the far corners, as CSS sizes it.
fn paint_gradient(w: u32, h: u32) -> Option<Pixmap> {
    let mut p = Pixmap::new(w, h)?;
    let (fw, fh) = (w as f32, h as f32);
    let angle = 45f32.to_radians();
    let (dx, dy) = (angle.sin(), -angle.cos());
    let half = (fw * dx.abs() + fh * dy.abs()) / 2.0;
    let (cx, cy) = (fw / 2.0, fh / 2.0);
    let stops = vec![
        GradientStop::new(0.0, rgb(0xf43f5e).skia(u8::MAX)),
        GradientStop::new(0.5, rgb(0x8b5cf6).skia(u8::MAX)),
        GradientStop::new(1.0, rgb(0x3b82f6).skia(u8::MAX)),
    ];
    let shader = LinearGradient::new(
        Point::from_xy(cx - dx * half, cy - dy * half),
        Point::from_xy(cx + dx * half, cy + dy * half),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    )?;
    cover(&mut p, shader);
    Some(p)
}

fn paint_solid(w: u32, h: u32) -> Option<Pixmap> {
    let mut p = Pixmap::new(w, h)?;
    p.fill(rgb(0x1e293b).skia(u8::MAX));
    Some(p)
}

/// The image scaled to cover the scene (`background-size: cover`), centred. Transparent parts
/// show black.
fn paint_image(image: &RgbaImage, w: u32, h: u32) -> Option<Pixmap> {
    let (iw, ih) = image.dimensions();
    let fit = (w as f32 / iw as f32).max(h as f32 / ih as f32);
    let sw = ((iw as f32 * fit).ceil() as u32).max(w);
    let sh = ((ih as f32 * fit).ceil() as u32).max(h);
    let scaled = imageops::resize(image, sw, sh, FilterType::Triangle);
    let cropped = imageops::crop_imm(&scaled, (sw - w) / 2, (sh - h) / 2, w, h).to_image();
    // Premultiplied, as tiny-skia stores pixels; over black, so every pixel ends opaque.
    let data = cropped
        .pixels()
        .flat_map(|p| {
            let [r, g, b, a] = p.0;
            let times = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;
            [times(r), times(g), times(b), u8::MAX]
        })
        .collect();
    Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
}

/// CSS `saturate(s)` on premultiplied pixels (the matrix is linear, so it holds either way).
fn saturate(p: &mut Pixmap, s: f32) {
    let m = [
        [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
    ];
    for px in p.data_mut().as_chunks_mut::<4>().0.iter_mut() {
        let (r, g, b, a) = (
            f32::from(px[0]),
            f32::from(px[1]),
            f32::from(px[2]),
            f32::from(px[3]),
        );
        for (c, row) in px.iter_mut().zip(m) {
            *c = (row[0] * r + row[1] * g + row[2] * b).round().clamp(0.0, a) as u8;
        }
    }
}

/// The backdrop behind `card`, blurred by σ = `sigma` and saturated, cut to the card. A margin
/// of 3σ around the card is blurred with it, so its edges see what lies beyond them, as a
/// browser's backdrop does; at the scene's own edges the blur repeats the last pixels.
fn frost(scene: &Pixmap, card: IntRect, sigma: f32, saturation: f32) -> Option<Pixmap> {
    let margin = (3.0 * sigma).ceil() as i32;
    let region = IntRect::from_ltrb(
        (card.left() - margin).max(0),
        (card.top() - margin).max(0),
        (card.right() + margin).min(scene.width() as i32),
        (card.bottom() + margin).min(scene.height() as i32),
    )?;
    let mut area = blur(scene.clone_rect(region)?, sigma);
    saturate(&mut area, saturation);
    area.clone_rect(IntRect::from_xywh(
        card.x() - region.x(),
        card.y() - region.y(),
        card.width(),
        card.height(),
    )?)
}

/// Decodes a background image, refusing absurd dimensions before allocating, and keeps a copy
/// small enough to repaint quickly (the preview never shows more than a window's worth).
fn decode(path: &Path) -> Result<RgbaImage, String> {
    const MAX_SIDE: u32 = 16_384;
    const PREVIEW_SIDE: u32 = 2_048;
    let name = files::file_name(path);
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|e| format!("Couldn't open {name}: {e}"))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("Couldn't read {name}: {e}"))?;
    let image = if image.width().max(image.height()) > PREVIEW_SIDE {
        image.thumbnail(PREVIEW_SIDE, PREVIEW_SIDE)
    } else {
        image
    };
    Ok(image.to_rgba8())
}

/// The worker's memory between requests: the painted scene (repainted only when the background
/// or the size changes) and the decoded image.
#[derive(Default)]
struct Painter {
    scene: Option<(Background, u32, u32, Pixmap)>,
    image: Option<(PathBuf, RgbaImage)>,
}

/// A new scene when it changed, and the card's frost.
struct Frame {
    scene: Option<Pixmap>,
    card: Pixmap,
}

impl Painter {
    fn render(&mut self, r: &Request) -> Result<Frame, String> {
        let stale = !matches!(&self.scene,
            Some((background, w, h, _)) if *background == r.background && *w == r.width && *h == r.height);
        if stale {
            let pixmap = match &r.background {
                Background::Abstract => paint_abstract(r.width, r.height, r.scale),
                Background::Gradient => paint_gradient(r.width, r.height),
                Background::Solid => paint_solid(r.width, r.height),
                Background::Image(path) => {
                    if !matches!(&self.image, Some((p, _)) if p == path) {
                        self.image = Some((path.clone(), decode(path)?));
                    }
                    self.image
                        .as_ref()
                        .and_then(|(_, image)| paint_image(image, r.width, r.height))
                }
            }
            .ok_or("Couldn't draw the preview background.")?;
            self.scene = Some((r.background.clone(), r.width, r.height, pixmap));
        }
        let scene = &self.scene.as_ref().expect("painted above").3;
        let card = frost(
            scene,
            r.card,
            r.blur as f32 * r.scale,
            r.saturation as f32 / 100.0,
        )
        .ok_or("Couldn't draw the glass.")?;
        Ok(Frame {
            scene: stale.then(|| scene.clone()),
            card,
        })
    }
}

// ---- State and wiring ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    /// The last valid tint; a half-typed colour leaves it alone.
    tint: Option<Rgb>,
    image: Option<PathBuf>,
    /// The preview area, in logical pixels.
    area: (f32, f32),
    /// The last request sent, so a tint or border change doesn't blur again.
    last: Option<Request>,
    worker: Option<LatestWorker<Request>>,
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
    let mut painter = Painter::default();
    let worker = LatestWorker::spawn("glassmorphism", move |request: Request| {
        let result = painter.render(&request);
        jobs::post(&weak, move |app| {
            let g = app.global::<Glassmorphism>();
            match result {
                Ok(frame) => {
                    if let Some(scene) = frame.scene {
                        g.set_scene(slint_image(&scene));
                    }
                    g.set_frost(slint_image(&frame.card));
                }
                Err(message) => {
                    g.set_error(message.into());
                    // An image that won't open is dropped; the preview goes back to Abstract.
                    if matches!(request.background, Background::Image(_)) {
                        with(|s| s.image = None);
                        g.set_background_index(0);
                        refresh(&app);
                    }
                }
            }
        });
    });
    with(|s| {
        s.worker = Some(worker);
        s.tint = Some(DEFAULT.tint);
    });

    let g = app.global::<Glassmorphism>();
    g.set_blur(DEFAULT.blur as f32);
    g.set_opacity(f32::from(DEFAULT.opacity));
    g.set_saturation(DEFAULT.saturation as f32);
    g.set_border(f32::from(DEFAULT.border));
    g.set_tint_hex(DEFAULT.tint.hex().into());

    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_tint_edited(move |text| {
        let Some(app) = weak.upgrade() else { return };
        let parsed = Rgb::parse(&text);
        app.global::<Glassmorphism>()
            .set_tint_invalid(parsed.is_none());
        if parsed.is_some() {
            with(|s| s.tint = parsed);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_tint_accepted(move |text| {
        let Some(app) = weak.upgrade() else { return };
        // Tidies a valid entry ("fff" → "#ffffff"); an invalid one stays for fixing.
        if let Some(color) = Rgb::parse(&text) {
            app.global::<Glassmorphism>()
                .set_tint_hex(color.hex().into());
        }
    });
    let weak = app.as_weak();
    g.on_open_image(move || {
        let Some(app) = weak.upgrade() else { return };
        let filter = ("Images", IMAGE_EXTENSIONS);
        if let Some(path) = files::choose_file("Choose a Background Image", Some(filter)) {
            use_image(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_resized(move |w, h| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.area = (w, h));
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let g = app.global::<Glassmorphism>();
        let code = if g.get_format_index() == 1 {
            g.get_tailwind()
        } else {
            g.get_css()
        };
        match clipboard::copy_text(&code) {
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
    refresh(app);
}

/// The formats `image` decodes here (AVIF is encode-only in this build).
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<Glassmorphism>().set_drop_hover(hover);
}

/// A dropped image becomes the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) => use_image(app, path),
        Err(paths) if paths.is_empty() => {}
        Err(_) => app
            .global::<Glassmorphism>()
            .set_error("Drop one image at a time.".into()),
    }
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.worker.as_mut().and_then(LatestWorker::shutdown))
        .into_iter()
        .collect()
}

fn use_image(app: &AppWindow, path: PathBuf) {
    let g = app.global::<Glassmorphism>();
    let name = files::file_name(&path);
    let known = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !known {
        let message = format!("{name} isn't an image this preview can show. Use PNG, JPEG, WebP, GIF, BMP or TIFF.");
        return g.set_error(message.into());
    }
    g.set_error("".into());
    g.set_image_name(name.into());
    with(|s| s.image = Some(path));
    g.set_background_index(3);
    refresh(app);
}

/// Shows the outputs and the card's tint and border at once, and asks the worker for a new
/// frost when the backdrop changed.
fn refresh(app: &AppWindow) {
    let g = app.global::<Glassmorphism>();
    let scale = app.window().scale_factor();
    let (tint, image, (w, h)) = with(|s| (s.tint.unwrap_or(DEFAULT.tint), s.image.clone(), s.area));
    let glass = Glass {
        blur: g.get_blur().round() as u32,
        opacity: g.get_opacity().round() as u8,
        saturation: g.get_saturation().round() as u32,
        border: g.get_border().round() as u8,
        tint,
    };
    g.set_css(css(&glass).into());
    g.set_tailwind(tailwind(&glass).into());
    g.set_tint(glass.tint.slint(alpha(glass.opacity)));
    g.set_edge(WHITE.slint(alpha(glass.border)));

    let names = if image.is_some() {
        &BACKGROUND_NAMES[..]
    } else {
        &BACKGROUND_NAMES[..3]
    };
    let index = select::index(g.get_background_index(), names.len());
    g.set_backgrounds(select::options(names.iter().copied()));
    if image.is_none() {
        g.set_image_name("".into());
    }
    let background = match (index, image) {
        (1, _) => Background::Gradient,
        (2, _) => Background::Solid,
        (3, Some(path)) => Background::Image(path),
        _ => Background::Abstract,
    };

    let card = card_layout(w, h);
    g.set_card_x(card.x);
    g.set_card_y(card.y);
    g.set_card_width(card.w);
    g.set_card_height(card.h);
    let physical = |v: f32| (v * scale).round();
    let Some(card_rect) = IntRect::from_xywh(
        physical(card.x) as i32,
        physical(card.y) as i32,
        physical(card.w) as u32,
        physical(card.h) as u32,
    ) else {
        return;
    };
    let request = Request {
        background,
        width: physical(w) as u32,
        height: physical(h) as u32,
        scale,
        card: card_rect,
        blur: glass.blur,
        saturation: glass.saturation,
    };
    with(|s| {
        if s.last.as_ref() == Some(&request) {
            return;
        }
        if let Some(worker) = &s.worker {
            worker.send(request.clone());
        }
        s.last = Some(request);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let p = pixmap.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    /// 40 × 20: black on the left half, white on the right.
    fn split_scene() -> Pixmap {
        let mut p = Pixmap::new(40, 20).unwrap();
        p.fill(WHITE.skia(u8::MAX));
        let mut paint = Paint::default();
        paint.set_color(rgb(0x000000).skia(u8::MAX));
        p.fill_rect(
            Rect::from_xywh(0.0, 0.0, 20.0, 20.0).unwrap(),
            &paint,
            Transform::identity(),
            None,
        );
        p
    }

    #[test]
    fn css_matches_the_website_template() {
        assert_eq!(
            css(&DEFAULT),
            "background: rgba(255, 255, 255, 0.15);\n\
             backdrop-filter: blur(10px) saturate(150%);\n\
             -webkit-backdrop-filter: blur(10px) saturate(150%);\n\
             border: 1px solid rgba(255, 255, 255, 0.2);\n\
             border-radius: 1rem;"
        );
    }

    #[test]
    fn tailwind_uses_arbitrary_values_between_named_blurs() {
        assert_eq!(
            tailwind(&DEFAULT),
            "bg-[rgba(255,255,255,0.15)] backdrop-blur-[10px] backdrop-saturate-[150%] \
             border border-white/20 rounded-2xl"
        );
    }

    #[test]
    fn tailwind_names_the_blurs_on_its_scale() {
        let glass = Glass { blur: 8, tint: rgb(0x0f172a), opacity: 40, ..DEFAULT };

        assert_eq!(
            tailwind(&glass),
            "bg-[rgba(15,23,42,0.4)] backdrop-blur-md backdrop-saturate-[150%] \
             border border-white/20 rounded-2xl"
        );
    }

    #[test]
    fn the_card_is_centred_at_most_448_wide_and_four_by_three() {
        assert_eq!(
            card_layout(612.0, 470.0),
            Card { x: 82.0, y: 67.0, w: 448.0, h: 336.0 }
        );
    }

    #[test]
    fn the_card_shrinks_to_fit_a_short_area() {
        assert_eq!(
            card_layout(612.0, 304.0),
            Card { x: 146.0, y: 32.0, w: 320.0, h: 240.0 }
        );
    }

    #[test]
    fn frost_without_blur_keeps_the_backdrop() {
        let card = IntRect::from_xywh(10, 5, 20, 10).unwrap();

        let frosted = frost(&split_scene(), card, 0.0, 1.0).unwrap();

        assert_eq!(pixel(&frosted, 9, 5), [0, 0, 0, 255]);
        assert_eq!(pixel(&frosted, 10, 5), [255, 255, 255, 255]);
    }

    #[test]
    fn frost_blurs_across_an_edge_behind_the_card() {
        let card = IntRect::from_xywh(10, 5, 20, 10).unwrap();

        let frosted = frost(&split_scene(), card, 4.0, 1.0).unwrap();

        let [r, _, _, a] = pixel(&frosted, 10, 5);
        assert!((110..=170).contains(&r), "{r}");
        assert_eq!(a, 255);
    }

    #[test]
    fn saturate_zero_leaves_the_luminance_grey() {
        let mut p = Pixmap::new(1, 1).unwrap();
        p.fill(rgb(0xff0000).skia(u8::MAX));

        saturate(&mut p, 0.0);

        assert_eq!(pixel(&p, 0, 0), [54, 54, 54, 255]);
    }

    #[test]
    fn saturate_above_one_pushes_colours_apart() {
        let mut p = Pixmap::new(1, 1).unwrap();
        p.fill(rgb(0x806060).skia(u8::MAX));

        saturate(&mut p, 2.0);

        assert_eq!(pixel(&p, 0, 0), [153, 89, 89, 255]);
    }

    #[test]
    fn an_image_covers_the_scene_cropped_to_its_centre() {
        // 4 × 1: red, green, green, blue; covering 1 × 1 keeps a middle pixel.
        let image = RgbaImage::from_fn(4, 1, |x, _| match x {
            0 => image::Rgba([255, 0, 0, 255]),
            3 => image::Rgba([0, 0, 255, 255]),
            _ => image::Rgba([0, 255, 0, 255]),
        });

        let scene = paint_image(&image, 1, 1).unwrap();

        assert_eq!(pixel(&scene, 0, 0), [0, 255, 0, 255]);
    }
}
