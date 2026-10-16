//! CSS Blob Generator: an organic shape made from CSS `border-radius` with a separate horizontal
//! and vertical radius per corner, copied as CSS, Tailwind or SVG. Pure logic first (the
//! website's corner normalisation, presets, a seeded randomiser and the three outputs, tested
//! without a window), then the `BlobGenerator` global (ui/tools/blob-generator.slint) wired to
//! it. The preview is the SVG output rendered with resvg on a worker, so what shows is exactly
//! what is saved, and the CSS's gradient alpha is the same number.

use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::box_shadow::{LatestWorker, Rgb, rgb, slint_image};
use crate::{
    AppWindow, BlobAxis, BlobCorner, BlobCornerRow, BlobGenerator, BlobPreset, clipboard, files,
    jobs,
};

pub const SLUG: &str = "blob-generator";

// ---- Logic --------------------------------------------------------------------------------

/// One corner's radii, as percentages of the box's width (`x`) and height (`y`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Radius {
    x: i32,
    y: i32,
}

/// Top-left, top-right, bottom-right, bottom-left: the order of CSS's `border-radius`.
type Corners = [Radius; 4];

const TL: usize = 0;
const TR: usize = 1;
const BR: usize = 2;
const BL: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Axis {
    X,
    Y,
}

const fn corners(v: [(i32, i32); 4]) -> Corners {
    [
        Radius { x: v[0].0, y: v[0].1 },
        Radius { x: v[1].0, y: v[1].1 },
        Radius { x: v[2].0, y: v[2].1 },
        Radius { x: v[3].0, y: v[3].1 },
    ]
}

const INITIAL: Corners = corners([(30, 30), (70, 30), (70, 70), (30, 70)]);
const SMOOTH: Corners = corners([(42, 38), (58, 36), (56, 62), (44, 64)]);
const ORGANIC: Corners = corners([(28, 48), (72, 22), (60, 78), (40, 52)]);
const WILD: Corners = corners([(12, 84), (88, 16), (76, 92), (24, 8)]);

fn get(r: Radius, axis: Axis) -> i32 {
    match axis {
        Axis::X => r.x,
        Axis::Y => r.y,
    }
}

fn set(r: &mut Radius, axis: Axis, value: i32) {
    match axis {
        Axis::X => r.x = value,
        Axis::Y => r.y = value,
    }
}

/// The corner sharing a side with `corner` along `axis`: the top and bottom sides pair the
/// horizontal radii, the left and right sides the vertical ones.
fn neighbour(corner: usize, axis: Axis) -> usize {
    match (axis, corner) {
        (Axis::X, TL) => TR,
        (Axis::X, TR) => TL,
        (Axis::X, BR) => BL,
        (Axis::X, _) => BR,
        (Axis::Y, TL) => BL,
        (Axis::Y, TR) => BR,
        (Axis::Y, BR) => TR,
        (Axis::Y, _) => TL,
    }
}

/// The diagonally opposite corner.
fn diagonal(corner: usize) -> usize {
    (corner + 2) % 4
}

/// Keeps the side `corner` shares along `axis` at 100% or less by lowering the neighbour, so
/// the browser never has to scale the radii down (which would move every corner at once).
fn constrain(c: &mut Corners, corner: usize, axis: Axis) {
    let other = neighbour(corner, axis);
    let room = (100 - get(c[corner], axis)).max(0);
    if get(c[other], axis) > room {
        set(&mut c[other], axis, room);
    }
}

fn clamp_percent(value: i32) -> i32 {
    value.clamp(0, 100)
}

/// Clamps every radius to 0–100 and fixes all four sides, as the website does for presets.
fn sanitize(mut c: Corners) -> Corners {
    for r in &mut c {
        r.x = clamp_percent(r.x);
        r.y = clamp_percent(r.y);
    }
    constrain(&mut c, TL, Axis::X);
    constrain(&mut c, BL, Axis::X);
    constrain(&mut c, TL, Axis::Y);
    constrain(&mut c, TR, Axis::Y);
    c
}

