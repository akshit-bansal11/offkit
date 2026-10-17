//! SVG family (svg-optimizer, svg-pattern, svg-animator): rendering untrusted SVG with
//! `resvg`, and the pieces the three previews share. Owner: svg family.
//!
//! Every SVG shown here came from the user, so every render parses with `options()`:
//! - An `<image>` or `<feImage>` href resolves to nothing. usvg's default opens any path it is
//!   given (a relative one against the working folder), so an SVG could otherwise read local
//!   files. `data:` images are refused too: a few bytes of PNG can declare a size whose decode
//!   exhausts memory, and the preview has no business decoding it.
//! - No resources folder, so nothing is resolved against one.
//! - The font database holds only Inter, the UI font the app already ships, so text renders
//!   without reading system fonts; every family, named or generic, falls back to it.
//! - Input is capped at `MAX_SVG_BYTES` and every pixmap at `MAX_SIDE` per side, so a
//!   `width="100000"` can't allocate gigabytes. Renders run off the UI thread (`Latest`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use resvg::tiny_skia::{self, Pixmap};
use resvg::usvg::{self, fontdb};
use slint::{Rgba8Pixel, SharedPixelBuffer};

/// The largest SVG any tool reads, in bytes.
pub const MAX_SVG_BYTES: usize = 10_000_000;
/// The longest side of any rendered preview, in pixels.
const MAX_SIDE: u32 = 4096;
/// The most output text a pane shows; Copy and Save use all of it. Laying out megabytes of
/// text is what's slow (see `tools::base64`).
const PREVIEW_BYTES: usize = 50_000;
const FONT_FAMILY: &str = "Inter Variable";
/// The transparency checkerboard behind a preview: light in both themes, as in image editors,
/// so dark artwork stays visible on the dark theme. Cells are this many logical pixels.
const CHECKER_CELL: f32 = 8.0;
const CHECKER_LIGHT: [u8; 3] = [0xff, 0xff, 0xff];
const CHECKER_DARK: [u8; 3] = [0xe3, 0xe3, 0xe6];

/// Parse options for untrusted SVG; see the module comment for what each part prevents.
pub fn options() -> usvg::Options<'static> {
    usvg::Options {
        resources_dir: None,
        font_family: FONT_FAMILY.into(),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        fontdb: fonts(),
        ..usvg::Options::default()
    }
}

fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_font_data(include_bytes!("../../ui/fonts/InterVariable.ttf").to_vec());
            db.set_serif_family(FONT_FAMILY);
            db.set_sans_serif_family(FONT_FAMILY);
            db.set_monospace_family(FONT_FAMILY);
            db.set_cursive_family(FONT_FAMILY);
            db.set_fantasy_family(FONT_FAMILY);
            Arc::new(db)
        })
        .clone()
}

/// Parses `svg` with the locked-down `options()`. The error is a sentence fragment for the
/// tool to place after the file's name or "This SVG".
pub fn parse(svg: &str) -> Result<usvg::Tree, String> {
    if svg.len() > MAX_SVG_BYTES {
        return Err(format!(
            "is over the {} limit",
            crate::files::format_size(MAX_SVG_BYTES as u64)
        ));
    }
    usvg::Tree::from_str(svg, &options()).map_err(|e| format!("couldn't be read as SVG ({e})"))
}

/// The scale that fits `tree` inside a `width` × `height` box, keeping its aspect ratio.
pub fn fit_scale(tree: &usvg::Tree, width: f32, height: f32) -> f32 {
    let size = tree.size();
    (width / size.width()).min(height / size.height())
}

/// Renders `tree` at `scale` over the transparency checkerboard (`cell_scale` is the display's
/// scale factor, so cells stay 8 logical pixels). `None` for an empty or zero-sized result.
pub fn render_on_checker(tree: &usvg::Tree, scale: f32, cell_scale: f32) -> Option<Pixmap> {
    let mut pixmap = canvas(tree, scale)?;
    paint_checker(&mut pixmap, CHECKER_CELL * cell_scale);
    draw(tree, &mut pixmap);
    Some(pixmap)
}

/// Renders `tree` at `scale` on transparency.
pub fn render(tree: &usvg::Tree, scale: f32) -> Option<Pixmap> {
    let mut pixmap = canvas(tree, scale)?;
    draw(tree, &mut pixmap);
    Some(pixmap)
}

