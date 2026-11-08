//! Palette Maker: a palette of 2 to 10 colours with hex fields, locks, add, remove and
//! move-up/move-down buttons. Randomise replaces the unlocked colours. The palette copies as CSS
//! custom properties, a Tailwind theme or JSON, in the same shapes as the website. Everything
//! runs on the UI thread.
//!
//! A field that isn't `#rrggbb` is marked invalid and keeps its text; the palette (and so every
//! export) keeps the last valid colour for it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::engine::color::{self, Rgba};
use crate::{AppWindow, MakerColor, PaletteMaker, clipboard};

pub const SLUG: &str = "palette-maker";

const MIN_COLORS: usize = 2;
const MAX_COLORS: usize = 10;
const DEFAULT_COLORS: [&str; 4] = ["#2563EB", "#8B5CF6", "#EC4899", "#F59E0B"];

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct Row {
    /// What the hex field shows.
    text: String,
    /// The last valid colour.
    rgba: Rgba,
    locked: bool,
    invalid: bool,
}

impl Row {
    fn new(rgba: Rgba) -> Self {
        Row {
            text: color::hex(rgba),
            rgba,
            locked: false,
            invalid: false,
        }
    }
}

struct State {
    rows: Vec<Row>,
    rng: u64,
}

/// `#rrggbb` only, in either case.
fn strict_hex(text: &str) -> Option<Rgba> {
    let t = text.trim();
    let digits = t.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    color::parse(t).map(|(_, c)| c)
}

/// xorshift64: enough for picking colours.
fn next(rng: &mut u64) -> u64 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    *rng
}

/// A random colour that is neither near black nor near white.
fn random_color(rng: &mut u64) -> Rgba {
    let mut channel = || 40 + u8::try_from(next(rng) % 176).unwrap_or(0);
    Rgba {
        r: channel(),
        g: channel(),
        b: channel(),
        a: 1.0,
    }
}

fn randomise(rows: &mut [Row], rng: &mut u64) {
    for row in rows.iter_mut().filter(|row| !row.locked) {
        *row = Row::new(random_color(rng));
    }
}

/// Swaps row `i` with its neighbour `delta` away, when there is one.
fn move_row(rows: &mut [Row], i: usize, delta: isize) {
    let Some(j) = i.checked_add_signed(delta) else {
        return;
    };
    if i < rows.len() && j < rows.len() {
        rows.swap(i, j);
    }
}

fn hexes(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| color::hex(row.rgba).to_lowercase())
        .collect()
}

fn css(colors: &[String]) -> String {
    let lines: Vec<String> = colors
        .iter()
        .enumerate()
        .map(|(i, c)| format!("  --color-{}: {c};", i + 1))
        .collect();
    format!(":root {{\n{}\n}}", lines.join("\n"))
}

fn tailwind(colors: &[String]) -> String {
    let lines: Vec<String> = colors
        .iter()
        .enumerate()
        .map(|(i, c)| format!("        \"palette-{}\": \"{c}\"", i + 1))
        .collect();
    format!(
        "{{\n  \"theme\": {{\n    \"extend\": {{\n      \"colors\": {{\n{}\n      }}\n    }}\n  }}\n}}",
        lines.join(",\n")
    )
}

fn json(colors: &[String]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ "colors": colors })).unwrap_or_default()
}

// ---- Wiring -------------------------------------------------------------------------------