/// One slider moved. `both` sets x and y together (4-point mode, or X/Y locked); `mirror`
/// copies the edit to the diagonal corner. Only the sides touched are constrained, so the
/// value just set never moves.
fn edit(mut c: Corners, corner: usize, axis: Axis, value: i32, both: bool, mirror: bool) -> Corners {
    let value = clamp_percent(value);
    let targets: &[usize] = if mirror {
        &[corner, diagonal(corner)]
    } else {
        &[corner]
    };
    let axes: &[Axis] = if both { &[Axis::X, Axis::Y] } else { &[axis] };
    for &t in targets {
        for &a in axes {
            set(&mut c[t], a, value);
        }
    }
    for &t in targets {
        for &a in axes {
            constrain(&mut c, t, a);
        }
    }
    c
}

/// SplitMix64: a tiny, well-mixed generator, so a seed always gives the same blob.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The website's Randomize (every radius 10–90%, then normalised), from a seed.
fn random_corners(seed: u32) -> Corners {
    let mut state = u64::from(seed);
    let mut pick = || 10 + (next(&mut state) % 81) as i32;
    sanitize([
        Radius { x: pick(), y: pick() },
        Radius { x: pick(), y: pick() },
        Radius { x: pick(), y: pick() },
        Radius { x: pick(), y: pick() },
    ])
}

