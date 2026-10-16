//! Gradient Library: the website's 46 named gradients, searchable by name and filtered by
//! feel as it did. Each copies as CSS or Tailwind, exports a 1600 × 900 PNG (rendered on a
//! worker), or opens in Gradient Maker for editing, which the website couldn't do.
//!
//! Every preset is `linear-gradient(to right, …)` (a test holds that), so the cards draw with
//! Slint's own linear gradient at 90°, which matches CSS exactly for a horizontal line.

use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

use slint::{Brush, ComponentHandle, GradientStop, LinearGradientBrush, ModelRc, VecModel};

use crate::engine::color::{self, Gradient};
use crate::registry::Registry;
use crate::tools::gradient_maker;
use crate::{AppWindow, GradientCard, GradientLibrary, files, jobs};

pub const SLUG: &str = "gradient-library";
const ALL: &str = "All";

// ---- Logic --------------------------------------------------------------------------------

/// "All", then every feel in the order the presets first use it, as on the website.
fn feels() -> Vec<&'static str> {
    let mut feels = vec![ALL];
    for (_, feel, _) in PRESETS {
        if !feels.contains(feel) {
            feels.push(feel);
        }
    }
    feels
}

/// The presets whose name holds `query` (trimmed, any case) and that have `feel` ("All" for
/// any), in their original order.
fn matching(query: &str, feel: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    PRESETS
        .iter()
        .enumerate()
        .filter(|(_, (name, f, _))| {
            name.to_lowercase().contains(&query) && (feel == ALL || *f == feel)
        })
        .map(|(i, _)| i)
        .collect()
}

fn preset(i: i32) -> Option<(&'static str, Gradient)> {
    let (name, _, css) = PRESETS.get(usize::try_from(i).ok()?)?;
    Some((name, Gradient::parse(css)?))
}

/// The card's art: the preset as a Slint brush, left to right like the CSS.
fn brush(css: &str) -> Brush {
    let stops = Gradient::parse(css)
        .and_then(|g| color::stops(&g).ok())
        .unwrap_or_default();
    Brush::LinearGradient(LinearGradientBrush::new(
        90.0,
        stops.into_iter().map(|(c, at)| GradientStop {
            color: color::slint_color(c),
            position: at as f32,
        }),
    ))
}

// ---- State and wiring ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    query: String,
    /// PNG exports still writing; joined on shutdown so none is cut short.
    exports: Vec<JoinHandle<()>>,
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
    let g = app.global::<GradientLibrary>();
    g.set_feels(ModelRc::new(VecModel::from(
        feels().into_iter().map(Into::into).collect::<Vec<slint::SharedString>>(),
    )));
    let weak = app.as_weak();
    g.on_search(move |query| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.query = query.to_string());
        show(&app);
    });
    let weak = app.as_weak();
    g.on_feel_changed(move |_| {
        if let Some(app) = weak.upgrade() {
            show(&app);
        }
    });
    let weak = app.as_weak();
    g.on_copy_css(move |i| {
        preset(i).is_some_and(|(_, g)| copy(&weak, &color::background_property(&g.css())))
    });
    let weak = app.as_weak();
    g.on_copy_tailwind(move |i| {
        preset(i).is_some_and(|(_, g)| copy(&weak, &color::tailwind_arbitrary(&g.css())))
    });
    let weak = app.as_weak();
    g.on_export(move |i| {
        if let (Some(app), Some((name, gradient))) = (weak.upgrade(), preset(i)) {
            export(&app, name, gradient);
        }
    });
    let weak = app.as_weak();
    g.on_edit(move |i| {
        if let (Some(app), Some((_, gradient))) = (weak.upgrade(), preset(i)) {
            gradient_maker::edit(&app, &gradient);
            show_tool(&app, gradient_maker::SLUG);
        }
    });
    show(app);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Exports still writing finish (a second or so each), so no PNG is left half-written.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| std::mem::take(&mut s.exports))
}

fn copy(weak: &slint::Weak<AppWindow>, text: &str) -> bool {
    let Some(app) = weak.upgrade() else {
        return false;
    };
    let g = app.global::<GradientLibrary>();
    match crate::clipboard::copy_text(text) {
        Ok(()) => true,
        Err(e) => {
            g.set_error(format!("Couldn't copy: {e}").into());
            false
        }
    }
}

fn export(app: &AppWindow, name: &str, gradient: Gradient) {
    let g = app.global::<GradientLibrary>();
    let Some(path) = files::choose_save_path("Export PNG", &color::png_name(name)) else {
        return;
    };
    g.set_error("".into());
    g.set_note(format!("Exporting {}…", files::file_name(&path)).into());
    let weak = app.as_weak();
    let handle = color::export_png(gradient, path, move |result| {
        jobs::post(&weak, move |app| {
            let g = app.global::<GradientLibrary>();
            match result {
                Ok(note) => g.set_note(note.into()),
                Err(error) => g.set_error(error.into()),
            }
        });
    });
    with(|s| {
        s.exports.retain(|h| !h.is_finished());
        s.exports.push(handle);
    });
}

