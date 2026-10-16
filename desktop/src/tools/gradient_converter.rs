//! Gradient Converter: a gradient pasted as CSS, a Tailwind class or a Bootstrap rule, shown as
//! all three (the website showed the two not pasted) with a rendered preview, and rewritten as
//! linear, radial or conic on request. The string rules are the website's: its input cleaning,
//! its Tailwind v3 direction classes, its Bootstrap block. The preview renders on a worker.

use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

use slint::ComponentHandle;

use crate::engine::color::{self, Gradient, Kind, Latest};
use crate::{AppWindow, GradientConverter, clipboard, jobs};

pub const SLUG: &str = "gradient-converter";
const SAMPLE: &str = "linear-gradient(135deg, #2563EB 0%, #7C3AED 50%, #EC4899 100%)";
const UTILITY: &str = "bg-gradient-to-";
/// Tailwind v3's direction suffixes and the CSS keywords they stand for.
const DIRECTIONS: [(&str, &str); 8] = [
    ("t", "to top"),
    ("tr", "to top right"),
    ("r", "to right"),
    ("br", "to bottom right"),
    ("b", "to bottom"),
    ("bl", "to bottom left"),
    ("l", "to left"),
    ("tl", "to top left"),
];
/// The angles the website also accepted for each suffix, in the same order.
const ANGLES: [&str; 8] = [
    "0deg", "45deg", "90deg", "135deg", "180deg", "225deg", "270deg", "315deg",
];

// ---- Logic --------------------------------------------------------------------------------