/// The value of `border-radius`: the four horizontal radii, a slash, the four vertical ones.
fn radius_value(c: &Corners) -> String {
    let [tl, tr, br, bl] = c;
    format!(
        "{}% {}% {}% {}% / {}% {}% {}% {}%",
        tl.x, tr.x, br.x, bl.x, tl.y, tr.y, br.y, bl.y
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fill {
    color: Rgb,
    /// A 135° fade from the colour to the colour at `END_ALPHA`.
    gradient: bool,
}

/// The gradient's end alpha, `80` in hex as the website's CSS has it. The preview (the SVG)
/// uses the same byte; the website's own preview used `40`, which didn't match its CSS.
const END_ALPHA: u8 = 0x80;

fn css(c: &Corners, fill: Fill) -> String {
    let hex = fill.color.hex();
    let background = if fill.gradient {
        format!("background: linear-gradient(135deg, {hex}, {hex}{END_ALPHA:02x});")
    } else {
        format!("background: {hex};")
    };
    format!("border-radius: {};\n{background}", radius_value(c))
}

/// Tailwind arbitrary values in the chosen colour (the website always wrote violet).
/// `/50` is Tailwind's nearest opacity step to `END_ALPHA`.
fn tailwind(c: &Corners, fill: Fill) -> String {
    let hex = fill.color.hex();
    let background = if fill.gradient {
        format!("bg-gradient-to-br from-[{hex}] to-[{hex}]/50")
    } else {
        format!("bg-[{hex}]")
    };
    format!("rounded-[{}] {background}", radius_value(c).replace(' ', "_"))
}

/// A number for SVG: at most two decimals, no trailing zeros.
fn num(v: f64) -> String {
    format!("{}", (v * 100.0).round() / 100.0)
}

/// The same shape as an SVG `size` pixels square: four straight sides joined by quarter
/// ellipses, which is how a browser draws `border-radius`. The normalisation keeps every side's
/// radii within 100%, so no corner needs scaling down.
fn svg(c: &Corners, fill: Fill, size: u32) -> String {
    let s = f64::from(size);
    let px = |percent: i32| f64::from(percent) * s / 100.0;
    let [tl, tr, br, bl] = c.map(|r| (px(r.x), px(r.y)));
    let arc = |(rx, ry): (f64, f64), x: f64, y: f64| {
        format!("A{} {} 0 0 1 {} {}", num(rx), num(ry), num(x), num(y))
    };
    let d = [
        format!("M{} 0", num(tl.0)),
        format!("H{}", num(s - tr.0)),
        arc(tr, s, tr.1),
        format!("V{}", num(s - br.1)),
        arc(br, s - br.0, s),
        format!("H{}", num(bl.0)),
        arc(bl, 0.0, s - bl.1),
        format!("V{}", num(tl.1)),
        arc(tl, tl.0, 0.0),
        "Z".into(),
    ]
    .join("");
    let hex = fill.color.hex();
    let size = num(s);
    let open = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" \
         viewBox=\"0 0 {size} {size}\">"
    );
    if fill.gradient {
        // 135° across a square runs corner to corner.
        let end = num(f64::from(END_ALPHA) / 255.0);
        format!(
            "{open}\n  <defs>\n    <linearGradient id=\"blob-fill\" x1=\"0\" y1=\"0\" \
             x2=\"{size}\" y2=\"{size}\" gradientUnits=\"userSpaceOnUse\">\n      \
             <stop offset=\"0\" stop-color=\"{hex}\"/>\n      \
             <stop offset=\"1\" stop-color=\"{hex}\" stop-opacity=\"{end}\"/>\n    \
             </linearGradient>\n  </defs>\n  <path d=\"{d}\" fill=\"url(#blob-fill)\"/>\n</svg>\n"
        )
    } else {
        format!("{open}\n  <path d=\"{d}\" fill=\"{hex}\"/>\n</svg>\n")
    }
}

/// The SVG drawn at `scale` physical pixels per CSS pixel.
fn render(svg: &str, scale: f32) -> Result<Pixmap, String> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size().scale_by(scale).ok_or("empty size")?;
    let mut pixmap = Pixmap::new(size.width(), size.height()).ok_or("empty size")?;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

// ---- State and wiring ---------------------------------------------------------------------

const DEFAULT_COLOR: Rgb = rgb(0x8b5cf6);
const DEFAULT_SIZE: u32 = 280;

struct State {
    corners: Corners,
    /// The last valid colour; a half-typed one leaves it alone.
    color: Rgb,
    svg: String,
    worker: Option<LatestWorker<(String, f32)>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            corners: INITIAL,
            color: DEFAULT_COLOR,
            svg: String::new(),
            worker: None,
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

fn corner_index(corner: BlobCorner) -> usize {
    match corner {
        BlobCorner::TopLeft => TL,
        BlobCorner::TopRight => TR,
        BlobCorner::BottomRight => BR,
        BlobCorner::BottomLeft => BL,
    }
}

pub fn init(app: &AppWindow) {
    let weak = app.as_weak();
    let worker = LatestWorker::spawn("blob", move |(svg, scale): (String, f32)| {
        match render(&svg, scale) {
            Ok(pixmap) => jobs::post(&weak, move |app| {
                app.global::<BlobGenerator>().set_preview(slint_image(&pixmap));
            }),
            Err(e) => eprintln!("couldn't draw the blob preview: {e}"),
        }
    });
    with(|s| s.worker = Some(worker));

    let g = app.global::<BlobGenerator>();
    g.set_eight_point(true);
    g.set_gradient(true);
    g.set_seed(1.0);
    g.set_max_seed(MAX_SEED as f32);
    g.set_size(DEFAULT_SIZE as f32);
    g.set_color_hex(DEFAULT_COLOR.hex().into());
    g.set_color(DEFAULT_COLOR.slint(u8::MAX));

    let weak = app.as_weak();
    g.on_edit(move |corner, axis, value| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<BlobGenerator>();
        let both = axis == BlobAxis::Both || g.get_lock_axes() || !g.get_eight_point();
        let axis = if axis == BlobAxis::Y { Axis::Y } else { Axis::X };
        let mirror = g.get_mirror();
        with(|s| {
            s.corners = edit(
                s.corners,
                corner_index(corner),
                axis,
                value.round() as i32,
                both,
                mirror,
            );
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_apply_preset(move |preset| {
        let Some(app) = weak.upgrade() else { return };
        let corners = match preset {
            BlobPreset::Smooth => SMOOTH,
            BlobPreset::Organic => ORGANIC,
            BlobPreset::Wild => WILD,
        };
        with(|s| s.corners = sanitize(corners));
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_randomize(move || {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<BlobGenerator>();
        let next_seed = (g.get_seed() as u32 % MAX_SEED) + 1;
        g.set_seed(next_seed as f32);
        with(|s| s.corners = random_corners(next_seed));
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_seed_changed(move |seed| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.corners = random_corners(seed.round() as u32));
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_color_edited(move |text| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<BlobGenerator>();
        let parsed = Rgb::parse(&text);
        g.set_color_invalid(parsed.is_none());
        if let Some(color) = parsed {
            g.set_color(color.slint(u8::MAX));
            with(|s| s.color = color);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_color_accepted(move |text| {
        let Some(app) = weak.upgrade() else { return };
        // Tidies a valid entry ("8B5" → "#88bb55"); an invalid one stays for fixing.
        if let Some(color) = Rgb::parse(&text) {
            app.global::<BlobGenerator>()
                .set_color_hex(color.hex().into());
        }
    });
    let weak = app.as_weak();
    g.on_copy(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let g = app.global::<BlobGenerator>();
        let code = match g.get_format_index() {
            1 => g.get_tailwind(),
            2 => g.get_svg(),
            _ => g.get_css(),
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
    let weak = app.as_weak();
    g.on_save_svg(move || {
        if let Some(app) = weak.upgrade() {
            save_svg(&app);
        }
    });
    refresh(app);
}

/// The highest seed; Randomize wraps back to 1 after it.
const MAX_SEED: u32 = 99_999;

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.worker.as_mut().and_then(LatestWorker::shutdown))
        .into_iter()
        .collect()
}

fn save_svg(app: &AppWindow) {
    let g = app.global::<BlobGenerator>();
    let Some(path) = files::choose_save_path("Save SVG", "blob.svg") else {
        return;
    };
    let svg = with(|s| s.svg.clone());
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        std::fs::write(output.path(), svg)?;
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

/// Shows the corners and the three outputs, and asks for a new preview.
fn refresh(app: &AppWindow) {
    let g = app.global::<BlobGenerator>();
    let scale = app.window().scale_factor();
    let size = g.get_size().round() as u32;
    let gradient = g.get_gradient();
    with(|s| {
        let c = &s.corners;
        let fill = Fill {
            color: s.color,
            gradient,
        };
        let names = ["Top left", "Top right", "Bottom right", "Bottom left"];
        let ids = [
            BlobCorner::TopLeft,
            BlobCorner::TopRight,
            BlobCorner::BottomRight,
            BlobCorner::BottomLeft,
        ];
        let rows: Vec<BlobCornerRow> = (0..4)
            .map(|i| BlobCornerRow {
                corner: ids[i],
                name: names[i].into(),
                x: c[i].x as f32,
                y: c[i].y as f32,
            })
            .collect();
        g.set_corners(ModelRc::new(VecModel::from(rows)));
        g.set_css(css(c, fill).into());
        g.set_tailwind(tailwind(c, fill).into());
        s.svg = svg(c, fill, size);
        g.set_svg(s.svg.clone().into());
        g.set_note("".into());
        if let Some(worker) = &s.worker {
            worker.send((s.svg.clone(), scale));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIOLET: Fill = Fill {
        color: DEFAULT_COLOR,
        gradient: true,
    };

    #[test]
    fn radius_value_lists_horizontal_then_vertical_radii() {
        assert_eq!(radius_value(&INITIAL), "30% 70% 70% 30% / 30% 30% 70% 70%");
    }

    #[test]
    fn css_matches_the_website_template() {
        assert_eq!(
            css(&INITIAL, VIOLET),
            "border-radius: 30% 70% 70% 30% / 30% 30% 70% 70%;\n\
             background: linear-gradient(135deg, #8b5cf6, #8b5cf680);"
        );
    }

    #[test]
    fn css_without_gradient_is_a_flat_colour() {
        let flat = Fill { gradient: false, ..VIOLET };

        assert_eq!(
            css(&INITIAL, flat),
            "border-radius: 30% 70% 70% 30% / 30% 30% 70% 70%;\nbackground: #8b5cf6;"
        );
    }

    #[test]
    fn tailwind_uses_the_chosen_colour() {
        let teal = Fill { color: rgb(0x14b8a6), gradient: true };

        assert_eq!(
            tailwind(&INITIAL, teal),
            "rounded-[30%_70%_70%_30%_/_30%_30%_70%_70%] bg-gradient-to-br from-[#14b8a6] \
             to-[#14b8a6]/50"
        );
    }

    #[test]
    fn tailwind_without_gradient_is_a_flat_colour() {
        let flat = Fill { gradient: false, ..VIOLET };

        assert_eq!(
            tailwind(&INITIAL, flat),
            "rounded-[30%_70%_70%_30%_/_30%_30%_70%_70%] bg-[#8b5cf6]"
        );
    }

    #[test]
    fn sanitize_caps_each_side_at_100_percent() {
        assert_eq!(
            sanitize(WILD),
            corners([(12, 84), (88, 16), (76, 84), (24, 8)])
        );
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        assert_eq!(
            sanitize(corners([(-5, 120), (0, 0), (0, 0), (0, 0)])),
            corners([(0, 100), (0, 0), (0, 0), (0, 0)])
        );
    }

    #[test]
    fn edit_lowers_the_neighbour_to_keep_the_side_valid() {
        let edited = edit(INITIAL, TL, Axis::X, 80, false, false);

        assert_eq!(edited, corners([(80, 30), (20, 30), (70, 70), (30, 70)]));
    }

    #[test]
    fn edit_with_both_axes_sets_x_and_y() {
        let edited = edit(INITIAL, TR, Axis::X, 40, true, false);

        assert_eq!(edited, corners([(30, 30), (40, 40), (70, 60), (30, 70)]));
    }

    #[test]
    fn edit_with_mirror_copies_to_the_diagonal_corner() {
        let edited = edit(INITIAL, TL, Axis::Y, 20, false, true);

        assert_eq!(edited, corners([(30, 20), (70, 30), (70, 20), (30, 70)]));
    }

    #[test]
    fn random_corners_are_the_same_for_the_same_seed() {
        assert_eq!(random_corners(42), random_corners(42));
    }

    #[test]
    fn random_corners_differ_between_seeds() {
        assert_ne!(random_corners(1), random_corners(2));
    }

    #[test]
    fn random_corners_for_seed_one_are_fixed() {
        assert_eq!(
            random_corners(1),
            corners([(42, 62), (58, 30), (81, 70), (19, 38)])
        );
    }

    #[test]
    fn svg_draws_quarter_ellipses_between_straight_sides() {
        let flat = Fill { gradient: false, ..VIOLET };

        assert_eq!(
            svg(&INITIAL, flat, 200),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\" \
             viewBox=\"0 0 200 200\">\n  <path d=\"M60 0H60A140 60 0 0 1 200 60V60\
             A140 140 0 0 1 60 200H60A60 140 0 0 1 0 60V60A60 60 0 0 1 60 0Z\" \
             fill=\"#8b5cf6\"/>\n</svg>\n"
        );
    }

    #[test]
    fn the_preview_fades_to_the_same_alpha_as_the_css() {
        let square = corners([(0, 0), (0, 0), (0, 0), (0, 0)]);

        let pixmap = render(&svg(&square, VIOLET, 100), 1.0).unwrap();

        // The far corner of a 135° gradient is its end: `END_ALPHA`, give or take rounding.
        let end = pixmap.pixel(99, 99).unwrap().alpha();
        assert!(end.abs_diff(END_ALPHA) <= 2, "{end}");
        let start = pixmap.pixel(0, 0).unwrap().alpha();
        assert!(start.abs_diff(u8::MAX) <= 2, "{start}");
    }

    #[test]
    fn the_preview_leaves_the_rounded_corners_empty() {
        let circle = corners([(50, 50), (50, 50), (50, 50), (50, 50)]);

        let pixmap = render(&svg(&circle, VIOLET, 100), 1.0).unwrap();

        assert_eq!(pixmap.pixel(2, 2).unwrap().alpha(), 0);
        assert!(pixmap.pixel(50, 50).unwrap().alpha() > 0);
    }
}
