//! Colors family engine, shared by its five tools. Two halves:
//!
//! - Colours: parsing and formatting, ported from the website's src/lib/design-tools/colors.ts
//!   (hex, rgb, hsl, hsv, oklch) with its number rules kept (`parseFloat` prefixes, `toFixed`
//!   rounding, the same clamps), plus what the website's browser parsed for it: CSS names,
//!   `transparent`, hwb, lab, lch and oklab.
//! - Gradients: parsing and converting CSS gradient strings, ported from gradients.ts, and a
//!   renderer that draws them to pixels the way CSS does. The previews and the PNG exports use
//!   that one renderer, so a preview is exactly what exports.

use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use resvg::tiny_skia::Pixmap;

use crate::files;

// ---- Colours ------------------------------------------------------------------------------

/// sRGB with 8-bit channels, as the website keeps them, and alpha from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

/// The syntax a colour was written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Hex,
    Rgb,
    Hsl,
    Hsv,
    Hwb,
    Lab,
    Lch,
    Oklab,
    Oklch,
    /// A CSS name or `transparent`.
    Named,
}

/// Reads a CSS colour, trying the website's order (hex, rgb, hsl, hsv, oklch), then what its
/// browser resolved (names, hwb, lab, lch, oklab). `None` when it isn't one.
pub fn parse(input: &str) -> Option<(Source, Rgba)> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(c) = parse_hex(s) {
        return Some((Source::Hex, c));
    }
    if let Some(([r, g, b], a)) = legacy_args(s, &["rgba", "rgb"]) {
        let c = Rgba {
            r: rgb_channel(r)?,
            g: rgb_channel(g)?,
            b: rgb_channel(b)?,
            a: alpha(a)?,
        };
        return Some((Source::Rgb, c));
    }
    if let Some(([h, s, l], a)) = legacy_args(s, &["hsla", "hsl"]) {
        return Some((Source::Hsl, hsl_to_rgb(hue(h)?, pct(s)?, pct(l)?, alpha(a)?)));
    }
    if let Some(([h, s, v], a)) = legacy_args(s, &["hsva", "hsba", "hsv", "hsb"]) {
        return Some((Source::Hsv, hsv_to_rgb(hue(h)?, pct(s)?, pct(v)?, alpha(a)?)));
    }
    if let Some(([l, c, h], a)) = modern_args(s, "oklch") {
        let l = match l.strip_suffix('%') {
            Some(p) => parse_float(p)? / 100.0,
            None => parse_float(l)?,
        };
        let (c, h) = (parse_float(c)?.max(0.0), hue(h)?);
        let (a_axis, b_axis) = from_polar(c, h);
        return Some((Source::Oklch, oklab_to_rgb(l.clamp(0.0, 1.0), a_axis, b_axis, alpha(a)?)));
    }
    if let Some(c) = parse_name(s) {
        return Some((Source::Named, c));
    }
    if let Some(([h, w, b], a)) = modern_args(s, "hwb") {
        return Some((Source::Hwb, hwb_to_rgb(hue(h)?, pct(w)?, pct(b)?, alpha(a)?)));
    }
    if let Some(([l, a_axis, b_axis], a)) = modern_args(s, "lab") {
        let l = scaled(l, 1.0)?.clamp(0.0, 100.0);
        let c = lab_to_rgb(l, scaled(a_axis, 1.25)?, scaled(b_axis, 1.25)?, alpha(a)?);
        return Some((Source::Lab, c));
    }
    if let Some(([l, c, h], a)) = modern_args(s, "lch") {
        let (l, c) = (scaled(l, 1.0)?.clamp(0.0, 100.0), scaled(c, 1.5)?.max(0.0));
        let (a_axis, b_axis) = from_polar(c, hue(h)?);
        return Some((Source::Lch, lab_to_rgb(l, a_axis, b_axis, alpha(a)?)));
    }
    if let Some(([l, a_axis, b_axis], a)) = modern_args(s, "oklab") {
        let l = scaled(l, 0.01)?.clamp(0.0, 1.0);
        let c = oklab_to_rgb(l, scaled(a_axis, 0.004)?, scaled(b_axis, 0.004)?, alpha(a)?);
        return Some((Source::Oklab, c));
    }
    None
}

/// `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`.
fn parse_hex(s: &str) -> Option<Rgba> {
    let digits = s.strip_prefix('#')?;
    if !matches!(digits.len(), 3 | 4 | 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let short = digits.len() <= 4;
    let channel = |i: usize| {
        let pair = if short {
            digits[i..=i].repeat(2)
        } else {
            digits[2 * i..2 * i + 2].to_string()
        };
        u8::from_str_radix(&pair, 16).ok()
    };
    let a = match digits.len() {
        4 | 8 => f64::from(channel(3)?) / 255.0,
        _ => 1.0,
    };
    Some(Rgba {
        r: channel(0)?,
        g: channel(1)?,
        b: channel(2)?,
        a,
    })
}

fn parse_name(s: &str) -> Option<Rgba> {
    let name = s.to_ascii_lowercase();
    if name == "transparent" {
        return Some(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        });
    }
    NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, [r, g, b])| Rgba {
            r: *r,
            g: *g,
            b: *b,
            a: 1.0,
        })
}

/// The CSS name of an opaque colour, when it has one (the first, alphabetically: `aqua`
/// before `cyan`).
pub fn name_of(c: Rgba) -> Option<&'static str> {
    (c.a >= 1.0)
        .then(|| NAMES.iter().find(|(_, rgb)| *rgb == [c.r, c.g, c.b]))
        .flatten()
        .map(|(n, _)| *n)
}

/// The arguments of `rgb(…)`, `hsl(…)` and friends, as the website's pattern reads them:
/// three values separated by commas or spaces, then optionally `,` or `/` and an alpha.
fn legacy_args<'a>(s: &'a str, names: &[&str]) -> Option<([&'a str; 3], Option<&'a str>)> {
    let inner = function_body(s, names)?;
    let (main, alpha) = match inner.split_once('/') {
        Some((main, alpha)) => (main, Some(alpha)),
        None => (inner, None),
    };
    if main.trim_start().starts_with(',') || main.trim_end().ends_with(',') {
        return None;
    }
    let mut tokens = main
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty());
    let values = [tokens.next()?, tokens.next()?, tokens.next()?];
    let alpha = match (alpha, tokens.next()) {
        (Some(_), Some(_)) => return None,
        (Some(a), None) => Some(a),
        // Without a slash, a fourth value is the alpha only after a comma.
        (None, Some(a)) => {
            let before = &main[..main.rfind(a)?];
            if !before.trim_end().ends_with(',') || tokens.next().is_some() {
                return None;
            }
            Some(a)
        }
        (None, None) => None,
    };
    (!values[2].contains(')')).then_some((values, alpha))
}

/// The arguments of `oklch(L C H / A)` and the CSS Color 4 functions: commas count as spaces,
/// at least three values, and the alpha after a `/`.
fn modern_args<'a>(s: &'a str, name: &str) -> Option<([&'a str; 3], Option<&'a str>)> {
    let inner = function_body(s, &[name])?;
    let mut parts = inner.split('/');
    let components = parts.next()?;
    let alpha = parts.next().map(str::trim).filter(|a| !a.is_empty());
    let mut values = components
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty());
    Some(([values.next()?, values.next()?, values.next()?], alpha))
}

/// What is inside `name(` … `)`, the name matched case-insensitively.
fn function_body<'a>(s: &'a str, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| {
        let head = s.get(..name.len() + 1)?;
        let matches = head[..name.len()].eq_ignore_ascii_case(name) && head.ends_with('(');
        let inner = s[name.len() + 1..].strip_suffix(')')?;
        (matches && !inner.trim().is_empty()).then_some(inner)
    })
}

/// JavaScript's `Number.parseFloat`: the longest number at the start ("50%" is 50, "12deg"
/// is 12), and `None` where JavaScript gives `NaN`.
fn parse_float(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    let digits = |from: usize| {
        from + bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let int_end = digits(end);
    let mut has_digits = int_end > end;
    end = int_end;
    if bytes.get(end) == Some(&b'.') {
        let frac_end = digits(end + 1);
        has_digits |= frac_end > end + 1;
        if has_digits {
            end = frac_end;
        }
    }
    if !has_digits {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exp_end = digits(end + 1 + sign);
        if exp_end > end + 1 + sign {
            end = exp_end;
        }
    }
    s[..end].parse().ok()
}

/// JavaScript's `Math.round`: halves round up, also below zero.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn rgb_channel(value: &str) -> Option<u8> {
    let n = match value.strip_suffix('%') {
        Some(p) => js_round(parse_float(p)? / 100.0 * 255.0),
        None => js_round(parse_float(value)?),
    };
    Some(n.clamp(0.0, 255.0) as u8)
}

