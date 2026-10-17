//! Color Converter: a colour in any syntax, shown in every format with a copy for each. The
//! website showed three of HEX, RGB, HSL and Tailwind (the ones not typed) and computed HSV and
//! OKLCH without showing them; here every format shows, the typed one included. Tailwind input
//! and the nearest-class output use Tailwind v3's whole palette, where the website had 11 of
//! its 22 hues. Everything runs on the UI thread: it is a few string conversions.

use std::path::PathBuf;
use std::thread::JoinHandle;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::engine::color::{self, Rgba, Source};
use crate::{AppWindow, ColorConverter, ColorFormat, clipboard};

pub const SLUG: &str = "color-converter";
/// The website's sample: a hex value, so it reads the same everywhere.
const SAMPLE: &str = "#663399";
/// The utility prefixes a pasted Tailwind class may carry.
const PREFIXES: [&str; 6] = ["bg-", "text-", "border-", "ring-", "fill-", "stroke-"];

// ---- Logic --------------------------------------------------------------------------------

/// A Tailwind shade (`violet-600`, `bg-violet-600`, `text-white`). A bare `white` or `black`
/// is left to the CSS names, as on the website.
fn tailwind_input(input: &str) -> Option<Rgba> {
    let lower = input.trim().to_ascii_lowercase();
    let unprefixed = PREFIXES.iter().find_map(|p| lower.strip_prefix(p));
    let name = unprefixed.unwrap_or(&lower);
    if unprefixed.is_none() && !name.contains('-') {
        return None;
    }
    let (_, hex) = TAILWIND.iter().find(|(n, _)| *n == name)?;
    color::parse(hex).map(|(_, c)| c)
}

/// The website's Tailwind output: the class whose colour is nearest in RGB, with the
/// distance when it isn't exact. Translucent colours keep their alpha as an opacity modifier.
fn tailwind_class(c: Rgba) -> String {
    let distance = |hex: &str| {
        color::parse(hex).map_or(f64::INFINITY, |(_, t)| {
            let d = |a: u8, b: u8| (f64::from(a) - f64::from(b)).powi(2);
            (d(c.r, t.r) + d(c.g, t.g) + d(c.b, t.b)).sqrt()
        })
    };
    // The first of equally near classes wins, as on the website.
    let (name, delta) = TAILWIND
        .iter()
        .map(|(name, hex)| (*name, distance(hex)))
        .fold(("", f64::INFINITY), |best, next| {
            if next.1 < best.1 { next } else { best }
        });
    let opacity = if c.a < 1.0 {
        format!("/[{}]", color::alpha_text(c.a))
    } else {
        String::new()
    };
    if delta == 0.0 {
        format!("bg-{name}{opacity}")
    } else {
        format!("bg-{name}{opacity} /* nearest match, Δ{} */", delta.round())
    }
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Hex => "HEX",
        Source::Rgb => "RGB",
        Source::Hsl => "HSL",
        Source::Hsv => "HSV",
        Source::Hwb => "HWB",
        Source::Lab => "Lab",
        Source::Lch => "LCH",
        Source::Oklab => "OKLab",
        Source::Oklch => "OKLCH",
        Source::Named => "a CSS name",
    }
}

