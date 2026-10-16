//! Palette Library: the website's 37 palettes, grouped by colour count as it grouped them, with
//! a search on the name. Clicking a colour copies its hex. The website's header promised the
//! whole palette as JSON and had no way to get it; here each palette copies as JSON or as CSS
//! custom properties. Everything runs on the UI thread: 37 small records.

use std::path::PathBuf;
use std::thread::JoinHandle;

use serde::Serialize;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::engine::color::{self, Rgba};
use crate::{AppWindow, LibrarySwatch, PaletteCard, PaletteLibrary, PaletteSection, clipboard};

pub const SLUG: &str = "palette-library";

// ---- Logic --------------------------------------------------------------------------------

/// The palettes whose name contains `query` (trimmed, any case), grouped by colour count from
/// fewest to most: (count, palette indices in their original order).
fn sections(query: &str) -> Vec<(usize, Vec<usize>)> {
    let query = query.trim().to_lowercase();
    let mut sections: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, (name, colors)) in PALETTES.iter().enumerate() {
        if !name.to_lowercase().contains(&query) {
            continue;
        }
        match sections.iter_mut().find(|(count, _)| *count == colors.len()) {
            Some((_, members)) => members.push(i),
            None => sections.push((colors.len(), vec![i])),
        }
    }
    sections.sort_by_key(|(count, _)| *count);
    sections
}

#[derive(Serialize)]
struct PaletteJson<'a> {
    name: &'a str,
    colors: &'a [&'a str],
}

fn json(name: &str, colors: &[&str]) -> String {
    serde_json::to_string_pretty(&PaletteJson { name, colors }).unwrap_or_default()
}

/// `:root { --cyber-punch-1: #120458; … }`, named after the palette.
fn css_variables(name: &str, colors: &[&str]) -> String {
    let prefix: String = name
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let lines: Vec<String> = colors
        .iter()
        .enumerate()
        .map(|(i, c)| format!("  --{prefix}-{}: {c};", i + 1))
        .collect();
    format!(":root {{\n{}\n}}", lines.join("\n"))
}

/// WCAG 2.2 relative luminance.
fn luminance(c: Rgba) -> f64 {
    let linear = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// Whether white reads better than black on `c`, by the WCAG contrast ratio.
fn light_ink(c: Rgba) -> bool {
    let l = luminance(c);
    1.05 / (l + 0.05) > (l + 0.05) / 0.05
}

// ---- Wiring -------------------------------------------------------------------------------

pub fn init(app: &AppWindow) {
    let g = app.global::<PaletteLibrary>();
    let weak = app.as_weak();
    g.on_search(move |query| {
        if let Some(app) = weak.upgrade() {
            show(&app, &query);
        }
    });
    let weak = app.as_weak();
    g.on_copy_color(move |hex| copy(&weak, hex.as_str()));
    let weak = app.as_weak();
    g.on_copy_json(move |i| match palette(i) {
        Some((name, colors)) => copy(&weak, &json(name, colors)),
        None => false,
    });
    let weak = app.as_weak();
    g.on_copy_css(move |i| match palette(i) {
        Some((name, colors)) => copy(&weak, &css_variables(name, colors)),
        None => false,
    });
    show(app, "");
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Nothing runs in the background.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}

fn palette(i: i32) -> Option<&'static (&'static str, &'static [&'static str])> {
    usize::try_from(i).ok().and_then(|i| PALETTES.get(i))
}