/// A saturation, lightness or value: `40%`, or 0.4 meaning the same.
fn pct(value: &str) -> Option<f64> {
    let n = match value.strip_suffix('%') {
        Some(p) => parse_float(p)?,
        None => parse_float(value)? * 100.0,
    };
    Some(n.clamp(0.0, 100.0))
}

/// A CSS Color 4 component: a number, or a percentage of `percent_unit` per 1%.
fn scaled(value: &str, percent_unit: f64) -> Option<f64> {
    match value.strip_suffix('%') {
        Some(p) => Some(parse_float(p)? * percent_unit),
        None => parse_float(value),
    }
}

fn alpha(value: Option<&str>) -> Option<f64> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Some(1.0);
    };
    let n = match value.strip_suffix('%') {
        Some(p) => parse_float(p)? / 100.0,
        None => parse_float(value)?,
    };
    Some(n.clamp(0.0, 1.0))
}

fn hue(value: &str) -> Option<f64> {
    Some(normalize_hue(parse_float(value)?))
}

fn normalize_hue(h: f64) -> f64 {
    let n = h % 360.0;
    if n < 0.0 { n + 360.0 } else { n }
}

/// JavaScript's `Number.parseFloat(x.toFixed(digits)).toString()`: at most `digits` decimals,
/// trailing zeros dropped. `toFixed` rounds an exact half up, so this rounds the exact decimal
/// expansion (which Rust prints) rather than the scaled float.
fn trim_number(x: f64, digits: usize) -> String {
    let exact = format!("{:.60}", x.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut kept: Vec<u8> = format!("{int}{}", &frac[..digits]).into_bytes();
    if frac.as_bytes()[digits] >= b'5' {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let text = String::from_utf8(kept).unwrap_or_default();
    let (int, frac) = text.split_at(text.len() - digits);
    let frac = frac.trim_end_matches('0');
    let sign = if x < 0.0 && (int.bytes().any(|b| b != b'0') || !frac.is_empty()) {
        "-"
    } else {
        ""
    };
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    }
}

/// An alpha as the formats write it: at most three decimals, trailing zeros dropped.
pub fn alpha_text(a: f64) -> String {
    trim_number(a, 3)
}

/// `#RRGGBB`, or `#RRGGBBAA` below full opacity, in capitals.
pub fn hex(c: Rgba) -> String {
    let base = format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b);
    if c.a >= 1.0 {
        base
    } else {
        format!("{base}{:02X}", js_round(c.a * 255.0) as u8)
    }
}

pub fn rgb(c: Rgba) -> String {
    if c.a >= 1.0 {
        format!("rgb({}, {}, {})", c.r, c.g, c.b)
    } else {
        format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, alpha_text(c.a))
    }
}

pub fn hsl(c: Rgba) -> String {
    let (h, s, l) = rgb_to_hsl(c);
    let value = format!("{}, {}%, {}%", js_round(h), trim_number(s, 1), trim_number(l, 1));
    if c.a >= 1.0 {
        format!("hsl({value})")
    } else {
        format!("hsla({value}, {})", alpha_text(c.a))
    }
}

pub fn hsv(c: Rgba) -> String {
    let (h, s, v) = rgb_to_hsv(c);
    let value = format!("{}, {}%, {}%", js_round(h), trim_number(s, 1), trim_number(v, 1));
    if c.a >= 1.0 {
        format!("hsv({value})")
    } else {
        format!("hsv({value}, {})", alpha_text(c.a))
    }
}

/// The CSS Color 4 functions put the alpha after a slash.
fn modern(name: &str, value: String, a: f64) -> String {
    if a >= 1.0 {
        format!("{name}({value})")
    } else {
        format!("{name}({value} / {})", alpha_text(a))
    }
}

pub fn hwb(c: Rgba) -> String {
    let (h, s, v) = rgb_to_hsv(c);
    let (w, b) = ((100.0 - s) * v / 100.0, 100.0 - v);
    let value = format!("{} {}% {}%", js_round(h), trim_number(w, 1), trim_number(b, 1));
    modern("hwb", value, c.a)
}

pub fn lab(c: Rgba) -> String {
    let (l, a, b) = rgb_to_lab(c);
    let value = format!("{}% {} {}", trim_number(l, 2), trim_number(a, 2), trim_number(b, 2));
    modern("lab", value, c.a)
}

pub fn lch(c: Rgba) -> String {
    let (l, a, b) = rgb_to_lab(c);
    let (chroma, h) = to_polar(a, b);
    let value = polar_value(trim_number(l, 2), chroma, 2, h);
    modern("lch", value, c.a)
}

/// `L% C H`. A grey's hue is rounding noise (the website printed white as hue 89.88), so it
/// reads 0 once the chroma rounds to 0.
fn polar_value(l: String, chroma: f64, chroma_digits: usize, h: f64) -> String {
    let chroma = trim_number(chroma, chroma_digits);
    let h = if chroma == "0" { 0.0 } else { h };
    format!("{l}% {chroma} {}", trim_number(h, 2))
}

pub fn oklab(c: Rgba) -> String {
    let (l, a, b) = rgb_to_oklab(c);
    let value = format!("{}% {} {}", trim_number(l * 100.0, 2), trim_number(a, 4), trim_number(b, 4));
    modern("oklab", value, c.a)
}

pub fn oklch(c: Rgba) -> String {
    let (l, a, b) = rgb_to_oklab(c);
    let (chroma, h) = to_polar(a, b);
    let value = polar_value(trim_number(l * 100.0, 2), chroma, 4, h);
    modern("oklch", value, c.a)
}

/// The hue sextant of the website's hsl and hsv conversions.
fn rgb_hue(r: f64, g: f64, b: f64, max: f64, delta: f64) -> f64 {
    if delta == 0.0 {
        return 0.0;
    }
    let h = if max == r {
        ((g - b) / delta) % 6.0
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    normalize_hue(h * 60.0)
}

fn unit_channels(c: Rgba) -> (f64, f64, f64) {
    (
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    )
}

/// Hue in degrees, saturation and lightness in percent.
fn rgb_to_hsl(c: Rgba) -> (f64, f64, f64) {
    let (r, g, b) = unit_channels(c);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let delta = max - min;
    let l = (max + min) / 2.0;
    let s = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * l - 1.0).abs())
    };
    (rgb_hue(r, g, b, max, delta), s * 100.0, l * 100.0)
}

/// Hue in degrees, saturation and value in percent.
fn rgb_to_hsv(c: Rgba) -> (f64, f64, f64) {
    let (r, g, b) = unit_channels(c);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let delta = max - min;
    let s = if max == 0.0 { 0.0 } else { delta / max * 100.0 };
    (rgb_hue(r, g, b, max, delta), s, max * 100.0)
}

