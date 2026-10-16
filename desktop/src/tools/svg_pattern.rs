//! SVG Pattern Generator: nine seamless tile patterns. The CSS and SVG it emits are the
//! website's strings exactly (src/app/(tools)/svg-pattern/page.tsx), number formatting
//! included; the preview is that SVG rendered with resvg at the display's resolution, so tiles
//! show at their true pixel size. Rendering runs off the UI thread (`svg::Latest`).

use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::engine::svg::{self, Latest};
use crate::select::index;
use crate::{AppWindow, SvgPattern, SvgPatternOption, clipboard, files, jobs};

pub const SLUG: &str = "svg-pattern";
const EXPORT_WIDTH: f64 = 1600.0;
const EXPORT_HEIGHT: f64 = 900.0;
/// The picker's thumbnails: a tile this many pixels wide at width 1, as on the website.
const THUMB_SIZE: f64 = 24.0;
const DEFAULT_BG: &str = "#0f172a";
const DEFAULT_FG: &str = "#3b82f6";

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Grid,
    Dots,
    Diagonal,
    Cross,
    Waves,
    Zigzag,
    Rings,
    Checker,
    VerticalLines,
}

impl Kind {
    /// The picker's order, the website's.
    const ALL: [Kind; 9] = [
        Kind::Grid,
        Kind::Dots,
        Kind::Diagonal,
        Kind::Cross,
        Kind::Waves,
        Kind::Zigzag,
        Kind::Rings,
        Kind::Checker,
        Kind::VerticalLines,
    ];

    /// The website's name for it, which is also the exported file's name.
    fn key(self) -> &'static str {
        match self {
            Kind::Grid => "grid",
            Kind::Dots => "dots",
            Kind::Diagonal => "diagonal",
            Kind::Cross => "cross",
            Kind::Waves => "waves",
            Kind::Zigzag => "zigzag",
            Kind::Rings => "rings",
            Kind::Checker => "checker",
            Kind::VerticalLines => "verticalLines",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Grid => "Grid",
            Kind::Dots => "Dots",
            Kind::Diagonal => "Diagonal",
            Kind::Cross => "Cross",
            Kind::Waves => "Waves",
            Kind::Zigzag => "Zigzag",
            Kind::Rings => "Rings",
            Kind::Checker => "Checker",
            Kind::VerticalLines => "Vertical lines",
        }
    }

    /// Checker squares fill whole cells, so it ignores the width setting.
    fn uses_width(self) -> bool {
        self != Kind::Checker
    }

    fn tile_height(self, size: f64) -> f64 {
        if self == Kind::Waves { size / 2.0 } else { size }
    }

    /// The pattern's shapes for one tile; `fg` is the 8-digit colour, `w` the width setting.
    fn body(self, fg: &str, s: f64, w: f64) -> String {
        match self {
            Kind::Grid => format!(
                r#"<path d="M {s} 0 L 0 0 0 {s}" fill="none" stroke="{fg}" stroke-width="{w}"/>"#
            ),
            Kind::Dots => format!(
                r#"<circle cx="{}" cy="{}" r="{}" fill="{fg}"/>"#,
                s / 2.0,
                s / 2.0,
                (s / 8.0) * w
            ),
            Kind::Diagonal => format!(
                r#"<path d="M0 {s} L{s} 0" stroke="{fg}" stroke-width="{w}" fill="none"/>"#
            ),
            Kind::Cross => format!(
                r#"<path d="M{} 0v{s}M0 {}h{s}" stroke="{fg}" stroke-width="{w}" fill="none"/>"#,
                s / 2.0,
                s / 2.0
            ),
            Kind::Waves => format!(
                r#"<path d="M0 {} Q {} 0 {} {} T {s} {}" stroke="{fg}" stroke-width="{w}" fill="none"/>"#,
                s / 4.0,
                s / 4.0,
                s / 2.0,
                s / 4.0,
                s / 4.0
            ),
            Kind::Zigzag => format!(
                r#"<path d="M0 {} L {} 0 L {} {} L {} 0 L {s} {}" stroke="{fg}" stroke-width="{w}" fill="none"/>"#,
                s / 2.0,
                s / 4.0,
                s / 2.0,
                s / 2.0,
                s * 0.75,
                s / 2.0
            ),
            Kind::Rings => format!(
                r#"<circle cx="{}" cy="{}" r="{}" stroke="{fg}" stroke-width="{w}" fill="none"/>"#,
                s / 2.0,
                s / 2.0,
                s / 3.0
            ),
            Kind::Checker => {
                let h = s / 2.0;
                format!(
                    r#"<rect width="{h}" height="{h}" fill="{fg}"/><rect x="{h}" y="{h}" width="{h}" height="{h}" fill="{fg}"/>"#
                )
            }
            Kind::VerticalLines => format!(
                r#"<path d="M{} 0 V{s}" stroke="{fg}" stroke-width="{w}"/>"#,
                s / 2.0
            ),
        }
    }
}

