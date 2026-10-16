//! Palette Extractor: the dominant, visibly distinct colours of one image, as swatches to copy
//! (hex, rgb, hsl) and a palette to export (CSS variables or JSON). Wires the
//! `PaletteExtractor` global (ui/tools/palette-extractor.slint).
//!
//! `quantize` is the website's algorithm (src/lib/design-tools/palette-extractor.ts), ported
//! exactly: histogram a ≤96 px copy into 4-bit-per-channel buckets, rank the buckets by size,
//! and greedily keep the averages that are far enough from every colour already kept. Decoding
//! runs on a thread; the small sample stays in the state, so changing the colour count is
//! instant.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use ::image::imageops::FilterType;
use ::image::{DynamicImage, RgbaImage};
use serde::Serialize;
use slint::{ComponentHandle, Image, ModelRc, VecModel};

use crate::engine::image::{self, INPUT_EXTENSIONS};
use crate::files::{self, file_name};
use crate::jobs;
use crate::{AppWindow, PaletteExport, PaletteExtractor, PaletteSwatch, SwatchFormat, clipboard};

pub const SLUG: &str = "palette-extractor";
/// The sample's longer side: enough pixels for the histogram, few enough to be instant.
const SAMPLE_SIZE: u32 = 96;
const BUCKET_BITS: u32 = 4;
/// Pixels more transparent than this don't count.
const MIN_ALPHA: u8 = 128;
/// Squared RGB distance below which two colours read as the same swatch.
const MIN_DISTANCE_SQ: i32 = 40 * 40;
/// The longer side of the image shown in the pane.
const PREVIEW_SIDE: u32 = 1200;

// ---- Logic --------------------------------------------------------------------------------

type Rgb = [u8; 3];

/// Up to `count` dominant colours of straight RGBA pixels, most common first.
fn quantize(rgba: &[u8], count: usize) -> Vec<Rgb> {
    #[derive(Default)]
    struct Bucket {
        n: u32,
        sum: [u32; 3],
    }
    let shift = 8 - BUCKET_BITS;
    // Buckets in first-seen order, so equal counts keep the order JavaScript's Map and its
    // stable sort give them.
    let mut buckets: Vec<Bucket> = Vec::new();
    let mut index: HashMap<u32, usize> = HashMap::new();
    for px in rgba.as_chunks::<4>().0 {
        if px[3] < MIN_ALPHA {
            continue;
        }
        let [r, g, b] = [px[0], px[1], px[2]].map(u32::from);
        let key = ((r >> shift) << (BUCKET_BITS * 2)) | ((g >> shift) << BUCKET_BITS) | (b >> shift);
        let i = *index.entry(key).or_insert_with(|| {
            buckets.push(Bucket::default());
            buckets.len() - 1
        });
        let bucket = &mut buckets[i];
        bucket.n += 1;
        bucket.sum[0] += r;
        bucket.sum[1] += g;
        bucket.sum[2] += b;
    }
    buckets.sort_by_key(|b| std::cmp::Reverse(b.n));

    let mut picked: Vec<Rgb> = Vec::new();
    for bucket in buckets {
        if picked.len() >= count {
            break;
        }
        // Math.round on a positive average: halves round up.
        let candidate = bucket.sum.map(|s| (f64::from(s) / f64::from(bucket.n)).round() as u8);
        let distinct = picked.iter().all(|c| {
            (0..3)
                .map(|i| (i32::from(c[i]) - i32::from(candidate[i])).pow(2))
                .sum::<i32>()
                >= MIN_DISTANCE_SQ
        });
        if distinct {
            picked.push(candidate);
        }
    }
    picked
}

/// The image at most `SAMPLE_SIZE` pixels on its longer side, never enlarged.
fn sample(image: &DynamicImage) -> RgbaImage {
    let longest = image.width().max(image.height());
    if longest <= SAMPLE_SIZE {
        return image.to_rgba8();
    }
    let scale = f64::from(SAMPLE_SIZE) / f64::from(longest);
    let side = |n: u32| ((f64::from(n) * scale).round() as u32).max(1);
    image
        .resize_exact(side(image.width()), side(image.height()), FilterType::Triangle)
        .to_rgba8()
}

fn hex([r, g, b]: Rgb) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn rgb([r, g, b]: Rgb) -> String {
    format!("rgb({r}, {g}, {b})")
}