/// The shared tail of hsl and hsv to rgb: chroma and the hue's second-largest component.
fn hue_to_rgb(h: f64, chroma: f64, m: f64, a: f64) -> Rgba {
    let h = normalize_hue(h);
    let x = chroma * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match h {
        h if h < 60.0 => (chroma, x, 0.0),
        h if h < 120.0 => (x, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, x),
        h if h < 240.0 => (0.0, x, chroma),
        h if h < 300.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let byte = |v: f64| js_round((v + m) * 255.0) as u8;
    Rgba {
        r: byte(r),
        g: byte(g),
        b: byte(b),
        a,
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64, a: f64) -> Rgba {
    let (s, l) = (s.clamp(0.0, 100.0) / 100.0, l.clamp(0.0, 100.0) / 100.0);
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    hue_to_rgb(h, chroma, l - chroma / 2.0, a)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64, a: f64) -> Rgba {
    let (s, v) = (s.clamp(0.0, 100.0) / 100.0, v.clamp(0.0, 100.0) / 100.0);
    let chroma = v * s;
    hue_to_rgb(h, chroma, v - chroma, a)
}

/// CSS Color 4: whiteness and blackness that add up to 100% or more make a grey.
fn hwb_to_rgb(h: f64, w: f64, b: f64, a: f64) -> Rgba {
    let (w, b) = (w / 100.0, b / 100.0);
    if w + b >= 1.0 {
        let grey = js_round(w / (w + b) * 255.0) as u8;
        return Rgba {
            r: grey,
            g: grey,
            b: grey,
            a,
        };
    }
    let v = 1.0 - b;
    hsv_to_rgb(h, (1.0 - w / v) * 100.0, v * 100.0, a)
}

fn to_polar(a: f64, b: f64) -> (f64, f64) {
    ((a * a + b * b).sqrt(), normalize_hue(b.atan2(a) * 180.0 / PI))
}

fn from_polar(chroma: f64, h: f64) -> (f64, f64) {
    let radians = normalize_hue(h) * PI / 180.0;
    (chroma * radians.cos(), chroma * radians.sin())
}

fn srgb_to_linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn linear_rgb(c: Rgba) -> [f64; 3] {
    let (r, g, b) = unit_channels(c);
    [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)]
}

/// Linear-light sRGB back to 8-bit channels, clamped into the gamut.
fn from_linear([r, g, b]: [f64; 3], a: f64) -> Rgba {
    let byte = |v: f64| js_round((linear_to_srgb(v) * 255.0).clamp(0.0, 255.0)) as u8;
    Rgba {
        r: byte(r),
        g: byte(g),
        b: byte(b),
        a,
    }
}

/// Björn Ottosson's OKLab, with the website's matrices: lightness 0–1, then a and b.
fn rgb_to_oklab(c: Rgba) -> (f64, f64, f64) {
    let [r, g, b] = linear_rgb(c);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    (
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    )
}

fn oklab_to_rgb(l: f64, a: f64, b: f64, alpha: f64) -> Rgba {
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    from_linear(
        [
            4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_,
            -1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_,
            -0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701 * s_,
        ],
        alpha,
    )
}

// CIE Lab as CSS Color 4 defines it: D50 white, Bradford adaptation from sRGB's D65. The
// matrices are the ones in the specification's sample code.
const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
const KAPPA: f64 = 24389.0 / 27.0;
const EPSILON: f64 = 216.0 / 24389.0;
const LINEAR_TO_XYZ_D65: [[f64; 3]; 3] = [
    [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
    [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
    [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
];
const XYZ_D65_TO_LINEAR: [[f64; 3]; 3] = [
    [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
    [-851781.0 / 878810.0, 1648619.0 / 878810.0, 36519.0 / 878810.0],
    [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
];
const D65_TO_D50: [[f64; 3]; 3] = [
    [1.047_929_792_544_997, 0.022_946_870_601_609_652, -0.050_192_266_289_205_24],
    [0.029_627_808_770_055_99, 0.990_434_426_753_879_9, -0.017_073_799_063_418_826],
    [-0.009_243_040_646_204_504, 0.015_055_191_490_298_152, 0.751_874_281_428_137_1],
];
const D50_TO_D65: [[f64; 3]; 3] = [
    [0.955_473_421_488_075, -0.023_098_454_948_764_71, 0.063_259_243_200_570_72],
    [-0.028_369_709_333_863_7, 1.009_995_398_081_304_1, 0.021_041_441_191_917_323],
    [0.012_314_014_864_481_998, -0.020_507_649_298_898_964, 1.330_365_926_242_124],
];

fn mul(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

/// Lightness 0–100, then a and b.
fn rgb_to_lab(c: Rgba) -> (f64, f64, f64) {
    let xyz = mul(&D65_TO_D50, mul(&LINEAR_TO_XYZ_D65, linear_rgb(c)));
    let f = |i: usize| {
        let t = xyz[i] / D50[i];
        if t > EPSILON {
            t.cbrt()
        } else {
            (KAPPA * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(0), f(1), f(2));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

fn lab_to_rgb(l: f64, a: f64, b: f64, alpha: f64) -> Rgba {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (a / 500.0 + fy, fy - b / 200.0);
    let inverse = |f: f64| {
        if f.powi(3) > EPSILON {
            f.powi(3)
        } else {
            (116.0 * f - 16.0) / KAPPA
        }
    };
    let y = if l > KAPPA * EPSILON {
        fy.powi(3)
    } else {
        l / KAPPA
    };
    let xyz = [inverse(fx) * D50[0], y * D50[1], inverse(fz) * D50[2]];
    from_linear(mul(&XYZ_D65_TO_LINEAR, mul(&D50_TO_D65, xyz)), alpha)
}

/// Premultiplied sRGB in 0–1, the space CSS gradients interpolate in.
fn premultiplied(c: Rgba) -> [f32; 4] {
    let a = c.a as f32;
    [
        f32::from(c.r) / 255.0 * a,
        f32::from(c.g) / 255.0 * a,
        f32::from(c.b) / 255.0 * a,
        a,
    ]
}

// ---- Gradients ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Linear,
    Radial,
    Conic,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Linear, Kind::Radial, Kind::Conic];

    fn name(self) -> &'static str {
        match self {
            Kind::Linear => "linear",
            Kind::Radial => "radial",
            Kind::Conic => "conic",
        }
    }
}

const DEFAULT_LINEAR: &str = "135deg";
const DEFAULT_RADIAL: &str = "circle at center";
const DEFAULT_CONIC: &str = "from 90deg at center";

/// The website's eight `to …` keywords and the angles it maps them to.
const DIRECTIONS: [(&str, u32); 8] = [
    ("to top", 0),
    ("to top right", 45),
    ("to right", 90),
    ("to bottom right", 135),
    ("to bottom", 180),
    ("to bottom left", 225),
    ("to left", 270),
    ("to top left", 315),
];

/// A gradient as the website's `parseGradientString` splits it: the orientation (the default
/// for the kind when none was written) and the stops, each kept as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: Kind,
    pub orientation: String,
    pub stops: Vec<String>,
}

impl Gradient {
    /// `linear-gradient(…)`, `radial-gradient(…)` or `conic-gradient(…)` with at least two
    /// stops; a trailing `;` is ignored.
    pub fn parse(input: &str) -> Option<Self> {
        let s = strip_trailing_semicolon(input.trim());
        let (kind, inner) = Kind::ALL.into_iter().find_map(|kind| {
            let head = format!("{}-gradient(", kind.name());
            let start = s.get(..head.len())?;
            let inner = s[head.len()..].strip_suffix(')')?;
            (start.eq_ignore_ascii_case(&head) && !inner.is_empty()).then_some((kind, inner))
        })?;
        let segments: Vec<String> = split_top_level(inner)
            .iter()
            .map(|s| s.trim().to_string())
            .collect();
        if segments.len() < 2 {
            return None;
        }
        let first = segments[0].to_lowercase();
        let has_orientation = match kind {
            Kind::Linear => first.starts_with("to ") || is_angle(&first),
            Kind::Radial => {
                ["circle", "ellipse", "at ", "closest-", "farthest-"]
                    .iter()
                    .any(|w| first.contains(w))
            }
            Kind::Conic => first.contains("from ") || first.contains("at "),
        };
        let (orientation, stops) = if has_orientation {
            (segments[0].clone(), segments[1..].to_vec())
        } else {
            (default_orientation(kind).to_string(), segments)
        };
        (stops.len() >= 2).then_some(Gradient {
            kind,
            orientation,
            stops,
        })
    }

    /// The website's `normalized`: `kind-gradient(orientation, stops)`.
    pub fn css(&self) -> String {
        css_from_parts(self.kind, &self.orientation, &self.stops)
    }

    /// The website's `convertGradientSyntax`: the same stops as another kind, keeping what of
    /// the orientation carries over (a conic's start angle becomes a linear angle and back).
    pub fn convert(&self, to: Kind) -> String {
        let orientation = match (to, self.kind) {
            (Kind::Linear, Kind::Linear) | (Kind::Radial, Kind::Radial) | (Kind::Conic, Kind::Conic) => {
                self.orientation.clone()
            }
            (Kind::Linear, Kind::Conic) => conic_from(&self.orientation)
                .unwrap_or(DEFAULT_LINEAR)
                .to_string(),
            (Kind::Conic, Kind::Linear) => {
                format!("from {} at center", linear_angle(&self.orientation))
            }
            (kind, _) => default_orientation(kind).to_string(),
        };
        css_from_parts(to, &orientation, &self.stops)
    }
}

fn default_orientation(kind: Kind) -> &'static str {
    match kind {
        Kind::Linear => DEFAULT_LINEAR,
        Kind::Radial => DEFAULT_RADIAL,
        Kind::Conic => DEFAULT_CONIC,
    }
}

fn css_from_parts(kind: Kind, orientation: &str, stops: &[String]) -> String {
    format!("{}-gradient({orientation}, {})", kind.name(), stops.join(", "))
}

/// `background: <gradient>;`
pub fn background_property(css: &str) -> String {
    format!("background: {};", strip_trailing_semicolon(css))
}

/// Tailwind's arbitrary value, `bg-[…]`, with every run of whitespace written as `_`.
pub fn tailwind_arbitrary(css: &str) -> String {
    let css = strip_trailing_semicolon(css);
    format!("bg-[{}]", css.split_whitespace().collect::<Vec<_>>().join("_"))
}

fn strip_trailing_semicolon(s: &str) -> &str {
    s.trim_end().trim_end_matches(';').trim_end()
}

/// Splits on commas outside parentheses: `rgba(0, 0, 0, .5) 10%` stays one stop.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() {
        parts.push(&s[start..]);
    }
    parts
}

/// The website's angle token: `-?\d+(\.\d+)?(deg|turn|rad|grad)`, case-insensitive.
fn is_angle(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    let Some(number) = ["deg", "turn", "grad", "rad"]
        .iter()
        .find_map(|unit| lower.strip_suffix(unit))
    else {
        return false;
    };
    let number = number.strip_prefix('-').unwrap_or(number);
    let (int, frac) = match number.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (number, None),
    };
    let all_digits = |d: &str| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit());
    all_digits(int) && frac.is_none_or(all_digits)
}

/// The angle after `from` in a conic orientation, as written.
fn conic_from(orientation: &str) -> Option<&str> {
    let at = orientation.to_ascii_lowercase().find("from")?;
    let rest = &orientation[at + 4..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    rest.split_whitespace().next().filter(|t| is_angle(t))
}

/// A linear orientation as an angle token: an angle stays as written, a `to …` keyword maps
/// to the website's angle, anything else is the default.
fn linear_angle(orientation: &str) -> String {
    let trimmed = orientation.trim();
    if is_angle(trimmed) {
        return trimmed.to_string();
    }
    let lower = trimmed.to_lowercase();
    DIRECTIONS
        .iter()
        .find(|(name, _)| *name == lower)
        .map_or(DEFAULT_LINEAR.to_string(), |(_, deg)| format!("{deg}deg"))
}

/// An angle in degrees: `deg`, `grad`, `rad` or `turn`.
fn angle_degrees(token: &str) -> Option<f64> {
    if !is_angle(token) {
        return None;
    }
    let n = parse_float(token)?;
    let lower = token.to_ascii_lowercase();
    Some(if lower.ends_with("grad") {
        n * 0.9
    } else if lower.ends_with("rad") {
        n * 180.0 / PI
    } else if lower.ends_with("turn") {
        n * 360.0
    } else {
        n
    })
}

/// Splits on whitespace outside parentheses: `rgba(0, 0, 0) 10%` is two tokens.
fn split_tokens(s: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let (mut depth, mut start) = (0usize, None);
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => {
                if let Some(from) = start.take() {
                    tokens.push(&s[from..i]);
                }
                continue;
            }
            _ => {}
        }
        start.get_or_insert(i);
    }
    if let Some(from) = start {
        tokens.push(&s[from..]);
    }
    tokens
}