/// An empty pixmap the size of `tree` at `scale`, shrunk to fit `MAX_SIDE` when larger.
fn canvas(tree: &usvg::Tree, scale: f32) -> Option<Pixmap> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let size = tree.size();
    let longest = size.width().max(size.height());
    let scale = scale.min(MAX_SIDE as f32 / longest);
    let side = |length: f32| (length * scale).round().clamp(1.0, MAX_SIDE as f32) as u32;
    Pixmap::new(side(size.width()), side(size.height()))
}

/// Draws `tree` stretched over the whole pixmap (`canvas` sized it to the tree's aspect).
fn draw(tree: &usvg::Tree, pixmap: &mut Pixmap) {
    let size = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        pixmap.width() as f32 / size.width(),
        pixmap.height() as f32 / size.height(),
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
}

fn paint_checker(pixmap: &mut Pixmap, cell: f32) {
    let [r, g, b] = CHECKER_LIGHT;
    pixmap.fill(tiny_skia::Color::from_rgba8(r, g, b, 0xff));
    let [r, g, b] = CHECKER_DARK;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(r, g, b, 0xff);
    let cell = cell.max(1.0);
    let (columns, rows) = (
        (pixmap.width() as f32 / cell).ceil() as u32,
        (pixmap.height() as f32 / cell).ceil() as u32,
    );
    for row in 0..rows {
        for column in (row % 2..columns).step_by(2) {
            if let Some(rect) =
                tiny_skia::Rect::from_xywh(column as f32 * cell, row as f32 * cell, cell, cell)
            {
                pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
            }
        }
    }
}

/// The pixmap as a buffer for `slint::Image::from_rgba8_premultiplied`. Built in the worker
/// (it is `Send`); the `Image` itself is made on the UI thread.
pub fn pixels(pixmap: &Pixmap) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::clone_from_slice(pixmap.data(), pixmap.width(), pixmap.height())
}

/// `#rgb` or `#rrggbb`, with or without the `#`, as lowercase `#rrggbb` and its channels.
/// Only this normalised form is ever written into an SVG, so typed text can't break out of
/// the attribute it lands in.
pub fn parse_hex(text: &str) -> Option<(String, [u8; 3])> {
    let digits = text.trim();
    let digits = digits.strip_prefix('#').unwrap_or(digits);
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let long: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_owned(),
        _ => return None,
    };
    let long = long.to_ascii_lowercase();
    let channel = |i: usize| u8::from_str_radix(&long[i..i + 2], 16).ok();
    Some((format!("#{long}"), [channel(0)?, channel(2)?, channel(4)?]))
}

/// The start of `text` that a pane shows, cut on a character boundary.
pub fn preview(text: &str) -> &str {
    &text[..text.floor_char_boundary(PREVIEW_BYTES)]
}

/// The note under a pane that shows only `preview(full)`; empty when it shows everything.
pub fn preview_note(full: &str) -> String {
    let shown = preview(full).len();
    if shown == full.len() {
        return String::new();
    }
    format!(
        "Showing the first {} of {}. Copy and Save use all of it.",
        crate::files::format_size(shown as u64),
        crate::files::format_size(full.len() as u64)
    )
}

/// Background work where only the newest request counts: a preview re-rendered as a slider
/// moves, or an optimisation rerun as text is typed. `spawn` makes every earlier `Ticket`
/// stale; the worker checks `is_current` before starting, and the UI thread checks it again
/// before showing a result, so a slow old render never replaces a newer one.
#[derive(Default)]
pub struct Latest {
    newest: Arc<AtomicU64>,
    workers: Vec<JoinHandle<()>>,
}

/// One request's claim to be the newest; see `Latest`.
#[derive(Clone)]
pub struct Ticket {
    id: u64,
    newest: Arc<AtomicU64>,
}

impl Ticket {
    pub fn is_current(&self) -> bool {
        self.newest.load(Ordering::SeqCst) == self.id
    }
}

impl Latest {
    /// Runs `work` on a new thread with a ticket that is current until the next `spawn`.
    pub fn spawn(&mut self, work: impl FnOnce(Ticket) + Send + 'static) {
        let ticket = self.ticket();
        self.workers.retain(|w| !w.is_finished());
        self.workers.push(thread::spawn(move || work(ticket)));
    }

    /// A new current ticket without a thread, making earlier ones stale (e.g. on Clear).
    pub fn ticket(&self) -> Ticket {
        Ticket {
            id: self.newest.fetch_add(1, Ordering::SeqCst) + 1,
            newest: self.newest.clone(),
        }
    }