pub fn init(app: &AppWindow) {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1;
    let rows = DEFAULT_COLORS
        .iter()
        .filter_map(|hex| strict_hex(hex))
        .map(Row::new)
        .collect();
    let state = Rc::new(RefCell::new(State { rows, rng: seed }));
    let g = app.global::<PaletteMaker>();

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_edit(move |i, text| {
        let (Some(app), Ok(i)) = (weak.upgrade(), usize::try_from(i)) else {
            return;
        };
        let mut st = st.borrow_mut();
        if let Some(row) = st.rows.get_mut(i) {
            row.text = text.to_string();
            match strict_hex(&text) {
                Some(c) => {
                    row.rgba = c;
                    row.invalid = false;
                }
                None => row.invalid = true,
            }
            refresh_row(&app, i, row);
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_set_locked(move |i, locked| {
        let (Some(app), Ok(i)) = (weak.upgrade(), usize::try_from(i)) else {
            return;
        };
        let mut st = st.borrow_mut();
        if let Some(row) = st.rows.get_mut(i) {
            row.locked = locked;
            refresh_row(&app, i, row);
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_move_color(move |i, delta| {
        if let (Some(app), Ok(i), Ok(delta)) =
            (weak.upgrade(), usize::try_from(i), isize::try_from(delta))
        {
            let mut st = st.borrow_mut();
            move_row(&mut st.rows, i, delta);
            show(&app, &st.rows);
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_remove(move |i| {
        if let (Some(app), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
            let mut st = st.borrow_mut();
            if st.rows.len() > MIN_COLORS && i < st.rows.len() {
                st.rows.remove(i);
                show(&app, &st.rows);
            }
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_add(move || {
        if let Some(app) = weak.upgrade() {
            let mut st = st.borrow_mut();
            if st.rows.len() < MAX_COLORS {
                let c = random_color(&mut st.rng);
                st.rows.push(Row::new(c));
                show(&app, &st.rows);
            }
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_randomise(move || {
        if let Some(app) = weak.upgrade() {
            let mut st = st.borrow_mut();
            let State { rows, rng } = &mut *st;
            randomise(rows, rng);
            show(&app, &st.rows);
        }
    });

    let (weak, st) = (app.as_weak(), state.clone());
    g.on_copy_css(move || copy(&weak, &css(&hexes(&st.borrow().rows))));
    let (weak, st) = (app.as_weak(), state.clone());
    g.on_copy_tailwind(move || copy(&weak, &tailwind(&hexes(&st.borrow().rows))));
    let (weak, st) = (app.as_weak(), state.clone());
    g.on_copy_json(move || copy(&weak, &json(&hexes(&st.borrow().rows))));

    show(app, &state.borrow().rows);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Nothing runs in the background.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}

fn copy(weak: &slint::Weak<AppWindow>, text: &str) -> bool {
    let Some(app) = weak.upgrade() else {
        return false;
    };
    let g = app.global::<PaletteMaker>();
    match clipboard::copy_text(text) {
        Ok(()) => {
            g.set_error("".into());
            true
        }
        Err(e) => {
            g.set_error(format!("Couldn't copy: {e}").into());
            false
        }
    }
}

fn item(row: &Row) -> MakerColor {
    MakerColor {
        color: color::slint_color(row.rgba),
        hex: row.text.clone().into(),
        locked: row.locked,
        invalid: row.invalid,
    }
}

/// Updates one row in place, so the field being typed in keeps its focus.
fn refresh_row(app: &AppWindow, i: usize, row: &Row) {
    app.global::<PaletteMaker>()
        .get_colors()
        .set_row_data(i, item(row));
}

/// Rebuilds the whole list and the add/remove limits.
fn show(app: &AppWindow, rows: &[Row]) {
    let g = app.global::<PaletteMaker>();
    g.set_can_add(rows.len() < MAX_COLORS);
    g.set_can_remove(rows.len() > MIN_COLORS);
    g.set_colors(ModelRc::new(VecModel::from(
        rows.iter().map(item).collect::<Vec<_>>(),
    )));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> Rgba {
        strict_hex(hex).unwrap()
    }

    fn rows(hexes: &[&str]) -> Vec<Row> {
        hexes.iter().map(|h| Row::new(rgb(h))).collect()
    }

    #[test]
    fn strict_hex_accepts_only_six_digit_hex() {
        assert!(strict_hex("#a1B2c3").is_some());
        assert!(strict_hex(" #a1b2c3 ").is_some());
        assert!(strict_hex("#abc").is_none());
        assert!(strict_hex("a1b2c3").is_none());
        assert!(strict_hex("#a1b2c3d4").is_none());
        assert!(strict_hex("#gggggg").is_none());
        assert!(strict_hex("red").is_none());
    }

    #[test]
    fn random_colours_stay_in_range_and_vary() {
        let mut rng = 12345;

        let colours: Vec<Rgba> = (0..50).map(|_| random_color(&mut rng)).collect();

        assert!(
            colours
                .iter()
                .all(|c| [c.r, c.g, c.b].iter().all(|&v| (40..216).contains(&v)))
        );
        assert!(colours.iter().any(|c| *c != colours[0]));
    }

    #[test]
    fn randomise_keeps_locked_rows() {
        let mut list = rows(&["#111111", "#222222", "#333333"]);
        list[0].locked = true;
        let mut rng = 99;

        randomise(&mut list, &mut rng);

        assert_eq!(list[0].rgba, rgb("#111111"));
        assert_ne!(list[1].rgba, rgb("#222222"));
        assert_ne!(list[2].rgba, rgb("#333333"));
    }

    #[test]
    fn move_row_swaps_within_range_only() {
        let mut list = rows(&["#111111", "#222222", "#333333"]);

        move_row(&mut list, 0, 1);
        assert_eq!(hexes(&list), ["#222222", "#111111", "#333333"]);
        move_row(&mut list, 0, -1);
        move_row(&mut list, 2, 1);
        assert_eq!(hexes(&list), ["#222222", "#111111", "#333333"]);
    }

    #[test]
    fn exports_match_the_website_shapes() {
        let colors = vec!["#112233".to_string(), "#445566".to_string()];

        assert_eq!(
            css(&colors),
            ":root {\n  --color-1: #112233;\n  --color-2: #445566;\n}"
        );
        assert_eq!(
            tailwind(&colors),
            "{\n  \"theme\": {\n    \"extend\": {\n      \"colors\": {\n        \"palette-1\": \"#112233\",\n        \"palette-2\": \"#445566\"\n      }\n    }\n  }\n}"
        );
        assert_eq!(
            json(&colors),
            "{\n  \"colors\": [\n    \"#112233\",\n    \"#445566\"\n  ]\n}"
        );
    }

    #[test]
    fn tailwind_export_parses_as_json() {
        let colors = vec!["#112233".to_string(), "#445566".to_string()];

        let value: serde_json::Value = serde_json::from_str(&tailwind(&colors)).unwrap();

        assert_eq!(
            value["theme"]["extend"]["colors"]["palette-2"],
            serde_json::json!("#445566")
        );
    }
}