/// A length along the gradient: a percentage, or pixels against the line's length in pixels.
#[derive(Clone, Copy)]
enum Length {
    Fraction(f64),
    Px(f64),
}

impl Length {
    fn parse(token: &str) -> Option<Self> {
        if let Some(p) = token.strip_suffix('%') {
            return p.parse().ok().map(|n: f64| Length::Fraction(n / 100.0));
        }
        if let Some(p) = token.strip_suffix("px") {
            return p.parse().ok().map(Length::Px);
        }
        (token == "0").then_some(Length::Px(0.0))
    }

    fn resolve(self, full: f64) -> f64 {
        match self {
            Length::Fraction(f) => f * full,
            Length::Px(px) => px,
        }
    }
}

/// Where the shape sits, in pixels, from lowercase tokens: `top left`, `30% 40%`, `20px`.
fn position(tokens: &[&str], w: f64, h: f64) -> Result<(f64, f64), String> {
    let coord = |t: &str, start: &str, end: &str, full: f64| match t {
        "center" => Some(full / 2.0),
        t if t == start => Some(0.0),
        t if t == end => Some(full),
        t => Length::parse(t).map(|l| l.resolve(full)),
    };
    let x = |t: &str| coord(t, "left", "right", w);
    let y = |t: &str| coord(t, "top", "bottom", h);
    let vertical = |t: &str| t == "top" || t == "bottom";
    let horizontal = |t: &str| t == "left" || t == "right";
    let point = match tokens {
        [one] if vertical(one) => (Some(w / 2.0), y(one)),
        [one] => (x(one), Some(h / 2.0)),
        [a, b] if vertical(a) || horizontal(b) => (x(b), y(a)),
        [a, b] => (x(a), y(b)),
        _ => (None, None),
    };
    match point {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(format!("Couldn't read the position “{}”.", tokens.join(" "))),
    }
}

/// How a pixel maps onto the gradient's line, from 0 (first stop) to 1 (last).
enum Shape {
    Linear { cx: f64, cy: f64, sin: f64, cos: f64, length: f64 },
    /// An ellipse (or circle, rx = ry) of radii rx, ry around (cx, cy).
    Radial { cx: f64, cy: f64, rx: f64, ry: f64 },
    /// Clockwise from `from` degrees, starting at the top.
    Conic { cx: f64, cy: f64, from: f64 },
}

impl Shape {
    fn new(kind: Kind, orientation: &str, w: f64, h: f64) -> Result<Self, String> {
        match kind {
            Kind::Linear => {
                let angle = linear_degrees(orientation, w, h)
                    .ok_or_else(|| format!("Couldn't read the direction “{orientation}”."))?;
                let (sin, cos) = (angle * PI / 180.0).sin_cos();
                let length = (w * sin).abs() + (h * cos).abs();
                Ok(Shape::Linear {
                    cx: w / 2.0,
                    cy: h / 2.0,
                    sin,
                    cos,
                    length,
                })
            }
            Kind::Radial => radial_shape(orientation, w, h),
            Kind::Conic => {
                let lower = orientation.to_ascii_lowercase();
                let tokens = split_tokens(&lower);
                let (mut from, mut rest) = (0.0, tokens.as_slice());
                if let ["from", angle, tail @ ..] = rest {
                    from = angle_degrees(angle)
                        .ok_or_else(|| format!("Couldn't read the angle “{angle}”."))?;
                    rest = tail;
                }
                let (cx, cy) = match rest {
                    [] => (w / 2.0, h / 2.0),
                    ["at", place @ ..] => position(place, w, h)?,
                    _ => return Err(format!("Couldn't read “{orientation}”.")),
                };
                Ok(Shape::Conic { cx, cy, from })
            }
        }
    }

    /// The length of the gradient line in pixels, for stop positions written in pixels.
    fn line_length(&self) -> f64 {
        match self {
            Shape::Linear { length, .. } => *length,
            Shape::Radial { rx, .. } => *rx,
            Shape::Conic { .. } => 1.0,
        }
    }