/// Where `needle` starts in `s` with no word character just before it (a regex `\b`).
fn find_word(s: &str, needle: &str) -> Option<usize> {
    s.match_indices(needle).map(|(i, _)| i).find(|&i| {
        !s[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// "Tailwind" for a `bg-[…]` or `bg-gradient-to-…` class, "Bootstrap" for its gradient variable
/// or class, otherwise "CSS".
fn detect(input: &str) -> &'static str {
    if input.contains("bg-[") || find_word(input, UTILITY).is_some() {
        "Tailwind"
    } else if input.contains("var(--bs-gradient)") || input.contains(".bg-gradient") {
        "Bootstrap"
    } else {
        "CSS"
    }
}

/// `[colour]` after `prefix`, which follows whitespace: Tailwind's `from-[…]`.
fn bracketed<'a>(s: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = s.trim_start();
    if rest.len() == s.len() {
        return None;
    }
    let inner = rest.strip_prefix(prefix)?.strip_prefix('[')?;
    let end = inner.find(']').filter(|&e| e > 0)?;
    Some((&inner[..end], &inner[end + 1..]))
}

/// `bg-gradient-to-<dir> from-[c] [via-[c]] [to-[c]]` as a linear gradient.
fn from_utility(s: &str) -> Option<String> {
    let after = &s[find_word(s, UTILITY)? + UTILITY.len()..];
    let dir_len = after.bytes().take_while(u8::is_ascii_lowercase).count();
    let dir = &after[..dir_len];
    if dir.is_empty() {
        return None;
    }
    let (from, rest) = bracketed(&after[dir_len..], "from-")?;
    let mut stops = vec![from];
    let mut rest = rest;
    for prefix in ["via-", "to-"] {
        if let Some((c, tail)) = bracketed(rest, prefix) {
            stops.push(c);
            rest = tail;
        }
    }
    let direction = DIRECTIONS
        .iter()
        .find(|(short, _)| *short == dir)
        .map_or(format!("to {dir}"), |(_, long)| (*long).to_string());
    Some(format!(
        "linear-gradient({direction}, {})",
        stops.join(", ")
    ))
}

/// The website's input cleaning: a Tailwind utility becomes CSS, a `bg-[…]` value is
/// unwrapped, a `background:` property name dropped, and a Bootstrap rule's
/// `background-image` taken out.
fn clean(input: &str) -> String {
    let trimmed = input.trim();
    let c = trimmed.strip_suffix(';').unwrap_or(trimmed);
    if let Some(css) = from_utility(c) {
        return css;
    }
    let mut c = match c.strip_prefix("bg-[").and_then(|s| s.strip_suffix(']')) {
        Some(inner) => inner.replace('_', " "),
        None => c.to_string(),
    };
    let lower = c.to_ascii_lowercase();
    if let Some(name) = ["background-image", "background", "--bs-gradient"]
        .iter()
        .find(|n| lower.starts_with(*n))
        && let Some(value) = c[name.len()..].trim_start().strip_prefix(':')
    {
        c = value.trim_start().to_string();
    }
    let lower = c.to_ascii_lowercase();
    if let Some(at) = lower.find("background-image")
        && let Some(value) = c[at + "background-image".len()..]
            .trim_start()
            .strip_prefix(':')
    {
        let value = value.split(';').next().unwrap_or_default().trim();
        if !value.is_empty() {
            c = value.to_string();
        }
    }
    c
}

/// The website's Tailwind output: a v3 direction class for a linear gradient of up to three
/// stops at one of the eight directions, else the arbitrary value. The class keeps each stop's
/// first word only, so positions drop.
fn tailwind(g: &Gradient) -> String {
    let orientation = g.orientation.trim().to_lowercase();
    let suffix = DIRECTIONS
        .iter()
        .zip(ANGLES)
        .find(|((_, keyword), angle)| *keyword == orientation || *angle == orientation)
        .map(|((short, _), _)| *short);
    let (Kind::Linear, true, Some(dir)) = (g.kind, g.stops.len() <= 3, suffix) else {
        return color::tailwind_arbitrary(&g.css());
    };
    let colors: Vec<&str> = g
        .stops
        .iter()
        .map(|s| s.split_whitespace().next().unwrap_or_default())
        .collect();
    match colors.as_slice() {
        [from, via, to] => format!("bg-gradient-to-{dir} from-[{from}] via-[{via}] to-[{to}]"),
        [from, to] => format!("bg-gradient-to-{dir} from-[{from}] to-[{to}]"),
        _ => format!("bg-gradient-to-{dir} from-[{}]", colors[0]),
    }
}

/// The website's Bootstrap 5 rule, the first stop's colour as the fallback background.
fn bootstrap(g: &Gradient) -> String {
    let fallback = g
        .stops
        .first()
        .and_then(|s| s.split(' ').next())
        .unwrap_or("#000");
    format!(
        ".custom-gradient {{\n  background-color: {fallback};\n  background-image: {};\n}}",
        g.css()
    )
}

fn kind_index(kind: Kind) -> i32 {
    match kind {
        Kind::Linear => 0,
        Kind::Radial => 1,
        Kind::Conic => 2,
    }
}

// ---- State and wiring ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    preview: Latest,
    /// Counts preview requests, so a slow render never replaces a newer one.
    generation: u64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn init(app: &AppWindow) {
    let g = app.global::<GradientConverter>();
    let weak = app.as_weak();
    g.on_edited(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_convert(move |i| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<GradientConverter>();
        let Some(parsed) = Gradient::parse(&clean(&g.get_input())) else {
            return;
        };
        let kind = Kind::ALL[crate::select::index(i, Kind::ALL.len())];
        g.set_input(parsed.convert(kind).into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy(move |text| {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        match clipboard::copy_text(&text) {
            Ok(()) => true,
            Err(e) => {
                app.global::<GradientConverter>()
                    .set_error(format!("Couldn't copy: {e}").into());
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

/// The preview thread finishes its render and ends.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.preview.shutdown())
}

fn refresh(app: &AppWindow) {
    let g = app.global::<GradientConverter>();
    let input = g.get_input();
    let parsed = Gradient::parse(&clean(&input));
    g.set_valid(parsed.is_some());
    g.set_detected(detect(&input).into());
    let Some(parsed) = parsed else {
        let error = if input.trim().is_empty() {
            ""
        } else {
            "Couldn't read that as a gradient. Paste a linear, radial or conic gradient, a \
             Tailwind gradient class, or a Bootstrap rule."
        };
        g.set_error(error.into());
        g.set_css("".into());
        g.set_tailwind("".into());
        g.set_bootstrap("".into());
        return;
    };
    g.set_error("".into());
    g.set_kind(kind_index(parsed.kind));
    g.set_stop_count(parsed.stops.len() as i32);
    g.set_css(color::background_property(&parsed.css()).into());
    g.set_tailwind(tailwind(&parsed).into());
    g.set_bootstrap(bootstrap(&parsed).into());
    let weak = app.as_weak();
    with(|s| {
        s.generation += 1;
        let id = s.generation;
        s.preview.preview(parsed, move |result| {
            jobs::post(&weak, move |app| {
                if with(|s| s.generation) != id {
                    return;
                }
                let g = app.global::<GradientConverter>();
                match result {
                    Ok(pixels) => g.set_preview(slint::Image::from_rgba8_premultiplied(pixels)),
                    Err(e) => g.set_error(e.into()),
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> Gradient {
        Gradient::parse(&clean(input)).unwrap()
    }

    // Expected strings follow the website's gradient-converter page and gradients.ts.

    #[test]
    fn detect_reads_tailwind_classes() {
        assert_eq!(detect("bg-gradient-to-r from-[#fff] to-[#000]"), "Tailwind");
        assert_eq!(detect("bg-[linear-gradient(red,blue)]"), "Tailwind");
    }

    #[test]
    fn detect_needs_a_word_boundary_before_the_utility() {
        assert_eq!(detect("xbg-gradient-to-r"), "CSS");
    }

    #[test]
    fn detect_reads_bootstrap() {
        assert_eq!(detect(".bg-gradient { }"), "Bootstrap");
    }

    #[test]
    fn detect_defaults_to_css() {
        assert_eq!(detect(SAMPLE), "CSS");
    }

    #[test]
    fn clean_turns_a_tailwind_utility_into_css() {
        assert_eq!(
            clean("bg-gradient-to-br from-[#2563EB] via-[#7C3AED] to-[#EC4899]"),
            "linear-gradient(to bottom right, #2563EB, #7C3AED, #EC4899)"
        );
    }

    #[test]
    fn clean_keeps_an_unknown_direction_as_a_keyword() {
        assert_eq!(
            clean("bg-gradient-to-x from-[red] to-[blue]"),
            "linear-gradient(to x, red, blue)"
        );
    }

    #[test]
    fn clean_unwraps_an_arbitrary_value() {
        assert_eq!(
            clean("bg-[linear-gradient(90deg,_red_0%,_blue_100%)]"),
            "linear-gradient(90deg, red 0%, blue 100%)"
        );
    }

    #[test]
    fn clean_drops_a_property_name_and_semicolon() {
        assert_eq!(
            clean("Background: linear-gradient(red, blue);"),
            "linear-gradient(red, blue)"
        );
    }

    #[test]
    fn clean_takes_background_image_out_of_a_rule() {
        assert_eq!(
            clean(
                ".custom-gradient {\n  background-color: red;\n  background-image: linear-gradient(red, blue);\n}"
            ),
            "linear-gradient(red, blue)"
        );
    }

    #[test]
    fn tailwind_names_a_direction_class_for_three_stops() {
        assert_eq!(
            tailwind(&parsed(SAMPLE)),
            "bg-gradient-to-br from-[#2563EB] via-[#7C3AED] to-[#EC4899]"
        );
    }

    #[test]
    fn tailwind_reads_a_direction_keyword() {
        assert_eq!(
            tailwind(&parsed("linear-gradient(to right, #0f2027, #2c5364)")),
            "bg-gradient-to-r from-[#0f2027] to-[#2c5364]"
        );
    }

    #[test]
    fn tailwind_falls_back_to_the_arbitrary_value() {
        assert_eq!(
            tailwind(&parsed("radial-gradient(red, blue)")),
            "bg-[radial-gradient(circle_at_center,_red,_blue)]"
        );
    }

    #[test]
    fn tailwind_falls_back_for_an_odd_angle() {
        assert_eq!(
            tailwind(&parsed("linear-gradient(10deg, red, blue)")),
            "bg-[linear-gradient(10deg,_red,_blue)]"
        );
    }

    #[test]
    fn bootstrap_uses_the_first_colour_as_the_fallback() {
        assert_eq!(
            bootstrap(&parsed(SAMPLE)),
            ".custom-gradient {\n  background-color: #2563EB;\n  background-image: linear-gradient(135deg, #2563EB 0%, #7C3AED 50%, #EC4899 100%);\n}"
        );
    }

    #[test]
    fn a_utility_round_trips_through_css_to_the_same_class() {
        let class = "bg-gradient-to-r from-[#0f2027] to-[#2c5364]";

        assert_eq!(tailwind(&parsed(class)), class);
    }
}