/// CSS `hsl()` with whole degrees and percentages.
fn hsl(color: Rgb) -> String {
    let [r, g, b] = color.map(|c| f64::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    let (hue, saturation) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let hue = if max == r {
            ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        (hue * 60.0, saturation)
    };
    format!(
        "hsl({}, {}%, {}%)",
        hue.round() as u32 % 360,
        (saturation * 100.0).round(),
        (lightness * 100.0).round()
    )
}

fn css(colors: &[Rgb]) -> String {
    let lines: String = colors
        .iter()
        .enumerate()
        .map(|(i, c)| format!("  --palette-{}: {};\n", i + 1, hex(*c)))
        .collect();
    format!(":root {{\n{lines}}}\n")
}

/// The website's export: `{"name": "Extracted Palette", "colors": [...]}`, 2-space indent.
fn json(colors: &[Rgb]) -> String {
    #[derive(Serialize)]
    struct Palette {
        name: &'static str,
        colors: Vec<String>,
    }
    let palette = Palette {
        name: "Extracted Palette",
        colors: colors.iter().map(|c| hex(*c)).collect(),
    };
    serde_json::to_string_pretty(&palette).unwrap_or_default()
}

// ---- State and wiring ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    /// Bumped for every image opened, so a slower earlier decode never replaces a later one.
    generation: u64,
    worker: Option<JoinHandle<()>>,
    sample: Option<RgbaImage>,
    colors: Vec<Rgb>,
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
    let g = app.global::<PaletteExtractor>();
    g.set_formats_hint(image::FORMATS_HINT.into());
    let weak = app.as_weak();
    g.on_open_image(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose an Image", Some(("Image", INPUT_EXTENSIONS))) {
            open_image(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            s.generation += 1;
            s.sample = None;
        });
        let g = app.global::<PaletteExtractor>();
        g.set_has_image(false);
        g.set_preview(Image::default());
        g.set_file_label("".into());
        g.set_busy(false);
        g.set_error("".into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_copy_swatch(move |i, format| {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let Some(color) = with(|s| s.colors.get(i as usize).copied()) else {
            return false;
        };
        let text = match format {
            SwatchFormat::Hex => hex(color),
            SwatchFormat::Rgb => rgb(color),
            SwatchFormat::Hsl => hsl(color),
        };
        copy(&app, &text)
    });
    let weak = app.as_weak();
    g.on_copy_export(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let text = export_text(&app);
        copy(&app, &text)
    });
    let weak = app.as_weak();
    g.on_save_export(move || {
        if let Some(app) = weak.upgrade() {
            save_export(&app);
        }
    });
    refresh(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<PaletteExtractor>().set_drop_hover(hover);
}

/// A dropped image is extracted; the palette comes from one image at a time.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) => open_image(app, path),
        Err(paths) if paths.is_empty() => {}
        Err(_) => app
            .global::<PaletteExtractor>()
            .set_error("Drop one image at a time.".into()),
    }
}

/// Hands back the decode thread, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.worker.take().into_iter().collect())
}

fn copy(app: &AppWindow, text: &str) -> bool {
    match clipboard::copy_text(text) {
        Ok(()) => true,
        Err(e) => {
            app.global::<PaletteExtractor>()
                .set_error(format!("Couldn't copy: {e}").into());
            false
        }
    }
}

fn open_image(app: &AppWindow, path: PathBuf) {
    let g = app.global::<PaletteExtractor>();
    let name = file_name(&path);
    let generation = with(|s| {
        s.generation += 1;
        s.generation
    });
    g.set_busy(true);
    g.set_error("".into());

    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let result = image::open(&path).map(|decoded| {
            let label = format!("{name} · {}", decoded.source.summary());
            let shown = image::pixel_buffer(&image::preview(&decoded.image, PREVIEW_SIDE));
            (label, shown, sample(&decoded.image))
        });
        jobs::post(&weak, move |app| {
            if with(|s| s.generation) != generation {
                return;
            }
            let g = app.global::<PaletteExtractor>();
            g.set_busy(false);
            match result {
                Ok((label, shown, sample)) => {
                    with(|s| s.sample = Some(sample));
                    g.set_preview(Image::from_rgba8(shown));
                    g.set_has_image(true);
                    g.set_file_label(label.into());
                }
                Err(e) => g.set_error(format!("Couldn't open {name}: {e}").into()),
            }
            refresh(&app);
        });
    });
    with(|s| s.worker = Some(worker));
}

fn export_text(app: &AppWindow) -> String {
    let format = app.global::<PaletteExtractor>().get_export_format();
    with(|s| match format {
        PaletteExport::Css => css(&s.colors),
        PaletteExport::Json => json(&s.colors),
    })
}

fn save_export(app: &AppWindow) {
    let g = app.global::<PaletteExtractor>();
    let suggested = match g.get_export_format() {
        PaletteExport::Css => "palette.css",
        PaletteExport::Json => "palette.json",
    };
    let text = export_text(app);
    let Some(path) = files::choose_save_path("Save Palette", suggested) else {
        return;
    };
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        std::fs::write(output.path(), &text)?;
        output.keep()
    });
    let name = file_name(&path);
    match result {
        Ok(_) => g.set_note(format!("Saved as {name}").into()),
        Err(e) => g.set_error(format!("Couldn't save {name}: {e}").into()),
    }
}