    fn t(&self, x: f64, y: f64) -> f64 {
        match *self {
            Shape::Linear { cx, cy, sin, cos, length } => {
                ((x - cx) * sin - (y - cy) * cos) / length + 0.5
            }
            Shape::Radial { cx, cy, rx, ry } => {
                (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt()
            }
            Shape::Conic { cx, cy, from } => {
                let degrees = (x - cx).atan2(cy - y) * 180.0 / PI;
                ((degrees - from) / 360.0).rem_euclid(1.0)
            }
        }
    }
}

/// A linear orientation in degrees, clockwise from "to top". A corner keyword points the
/// line at that corner's quadrant, perpendicular to the diagonal between the other two, so it
/// depends on the box's shape (45° only for a square).
fn linear_degrees(orientation: &str, w: f64, h: f64) -> Option<f64> {
    let lower = orientation.trim().to_ascii_lowercase();
    if let Some(deg) = angle_degrees(&lower) {
        return Some(deg);
    }
    let mut words = lower.strip_prefix("to ")?.split_whitespace().collect::<Vec<_>>();
    words.sort_unstable();
    let corner = h.atan2(w) * 180.0 / PI;
    Some(match words.as_slice() {
        ["top"] => 0.0,
        ["right"] => 90.0,
        ["bottom"] => 180.0,
        ["left"] => 270.0,
        ["right", "top"] => corner,
        ["bottom", "right"] => 180.0 - corner,
        ["bottom", "left"] => 180.0 + corner,
        ["left", "top"] => 360.0 - corner,
        _ => return None,
    })
}

/// `[circle | ellipse] [size] [at position]`, sized as CSS sizes it: `farthest-corner`
/// unless a keyword or radii say otherwise.
fn radial_shape(orientation: &str, w: f64, h: f64) -> Result<Shape, String> {
    let lower = orientation.to_ascii_lowercase();
    let tokens = split_tokens(&lower);
    let split = tokens.iter().position(|t| *t == "at").unwrap_or(tokens.len());
    let (cx, cy) = match &tokens[split..] {
        [] => (w / 2.0, h / 2.0),
        [_, place @ ..] => position(place, w, h)?,
    };
    let unreadable = || format!("Couldn't read the radial shape “{orientation}”.");
    let mut circle = None;
    let mut keyword = "farthest-corner";
    let mut radii = Vec::new();
    for token in &tokens[..split] {
        match *token {
            "circle" => circle = Some(true),
            "ellipse" => circle = Some(false),
            "closest-side" | "closest-corner" | "farthest-side" | "farthest-corner" => keyword = token,
            t => radii.push(Length::parse(t).ok_or_else(unreadable)?),
        }
    }
    let (rx, ry) = match (radii.as_slice(), circle) {
        ([r], Some(true) | None) => {
            let r = r.resolve(w);
            (r, r)
        }
        ([rx, ry], Some(false) | None) => (rx.resolve(w), ry.resolve(h)),
        ([], _) => sized(keyword, circle.unwrap_or(false), cx, cy, w, h),
        _ => return Err(unreadable()),
    };
    // A zero radius paints the last colour everywhere, as in a browser.
    Ok(Shape::Radial {
        cx,
        cy,
        rx: rx.max(1e-9),
        ry: ry.max(1e-9),
    })
}

/// The radii a size keyword gives, from the distances to the nearest or farthest sides.
fn sized(keyword: &str, circle: bool, cx: f64, cy: f64, w: f64, h: f64) -> (f64, f64) {
    let (left, right, top, bottom) = (cx.abs(), (w - cx).abs(), cy.abs(), (h - cy).abs());
    let closest = keyword.starts_with("closest");
    let (dx, dy) = if closest {
        (left.min(right), top.min(bottom))
    } else {
        (left.max(right), top.max(bottom))
    };
    let corner = keyword.ends_with("corner");
    match (circle, corner) {
        (true, true) => (dx.hypot(dy), dx.hypot(dy)),
        (true, false) if closest => (dx.min(dy), dx.min(dy)),
        (true, false) => (dx.max(dy), dx.max(dy)),
        // The ellipse with the sides' aspect ratio that passes through that corner.
        (false, true) => (dx * std::f64::consts::SQRT_2, dy * std::f64::consts::SQRT_2),
        (false, false) => (dx, dy),
    }
}

/// One resolved colour stop: its colour (premultiplied) at a fraction of the line.
#[derive(Clone, Copy)]
struct Stop {
    color: [f32; 4],
    at: f64,
}

fn stop_position(token: &str, kind: Kind, line: f64) -> Option<f64> {
    if kind == Kind::Conic
        && let Some(deg) = angle_degrees(token)
    {
        return Some(deg / 360.0);
    }
    match Length::parse(token)? {
        Length::Fraction(f) => Some(f),
        Length::Px(px) => Some(px / line),
    }
}

/// The colours and where they sit as fractions of the line, placed as CSS places them. Pixel
/// positions need the line's length, so this is for gradients positioned in percent (the
/// presets, and the Gradient Maker's own).
pub fn stops(g: &Gradient) -> Result<Vec<(Rgba, f64)>, String> {
    resolve_stops(g, 1.0)
}

/// The stops placed as CSS places them: a missing first or last position is 0% or 100%, a
/// position before an earlier one moves up to it, and the rest spread evenly between.
fn resolve_stops(g: &Gradient, line: f64) -> Result<Vec<(Rgba, f64)>, String> {
    if g.stops.is_empty() {
        return Err("The gradient has no colour stops.".into());
    }
    let mut stops: Vec<(Rgba, Option<f64>)> = Vec::new();
    for written in &g.stops {
        let tokens = split_tokens(written);
        let Some((color, positions)) = tokens.split_first() else {
            return Err("One of the stops is empty.".into());
        };
        let (_, color) =
            parse(color).ok_or_else(|| format!("Couldn't read “{color}” as a colour."))?;
        if positions.len() > 2 {
            return Err(format!("Couldn't read the stop “{written}”."));
        }
        if positions.is_empty() {
            stops.push((color, None));
        }
        for p in positions {
            let at = stop_position(p, g.kind, line)
                .ok_or_else(|| format!("Couldn't read the position “{p}”."))?;
            stops.push((color, Some(at)));
        }
    }
    let last = stops.len() - 1;
    stops[0].1.get_or_insert(0.0);
    stops[last].1.get_or_insert(1.0);
    let mut highest = f64::MIN;
    for (_, at) in &mut stops {
        if let Some(at) = at {
            *at = at.max(highest);
            highest = *at;
        }
    }
    let mut i = 0;
    while i < last {
        let start = i;
        i += 1;
        while stops[i].1.is_none() {
            i += 1;
        }
        let (from, to) = (stops[start].1.unwrap_or(0.0), stops[i].1.unwrap_or(1.0));
        let gap = (i - start) as f64;
        for (step, stop) in stops[start + 1..i].iter_mut().enumerate() {
            stop.1 = Some(from + (to - from) * (step + 1) as f64 / gap);
        }
    }
    Ok(stops
        .into_iter()
        .map(|(c, at)| (c, at.unwrap_or(0.0)))
        .collect())
}

/// The colour at `t` on the line: the end colours beyond the ends, linear in between.
fn sample(stops: &[Stop], t: f64) -> [f32; 4] {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if t <= first.at {
        return first.color;
    }
    if t >= last.at {
        return last.color;
    }
    let i = stops.partition_point(|s| s.at <= t);
    let (a, b) = (stops[i - 1], stops[i]);
    let f = ((t - a.at) / (b.at - a.at)) as f32;
    std::array::from_fn(|c| a.color[c] + (b.color[c] - a.color[c]) * f)
}

/// Draws `g` into a `width` × `height` pixmap as a browser would. The error is the sentence a
/// person sees, naming the part that can't be drawn.
pub fn render(g: &Gradient, width: u32, height: u32) -> Result<Pixmap, String> {
    let (w, h) = (f64::from(width), f64::from(height));
    let shape = Shape::new(g.kind, &g.orientation, w, h)?;
    let stops: Vec<Stop> = resolve_stops(g, shape.line_length())?
        .into_iter()
        .map(|(c, at)| Stop {
            color: premultiplied(c),
            at,
        })
        .collect();
    let mut pixmap =
        Pixmap::new(width, height).ok_or_else(|| "The image size is out of range.".to_string())?;
    let row = width as usize * 4;
    for (y, line) in pixmap.data_mut().chunks_exact_mut(row).enumerate() {
        for (x, px) in line.chunks_exact_mut(4).enumerate() {
            let color = sample(&stops, shape.t(x as f64 + 0.5, y as f64 + 0.5));
            for (out, c) in px.iter_mut().zip(color) {
                *out = (c * 255.0 + 0.5) as u8;
            }
        }
    }
    Ok(pixmap)
}

/// Encodes `g` drawn at `width` × `height` as a PNG file's bytes.
pub fn png(g: &Gradient, width: u32, height: u32) -> Result<Vec<u8>, String> {
    render(g, width, height)?
        .encode_png()
        .map_err(|e| format!("Couldn't encode the PNG: {e}"))
}

/// A rendered gradient as pixels for `slint::Image::from_rgba8_premultiplied`; unlike an
/// `Image`, they can be made on a worker and sent to the UI thread.
pub fn pixels(p: &Pixmap) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    slint::SharedPixelBuffer::clone_from_slice(p.data(), p.width(), p.height())
}