fn copy(weak: &slint::Weak<AppWindow>, text: &str) -> bool {
    let Some(app) = weak.upgrade() else {
        return false;
    };
    let g = app.global::<PaletteLibrary>();
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

fn show(app: &AppWindow, query: &str) {
    let g = app.global::<PaletteLibrary>();
    let found = sections(query);
    let sections: Vec<PaletteSection> = found
        .iter()
        .map(|(count, members)| PaletteSection {
            title: format!("{count} Colors").into(),
            cards: ModelRc::new(VecModel::from(
                members.iter().map(|&i| card(i)).collect::<Vec<_>>(),
            )),
        })
        .collect();
    let empty = if found.is_empty() {
        format!("No palettes match “{}”.", query.trim())
    } else {
        String::new()
    };
    g.set_empty_text(empty.into());
    g.set_sections(ModelRc::new(VecModel::from(sections)));
}

fn card(i: usize) -> PaletteCard {
    let (name, colors) = PALETTES[i];
    let swatches: Vec<LibrarySwatch> = colors
        .iter()
        .filter_map(|hex| color::parse(hex))
        .map(|(_, c)| LibrarySwatch {
            color: color::slint_color(c),
            hex: color::hex(c).into(),
            light_ink: light_ink(c),
        })
        .collect();
    PaletteCard {
        index: i as i32,
        name: name.into(),
        swatches: ModelRc::new(VecModel::from(swatches)),
    }
}

// ---- Data ---------------------------------------------------------------------------------

/// The website's 37 palettes (src/lib/design-tools/data/palette-presets.ts): name, colours.
/// Generated from that file, not retyped.
const PALETTES: &[(&str, &[&str])] = &[
    ("Minimalist Cool", &["#E2E8F0", "#94A3B8", "#0F172A"]),
    ("Warm Contrast", &["#FFFBEB", "#F59E0B", "#78350F"]),
    ("Forest Basics", &["#ECFDF5", "#10B981", "#064E3B"]),
    ("Cyber Punch", &["#120458", "#7A04EB", "#FE75FE", "#FF00A0"]),
    ("Desert Oasis", &["#2C363F", "#E75A7C", "#F2F5EA", "#D6EADF"]),
    ("Vintage Card", &["#EAE2B7", "#FCBF49", "#F77F00", "#D62828"]),
    ("Velvet Blossom", &["#735D78", "#E9BBBE", "#EBA5A2", "#246A73", "#231C36"]),
    ("Lavender Hush", &["#9B9E99", "#BBA4BD", "#DBCFE6", "#E9DDEB", "#ECEBF5"]),
    ("City Haven", &["#757C8D", "#A68F83", "#AE898F", "#8F6D6E", "#283E4C"]),
    ("Sundown Spark", &["#A23438", "#BB5E61", "#D99C97", "#F3C9AF", "#FBE3D0"]),
    ("Coastal Retreat", &["#335765", "#74A8A4", "#B6D9E0", "#DBE2DC", "#7F543D"]),
    ("Bay Bloom", &["#6F9C9F", "#C0DFD4", "#F0D6B6", "#E08374", "#AC4E5C"]),
    ("Candy Breeze", &["#FF8082", "#ED979A", "#DCB3B5", "#B2E1E1", "#A4FDFD"]),
    ("Metropolis Night", &["#272727", "#484848", "#FFB900", "#009EFF", "#D7D7D7"]),
    ("Carnival Brights", &["#F0932B", "#EB4D4B", "#6AB04C", "#4834D4", "#30336B"]),
    ("Spring Sorbet", &["#A29BFE", "#FFEAA7", "#FAB1A0", "#74B9FF", "#55EFC4"]),
    ("Earthy Greens", &["#6B705C", "#A5A58D", "#B7B7A4", "#DDBEA9", "#FFE8D6"]),
    ("Spiced Plum", &["#320A28", "#511730", "#8E443D", "#E08E45", "#FACFAD"]),
    ("Tropical Harmony", &["#023A38", "#06585B", "#B767AE", "#FF7742", "#FFC847"]),
    ("Aqua Serenity", &["#026968", "#1B8887", "#3BA7A6", "#74CECD", "#B9E2E2"]),
    ("Retro Soul", &["#194A4D", "#318583", "#EBE8DD", "#E3BE68", "#764B4B"]),
    ("Summer Splash (Blue)", &["#006C70", "#20929B", "#E6F3E4", "#CFED76", "#356FEA"]),
    ("Summer Lilac", &["#0B6568", "#3F959D", "#67BEC2", "#E6C6F8", "#FDE2F8"]),
    ("Tokyo Pop", &["#08464A", "#0C757D", "#3A9CA3", "#EEDDEB", "#FC8FD6"]),
    ("Navy Terracotta", &["#2C3646", "#BF4C4E", "#EFEADF", "#EDADC4", "#C98F62"]),
    ("Fresh", &["#2B1B17", "#5C4033", "#6A7B54", "#7DA1B1", "#D8DCD6", "#F3EFE9"]),
    ("Spiced", &["#2B1E1A", "#861D1D", "#BF6F31", "#F4B34C", "#EBCCBA", "#E8E2D1"]),
    ("Frosted", &["#2A3439", "#4B5963", "#707C84", "#A9C0CE", "#DDE7ED", "#E6E6E6"]),
    ("Plum Fade", &["#150016", "#29104A", "#522C5D", "#845162", "#E3B6B1", "#F7E3DB"]),
    ("Regal", &["#11100F", "#5D1C34", "#A67D44", "#899481", "#CDBCAB", "#EFE9E1"]),
    ("Sanddune", &["#3A2D28", "#A48374", "#CBAD8D", "#D1C7BD", "#EBE3DB", "#F1EDE6"]),
    ("Dusk", &["#3D2321", "#6E3F40", "#9C5B45", "#9BA9B3", "#D0D7E0", "#F1F1F1"]),
    ("Ocean Depths", &["#031716", "#032F30", "#0A7075", "#0C969C", "#6BA3BE", "#274D60"]),
    ("Twilight Peach", &["#03122F", "#19305C", "#413B61", "#AE7DAC", "#F3DADF", "#F1916D"]),
    ("Crimson Dusk", &["#181A2F", "#242E49", "#37415C", "#FDA481", "#B4182D", "#54162B"]),
    ("Dusty Rose & Blue", &["#0D1E4C", "#C48CB3", "#E5C9D7", "#83A6CE", "#26415E", "#0B1B32"]),
    ("Sunset Hues", &["#1B1931", "#44174E", "#662249", "#A34054", "#ED9E59", "#E98CB9"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(found: &[(usize, Vec<usize>)]) -> Vec<(usize, usize)> {
        found.iter().map(|(count, members)| (*count, members.len())).collect()
    }

    #[test]
    fn there_are_37_palettes() {
        assert_eq!(PALETTES.len(), 37);
    }

    #[test]
    fn sections_group_by_colour_count_as_the_website_did() {
        assert_eq!(sizes(&sections("")), [(3, 3), (4, 3), (5, 19), (6, 12)]);
    }

    #[test]
    fn sections_search_the_name_in_any_case() {
        assert_eq!(sections("  CYBER punch "), [(4, vec![3])]);
    }

    #[test]
    fn sections_of_an_unknown_name_are_empty() {
        assert_eq!(sections("no such palette"), []);
    }

    #[test]
    fn json_holds_the_name_and_colours() {
        assert_eq!(
            json("Cyber Punch", &["#120458", "#7A04EB"]),
            "{\n  \"name\": \"Cyber Punch\",\n  \"colors\": [\n    \"#120458\",\n    \"#7A04EB\"\n  ]\n}"
        );
    }

    #[test]
    fn json_parses_back_to_the_same_shape() {
        let text = json(PALETTES[3].0, PALETTES[3].1);

        let value: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "name": "Cyber Punch",
                "colors": ["#120458", "#7A04EB", "#FE75FE", "#FF00A0"]
            })
        );
    }

    #[test]
    fn css_variables_are_named_after_the_palette() {
        assert_eq!(
            css_variables("Sunset & Sea", &["#1B1931", "#44174E"]),
            ":root {\n  --sunset-sea-1: #1B1931;\n  --sunset-sea-2: #44174E;\n}"
        );
    }

    #[test]
    fn light_ink_goes_on_dark_colours() {
        assert!(light_ink(color::parse("#120458").unwrap().1));
    }

    #[test]
    fn dark_ink_goes_on_light_colours() {
        assert!(!light_ink(color::parse("#E2E8F0").unwrap().1));
    }
}