/// What the controls describe; `bg` and `fg` are validated `#rrggbb`.
#[derive(Clone, Debug, PartialEq)]
struct Pattern {
    kind: Kind,
    size: f64,
    width: f64,
    /// 0 to 100.
    opacity: f64,
    bg: String,
    fg: String,
}

impl Pattern {
    /// The foreground with the opacity as a two-digit alpha, e.g. `#3b82f680` at 50%.
    fn fg_with_alpha(&self) -> String {
        let alpha = ((self.opacity / 100.0) * 255.0).round() as u8;
        format!("{}{alpha:02x}", self.fg)
    }

    /// One tile on its background: what the CSS embeds.
    fn tile_svg(&self) -> String {
        tile_svg(self.kind, &self.bg, &self.fg_with_alpha(), self.size, self.width)
    }

    /// A `width` × `height` canvas filled with the pattern: the export at 1600 × 900, and the
    /// preview at the size it's shown.
    fn canvas_svg(&self, width: f64, height: f64) -> String {
        let (s, bg) = (self.size, &self.bg);
        let tile_height = self.kind.tile_height(s);
        let body = self.kind.body(&self.fg_with_alpha(), s, self.width);
        format!(
            r#"<svg width="{width}" height="{height}" viewBox="0 0 {width} {height}" xmlns="http://www.w3.org/2000/svg"><defs><pattern id="pattern" patternUnits="userSpaceOnUse" width="{s}" height="{tile_height}">{body}</pattern></defs><rect width="{width}" height="{height}" fill="{bg}"/><rect width="{width}" height="{height}" fill="url(#pattern)"/></svg>"#
        )
    }

    fn export_svg(&self) -> String {
        self.canvas_svg(EXPORT_WIDTH, EXPORT_HEIGHT)
    }

    fn css(&self) -> String {
        format!(
            "background-color: {};\nbackground-image: url(\"data:image/svg+xml;utf8,{}\");",
            self.bg,
            encode_uri_component(&self.tile_svg())
        )
    }

    /// "Dots · 32 × 32 px". The website always printed size × size; waves tiles are half height.
    fn caption(&self) -> String {
        format!(
            "{} · {} × {} px",
            self.kind.label(),
            self.size,
            self.kind.tile_height(self.size)
        )
    }
}

fn tile_svg(kind: Kind, bg: &str, fg: &str, size: f64, width: f64) -> String {
    let tile_height = kind.tile_height(size);
    let body = kind.body(fg, size, width);
    format!(
        r#"<svg width="{size}" height="{tile_height}" viewBox="0 0 {size} {tile_height}" xmlns="http://www.w3.org/2000/svg"><rect width="{size}" height="{tile_height}" fill="{bg}"/>{body}</svg>"#
    )
}

/// JavaScript's `encodeURIComponent`: everything but `A–Z a–z 0–9 - _ . ! ~ * ' ( )` becomes
/// `%XX` per UTF-8 byte, in upper-case hex.
fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ---- State and wiring ---------------------------------------------------------------------