/// The colour as Slint draws it.
pub fn slint_color(c: Rgba) -> slint::Color {
    slint::Color::from_argb_u8(js_round(c.a * 255.0) as u8, c.r, c.g, c.b)
}

/// A colour Slint reports (the picker's), back as 8-bit channels and alpha.
pub fn from_slint(c: slint::Color) -> Rgba {
    Rgba {
        r: c.red(),
        g: c.green(),
        b: c.blue(),
        a: f64::from(c.alpha()) / 255.0,
    }
}

/// The live previews' pixels: the export's 16:9 at half size, sharp in a 400-point pane at 2×.
pub const PREVIEW_WIDTH: u32 = 800;
pub const PREVIEW_HEIGHT: u32 = 450;

/// The PNG export's size, the website's (src/lib/design-tools/constants.ts).
pub const EXPORT_WIDTH: u32 = 1600;
pub const EXPORT_HEIGHT: u32 = 900;

/// Writes `g` as an export-sized PNG to `path`, a path the user chose in a save dialog: through
/// a temporary file, so a failure leaves any file already there untouched. Slow in a debug
/// build, so call it off the UI thread. The message is the sentence to show either way.
pub fn save_png(g: &Gradient, path: &Path) -> Result<String, String> {
    let name = files::file_name(path);
    let failed = |e: String| format!("Couldn't save {name}: {e}");
    let bytes = png(g, EXPORT_WIDTH, EXPORT_HEIGHT).map_err(failed)?;
    let out = files::reserve_replacing(path).map_err(|e| failed(e.to_string()))?;
    fs::write(out.path(), bytes).map_err(|e| failed(e.to_string()))?;
    out.keep().map_err(|e| failed(e.to_string()))?;
    Ok(format!("Saved {name}"))
}

/// `save_png` on its own thread; `done` gets its message there. Returns the thread so the app
/// can let it finish on quit.
pub fn export_png(
    g: Gradient,
    path: PathBuf,
    done: impl FnOnce(Result<String, String>) + Send + 'static,
) -> JoinHandle<()> {
    thread::spawn(move || done(save_png(&g, &path)))
}

/// The file name the website gave a gradient's PNG: the name in lower case, every run of other
/// characters a `-`.
pub fn png_name(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') || slug.is_empty() {
            slug.push('-');
        }
    }
    format!("{slug}.png")
}

/// A linear orientation as degrees, the way the website maps it: an angle as written, a
/// `to …` keyword at its 45° step, anything else 135°.
pub fn linear_angle_degrees(orientation: &str) -> f64 {
    angle_degrees(&linear_angle(orientation)).unwrap_or(135.0)
}

// ---- Background rendering -----------------------------------------------------------------

/// A thread that runs only the newest job it's given: while one runs, newer jobs replace the
/// waiting one, so dragging a slider renders as often as rendering keeps up, never a backlog.
pub struct Latest {
    tx: Option<mpsc::Sender<Box<dyn FnOnce() + Send>>>,
    handle: Option<JoinHandle<()>>,
}

impl Default for Latest {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let handle = thread::spawn(move || {
            while let Ok(mut job) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    job = newer;
                }
                job();
            }
        });
        Latest {
            tx: Some(tx),
            handle: Some(handle),
        }
    }
}

impl Latest {
    /// Renders `g` at the preview size on this thread and hands the pixels, or the sentence
    /// saying what can't be drawn, to `done` there.
    pub fn preview(
        &self,
        g: Gradient,
        done: impl FnOnce(Result<slint::SharedPixelBuffer<slint::Rgba8Pixel>, String>) + Send + 'static,
    ) {
        self.run(move || done(render(&g, PREVIEW_WIDTH, PREVIEW_HEIGHT).map(|p| pixels(&p))));
    }

    pub fn run(&self, job: impl FnOnce() + Send + 'static) {
        if let Some(tx) = &self.tx
            && tx.send(Box::new(job)).is_err()
        {
            eprintln!("the gradient preview thread has stopped");
        }
    }

    /// Lets the thread finish its current job and end; returns it for joining.
    pub fn shutdown(&mut self) -> Vec<JoinHandle<()>> {
        self.tx = None;
        self.handle.take().into_iter().collect()
    }
}

// ---- Data ---------------------------------------------------------------------------------