/// What a valid input shows: the colour, the syntax it was read as, and every format.
#[derive(Debug, PartialEq)]
struct Converted {
    color: Rgba,
    detected: &'static str,
    formats: Vec<(&'static str, String)>,
}

fn convert(input: &str) -> Option<Converted> {
    let (detected, c) = match tailwind_input(input) {
        Some(c) => ("Tailwind", c),
        None => {
            let (source, c) = color::parse(input)?;
            (source_label(source), c)
        }
    };
    let mut formats = vec![
        ("HEX", color::hex(c)),
        ("RGB", color::rgb(c)),
        ("HSL", color::hsl(c)),
        ("HSV", color::hsv(c)),
        ("HWB", color::hwb(c)),
        ("Lab", color::lab(c)),
        ("LCH", color::lch(c)),
        ("OKLab", color::oklab(c)),
        ("OKLCH", color::oklch(c)),
        ("Tailwind", tailwind_class(c)),
    ];
    if let Some(name) = color::name_of(c) {
        formats.push(("CSS name", name.to_string()));
    }
    Some(Converted {
        color: c,
        detected,
        formats,
    })
}

// ---- Wiring -------------------------------------------------------------------------------

pub fn init(app: &AppWindow) {
    let g = app.global::<ColorConverter>();
    let weak = app.as_weak();
    g.on_edited(move |_| {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_picked(move |c| {
        let Some(app) = weak.upgrade() else { return };
        app.global::<ColorConverter>()
            .set_input(color::hex(color::from_slint(c)).into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy_format(move |i| {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let g = app.global::<ColorConverter>();
        let Some(format) = usize::try_from(i)
            .ok()
            .and_then(|i| g.get_formats().row_data(i))
        else {
            return false;
        };
        match clipboard::copy_text(&format.value) {
            Ok(()) => true,
            Err(e) => {
                g.set_error(format!("Couldn't copy: {e}").into());
                false
            }
        }
    });
    g.set_input(SAMPLE.into());
    refresh(app);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Nothing runs in the background.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}

fn refresh(app: &AppWindow) {
    let g = app.global::<ColorConverter>();
    let input = g.get_input();
    let converted = convert(&input);
    let error = match (&converted, input.trim().is_empty()) {
        (None, false) => format!(
            "Couldn't read “{}” as a colour. Try HEX, RGB, HSL, HSV, OKLCH, a CSS name, or a \
             Tailwind shade such as violet-600.",
            input.trim()
        ),
        _ => String::new(),
    };
    g.set_error(error.into());
    g.set_valid(converted.is_some());
    let Some(converted) = converted else {
        g.set_formats(ModelRc::default());
        return;
    };
    g.set_color(color::slint_color(converted.color));
    g.set_detected(converted.detected.into());
    let rows: Vec<ColorFormat> = converted
        .formats
        .into_iter()
        .map(|(label, value)| ColorFormat {
            label: label.into(),
            value: value.into(),
        })
        .collect();
    g.set_formats(ModelRc::new(VecModel::from(rows)));
}

// ---- Data ---------------------------------------------------------------------------------

/// Tailwind v3's palette: every hue at 50–950, from tailwindcss 3.4.19/colors.js (the
/// version the website builds with), then white and black. The website's own table held 11 of
/// these 22 hues, each value identical to this file's.
const TAILWIND: &[(&str, &str)] = &[
    ("slate-50", "#f8fafc"),
    ("slate-100", "#f1f5f9"),
    ("slate-200", "#e2e8f0"),
    ("slate-300", "#cbd5e1"),
    ("slate-400", "#94a3b8"),
    ("slate-500", "#64748b"),
    ("slate-600", "#475569"),
    ("slate-700", "#334155"),
    ("slate-800", "#1e293b"),
    ("slate-900", "#0f172a"),
    ("slate-950", "#020617"),
    ("gray-50", "#f9fafb"),
    ("gray-100", "#f3f4f6"),
    ("gray-200", "#e5e7eb"),
    ("gray-300", "#d1d5db"),
    ("gray-400", "#9ca3af"),
    ("gray-500", "#6b7280"),
    ("gray-600", "#4b5563"),
    ("gray-700", "#374151"),
    ("gray-800", "#1f2937"),
    ("gray-900", "#111827"),
    ("gray-950", "#030712"),
    ("zinc-50", "#fafafa"),
    ("zinc-100", "#f4f4f5"),
    ("zinc-200", "#e4e4e7"),
    ("zinc-300", "#d4d4d8"),
    ("zinc-400", "#a1a1aa"),
    ("zinc-500", "#71717a"),
    ("zinc-600", "#52525b"),
    ("zinc-700", "#3f3f46"),
    ("zinc-800", "#27272a"),
    ("zinc-900", "#18181b"),
    ("zinc-950", "#09090b"),
    ("neutral-50", "#fafafa"),
    ("neutral-100", "#f5f5f5"),
    ("neutral-200", "#e5e5e5"),
    ("neutral-300", "#d4d4d4"),
    ("neutral-400", "#a3a3a3"),
    ("neutral-500", "#737373"),
    ("neutral-600", "#525252"),
    ("neutral-700", "#404040"),
    ("neutral-800", "#262626"),
    ("neutral-900", "#171717"),
    ("neutral-950", "#0a0a0a"),
    ("stone-50", "#fafaf9"),
    ("stone-100", "#f5f5f4"),
    ("stone-200", "#e7e5e4"),
    ("stone-300", "#d6d3d1"),
    ("stone-400", "#a8a29e"),
    ("stone-500", "#78716c"),
    ("stone-600", "#57534e"),
    ("stone-700", "#44403c"),
    ("stone-800", "#292524"),
    ("stone-900", "#1c1917"),
    ("stone-950", "#0c0a09"),
    ("red-50", "#fef2f2"),
    ("red-100", "#fee2e2"),
    ("red-200", "#fecaca"),
    ("red-300", "#fca5a5"),
    ("red-400", "#f87171"),
    ("red-500", "#ef4444"),
    ("red-600", "#dc2626"),
    ("red-700", "#b91c1c"),
    ("red-800", "#991b1b"),
    ("red-900", "#7f1d1d"),
    ("red-950", "#450a0a"),
    ("orange-50", "#fff7ed"),
    ("orange-100", "#ffedd5"),
    ("orange-200", "#fed7aa"),
    ("orange-300", "#fdba74"),
    ("orange-400", "#fb923c"),
    ("orange-500", "#f97316"),
    ("orange-600", "#ea580c"),
    ("orange-700", "#c2410c"),
    ("orange-800", "#9a3412"),
    ("orange-900", "#7c2d12"),
    ("orange-950", "#431407"),
    ("amber-50", "#fffbeb"),
    ("amber-100", "#fef3c7"),
    ("amber-200", "#fde68a"),
    ("amber-300", "#fcd34d"),
    ("amber-400", "#fbbf24"),
    ("amber-500", "#f59e0b"),
    ("amber-600", "#d97706"),
    ("amber-700", "#b45309"),
    ("amber-800", "#92400e"),
    ("amber-900", "#78350f"),
    ("amber-950", "#451a03"),
    ("yellow-50", "#fefce8"),
    ("yellow-100", "#fef9c3"),
    ("yellow-200", "#fef08a"),
    ("yellow-300", "#fde047"),
    ("yellow-400", "#facc15"),
    ("yellow-500", "#eab308"),
    ("yellow-600", "#ca8a04"),
    ("yellow-700", "#a16207"),
    ("yellow-800", "#854d0e"),
    ("yellow-900", "#713f12"),
    ("yellow-950", "#422006"),
    ("lime-50", "#f7fee7"),
    ("lime-100", "#ecfccb"),
    ("lime-200", "#d9f99d"),
    ("lime-300", "#bef264"),
    ("lime-400", "#a3e635"),
    ("lime-500", "#84cc16"),
    ("lime-600", "#65a30d"),
    ("lime-700", "#4d7c0f"),
    ("lime-800", "#3f6212"),
    ("lime-900", "#365314"),
    ("lime-950", "#1a2e05"),
    ("green-50", "#f0fdf4"),
    ("green-100", "#dcfce7"),
    ("green-200", "#bbf7d0"),
    ("green-300", "#86efac"),
    ("green-400", "#4ade80"),
    ("green-500", "#22c55e"),
    ("green-600", "#16a34a"),
    ("green-700", "#15803d"),
    ("green-800", "#166534"),
    ("green-900", "#14532d"),
    ("green-950", "#052e16"),
    ("emerald-50", "#ecfdf5"),
    ("emerald-100", "#d1fae5"),
    ("emerald-200", "#a7f3d0"),
    ("emerald-300", "#6ee7b7"),
    ("emerald-400", "#34d399"),
    ("emerald-500", "#10b981"),
    ("emerald-600", "#059669"),
    ("emerald-700", "#047857"),
    ("emerald-800", "#065f46"),
    ("emerald-900", "#064e3b"),
    ("emerald-950", "#022c22"),
    ("teal-50", "#f0fdfa"),
    ("teal-100", "#ccfbf1"),
    ("teal-200", "#99f6e4"),
    ("teal-300", "#5eead4"),
    ("teal-400", "#2dd4bf"),
    ("teal-500", "#14b8a6"),
    ("teal-600", "#0d9488"),
    ("teal-700", "#0f766e"),
    ("teal-800", "#115e59"),
    ("teal-900", "#134e4a"),
    ("teal-950", "#042f2e"),
    ("cyan-50", "#ecfeff"),
    ("cyan-100", "#cffafe"),
    ("cyan-200", "#a5f3fc"),
    ("cyan-300", "#67e8f9"),
    ("cyan-400", "#22d3ee"),
    ("cyan-500", "#06b6d4"),
    ("cyan-600", "#0891b2"),
    ("cyan-700", "#0e7490"),
    ("cyan-800", "#155e75"),
    ("cyan-900", "#164e63"),
    ("cyan-950", "#083344"),
    ("sky-50", "#f0f9ff"),
    ("sky-100", "#e0f2fe"),
    ("sky-200", "#bae6fd"),
    ("sky-300", "#7dd3fc"),
    ("sky-400", "#38bdf8"),
    ("sky-500", "#0ea5e9"),
    ("sky-600", "#0284c7"),
    ("sky-700", "#0369a1"),
    ("sky-800", "#075985"),
    ("sky-900", "#0c4a6e"),
    ("sky-950", "#082f49"),
    ("blue-50", "#eff6ff"),
    ("blue-100", "#dbeafe"),
    ("blue-200", "#bfdbfe"),
    ("blue-300", "#93c5fd"),
    ("blue-400", "#60a5fa"),
    ("blue-500", "#3b82f6"),
    ("blue-600", "#2563eb"),
    ("blue-700", "#1d4ed8"),
    ("blue-800", "#1e40af"),
    ("blue-900", "#1e3a8a"),
    ("blue-950", "#172554"),
    ("indigo-50", "#eef2ff"),
    ("indigo-100", "#e0e7ff"),
    ("indigo-200", "#c7d2fe"),
    ("indigo-300", "#a5b4fc"),
    ("indigo-400", "#818cf8"),
    ("indigo-500", "#6366f1"),
    ("indigo-600", "#4f46e5"),
    ("indigo-700", "#4338ca"),
    ("indigo-800", "#3730a3"),
    ("indigo-900", "#312e81"),
    ("indigo-950", "#1e1b4b"),
    ("violet-50", "#f5f3ff"),
    ("violet-100", "#ede9fe"),
    ("violet-200", "#ddd6fe"),
    ("violet-300", "#c4b5fd"),
    ("violet-400", "#a78bfa"),
    ("violet-500", "#8b5cf6"),
    ("violet-600", "#7c3aed"),
    ("violet-700", "#6d28d9"),
    ("violet-800", "#5b21b6"),
    ("violet-900", "#4c1d95"),
    ("violet-950", "#2e1065"),
    ("purple-50", "#faf5ff"),
    ("purple-100", "#f3e8ff"),
    ("purple-200", "#e9d5ff"),
    ("purple-300", "#d8b4fe"),
    ("purple-400", "#c084fc"),
    ("purple-500", "#a855f7"),
    ("purple-600", "#9333ea"),
    ("purple-700", "#7e22ce"),
    ("purple-800", "#6b21a8"),
    ("purple-900", "#581c87"),
    ("purple-950", "#3b0764"),
    ("fuchsia-50", "#fdf4ff"),
    ("fuchsia-100", "#fae8ff"),
    ("fuchsia-200", "#f5d0fe"),
    ("fuchsia-300", "#f0abfc"),
    ("fuchsia-400", "#e879f9"),
    ("fuchsia-500", "#d946ef"),
    ("fuchsia-600", "#c026d3"),
    ("fuchsia-700", "#a21caf"),
    ("fuchsia-800", "#86198f"),
    ("fuchsia-900", "#701a75"),
    ("fuchsia-950", "#4a044e"),
    ("pink-50", "#fdf2f8"),
    ("pink-100", "#fce7f3"),
    ("pink-200", "#fbcfe8"),
    ("pink-300", "#f9a8d4"),
    ("pink-400", "#f472b6"),
    ("pink-500", "#ec4899"),
    ("pink-600", "#db2777"),
    ("pink-700", "#be185d"),
    ("pink-800", "#9d174d"),
    ("pink-900", "#831843"),
    ("pink-950", "#500724"),
    ("rose-50", "#fff1f2"),
    ("rose-100", "#ffe4e6"),
    ("rose-200", "#fecdd3"),
    ("rose-300", "#fda4af"),
    ("rose-400", "#fb7185"),
    ("rose-500", "#f43f5e"),
    ("rose-600", "#e11d48"),
    ("rose-700", "#be123c"),
    ("rose-800", "#9f1239"),
    ("rose-900", "#881337"),
    ("rose-950", "#4c0519"),
    ("white", "#ffffff"),
    ("black", "#000000"),
];

#[cfg(test)]
mod tests {
    use super::*;

    const VIOLET_600: Rgba = Rgba {
        r: 124,
        g: 58,
        b: 237,
        a: 1.0,
    };

    fn labels(c: &Converted) -> Vec<&str> {
        c.formats.iter().map(|(label, _)| *label).collect()
    }

    #[test]
    fn the_palette_holds_22_hues_of_11_shades_and_white_and_black() {
        assert_eq!(TAILWIND.len(), 244);
    }

    #[test]
    fn tailwind_class_names_an_exact_match() {
        assert_eq!(tailwind_class(VIOLET_600), "bg-violet-600");
    }

    #[test]
    fn tailwind_class_says_how_far_the_nearest_match_is() {
        let c = color::parse("#7F3BED").unwrap().1;

        assert_eq!(tailwind_class(c), "bg-violet-600 /* nearest match, Δ3 */");
    }

    #[test]
    fn tailwind_class_keeps_alpha_as_an_opacity_modifier() {
        let c = Rgba {
            a: 0.5,
            ..VIOLET_600
        };

        assert_eq!(tailwind_class(c), "bg-violet-600/[0.5]");
    }

    #[test]
    fn tailwind_class_reaches_hues_the_website_lacked() {
        let c = color::parse("#a855f7").unwrap().1;

        assert_eq!(tailwind_class(c), "bg-purple-500");
    }

    #[test]
    fn tailwind_input_reads_a_shade_with_or_without_a_prefix() {
        assert_eq!(tailwind_input("violet-600"), Some(VIOLET_600));
        assert_eq!(tailwind_input(" BG-violet-600 "), Some(VIOLET_600));
    }

    #[test]
    fn tailwind_input_reads_prefixed_white() {
        assert_eq!(
            tailwind_input("text-white"),
            color::parse("#ffffff").map(|(_, c)| c)
        );
    }

    #[test]
    fn tailwind_input_leaves_bare_names_to_css() {
        assert_eq!(tailwind_input("white"), None);
    }

    #[test]
    fn convert_reports_tailwind_input() {
        assert_eq!(convert("violet-600").unwrap().detected, "Tailwind");
    }

    #[test]
    fn convert_shows_every_format_and_the_css_name() {
        let converted = convert("#663399").unwrap();

        assert_eq!(
            labels(&converted),
            [
                "HEX", "RGB", "HSL", "HSV", "HWB", "Lab", "LCH", "OKLab", "OKLCH", "Tailwind",
                "CSS name"
            ]
        );
        assert_eq!(converted.formats[10].1, "rebeccapurple");
    }

    #[test]
    fn convert_shows_the_website_values_for_its_sample() {
        let converted = convert("#663399").unwrap();

        assert_eq!(converted.formats[2].1, "hsl(270, 50%, 40%)");
        assert_eq!(converted.formats[3].1, "hsv(270, 66.7%, 60%)");
        assert_eq!(converted.formats[8].1, "oklch(44.03% 0.1603 303.37)");
    }

    #[test]
    fn convert_of_nonsense_is_nothing() {
        assert_eq!(convert("nonsense"), None);
    }
}
