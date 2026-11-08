//! Color Blocks: the website's 639 solid colours as a wall of swatches, filtered by hue family
//! and by lightness. Clicking a block copies its hex. The colours are read from the website's
//! data file at compile time (src/lib/design-tools/data/color-blocks.ts), so the two cannot
//! drift. Everything runs on the UI thread: 639 small records.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread::JoinHandle;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::palette_library::light_ink;
use crate::engine::color::{self, Rgba};
use crate::{AppWindow, BlockItem, ColorBlocks, clipboard};

pub const SLUG: &str = "color-blocks";

const SOURCE: &str = include_str!("../../../src/lib/design-tools/data/color-blocks.ts");

/// The hue families, in the website's order. "neutral" is last.
const FAMILIES: [&str; 9] = [
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink", "neutral",
];

// ---- Logic --------------------------------------------------------------------------------

/// Every `"#rrggbb"` string literal in the data file.
fn parse_blocks(source: &str) -> Vec<Rgba> {
    source
        .split('"')
        .filter(|part| part.len() == 7 && part.starts_with('#'))
        .filter_map(|part| color::parse(part).map(|(_, c)| c))
        .collect()
}

fn blocks() -> &'static [Rgba] {
    static BLOCKS: OnceLock<Vec<Rgba>> = OnceLock::new();
    BLOCKS.get_or_init(|| parse_blocks(SOURCE))
}

/// Hue in degrees, saturation and lightness from 0 to 1.
fn hsl(c: Rgba) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    ((h * 60.0).rem_euclid(360.0), s, l)
}

/// The index into `FAMILIES` of a colour's hue family.
fn family(c: Rgba) -> usize {
    let (h, s, l) = hsl(c);
    if s < 0.12 || l < 0.08 || l > 0.95 {
        return 8;
    }
    if !(15.0..350.0).contains(&h) {
        0
    } else if h < 45.0 {
        1
    } else if h < 70.0 {
        2
    } else if h < 165.0 {
        3
    } else if h < 200.0 {
        4
    } else if h < 255.0 {
        5
    } else if h < 290.0 {
        6
    } else {
        7
    }
}

/// The blocks matching the family (0 = all, else 1 + index into `FAMILIES`) and the lightness
/// (0 = all, 1 = light, 2 = dark).
fn matching(family_index: i32, tone_index: i32) -> Vec<Rgba> {
    blocks()
        .iter()
        .copied()
        .filter(|&c| family_index <= 0 || family(c) as i32 == family_index - 1)
        .filter(|&c| match tone_index {
            1 => !light_ink(c),
            2 => light_ink(c),
            _ => true,
        })
        .collect()
}

// ---- Wiring -------------------------------------------------------------------------------

pub fn init(app: &AppWindow) {
    let g = app.global::<ColorBlocks>();
    let options: Vec<SharedString> = std::iter::once("All".to_string())
        .chain(FAMILIES.iter().map(|name| {
            let mut chars = name.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }))
        .map(SharedString::from)
        .collect();
    g.set_family_options(ModelRc::new(VecModel::from(options)));
    let weak = app.as_weak();
    g.on_filter(move |family, tone| {
        if let Some(app) = weak.upgrade() {
            show(&app, family, tone);
        }
    });
    let weak = app.as_weak();
    g.on_copy_color(move |hex| copy(&weak, hex.as_str()));
    show(app, 0, 0);
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
    let g = app.global::<ColorBlocks>();
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

fn show(app: &AppWindow, family_index: i32, tone_index: i32) {
    let g = app.global::<ColorBlocks>();
    let found = matching(family_index, tone_index);
    g.set_count_text(format!("{} of {} colors", found.len(), blocks().len()).into());
    let items: Vec<BlockItem> = found
        .into_iter()
        .map(|c| BlockItem {
            color: color::slint_color(c),
            hex: color::hex(c).into(),
            light_ink: light_ink(c),
        })
        .collect();
    g.set_blocks(ModelRc::new(VecModel::from(items)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> Rgba {
        color::parse(hex).unwrap().1
    }

    #[test]
    fn there_are_639_blocks() {
        assert_eq!(blocks().len(), 639);
    }

    #[test]
    fn hue_families_match_the_website() {
        let named = |hex: &str| FAMILIES[family(rgb(hex))];
        assert_eq!(named("#ff0000"), "red");
        assert_eq!(named("#ff8000"), "orange");
        assert_eq!(named("#ffff00"), "yellow");
        assert_eq!(named("#00ff00"), "green");
        assert_eq!(named("#00ffff"), "cyan");
        assert_eq!(named("#0000ff"), "blue");
        assert_eq!(named("#8000ff"), "purple");
        assert_eq!(named("#ff00aa"), "pink");
        assert_eq!(named("#808080"), "neutral");
        assert_eq!(named("#000000"), "neutral");
        assert_eq!(named("#ffffff"), "neutral");
    }

    #[test]
    fn family_and_lightness_match_the_shared_web_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/lib/design-tools/data/color-parity.json"
        ))
        .unwrap();
        let entries = fixture.as_array().unwrap();

        assert!(entries.len() >= 20);
        for entry in entries {
            let hex = entry["hex"].as_str().unwrap();
            let c = rgb(hex);
            assert_eq!(FAMILIES[family(c)], entry["family"].as_str().unwrap(), "{hex}");
            let lightness = if light_ink(c) { "dark" } else { "light" };
            assert_eq!(lightness, entry["lightness"].as_str().unwrap(), "{hex}");
        }
    }

    #[test]
    fn no_filter_keeps_every_block() {
        assert_eq!(matching(0, 0).len(), 639);
    }

    #[test]
    fn light_and_dark_split_the_blocks() {
        assert_eq!(matching(0, 1).len() + matching(0, 2).len(), 639);
    }

    #[test]
    fn a_family_filter_keeps_only_that_family() {
        let reds = matching(1, 0);

        assert!(!reds.is_empty());
        assert!(reds.iter().all(|&c| family(c) == 0));
    }
}