/// The 148 CSS named colours (CSS Color 4), from the color-name package in the website's
/// dependencies. The browser resolved these for the website.
const NAMES: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("grey", [128, 128, 128]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

#[cfg(test)]
mod tests {
    use super::*;

    const VIOLET: Rgba = Rgba {
        r: 124,
        g: 58,
        b: 237,
        a: 1.0,
    };
    const HALF_VIOLET: Rgba = Rgba { a: 0.5, ..VIOLET };

    fn color(input: &str) -> Rgba {
        parse(input).unwrap().1
    }

    fn gradient(input: &str) -> Gradient {
        Gradient::parse(input).unwrap()
    }

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = pixmap.pixel(x, y).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    // Expected values in the colour and gradient-string tests come from running the website's
    // colors.ts and gradients.ts, which have no tests of their own.

    #[test]
    fn hex_formats_in_capitals() {
        assert_eq!(hex(VIOLET), "#7C3AED");
    }

    #[test]
    fn hex_adds_the_alpha_byte_below_full_opacity() {
        assert_eq!(hex(color("#7c3aed80")), "#7C3AED80");
    }

    #[test]
    fn rgb_formats_opaque_and_translucent() {
        assert_eq!(rgb(VIOLET), "rgb(124, 58, 237)");
        assert_eq!(rgb(color("#7c3aed80")), "rgba(124, 58, 237, 0.502)");
    }

    #[test]
    fn hsl_rounds_the_hue_and_trims_one_decimal() {
        assert_eq!(hsl(VIOLET), "hsl(262, 83.3%, 57.8%)");
        assert_eq!(hsl(HALF_VIOLET), "hsla(262, 83.3%, 57.8%, 0.5)");
    }

    #[test]
    fn hsv_keeps_the_website_alpha_form() {
        assert_eq!(hsv(VIOLET), "hsv(262, 75.5%, 92.9%)");
        assert_eq!(hsv(HALF_VIOLET), "hsv(262, 75.5%, 92.9%, 0.5)");
    }

    #[test]
    fn oklch_matches_the_website() {
        assert_eq!(oklch(VIOLET), "oklch(54.13% 0.2466 293.01)");
        assert_eq!(oklch(HALF_VIOLET), "oklch(54.13% 0.2466 293.01 / 0.5)");
    }

    #[test]
    fn oklch_of_black_has_no_hue() {
        assert_eq!(oklch(color("#000000")), "oklch(0% 0 0)");
    }

    // A deliberate change: the website printed "oklch(100% 0 89.88)", a hue made of noise.
    #[test]
    fn oklch_of_white_has_no_hue() {
        assert_eq!(oklch(color("#ffffff")), "oklch(100% 0 0)");
    }

    #[test]
    fn alpha_trims_to_three_decimals() {
        assert_eq!(rgb(color("rgba(0,0,0,0.12345)")), "rgba(0, 0, 0, 0.123)");
    }

    #[test]
    fn short_hex_doubles_each_digit() {
        assert_eq!(hex(color("#abcd")), "#AABBCCDD");
    }

    #[test]
    fn rgb_reads_spaces_slash_and_percent_alpha() {
        assert_eq!(
            parse("rgb(124 58 237 / 50%)"),
            Some((Source::Rgb, HALF_VIOLET))
        );
    }

    #[test]
    fn rgb_reads_percent_channels() {
        assert_eq!(hex(color("rgb(10%, 20%, 30%)")), "#1A334D");
    }

    #[test]
    fn rgb_clamps_out_of_range_channels() {
        assert_eq!(hex(color("rgb(300, -5, 128)")), "#FF0080");
    }

    #[test]
    fn rgb_needs_a_comma_or_slash_before_alpha() {
        assert_eq!(parse("rgb(1 2 3 0.5)"), None);
    }

    #[test]
    fn rgb_needs_three_channels() {
        assert_eq!(parse("rgb(1,2)"), None);
    }

    #[test]
    fn hsl_reads_space_syntax() {
        assert_eq!(hex(color("hsl(263 83% 58%)")), "#7F3BED");
    }

    #[test]
    fn hsl_reads_fractions_as_percentages() {
        assert_eq!(
            rgb(color("hsla(263, 0.83, 0.58, 0.5)")),
            "rgba(127, 59, 237, 0.5)"
        );
    }

    #[test]
    fn hsl_wraps_a_negative_hue() {
        assert_eq!(hex(color("hsl(-30 100% 50%)")), "#FF0080");
    }

    #[test]
    fn hsb_is_another_name_for_hsv() {
        assert_eq!(
            parse("hsb(120 100% 50%)"),
            Some((Source::Hsv, color("#008000")))
        );
    }

    #[test]
    fn hsv_reads_comma_syntax() {
        assert_eq!(hex(color("hsv(263, 75%, 93%)")), "#7F3BED");
    }

    #[test]
    fn oklch_reads_fraction_or_percent_lightness() {
        assert_eq!(hex(color("oklch(0.6 0.2 300)")), "#955BE3");
        assert_eq!(hex(color("oklch(62.8% 0.2577 29.23 / 0.5)")), "#FF000080");
    }

    #[test]
    fn oklch_needs_three_components() {
        assert_eq!(parse("oklch(0.5 0.1)"), None);
    }

    #[test]
    fn a_five_digit_hex_is_not_a_colour() {
        assert_eq!(parse("#12345"), None);
    }

    #[test]
    fn words_that_are_not_numbers_are_rejected() {
        assert_eq!(parse("rgb(red, 2, 3)"), None);
    }

    #[test]
    fn names_resolve_case_insensitively() {
        assert_eq!(
            parse("RebeccaPurple"),
            Some((Source::Named, color("#663399")))
        );
    }

    #[test]
    fn transparent_is_clear_black() {
        assert_eq!(hex(color("transparent")), "#00000000");
    }

    #[test]
    fn there_are_148_names() {
        assert_eq!(NAMES.len(), 148);
    }

    #[test]
    fn name_of_finds_the_first_alphabetical_name() {
        assert_eq!(name_of(color("#00ffff")), Some("aqua"));
    }

    #[test]
    fn name_of_skips_translucent_colours() {
        assert_eq!(name_of(color("#00ffff80")), None);
    }

    // lab, lch, hwb: CSS Color 4 maths, cross-checked against colord 2.10 (the website's
    // dependency) to within its rounding.

    #[test]
    fn lab_of_red() {
        assert_eq!(lab(color("red")), "lab(54.29% 80.8 69.89)");
    }

    #[test]
    fn lch_of_violet() {
        assert_eq!(lch(VIOLET), "lch(42.23% 98.71 305.25)");
    }

    #[test]
    fn hwb_of_violet() {
        assert_eq!(hwb(VIOLET), "hwb(262 22.7% 7.1%)");
    }

    #[test]
    fn hwb_greys_out_when_white_and_black_reach_100() {
        assert_eq!(hex(color("hwb(0 60% 60%)")), "#808080");
    }

    #[test]
    fn oklab_of_violet() {
        assert_eq!(oklab(VIOLET), "oklab(54.13% 0.0964 -0.227)");
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(color(&hex(VIOLET)), VIOLET);
    }

    #[test]
    fn rgb_round_trips() {
        assert_eq!(color(&rgb(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn hsl_round_trips_where_one_decimal_is_exact() {
        let rebecca = Rgba {
            a: 0.5,
            ..color("#663399")
        };

        assert_eq!(color(&hsl(rebecca)), rebecca);
    }

    #[test]
    fn hsl_loses_a_little_to_its_one_decimal() {
        assert_eq!(hex(color(&hsl(VIOLET))), "#7B3AED");
    }

    #[test]
    fn hsv_round_trips() {
        assert_eq!(color(&hsv(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn hwb_round_trips() {
        assert_eq!(color(&hwb(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn lab_round_trips() {
        assert_eq!(color(&lab(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn lch_round_trips() {
        assert_eq!(color(&lch(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn oklab_round_trips() {
        assert_eq!(color(&oklab(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn oklch_round_trips() {
        assert_eq!(color(&oklch(HALF_VIOLET)), HALF_VIOLET);
    }

    #[test]
    fn each_syntax_reports_its_source() {
        let inputs = [
            "#fff",
            "rgb(1 2 3)",
            "hsl(1 2% 3%)",
            "hsv(1 2% 3%)",
            "hwb(1 2% 3%)",
            "lab(1 2 3)",
            "lch(1 2 3)",
            "oklab(1 2 3)",
            "oklch(1 2 3)",
            "white",
        ];

        let sources = inputs.map(|s| parse(s).map(|(source, _)| source));

        assert_eq!(
            sources,
            [
                Some(Source::Hex),
                Some(Source::Rgb),
                Some(Source::Hsl),
                Some(Source::Hsv),
                Some(Source::Hwb),
                Some(Source::Lab),
                Some(Source::Lch),
                Some(Source::Oklab),
                Some(Source::Oklch),
                Some(Source::Named),
            ]
        );
    }

    #[test]
    fn parse_float_reads_a_leading_number() {
        assert_eq!(parse_float(" 12.5deg"), Some(12.5));
        assert_eq!(parse_float("-.5e1x"), Some(-5.0));
        assert_eq!(parse_float("deg"), None);
    }

    #[test]
    fn trim_number_rounds_exact_halves_up_like_to_fixed() {
        assert_eq!(trim_number(0.125, 2), "0.13");
        assert_eq!(trim_number(2.25, 1), "2.3");
    }

    #[test]
    fn trim_number_drops_trailing_zeros_and_negative_zero() {
        assert_eq!(trim_number(40.0, 1), "40");
        assert_eq!(trim_number(-0.00001, 2), "0");
    }

    // ---- Gradients ----

    #[test]
    fn gradient_parse_keeps_orientation_and_stops() {
        assert_eq!(
            gradient("linear-gradient(135deg, #2563EB 0%, #7C3AED 50%, #EC4899 100%)"),
            Gradient {
                kind: Kind::Linear,
                orientation: "135deg".into(),
                stops: vec![
                    "#2563EB 0%".into(),
                    "#7C3AED 50%".into(),
                    "#EC4899 100%".into()
                ],
            }
        );
    }

    #[test]
    fn gradient_parse_ignores_a_trailing_semicolon() {
        assert_eq!(
            gradient("linear-gradient(to right, #0f2027, #2c5364);").css(),
            "linear-gradient(to right, #0f2027, #2c5364)"
        );
    }

    #[test]
    fn gradient_parse_fills_in_the_default_radial_shape() {
        assert_eq!(
            gradient("radial-gradient(red, rgba(0, 0, 255, 0.5) 40%, blue)").css(),
            "radial-gradient(circle at center, red, rgba(0, 0, 255, 0.5) 40%, blue)"
        );
    }

    #[test]
    fn gradient_parse_fills_in_the_default_conic_start() {
        assert_eq!(
            gradient("conic-gradient(red, blue)").orientation,
            "from 90deg at center"
        );
    }

    #[test]
    fn gradient_parse_is_case_insensitive_and_reads_turns() {
        assert_eq!(
            gradient("LINEAR-GRADIENT(0.25turn, red, blue)").orientation,
            "0.25turn"
        );
    }

    #[test]
    fn gradient_parse_needs_two_stops() {
        assert_eq!(Gradient::parse("linear-gradient(to right, red)"), None);
        assert_eq!(Gradient::parse("linear-gradient(red)"), None);
    }

    #[test]
    fn gradient_parse_rejects_repeating_gradients() {
        assert_eq!(Gradient::parse("repeating-linear-gradient(red, blue)"), None);
    }

    #[test]
    fn convert_linear_to_radial() {
        let g = gradient("linear-gradient(135deg, #2563EB 0%, #7C3AED 50%, #EC4899 100%)");

        assert_eq!(
            g.convert(Kind::Radial),
            "radial-gradient(circle at center, #2563EB 0%, #7C3AED 50%, #EC4899 100%)"
        );
    }

    #[test]
    fn convert_linear_angle_to_a_conic_start() {
        let g = gradient("linear-gradient(135deg, #2563EB 0%, #7C3AED 50%, #EC4899 100%)");

        assert_eq!(
            g.convert(Kind::Conic),
            "conic-gradient(from 135deg at center, #2563EB 0%, #7C3AED 50%, #EC4899 100%)"
        );
    }

    #[test]
    fn convert_maps_a_direction_keyword_to_a_conic_angle() {
        assert_eq!(
            gradient("linear-gradient(to right, #0f2027, #2c5364)").convert(Kind::Conic),
            "conic-gradient(from 90deg at center, #0f2027, #2c5364)"
        );
    }

    #[test]
    fn convert_takes_a_conic_start_as_the_linear_angle() {
        assert_eq!(
            gradient("conic-gradient(from 45deg at center, red, blue)").convert(Kind::Linear),
            "linear-gradient(45deg, red, blue)"
        );
    }

    #[test]
    fn convert_radial_to_linear_uses_the_default_angle() {
        assert_eq!(
            gradient("radial-gradient(ellipse at top left, red, blue)").convert(Kind::Linear),
            "linear-gradient(135deg, red, blue)"
        );
    }

    #[test]
    fn convert_keeps_a_radial_shape() {
        assert_eq!(
            gradient("radial-gradient(ellipse at top left, red, blue)").convert(Kind::Radial),
            "radial-gradient(ellipse at top left, red, blue)"
        );
    }

    #[test]
    fn background_property_wraps_the_gradient() {
        assert_eq!(
            background_property("linear-gradient(to right, red, blue);"),
            "background: linear-gradient(to right, red, blue);"
        );
    }

    #[test]
    fn tailwind_arbitrary_replaces_whitespace_runs() {
        assert_eq!(
            tailwind_arbitrary("linear-gradient(315deg, #2563EB 0%,  #EC4899 100%);"),
            "bg-[linear-gradient(315deg,_#2563EB_0%,_#EC4899_100%)]"
        );
    }

    // Rendering: the expected pixels follow the CSS Images definitions.

    #[test]
    fn render_linear_runs_from_the_first_stop_to_the_last() {
        let pixmap = render(&gradient("linear-gradient(to right, red, blue)"), 100, 10).unwrap();

        assert_eq!(pixel(&pixmap, 0, 5), [254, 0, 1, 255]);
        assert_eq!(pixel(&pixmap, 99, 5), [1, 0, 254, 255]);
    }

    #[test]
    fn render_linear_zero_degrees_points_up() {
        let pixmap = render(&gradient("linear-gradient(0deg, red, blue)"), 10, 100).unwrap();

        assert_eq!(pixel(&pixmap, 5, 0), [1, 0, 254, 255]);
    }

    #[test]
    fn render_corner_keyword_runs_the_midpoint_through_the_other_corners() {
        let pixmap =
            render(&gradient("linear-gradient(to top right, red, blue)"), 160, 90).unwrap();

        assert_eq!(pixel(&pixmap, 0, 0), [128, 0, 127, 255]);
    }

    #[test]
    fn render_a_45_degree_angle_is_not_the_corner_keyword() {
        let pixmap = render(&gradient("linear-gradient(45deg, red, blue)"), 160, 90).unwrap();

        assert_eq!(pixel(&pixmap, 0, 0), [163, 0, 92, 255]);
    }

    #[test]
    fn render_honours_the_radial_shape_and_position() {
        let pixmap = render(
            &gradient("radial-gradient(ellipse at top left, red, blue)"),
            160,
            90,
        )
        .unwrap();

        assert_eq!(pixel(&pixmap, 0, 0), [254, 0, 1, 255]);
        assert_eq!(pixel(&pixmap, 159, 89), [1, 0, 254, 255]);
    }

    #[test]
    fn render_circle_reaches_the_farthest_corner() {
        let pixmap = render(
            &gradient("radial-gradient(circle at center, red, blue)"),
            100,
            100,
        )
        .unwrap();

        assert_eq!(pixel(&pixmap, 50, 50), [252, 0, 3, 255]);
        assert_eq!(pixel(&pixmap, 0, 0), [3, 0, 252, 255]);
    }

    #[test]
    fn render_conic_turns_clockwise_from_the_start_angle() {
        let pixmap = render(&gradient("conic-gradient(from 90deg, red, blue)"), 100, 100).unwrap();

        assert_eq!(pixel(&pixmap, 99, 50), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 0, 50), [128, 0, 127, 255]);
    }

    #[test]
    fn render_moves_a_stop_placed_before_an_earlier_one() {
        let pixmap = render(
            &gradient("linear-gradient(to right, red 60%, blue 20%)"),
            100,
            1,
        )
        .unwrap();

        assert_eq!(pixel(&pixmap, 59, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 60, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn render_spreads_unplaced_stops_evenly() {
        let pixmap = render(
            &gradient("linear-gradient(to right, red, lime, blue 50%)"),
            100,
            1,
        )
        .unwrap();

        assert_eq!(pixel(&pixmap, 25, 0), [0, 250, 5, 255]);
    }

    #[test]
    fn render_interpolates_premultiplied() {
        let pixmap = render(
            &gradient("linear-gradient(to right, transparent, white)"),
            2,
            1,
        )
        .unwrap();

        assert_eq!(pixel(&pixmap, 0, 0), [64, 64, 64, 64]);
    }

    #[test]
    fn render_names_a_colour_it_cannot_read() {
        assert_eq!(
            render(&gradient("linear-gradient(red, nonsense)"), 2, 2).err(),
            Some("Couldn't read “nonsense” as a colour.".into())
        );
    }

    #[test]
    fn render_names_a_position_it_cannot_read() {
        assert_eq!(
            render(&gradient("linear-gradient(red 1em, blue)"), 2, 2).err(),
            Some("Couldn't read the position “1em”.".into())
        );
    }

    #[test]
    fn png_has_the_requested_size() {
        let bytes = png(
            &gradient("radial-gradient(ellipse at bottom, red, blue)"),
            160,
            90,
        )
        .unwrap();

        let decoded = Pixmap::decode_png(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (160, 90));
    }

    #[test]
    fn stops_spread_unplaced_colours_evenly() {
        let placed = stops(&gradient("linear-gradient(to right, #0f2027, #203a43, #2c5364)"));

        assert_eq!(
            placed.map(|s| s.iter().map(|(c, at)| (hex(*c), *at)).collect::<Vec<_>>()),
            Ok(vec![
                ("#0F2027".to_string(), 0.0),
                ("#203A43".to_string(), 0.5),
                ("#2C5364".to_string(), 1.0)
            ])
        );
    }

    #[test]
    fn slint_colours_round_trip_with_alpha() {
        let c = from_slint(slint::Color::from_argb_u8(128, 124, 58, 237));

        assert_eq!(hex(c), "#7C3AED80");
        assert_eq!(slint_color(c), slint::Color::from_argb_u8(128, 124, 58, 237));
    }

    #[test]
    fn png_name_follows_the_website() {
        assert_eq!(png_name("Sunset & Sea"), "sunset-sea.png");
    }

    #[test]
    fn linear_angle_degrees_maps_keywords_and_angles() {
        assert_eq!(linear_angle_degrees("to right"), 90.0);
        assert_eq!(linear_angle_degrees("0.5turn"), 180.0);
        assert_eq!(linear_angle_degrees("sideways"), 135.0);
    }

    // The website's PNG export drew every radial gradient as a centred circle; this one sits
    // at the top left, so that corner is the first colour and the far corner the last.
    #[test]
    fn save_png_writes_the_export_size_and_honours_the_radial_shape() {
        let path = std::env::temp_dir().join("open-tools-colors-radial-export.png");
        let g = gradient("radial-gradient(ellipse at top left, red, blue)");

        let message = save_png(&g, &path);

        let saved = Pixmap::load_png(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(message, Ok("Saved open-tools-colors-radial-export.png".into()));
        assert_eq!((saved.width(), saved.height()), (1600, 900));
        assert_eq!(pixel(&saved, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&saved, 1599, 899), [0, 0, 255, 255]);
    }

    #[test]
    fn latest_runs_a_job_and_stops() {
        let mut latest = Latest::default();
        let (tx, rx) = mpsc::channel();

        latest.run(move || tx.send(7).unwrap());

        assert_eq!(rx.recv(), Ok(7));
        assert_eq!(latest.shutdown().len(), 1);
    }
}