/// Shows another tool, as choosing it in the sidebar does (main.rs `select_tool`, which tools
/// can't reach).
fn show_tool(app: &AppWindow, slug: &str) {
    let Ok(registry) = Registry::load() else {
        return;
    };
    if let Some(tool) = registry.tool(slug) {
        app.set_current_slug(tool.slug.as_str().into());
        app.set_current_name(tool.name.as_str().into());
        app.set_current_description(tool.description.as_str().into());
    }
}

fn show(app: &AppWindow) {
    let g = app.global::<GradientLibrary>();
    let feels = feels();
    let feel = feels[crate::select::index(g.get_feel(), feels.len())];
    let query = with(|s| s.query.clone());
    let found = matching(&query, feel);
    let cards: Vec<GradientCard> = found
        .iter()
        .map(|&i| {
            let (name, feel, css) = PRESETS[i];
            GradientCard {
                index: i as i32,
                name: name.into(),
                feel: feel.into(),
                fill: brush(css),
            }
        })
        .collect();
    let empty = match (found.is_empty(), query.trim()) {
        (false, _) => String::new(),
        (true, "") => format!("No gradients have the feel “{feel}”."),
        (true, q) => format!("No gradients match “{q}”."),
    };
    g.set_empty_text(empty.into());
    g.set_cards(ModelRc::new(VecModel::from(cards)));
}

// ---- Data ---------------------------------------------------------------------------------