    /// Makes every ticket stale and hands back the threads, for `tools::shutdown_all`.
    pub fn shutdown(&mut self) -> Vec<JoinHandle<()>> {
        self.ticket();
        std::mem::take(&mut self.workers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A 2×2 opaque red PNG in a fresh temp folder.
    fn red_png(test: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("offkit-svg-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.png");
        ::image::RgbaImage::from_pixel(2, 2, ::image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        (dir, path)
    }

    fn image_svg(href: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="2" height="2"><image xlink:href="{href}" width="2" height="2"/></svg>"#
        )
    }

    /// The highest alpha in the render: 0 means nothing was drawn.
    fn max_alpha(tree: &usvg::Tree) -> u8 {
        let pixmap = render(tree, 1.0).unwrap();
        pixmap.pixels().iter().map(|p| p.alpha()).max().unwrap()
    }

    fn escaped(path: &Path) -> String {
        path.display().to_string().replace('&', "&amp;")
    }

    #[test]
    fn the_image_fixture_loads_without_the_lockdown() {
        let (dir, png) = red_png("control");
        let svg = image_svg(&escaped(&png));

        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();

        assert_eq!(max_alpha(&tree), 255);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_image_path_href_loads_nothing() {
        let (dir, png) = red_png("path");
        let svg = image_svg(&escaped(&png));

        let tree = parse(&svg).unwrap();

        assert_eq!(max_alpha(&tree), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_image_file_url_href_loads_nothing() {
        let (dir, png) = red_png("url");
        let svg = image_svg(&format!("file:///{}", escaped(&png).replace('\\', "/")));

        let tree = parse(&svg).unwrap();

        assert_eq!(max_alpha(&tree), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn fe_image_svg(href: &str) -> String {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><filter id="f" x="0" y="0" width="1" height="1"><feImage href="{href}"/></filter><rect width="2" height="2" fill="#00f" fill-opacity="0" filter="url(#f)"/></svg>"##
        )
    }

    #[test]
    fn the_fe_image_fixture_loads_without_the_lockdown() {
        let (dir, png) = red_png("fe-control");
        let svg = fe_image_svg(&escaped(&png));

        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();

        assert_eq!(max_alpha(&tree), 255);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_fe_image_path_href_loads_nothing() {
        let (dir, png) = red_png("feimage");
        let svg = fe_image_svg(&escaped(&png));

        let tree = parse(&svg).unwrap();

        assert_eq!(max_alpha(&tree), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_data_url_image_loads_nothing() {
        let (dir, png) = red_png("data");
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            std::fs::read(&png).unwrap(),
        );
        let svg = image_svg(&format!("data:image/png;base64,{encoded}"));

        let tree = parse(&svg).unwrap();

        assert_eq!(max_alpha(&tree), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn text_renders_with_the_bundled_font() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><text x="0" y="16" font-family="No Such Font" font-size="16">Hi</text></svg>"#;

        let tree = parse(svg).unwrap();

        assert_eq!(max_alpha(&tree), 255);
    }

    #[test]
    fn input_over_the_cap_is_refused() {
        let svg = format!("<svg>{}</svg>", " ".repeat(MAX_SVG_BYTES));

        let error = parse(&svg).err();

        assert_eq!(error.as_deref(), Some("is over the 10.0 MB limit"));
    }

    #[test]
    fn a_huge_declared_size_renders_within_the_pixmap_cap() {
        let tree =
            parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="50000"/>"#)
                .unwrap();

        let pixmap = render(&tree, 1.0).unwrap();

        assert_eq!((pixmap.width(), pixmap.height()), (4096, 2048));
    }

    #[test]
    fn fit_scale_fits_the_tighter_side() {
        let tree =
            parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"/>"#).unwrap();

        assert_eq!(fit_scale(&tree, 100.0, 100.0), 0.5);
    }

    #[test]
    fn hex_accepts_short_and_long_forms_with_or_without_hash() {
        assert_eq!(
            parse_hex("#ABC"),
            Some(("#aabbcc".into(), [0xaa, 0xbb, 0xcc]))
        );
        assert_eq!(
            parse_hex(" 3b82f6 "),
            Some(("#3b82f6".into(), [0x3b, 0x82, 0xf6]))
        );
    }

    #[test]
    fn hex_refuses_anything_that_could_leave_an_attribute() {
        assert_eq!(parse_hex("#12\"/><script>"), None);
        assert_eq!(parse_hex("#1234"), None);
        assert_eq!(parse_hex("red"), None);
    }

    #[test]
    fn a_new_ticket_makes_the_previous_one_stale() {
        let latest = Latest::default();
        let first = latest.ticket();

        let second = latest.ticket();

        assert!(!first.is_current());
        assert!(second.is_current());
    }
}