struct State {
    /// The last valid colours: a half-typed hex keeps the pattern on these.
    bg: String,
    fg: String,
    /// The preview's size in logical pixels, from the view.
    canvas: (f32, f32),
    renders: Latest,
}

impl Default for State {
    fn default() -> Self {
        Self {
            bg: DEFAULT_BG.into(),
            fg: DEFAULT_FG.into(),
            canvas: (0.0, 0.0),
            renders: Latest::default(),
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
    let g = app.global::<SvgPattern>();
    g.set_options(ModelRc::new(VecModel::from(
        Kind::ALL
            .iter()
            .map(|k| SvgPatternOption {
                label: k.label().into(),
                thumbnail: slint::Image::default(),
            })
            .collect::<Vec<_>>(),
    )));
    g.set_bg_hex(DEFAULT_BG.into());
    g.set_fg_hex(DEFAULT_FG.into());
    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_bg_edited(move |text| {
        let Some(app) = weak.upgrade() else { return };
        let valid = svg::parse_hex(&text).map(|(hex, _)| hex);
        app.global::<SvgPattern>().set_bg_invalid(valid.is_none());
        if let Some(hex) = valid {
            with(|s| s.bg = hex);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_fg_edited(move |text| {
        let Some(app) = weak.upgrade() else { return };
        let valid = svg::parse_hex(&text).map(|(hex, _)| hex);
        app.global::<SvgPattern>().set_fg_invalid(valid.is_none());
        if let Some(hex) = valid {
            with(|s| s.fg = hex);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_preview_resized(move |width, height| {
        let Some(app) = weak.upgrade() else { return };
        let changed = with(|s| std::mem::replace(&mut s.canvas, (width, height)) != (width, height));
        if changed {
            render(&app);
        }
    });
    let weak = app.as_weak();
    g.on_copy_css(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        copy(&app, pattern(&app).css())
    });
    let weak = app.as_weak();
    g.on_copy_svg(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        copy(&app, pattern(&app).export_svg())
    });
    let weak = app.as_weak();
    g.on_save_svg(move || {
        if let Some(app) = weak.upgrade() {
            save(&app);
        }
    });
    refresh(app);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Stops pending renders and hands back their threads.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.renders.shutdown())
}

/// The pattern the controls describe. Snaps the scale to the website's even steps.
fn pattern(app: &AppWindow) -> Pattern {
    let g = app.global::<SvgPattern>();
    let size = ((g.get_size() / 2.0).round() * 2.0).clamp(8.0, 128.0);
    if size != g.get_size() {
        g.set_size(size);
    }
    let (bg, fg) = with(|s| (s.bg.clone(), s.fg.clone()));
    Pattern {
        kind: Kind::ALL[index(g.get_current_index(), Kind::ALL.len())],
        size: f64::from(size),
        // Tenths, so 1.1 is the same double the website's slider gives (11 / 10 rounds to it).
        width: f64::from(g.get_width_tenths().round()) / 10.0,
        opacity: f64::from(g.get_opacity().round()),
        bg,
        fg,
    }
}

fn refresh(app: &AppWindow) {
    let g = app.global::<SvgPattern>();
    let p = pattern(app);
    let colour = |hex: &str| {
        svg::parse_hex(hex)
            .map(|(_, [r, g, b])| slint::Color::from_rgb_u8(r, g, b))
            .unwrap_or_default()
    };
    g.set_bg(colour(&p.bg));
    g.set_fg(colour(&p.fg));
    g.set_uses_width(p.kind.uses_width());
    g.set_caption(p.caption().into());
    g.set_css(p.css().into());
    g.set_svg(p.export_svg().into());
    render(app);
}

/// Renders the preview and the picker's thumbnails in the background.
fn render(app: &AppWindow) {
    let p = pattern(app);
    let scale = app.window().scale_factor();
    let weak = app.as_weak();
    with(|s| {
        let (width, height) = s.canvas;
        s.renders.spawn(move |ticket| {
            if !ticket.is_current() {
                return;
            }
            let preview = (width >= 1.0 && height >= 1.0)
                .then(|| p.canvas_svg(f64::from(width.ceil()), f64::from(height.ceil())))
                .and_then(|text| rasterise(&text, scale));
            let fg = p.fg_with_alpha();
            let thumbnails: Vec<_> = Kind::ALL
                .iter()
                .map(|k| rasterise(&tile_svg(*k, "transparent", &fg, THUMB_SIZE, 1.0), scale))
                .collect();
            jobs::post(&weak, move |app| {
                if !ticket.is_current() {
                    return;
                }
                let g = app.global::<SvgPattern>();
                let image = |pixels: Option<_>| {
                    pixels
                        .map(slint::Image::from_rgba8_premultiplied)
                        .unwrap_or_default()
                };
                g.set_preview(image(preview));
                g.set_options(ModelRc::new(VecModel::from(
                    Kind::ALL
                        .iter()
                        .zip(thumbnails)
                        .map(|(k, pixels)| SvgPatternOption {
                            label: k.label().into(),
                            thumbnail: image(pixels),
                        })
                        .collect::<Vec<_>>(),
                )));
            });
        });
    });
}

/// Our own markup, so a failure is a bug: logged, and the preview stays empty.
fn rasterise(text: &str, scale: f32) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    match svg::parse(text) {
        Ok(tree) => svg::render(&tree, scale).map(|pixmap| svg::pixels(&pixmap)),
        Err(e) => {
            eprintln!("the generated pattern {e}");
            None
        }
    }
}

fn copy(app: &AppWindow, text: String) -> bool {
    match clipboard::copy_text(&text) {
        Ok(()) => true,
        Err(e) => {
            app.global::<SvgPattern>()
                .set_error(format!("Couldn't copy: {e}").into());
            false
        }
    }
}

fn save(app: &AppWindow) {
    let p = pattern(app);
    let g = app.global::<SvgPattern>();
    let Some(path) = files::choose_save_path("Export SVG", &format!("{}.svg", p.kind.key())) else {
        return;
    };
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        std::fs::write(output.path(), p.export_svg())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The website's defaults, with per-test overrides.
    fn pattern(kind: Kind) -> Pattern {
        Pattern {
            kind,
            size: 32.0,
            width: 1.0,
            opacity: 50.0,
            bg: DEFAULT_BG.into(),
            fg: DEFAULT_FG.into(),
        }
    }

    #[test]
    fn opacity_becomes_a_two_digit_alpha() {
        let p = Pattern {
            opacity: 2.0,
            ..pattern(Kind::Dots)
        };

        assert_eq!(p.fg_with_alpha(), "#3b82f605");
    }

    #[test]
    fn half_opacity_rounds_up_like_math_round() {
        assert_eq!(pattern(Kind::Dots).fg_with_alpha(), "#3b82f680");
    }

    #[test]
    fn dots_tile_matches_the_website() {
        assert_eq!(
            pattern(Kind::Dots).tile_svg(),
            r##"<svg width="32" height="32" viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><rect width="32" height="32" fill="#0f172a"/><circle cx="16" cy="16" r="4" fill="#3b82f680"/></svg>"##
        );
    }

    #[test]
    fn waves_tile_is_half_height() {
        let p = Pattern {
            size: 10.0,
            width: 1.5,
            ..pattern(Kind::Waves)
        };

        assert_eq!(
            p.tile_svg(),
            r##"<svg width="10" height="5" viewBox="0 0 10 5" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="5" fill="#0f172a"/><path d="M0 2.5 Q 2.5 0 5 2.5 T 10 2.5" stroke="#3b82f680" stroke-width="1.5" fill="none"/></svg>"##
        );
    }

    #[test]
    fn every_body_matches_the_website_template() {
        let bodies: Vec<String> = Kind::ALL
            .iter()
            .map(|k| k.body("#ffffff80", 20.0, 1.1))
            .collect();

        assert_eq!(
            bodies,
            [
                r##"<path d="M 20 0 L 0 0 0 20" fill="none" stroke="#ffffff80" stroke-width="1.1"/>"##,
                r##"<circle cx="10" cy="10" r="2.75" fill="#ffffff80"/>"##,
                r##"<path d="M0 20 L20 0" stroke="#ffffff80" stroke-width="1.1" fill="none"/>"##,
                r##"<path d="M10 0v20M0 10h20" stroke="#ffffff80" stroke-width="1.1" fill="none"/>"##,
                r##"<path d="M0 5 Q 5 0 10 5 T 20 5" stroke="#ffffff80" stroke-width="1.1" fill="none"/>"##,
                r##"<path d="M0 10 L 5 0 L 10 10 L 15 0 L 20 10" stroke="#ffffff80" stroke-width="1.1" fill="none"/>"##,
                r##"<circle cx="10" cy="10" r="6.666666666666667" stroke="#ffffff80" stroke-width="1.1" fill="none"/>"##,
                r##"<rect width="10" height="10" fill="#ffffff80"/><rect x="10" y="10" width="10" height="10" fill="#ffffff80"/>"##,
                r##"<path d="M10 0 V20" stroke="#ffffff80" stroke-width="1.1"/>"##,
            ]
        );
    }

    #[test]
    fn dot_radius_keeps_javascripts_float_digits() {
        let body = Kind::Dots.body("#000000ff", 32.0, 1.1);

        assert_eq!(
            body,
            r##"<circle cx="16" cy="16" r="4.4" fill="#000000ff"/>"##
        );
    }

    #[test]
    fn export_svg_matches_the_website() {
        assert_eq!(
            pattern(Kind::Grid).export_svg(),
            r##"<svg width="1600" height="900" viewBox="0 0 1600 900" xmlns="http://www.w3.org/2000/svg"><defs><pattern id="pattern" patternUnits="userSpaceOnUse" width="32" height="32"><path d="M 32 0 L 0 0 0 32" fill="none" stroke="#3b82f680" stroke-width="1"/></pattern></defs><rect width="1600" height="900" fill="#0f172a"/><rect width="1600" height="900" fill="url(#pattern)"/></svg>"##
        );
    }

    #[test]
    fn css_embeds_the_tile_like_encode_uri_component() {
        let p = Pattern {
            size: 8.0,
            ..pattern(Kind::Checker)
        };

        assert_eq!(
            p.css(),
            "background-color: #0f172a;\nbackground-image: url(\"data:image/svg+xml;utf8,%3Csvg%20width%3D%228%22%20height%3D%228%22%20viewBox%3D%220%200%208%208%22%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%3E%3Crect%20width%3D%228%22%20height%3D%228%22%20fill%3D%22%230f172a%22%2F%3E%3Crect%20width%3D%224%22%20height%3D%224%22%20fill%3D%22%233b82f680%22%2F%3E%3Crect%20x%3D%224%22%20y%3D%224%22%20width%3D%224%22%20height%3D%224%22%20fill%3D%22%233b82f680%22%2F%3E%3C%2Fsvg%3E\");"
        );
    }

    #[test]
    fn encode_uri_component_keeps_its_unreserved_marks() {
        assert_eq!(encode_uri_component("a-_.!~*'()é #"), "a-_.!~*'()%C3%A9%20%23");
    }

    #[test]
    fn caption_gives_the_real_tile_size() {
        assert_eq!(pattern(Kind::Waves).caption(), "Waves · 32 × 16 px");
    }

    #[test]
    fn a_transparent_thumbnail_background_draws_nothing() {
        let tree = svg::parse(&tile_svg(Kind::Checker, "transparent", "#00000000", 24.0, 1.0)).unwrap();

        let pixmap = svg::render(&tree, 1.0).unwrap();

        assert!(pixmap.pixels().iter().all(|p| p.alpha() == 0));
    }
}