/// Recounts the palette from the sample and shows it.
fn refresh(app: &AppWindow) {
    let g = app.global::<PaletteExtractor>();
    // The field keeps the count in range; at least one colour whatever it holds.
    let count = g.get_count().round().max(1.0) as usize;
    let colors = with(|s| {
        s.colors = s
            .sample
            .as_ref()
            .map(|sample| quantize(sample.as_raw(), count))
            .unwrap_or_default();
        s.colors.clone()
    });
    let empty_sample = with(|s| s.sample.is_some()) && colors.is_empty();
    if empty_sample {
        g.set_error("The image has no visible pixels to sample.".into());
    }
    let swatches: Vec<PaletteSwatch> = colors
        .iter()
        .map(|c| PaletteSwatch {
            color: slint::Color::from_rgb_u8(c[0], c[1], c[2]),
            hex: hex(*c).into(),
            detail: format!("{} · {}", rgb(*c), hsl(*c)).into(),
        })
        .collect();
    g.set_swatches(ModelRc::new(VecModel::from(swatches)));
    g.set_export_text(if colors.is_empty() {
        "".into()
    } else {
        export_text(app).into()
    });
    g.set_note("".into());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` opaque pixels of `color`, as straight RGBA bytes.
    fn pixels(color: Rgb, n: usize) -> Vec<u8> {
        [color[0], color[1], color[2], 255].repeat(n)
    }

    #[test]
    fn colours_come_most_common_first_skipping_near_duplicates() {
        let fixture = [
            pixels([255, 0, 0], 60),
            pixels([0, 0, 255], 25),
            pixels([0, 255, 0], 10),
            // Its own bucket, but within 40 of red: the same swatch to the eye.
            pixels([240, 20, 20], 5),
        ]
        .concat();

        let colors: Vec<String> = quantize(&fixture, 10).into_iter().map(hex).collect();

        assert_eq!(colors, ["#FF0000", "#0000FF", "#00FF00"]);
    }

    #[test]
    fn a_bucket_is_its_rounded_average() {
        // 0 and 15 share the 4-bit bucket 0; their average 7.5 rounds up, as Math.round does.
        let fixture = [pixels([0, 0, 0], 1), pixels([15, 15, 15], 1)].concat();

        assert_eq!(quantize(&fixture, 10), [[8, 8, 8]]);
    }

    #[test]
    fn mostly_transparent_pixels_are_ignored() {
        let fixture = [[200, 100, 50, 127].repeat(9), pixels([10, 10, 10], 1)].concat();

        assert_eq!(quantize(&fixture, 10), [[10, 10, 10]]);
    }

    #[test]
    fn the_count_caps_the_palette() {
        let fixture = [pixels([255, 0, 0], 3), pixels([0, 0, 255], 2), pixels([0, 255, 0], 1)].concat();

        assert_eq!(quantize(&fixture, 2), [[255, 0, 0], [0, 0, 255]]);
    }

    #[test]
    fn equal_counts_keep_first_seen_order() {
        let fixture = [pixels([0, 0, 255], 2), pixels([255, 0, 0], 2)].concat();

        assert_eq!(quantize(&fixture, 10), [[0, 0, 255], [255, 0, 0]]);
    }

    #[test]
    fn the_sample_is_at_most_96_pixels_long() {
        let wide = DynamicImage::ImageRgba8(RgbaImage::new(1000, 250));

        let small = sample(&wide);

        assert_eq!(small.dimensions(), (96, 24));
    }

    #[test]
    fn formats_read_as_css() {
        assert_eq!(hex([51, 102, 153]), "#336699");
        assert_eq!(rgb([51, 102, 153]), "rgb(51, 102, 153)");
        assert_eq!(hsl([51, 102, 153]), "hsl(210, 50%, 40%)");
    }

    #[test]
    fn hsl_of_grey_and_pure_hues() {
        assert_eq!(hsl([128, 128, 128]), "hsl(0, 0%, 50%)");
        assert_eq!(hsl([255, 0, 0]), "hsl(0, 100%, 50%)");
        assert_eq!(hsl([255, 0, 255]), "hsl(300, 100%, 50%)");
    }

    #[test]
    fn css_export_numbers_the_variables() {
        assert_eq!(
            css(&[[255, 0, 0], [0, 0, 255]]),
            ":root {\n  --palette-1: #FF0000;\n  --palette-2: #0000FF;\n}\n"
        );
    }

    #[test]
    fn json_export_matches_the_website() {
        assert_eq!(
            json(&[[255, 0, 0]]),
            "{\n  \"name\": \"Extracted Palette\",\n  \"colors\": [\n    \"#FF0000\"\n  ]\n}"
        );
    }

}