/// The website's 46 presets (src/lib/design-tools/data/gradient-presets.ts): name, feel, CSS.
/// Generated from that file, not retyped.
const PRESETS: &[(&str, &str, &str)] = &[
    ("Ocean Sunset", "Dark & Moody", "linear-gradient(to right, #0f2027, #2c5364)"),
    ("Midnight Glow", "Dark & Moody", "linear-gradient(to right, #141e30, #243b55)"),
    ("Crimson Velvet", "Dark & Moody", "linear-gradient(to right, #2C0F12, #6B1E23)"),
    ("Earthy Sage", "Earthy & Natural", "linear-gradient(to right, #B6B6A3, #A4A48C, #6B705C)"),
    ("Misty Blue", "Cool & Serene", "linear-gradient(to right, #384B70, #507687, #FCFAEE)"),
    ("Soft Peach", "Light & Airy", "linear-gradient(to right, #F4EADF, #E3D4CA, #D4BCAF)"),
    ("Warm Caramel", "Warm & Cozy", "linear-gradient(to right, #E8C9AB, #BD976C, #8D6B48)"),
    ("Forest Night", "Earthy & Natural", "linear-gradient(to right, #076653, #0C342C, #06231D)"),
    ("Vibrant Meadow", "Vibrant & Energetic", "linear-gradient(to right, #E2FBCE, #E3EF26, #076653)"),
    ("Lavender Haze", "Pastel & Dreamy", "linear-gradient(to right, #F1FEC6, #A882DD)"),
    ("Pink Pop", "Vibrant & Energetic", "linear-gradient(to right, #FFE5F1, #F042FF)"),
    ("Champagne Silk", "Light & Airy", "linear-gradient(to right, #FAF3E0, #EADBC8, #D7B49E)"),
    ("Neon Berry", "Vibrant & Energetic", "linear-gradient(to right, #FFE5F1, #F042FF, #7226FF)"),
    ("Violet Deep", "Dark & Moody", "linear-gradient(to right, #7226FF, #160078, #010030)"),
    ("Cotton Candy", "Pastel & Dreamy", "linear-gradient(to right, #87F5F5, #FFE5F1, #F042FF)"),
    ("Emerald Whisper", "Cool & Serene", "linear-gradient(to right, #0F3D3E, #467373, #D2E8E3)"),
    ("Velvet Indigo", "Cool & Serene", "linear-gradient(to right, #1F1C2C, #928DAB, #E0E0E0)"),
    ("Ivory Gold", "Metallic & Luxe", "linear-gradient(to right, #FFFDF7, #F2E3BC, #D4AF37)"),
    ("Royal Emerald", "Cool & Serene", "linear-gradient(to right, #002B36, #14655B, #A7FFEB)"),
    ("Midnight Plum", "Dark & Moody", "linear-gradient(to right, #1E1E2F, #4C2A59, #9E4B8A)"),
    ("Mocha Luxe", "Earthy & Natural", "linear-gradient(to right, #3E2C2C, #7B5E57, #D6C3B3)"),
    ("Velvet Teal", "Cool & Serene", "linear-gradient(to right, #1B4242, #4FBDBA, #CDEED6)"),
    ("Satin Bronze", "Metallic & Luxe", "linear-gradient(to right, #1C1C1C, #5A4E4E, #BFA27A)"),
    ("Rose Quartz Mist", "Light & Airy", "linear-gradient(to right, #F5F5F5, #E4B6C0, #C18C94)"),
    ("Ink Sapphire", "Dark & Moody", "linear-gradient(to right, #090A1A, #344E89, #A0C4FF)"),
    ("Crimson Smoke", "Warm & Cozy", "linear-gradient(to right, #3B0A0A, #8A2B2B, #F2C9C9)"),
    ("Chrome Olive", "Metallic & Luxe", "linear-gradient(to right, #1C1C1A, #616247, #D7D6C2)"),
    ("Storm Marble", "Cool & Serene", "linear-gradient(to right, #2A2E35, #7B8FA1, #E4E9EC)"),
    ("Velvet Sunset", "Warm & Cozy", "linear-gradient(to right, #3F1D38, #AF5279, #FFD6A5)"),
    ("Pine Fog", "Earthy & Natural", "linear-gradient(to right, #1F2A24, #5B7768, #DAE3D7)"),
    ("Sapphire Drift", "Cool & Serene", "linear-gradient(to right, #0A0F3C, #2C5DA9, #C8DAF9)"),
    ("Golden Bloom", "Metallic & Luxe", "linear-gradient(to right, #4D3F29, #C49A6C, #FFF3D1)"),
    ("Blue Mirage", "Cool & Serene", "linear-gradient(to right, #001F3F, #41729F, #E1F1FF)"),
    ("Oyster Glow", "Light & Airy", "linear-gradient(to right, #F8F4F0, #DDD0C8, #B0A89F)"),
    ("Velvet Petrol", "Dark & Moody", "linear-gradient(to right, #0A0F0D, #2D4F4A, #8DB1A4)"),
    ("Royal Aurora", "Vibrant & Energetic", "linear-gradient(to right, #3E2F5B, #E94560)"),
    ("Sunset Glow", "Warm & Cozy", "linear-gradient(to right, #FF5450, #AB0062)"),
    ("Electric Lime", "Vibrant & Energetic", "linear-gradient(to right, #FEF500, #00A800)"),
    ("Twilight Lavender", "Cool & Serene", "linear-gradient(to right, #262D57, #A89FC8)"),
    ("Sunset", "Warm & Cozy", "linear-gradient(to right, #EF4444, #EAB308, #EC4899)"),
    ("Mint", "Cool & Serene", "linear-gradient(to right, #BBF7D0, #86EFAC, #3B82F6)"),
    ("Amethyst", "Dark & Moody", "linear-gradient(to right, #6B21A8, #4C1D95, #6B21A8)"),
    ("Aurora Glow", "Pastel & Dreamy", "linear-gradient(to right, #8CE8F1, #CB70C4)"),
    ("Mystic Plum", "Dark & Moody", "linear-gradient(to right, #87488F, #41206D)"),
    ("Forest Mist", "Cool & Serene", "linear-gradient(to right, #A9D8B8, #1B604E)"),
    ("Seafoam Sunset", "Cool & Serene", "linear-gradient(to right, #02656B, #ECB99E)"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_46_presets() {
        assert_eq!(PRESETS.len(), 46);
    }

    #[test]
    fn feels_keep_the_order_the_presets_first_use_them() {
        assert_eq!(
            feels(),
            [
                "All",
                "Dark & Moody",
                "Earthy & Natural",
                "Cool & Serene",
                "Light & Airy",
                "Warm & Cozy",
                "Metallic & Luxe",
                "Vibrant & Energetic",
                "Pastel & Dreamy"
            ]
        );
    }

    #[test]
    fn every_preset_runs_left_to_right_through_readable_colours() {
        let unreadable: Vec<&str> = PRESETS
            .iter()
            .filter(|(_, _, css)| {
                Gradient::parse(css)
                    .is_none_or(|g| g.orientation != "to right" || color::stops(&g).is_err())
            })
            .map(|(name, _, _)| *name)
            .collect();

        assert_eq!(unreadable, Vec::<&str>::new());
    }

    #[test]
    fn matching_filters_by_feel() {
        assert_eq!(matching("", "Pastel & Dreamy").len(), 3);
    }

    #[test]
    fn matching_searches_the_name_in_any_case() {
        assert_eq!(matching(" OCEAN sunset", ALL), [0]);
    }

    #[test]
    fn matching_needs_both_the_name_and_the_feel() {
        assert_eq!(matching("ocean sunset", "Light & Airy"), Vec::<usize>::new());
    }

    #[test]
    fn brush_has_a_stop_per_colour() {
        let Brush::LinearGradient(gradient) = brush(PRESETS[3].2) else {
            panic!("not a linear gradient");
        };

        assert_eq!(gradient.stops().count(), 3);
    }
}
