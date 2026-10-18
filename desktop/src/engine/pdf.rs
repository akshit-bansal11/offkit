//! PDF family (pdf-toolkit, pdf-converter, pdf-merger, pdf-splitter, pdf-compressor,
//! convert-to-pdf, convert-from-pdf): one engine on `pdfium-render`, which every tool is a thin
//! UI over. The website's PDF tools each carried their own copy of these operations; here they
//! exist once.
//!
//! - `pdfium`: the library, located and bound once (`scripts/fetch-pdfium.sh` fetches it).
//! - `open`: the only way a tool opens a PDF. It refuses anything that isn't a regular local file.
//! - `parse_ranges`: "1-3, 5" as typed, in order.
//! - `assemble`: builds a new PDF from pages of one or more PDFs, in any order, each turned by
//!   quarter turns. Merge, split, extract, delete, rotate and reorder are all calls to it.
//! - `images_to_pdf`, `render_to_folder` and `thumbnails`: images in, pages out, and back.
//! - `compress`: re-encodes embedded images with `lopdf` (text and vectors stay), or rasterises
//!   every page, and never keeps an output that isn't smaller than its input.
//!
//! Pdfium isn't thread-safe; pdfium-render's `thread_safe` feature (on by default) serialises
//! every call, so workers may share the one `Pdfium`. PDFs are untrusted: rendering is capped at
//! `MAX_RENDER_PIXELS` and runs on workers, never on the UI thread.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufWriter, Cursor, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ::image::codecs::jpeg::JpegEncoder;
use ::image::codecs::png::PngEncoder;
use ::image::codecs::webp::WebPEncoder;
use ::image::imageops::FilterType;
use ::image::{
    DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbImage, RgbaImage,
};
use lopdf::{LoadOptions, Object, ObjectId, SaveOptions};
use pdfium_render::prelude::*;

use crate::files::file_name;
use crate::jobs::{CancelToken, JobError};

/// Where the PDFium builds the fetch script uses come from, for the missing-library screen.
pub const PDFIUM_DOWNLOADS: &str = "https://github.com/bblanchon/pdfium-binaries/releases";
/// The largest page render, in pixels: A4 or Letter at 600 dpi fits. Pages asking for more are
/// rendered smaller, so a huge or hostile page can't exhaust memory (about 144 MB per render).
const MAX_RENDER_PIXELS: f32 = 36_000_000.0;
/// The longest a rendered page may be on either side, in pixels: a 1 x 14400 pt strip at 600 dpi
/// is under the pixel cap yet 120,000 px tall.
const MAX_RENDER_EDGE: f32 = 20_000.0;
/// The most pages that get thumbnails; the rest of a huge PDF still splits and merges, unpictured.
pub const MAX_THUMBNAIL_PAGES: usize = 500;
/// The most one PDF stream may decompress to while compressing: a bomb is refused, not inflated.
const MAX_STREAM_BYTES: usize = 256 * 1024 * 1024;
/// Decoding limits for images going into a PDF and images inside one.
const MAX_IMAGE_EDGE: u32 = 20_000;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
/// JPEG quality for images that must be re-encoded on their way into a PDF (a phone photo
/// whose EXIF orientation has to be applied): close to the source, unlike the compressor's.
const REENCODE_QUALITY: u8 = 92;
const POINTS_PER_INCH: f32 = 72.0;

// ---- The library ----------------------------------------------------------------------------

/// The PDFium every tool shares, bound once; `None` when it isn't there or won't load.
pub fn pdfium() -> Option<&'static Pdfium> {
    static SHARED: OnceLock<Option<Pdfium>> = OnceLock::new();
    SHARED
        .get_or_init(|| {
            let path = find_library(&library_dirs())?;
            match Pdfium::bind_to_library(&path) {
                Ok(bindings) => Some(Pdfium::new(bindings)),
                Err(e) => {
                    eprintln!("couldn't load {}: {e}", path.display());
                    None
                }
            }
        })
        .as_ref()
}

/// Next to the executable, then where the installer puts the app's resources (macOS: the
/// bundle's `Contents/Resources`; deb and AppImage: `usr/lib/offkit`; the Windows installer
/// uses the executable's own folder), then `desktop/vendor/<os>/pdfium/` in development builds,
/// where `scripts/fetch-pdfium.sh` puts it. Never the system's library path: the app binds the
/// build it was tested with.
fn library_dirs() -> Vec<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    // The exe sits in `Contents/MacOS` or `usr/bin`, so the resources are a sibling folder.
    let bundle = exe_dir.as_ref().and_then(|dir| {
        if cfg!(target_os = "macos") {
            Some(dir.join("../Resources"))
        } else if cfg!(target_os = "linux") {
            Some(dir.join("../lib/offkit"))
        } else {
            None
        }
    });
    let vendor = cfg!(debug_assertions).then(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor")
            .join(std::env::consts::OS)
            .join("pdfium")
    });
    exe_dir.into_iter().chain(bundle).chain(vendor).collect()
}

fn find_library(dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .map(Pdfium::pdfium_platform_library_name_at_path)
        .find(|p| p.is_file())
}

// ---- Opening --------------------------------------------------------------------------------

/// Opens a PDF the user chose. Only a regular file is handed to pdfium, read through our own
/// file handle, so it never sees a device, a pipe or anything but that file. The error is the
/// sentence a row shows.
pub fn open<'a>(pdfium: &'a Pdfium, path: &Path) -> Result<PdfDocument<'a>, String> {
    let name = file_name(path);
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Err(format!("{name} isn't a file.")),
        Err(e) => return Err(format!("Couldn't read {name}: {e}")),
    }
    pdfium.load_pdf_from_file(path, None).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => format!(
            "{name} is password-protected. Save a copy without the password in a PDF viewer, \
             then add that."
        ),
        PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::FormatError | PdfiumInternalError::FileError,
        ) => format!("{name} isn't a PDF, or it's damaged."),
        PdfiumError::IoError(e) => format!("Couldn't read {name}: {e}"),
        e => format!("Couldn't open {name}: {e}"),
    })
}

pub fn page_count(pdfium: &Pdfium, path: &Path) -> Result<u32, String> {
    let doc = open(pdfium, path)?;
    Ok(doc.pages().len().max(0) as u32)
}

fn save(doc: &PdfDocument, out: &Path) -> Result<(), JobError> {
    doc.save_to_file(out)
        .map_err(|e| JobError::Failed(format!("Couldn't save the PDF: {e}")))
}

// ---- Page ranges ----------------------------------------------------------------------------

/// Reads page ranges as typed: "1-3, 5", "8-" (to the end), "-4" (from the start), in the
/// order written, repeats kept ("3, 1, 3"). Pages are 1-based, as people count them. Every
/// part that doesn't fit `page_count` is named in the error.
pub fn parse_ranges(text: &str, page_count: u32) -> Result<Vec<RangeInclusive<u32>>, String> {
    let parts: Vec<&str> = text
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("Type the pages to use, like 1-3, 5.".into());
    }
    let mut ranges = Vec::new();
    let mut problems = Vec::new();
    for part in parts {
        match parse_part(part, page_count) {
            Ok(range) => ranges.push(range),
            Err(problem) => problems.push(problem),
        }
    }
    if problems.is_empty() {
        Ok(ranges)
    } else {
        Err(problems.join(" "))
    }
}

fn parse_part(part: &str, page_count: u32) -> Result<RangeInclusive<u32>, String> {
    // People paste en and em dashes from documents.
    let normal = part.replace(['–', '—'], "-");
    let page = |s: &str, default: u32| -> Result<u32, String> {
        let s = s.trim();
        if s.is_empty() {
            return Ok(default);
        }
        let n: u32 = s
            .parse()
            .map_err(|_| format!("“{part}” isn't a page number or range."))?;
        if n == 0 {
            return Err(format!("“{part}” isn't a page: pages start at 1."));
        }
        if n > page_count {
            return Err(format!("“{part}” is past the last page, {page_count}."));
        }
        Ok(n)
    };
    match normal.split_once('-') {
        None => page(&normal, 0).map(|n| n..=n),
        Some((a, b)) => {
            let (start, end) = (page(a, 1)?, page(b, page_count)?);
            if start > end {
                return Err(format!("“{part}” is backwards."));
            }
            Ok(start..=end)
        }
    }
}

/// The pages of `ranges` as 0-based indices, in order, repeats kept.
pub fn flatten(ranges: &[RangeInclusive<u32>]) -> Vec<u32> {
    ranges
        .iter()
        .flat_map(|r| r.clone().map(|p| p - 1))
        .collect()
}

/// `pages` (0-based) as the shortest range text, sorted: [0, 1, 2, 4] is "1-3, 5".
pub fn format_pages(pages: &[u32]) -> String {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for p in sorted {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == p => *end = p,
            _ => runs.push((p, p)),
        }
    }
    runs.iter()
        .map(|&(a, b)| {
            if a == b {
                format!("{}", a + 1)
            } else {
                format!("{}-{}", a + 1, b + 1)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- Assembling -----------------------------------------------------------------------------

/// One page of an assembled PDF: page `index` (0-based) of `sources[source]`, turned clockwise
/// by `turns` quarter turns on top of its own rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageRef {
    pub source: usize,
    pub index: u32,
    pub turns: u8,
}

impl PageRef {
    pub fn new(source: usize, index: u32) -> Self {
        Self {
            source,
            index,
            turns: 0,
        }
    }
}

/// Every page of a PDF with `count` pages, in order.
pub fn all_pages(source: usize, count: u32) -> Vec<PageRef> {
    (0..count).map(|i| PageRef::new(source, i)).collect()
}

/// Writes a new PDF to `out` made of `pages`. Consecutive pages from one source are imported in
/// one call, so the fonts and images they share are copied once, not once per page.
pub fn assemble(
    pdfium: &Pdfium,
    sources: &[PathBuf],
    pages: &[PageRef],
    out: &Path,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<(), JobError> {
    assemble_each(
        pdfium,
        sources,
        &[(out.to_path_buf(), pages.to_vec())],
        cancel,
        progress,
    )
}

/// `assemble` for many outputs from the same sources (a split): writes each `(path, pages)`,
/// opening every source once for all of them.
pub fn assemble_each(
    pdfium: &Pdfium,
    sources: &[PathBuf],
    outputs: &[(PathBuf, Vec<PageRef>)],
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<(), JobError> {
    let mut docs: Vec<Option<PdfDocument>> = sources.iter().map(|_| None).collect();
    let total: usize = outputs.iter().map(|(_, pages)| pages.len()).sum();
    let mut done = 0;
    for (out, pages) in outputs {
        let dest = build(pdfium, sources, &mut docs, pages, cancel)?;
        save(&dest, out)?;
        done += pages.len();
        progress(done as f32 / total.max(1) as f32);
    }
    Ok(())
}

fn build<'a>(
    pdfium: &'a Pdfium,
    sources: &[PathBuf],
    docs: &mut [Option<PdfDocument<'a>>],
    pages: &[PageRef],
    cancel: &CancelToken,
) -> Result<PdfDocument<'a>, JobError> {
    let mut dest = pdfium
        .create_new_pdf()
        .map_err(|e| JobError::Failed(format!("Couldn't create the PDF: {e}")))?;
    for run in pages.chunk_by(|a, b| a.source == b.source) {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let source = run[0].source;
        let slot = docs
            .get_mut(source)
            .ok_or_else(|| JobError::Failed("A page refers to a file that isn't listed.".into()))?;
        if slot.is_none() {
            *slot = Some(open(pdfium, &sources[source]).map_err(JobError::Failed)?);
        }
        let Some(doc) = slot.as_ref() else {
            continue;
        };
        let wanted: Vec<u32> = run.iter().map(|p| p.index).collect();
        let at = dest.pages().len();
        dest.pages_mut()
            .copy_pages_from_document(doc, &import_list(&wanted), at)
            .map_err(|e| {
                JobError::Failed(format!(
                    "Couldn't copy pages from {}: {e}",
                    file_name(&sources[source])
                ))
            })?;
    }
    for (i, page) in pages.iter().enumerate() {
        if page.turns % 4 == 0 {
            continue;
        }
        let mut target = dest
            .pages()
            .get(i as PdfPageIndex)
            .map_err(|e| JobError::Failed(format!("Couldn't rotate page {}: {e}", i + 1)))?;
        let current = target.rotation().map(quarter_turns).unwrap_or(0);
        target.set_rotation(rotation(current + page.turns));
    }
    Ok(dest)
}

/// Pdfium's import list: 1-based, in order, with runs of consecutive pages shortened
/// ("3,1-2,9"), so a thousand-page merge isn't a thousand numbers.
fn import_list(indices: &[u32]) -> String {
    indices
        .chunk_by(|a, b| a + 1 == *b)
        .map(|run| match run {
            [one] => format!("{}", one + 1),
            _ => format!("{}-{}", run[0] + 1, run[run.len() - 1] + 1),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn quarter_turns(rotation: PdfPageRenderRotation) -> u8 {
    match rotation {
        PdfPageRenderRotation::None => 0,
        PdfPageRenderRotation::Degrees90 => 1,
        PdfPageRenderRotation::Degrees180 => 2,
        PdfPageRenderRotation::Degrees270 => 3,
    }
}

fn rotation(turns: u8) -> PdfPageRenderRotation {
    match turns % 4 {
        1 => PdfPageRenderRotation::Degrees90,
        2 => PdfPageRenderRotation::Degrees180,
        3 => PdfPageRenderRotation::Degrees270,
        _ => PdfPageRenderRotation::None,
    }
}

// ---- Images to PDF --------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageSize {
    /// The page is the image's size, one pixel to a point, plus the margins.
    Fit,
    A4,
    Letter,
}

impl PageSize {
    pub const ALL: [PageSize; 3] = [PageSize::Fit, PageSize::A4, PageSize::Letter];

    pub fn label(self) -> &'static str {
        match self {
            PageSize::Fit => "Fit to image",
            PageSize::A4 => "A4",
            PageSize::Letter => "US Letter",
        }
    }

    /// Portrait width and height in points; `None` for `Fit`.
    fn points(self) -> Option<(f32, f32)> {
        match self {
            PageSize::Fit => None,
            PageSize::A4 => Some((595.28, 841.89)),
            PageSize::Letter => Some((612.0, 792.0)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    /// Landscape pages for landscape images.
    Auto,
    Portrait,
    Landscape,
}

impl Orientation {
    pub const ALL: [Orientation; 3] = [
        Orientation::Auto,
        Orientation::Portrait,
        Orientation::Landscape,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Orientation::Auto => "Match image",
            Orientation::Portrait => "Portrait",
            Orientation::Landscape => "Landscape",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub size: PageSize,
    pub orientation: Orientation,
    /// Space around the image on every side, in points.
    pub margin: f32,
}

/// Where an image of `width`×`height` pixels sits: the page size and the image's rectangle on
/// it (x, y, w, h), in points. The image keeps its proportions, centred, never cropped.
fn place(layout: Layout, width: u32, height: u32) -> ((f32, f32), (f32, f32, f32, f32)) {
    let (iw, ih) = (width as f32, height as f32);
    let m = layout.margin;
    let Some((short, long)) = layout.size.points() else {
        return ((iw + 2.0 * m, ih + 2.0 * m), (m, m, iw, ih));
    };
    let landscape = match layout.orientation {
        Orientation::Auto => iw > ih,
        Orientation::Portrait => false,
        Orientation::Landscape => true,
    };
    let (pw, ph) = if landscape {
        (long, short)
    } else {
        (short, long)
    };
    let scale = ((pw - 2.0 * m) / iw).min((ph - 2.0 * m) / ih).max(0.0);
    let (w, h) = (iw * scale, ih * scale);
    ((pw, ph), ((pw - w) / 2.0, (ph - h) / 2.0, w, h))
}

/// Decoding limits for an untrusted image, so a small file can't claim gigabytes of pixels.
fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_EDGE);
    limits.max_image_height = Some(MAX_IMAGE_EDGE);
    limits.max_alloc = Some(MAX_IMAGE_BYTES);
    limits
}

/// Decodes an image file the user chose, applying its EXIF orientation.
pub fn decode_image(path: &Path) -> Result<DynamicImage, String> {
    let name = file_name(path);
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|e| format!("Couldn't read {name}: {e}"))?;
    reader.limits(limits());
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("{name} isn't an image this app can read: {e}"))?;
    let orientation = decoder
        .orientation()
        .unwrap_or(::image::metadata::Orientation::NoTransforms);
    let mut image =
        DynamicImage::from_decoder(decoder).map_err(|e| format!("Couldn't decode {name}: {e}"))?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// A JPEG that can go into the PDF byte for byte: its bytes and size, unless its EXIF says to
/// turn it (a PDF ignores EXIF, so that one is decoded and re-encoded upright instead).
fn passthrough_jpeg(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = fs::read(path).ok()?;
    if ::image::guess_format(&bytes).ok()? != ImageFormat::Jpeg {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Jpeg);
    reader.limits(limits());
    let (upright, (w, h)) = {
        let mut decoder = reader.into_decoder().ok()?;
        let upright = matches!(
            decoder.orientation(),
            Ok(::image::metadata::Orientation::NoTransforms)
        );
        (upright, decoder.dimensions())
    };
    upright.then_some((bytes, w, h))
}

fn encode_jpeg(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut jpeg = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut jpeg, quality.clamp(1, 100));
    match image {
        DynamicImage::ImageLuma8(gray) => gray.write_with_encoder(encoder),
        other => other.to_rgb8().write_with_encoder(encoder),
    }
    .map_err(|e| format!("Couldn't encode an image: {e}"))?;
    Ok(jpeg)
}

/// Writes one PDF with a page per image, in order. JPEGs go in untouched (`/DCTDecode`, no
/// generation loss) unless EXIF turns them; other formats are stored losslessly.
pub fn images_to_pdf(
    pdfium: &Pdfium,
    images: &[PathBuf],
    layout: Layout,
    out: &Path,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<(), JobError> {
    let mut doc = pdfium
        .create_new_pdf()
        .map_err(|e| JobError::Failed(format!("Couldn't create the PDF: {e}")))?;
    for (i, path) in images.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let name = file_name(path);
        let failed = |e: PdfiumError| JobError::Failed(format!("Couldn't add {name}: {e}"));
        let (object, width, height) = match passthrough_jpeg(path) {
            Some((bytes, w, h)) => (
                PdfPageImageObject::new_from_jpeg_reader(&doc, Cursor::new(bytes))
                    .map_err(failed)?,
                w,
                h,
            ),
            None => {
                let image = decode_image(path).map_err(JobError::Failed)?;
                let object = match fs::read(path)
                    .ok()
                    .and_then(|b| ::image::guess_format(&b).ok())
                {
                    // A JPEG that had to be turned upright: still a photo, so still a JPEG.
                    Some(ImageFormat::Jpeg) => {
                        let jpeg =
                            encode_jpeg(&image, REENCODE_QUALITY).map_err(JobError::Failed)?;
                        PdfPageImageObject::new_from_jpeg_reader(&doc, Cursor::new(jpeg))
                    }
                    // Screenshots, drawings, transparency: stored losslessly.
                    _ => PdfPageImageObject::new(&doc, &image),
                }
                .map_err(failed)?;
                (object, image.width(), image.height())
            }
        };
        let ((pw, ph), (x, y, w, h)) = place(layout, width, height);
        let mut object = object;
        object.scale(w, h).map_err(failed)?;
        object
            .translate(PdfPoints::new(x), PdfPoints::new(y))
            .map_err(failed)?;
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(pw),
                PdfPoints::new(ph),
            ))
            .map_err(failed)?;
        page.objects_mut()
            .add_image_object(object)
            .map_err(failed)?;
        progress((i + 1) as f32 / images.len() as f32);
    }
    save(&doc, out)
}

// ---- Rendering ------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Raster {
    Png,
    /// Quality 1-100.
    Jpeg(u8),
    /// Lossless: the `image` crate has no lossy WebP encoder.
    Webp,
}

impl Raster {
    pub fn extension(self) -> &'static str {
        match self {
            Raster::Png => "png",
            Raster::Jpeg(_) => "jpg",
            Raster::Webp => "webp",
        }
    }
}

/// The render scale for a page of `width`×`height` points at `dpi`, lowered so the bitmap stays
/// under `MAX_RENDER_PIXELS` and `MAX_RENDER_EDGE`. A size or scale that isn't a positive
/// number is an error: it would cap nothing.
fn render_scale(width: f32, height: f32, dpi: f32) -> Result<f32, String> {
    let unreadable = || "This page's size can't be read.".to_string();
    let scale = dpi / POINTS_PER_INCH;
    if ![width, height, scale]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
    {
        return Err(unreadable());
    }
    let pixels = width * scale * height * scale;
    let by_area = if pixels > MAX_RENDER_PIXELS {
        (MAX_RENDER_PIXELS / pixels).sqrt()
    } else {
        1.0
    };
    let by_edge = MAX_RENDER_EDGE / (width.max(height) * scale);
    let capped = scale * by_area.min(by_edge).min(1.0);
    if capped.is_finite() && capped > 0.0 {
        Ok(capped)
    } else {
        Err(unreadable())
    }
}

/// A page at `dpi` (pixels = points / 72 × dpi), on white, within `MAX_RENDER_PIXELS`.
fn render(page: &PdfPage, dpi: f32) -> Result<RgbImage, String> {
    let scale = render_scale(page.width().value, page.height().value, dpi)?;
    let config = PdfRenderConfig::new().scale_page_by_factor(scale);
    page.render_with_config(&config)
        .and_then(|bitmap| bitmap.as_image())
        .map(DynamicImage::into_rgb8)
        .map_err(|e| format!("Couldn't render a page: {e}"))
}

fn write_raster(image: &RgbImage, format: Raster, path: &Path) -> Result<(), String> {
    let file =
        File::create_new(path).map_err(|e| format!("Couldn't create {}: {e}", file_name(path)))?;
    let mut w = BufWriter::new(file);
    match format {
        Raster::Png => image.write_with_encoder(PngEncoder::new(&mut w)),
        Raster::Jpeg(q) => {
            image.write_with_encoder(JpegEncoder::new_with_quality(&mut w, q.clamp(1, 100)))
        }
        Raster::Webp => image.write_with_encoder(WebPEncoder::new_lossless(&mut w)),
    }
    .map_err(|e| format!("Couldn't write {}: {e}", file_name(path)))?;
    w.flush()
        .map_err(|e| format!("Couldn't write {}: {e}", file_name(path)))
}

/// "Page 07.png": numbered as people count, padded to the widest number so files sort in order.
pub fn page_file_name(page: u32, page_count: u32, ext: &str) -> String {
    let width = page_count.max(1).to_string().len();
    format!("Page {:0width$}.{ext}", page + 1)
}

/// How `render_to_folder` draws each page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageImages {
    pub dpi: f32,
    pub format: Raster,
}

/// Renders `pages` (0-based; repeats are skipped) of `input` into `dir`, one file per page.
/// Returns how many it wrote.
pub fn render_to_folder(
    pdfium: &Pdfium,
    input: &Path,
    pages: &[u32],
    images: PageImages,
    dir: &Path,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<usize, JobError> {
    let PageImages { dpi, format } = images;
    let doc = open(pdfium, input).map_err(JobError::Failed)?;
    let count = doc.pages().len().max(0) as u32;
    let mut seen = HashSet::new();
    let unique: Vec<u32> = pages.iter().copied().filter(|p| seen.insert(*p)).collect();
    for (n, &index) in unique.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let page = doc
            .pages()
            .get(index as PdfPageIndex)
            .map_err(|e| JobError::Failed(format!("Couldn't read page {}: {e}", index + 1)))?;
        let image = render(&page, dpi).map_err(JobError::Failed)?;
        let name = page_file_name(index, count, format.extension());
        write_raster(&image, format, &dir.join(name)).map_err(JobError::Failed)?;
        progress((n + 1) as f32 / unique.len() as f32);
    }
    Ok(unique.len())
}

/// Renders the first `MAX_THUMBNAIL_PAGES` pages of `path` to fit a `max_edge` square, in
/// order, handing each to `each` as it's done (page index, image). Returns how many pages were
/// left without one. Stops quietly when cancelled.
pub fn thumbnails(
    pdfium: &Pdfium,
    path: &Path,
    max_edge: u32,
    cancel: &CancelToken,
    mut each: impl FnMut(u32, RgbaImage),
) -> Result<usize, String> {
    let doc = open(pdfium, path)?;
    let edge = max_edge as Pixels;
    let config = PdfRenderConfig::new()
        .set_target_width(edge)
        .set_maximum_height(edge);
    let total = doc.pages().len().max(0) as usize;
    for (i, page) in doc.pages().iter().enumerate().take(MAX_THUMBNAIL_PAGES) {
        if cancel.is_cancelled() {
            return Ok(0);
        }
        let image = page
            .render_with_config(&config)
            .and_then(|bitmap| bitmap.as_image())
            .map_err(|e| format!("Couldn't render page {}: {e}", i + 1))?;
        each(i as u32, image.into_rgba8());
    }
    Ok(total.saturating_sub(MAX_THUMBNAIL_PAGES))
}

// ---- Compression ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompressMode {
    /// Re-encode embedded photos as JPEG at `dpi`; text and vector drawings stay as they are.
    Images,
    /// Every page becomes one JPEG. Always shrinks scans; text can't be selected afterwards.
    Rasterise,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressSettings {
    pub mode: CompressMode,
    /// Images are downscaled to at most this resolution on the largest page.
    pub dpi: f32,
    /// JPEG quality, 1-100.
    pub quality: u8,
}

#[derive(Debug, PartialEq)]
pub enum Compressed {
    Smaller {
        before: u64,
        after: u64,
    },
    /// The output wasn't smaller than the input; the caller keeps nothing.
    NoGain,
}

/// Writes a compressed copy of `input` to `out`, checks pdfium can read it back with every
/// page, and reports whether it's smaller. On `NoGain` the caller must discard `out`: an output
/// at least as large as its input is never worth keeping.
pub fn compress(
    pdfium: &Pdfium,
    input: &Path,
    out: &Path,
    settings: CompressSettings,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<Compressed, JobError> {
    let pages = page_count(pdfium, input).map_err(JobError::Failed)?;
    match settings.mode {
        CompressMode::Images => recompress_images(pdfium, input, out, settings, cancel, progress)?,
        CompressMode::Rasterise => rasterise(pdfium, input, out, settings, cancel, progress)?,
    }
    let name = file_name(input);
    match page_count(pdfium, out) {
        Ok(n) if n == pages => {}
        _ => {
            return Err(JobError::Failed(format!(
                "Compressing {name} didn't produce a readable PDF, so nothing was saved."
            )));
        }
    }
    let size = |p: &Path| fs::metadata(p).map(|m| m.len());
    let (before, after) = size(input)
        .and_then(|b| size(out).map(|a| (b, a)))
        .map_err(|e| JobError::Failed(format!("Couldn't read {name}: {e}")))?;
    Ok(if after < before {
        Compressed::Smaller { before, after }
    } else {
        Compressed::NoGain
    })
}

/// The largest page edge in pixels at `dpi`: no image needs more than that.
fn image_cap(pdfium: &Pdfium, input: &Path, dpi: f32) -> Result<u32, JobError> {
    let doc = open(pdfium, input).map_err(JobError::Failed)?;
    let longest = doc
        .pages()
        .page_sizes()
        .map_err(|e| JobError::Failed(format!("Couldn't read {}: {e}", file_name(input))))?
        .iter()
        .map(|r| r.width().value.max(r.height().value))
        .fold(0.0, f32::max);
    Ok((longest / POINTS_PER_INCH * dpi).ceil().max(1.0) as u32)
}

/// Re-encodes every embedded 8-bit RGB or grey image (plain, Flate or JPEG) as a JPEG of at
/// most `image_cap` pixels on its long side, keeping each one only where it's smaller. Masks,
/// CMYK, indexed and other exotic images are left alone.
///
/// ponytail: caps every image at the largest page's size, without reading where it's drawn;
/// an image shown at a quarter page keeps up to twice the pixels it needs. Parse the content
/// streams' `cm` operators to size each image if that leaves too much on the table.
fn recompress_images(
    pdfium: &Pdfium,
    input: &Path,
    out: &Path,
    settings: CompressSettings,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<(), JobError> {
    let name = file_name(input);
    let cap = image_cap(pdfium, input, settings.dpi)?;
    let options = LoadOptions {
        max_decompressed_size: Some(MAX_STREAM_BYTES),
        ..Default::default()
    };
    let mut doc = lopdf::Document::load_with_options(input, options)
        .map_err(|e| JobError::Failed(format!("Couldn't read {name}: {e}")))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(JobError::Failed(format!(
            "{name} is encrypted, so its images can't be recompressed. Try Rasterise pages."
        )));
    }
    let images: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| {
            o.as_stream().is_ok_and(|s| {
                s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
            })
        })
        .map(|(id, _)| *id)
        .collect();
    for (n, id) in images.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        if let Some((jpeg, w, h)) = smaller_jpeg(&doc, *id, cap, settings.quality)
            && let Ok(stream) = doc.get_object_mut(*id).and_then(Object::as_stream_mut)
        {
            stream
                .dict
                .set("Filter", Object::Name(b"DCTDecode".to_vec()));
            stream.dict.remove(b"DecodeParms");
            stream.dict.set("Width", i64::from(w));
            stream.dict.set("Height", i64::from(h));
            stream.dict.set("BitsPerComponent", 8);
            stream.set_content(jpeg);
        }
        progress((n + 1) as f32 / images.len() as f32);
    }
    doc.prune_objects();
    doc.compress();
    let file =
        File::create(out).map_err(|e| JobError::Failed(format!("Couldn't save the PDF: {e}")))?;
    let mut w = BufWriter::new(file);
    let options = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .build();
    doc.save_with_options(&mut w, options)
        .map_err(|e| JobError::Failed(format!("Couldn't save the PDF: {e}")))?;
    w.flush()
        .map_err(|e| JobError::Failed(format!("Couldn't save the PDF: {e}")))
}

/// The image `id` re-encoded as a JPEG with its new size, if it's a kind this can re-encode and
/// the result is smaller than what's stored now.
fn smaller_jpeg(
    doc: &lopdf::Document,
    id: ObjectId,
    cap: u32,
    quality: u8,
) -> Option<(Vec<u8>, u32, u32)> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let dict = &stream.dict;
    let flag = |key: &[u8]| dict.get(key).and_then(Object::as_bool).unwrap_or(false);
    if flag(b"ImageMask")
        || dict.has(b"Mask")
        || dict.has(b"Decode")
        || dict.has(b"SMaskInData")
        || dict
            .get(b"BitsPerComponent")
            .and_then(Object::as_i64)
            .ok()?
            != 8
    {
        return None;
    }
    let channels = channels(doc, dict.get(b"ColorSpace").ok()?)?;
    let width = u32::try_from(dict.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let height = u32::try_from(dict.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    if width == 0 || height == 0 || width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE {
        return None;
    }
    let filters = stream.filters().unwrap_or_default();
    let image = match filters.as_slice() {
        [b"DCTDecode"] => {
            let mut reader =
                ImageReader::with_format(Cursor::new(&stream.content), ImageFormat::Jpeg);
            reader.limits(limits());
            let decoded = reader.decode().ok()?;
            if (decoded.width(), decoded.height()) != (width, height) {
                return None;
            }
            match channels {
                1 => DynamicImage::ImageLuma8(decoded.to_luma8()),
                _ => DynamicImage::ImageRgb8(decoded.to_rgb8()),
            }
        }
        [] | [b"FlateDecode"] => {
            let raw = stream
                .decompressed_content_with_limit(MAX_STREAM_BYTES)
                .ok()?;
            match channels {
                1 => DynamicImage::ImageLuma8(GrayImage::from_raw(width, height, raw)?),
                _ => DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, raw)?),
            }
        }
        _ => return None,
    };
    let long = width.max(height);
    let image = if long > cap {
        let scale = cap as f32 / long as f32;
        let (w, h) = (
            ((width as f32 * scale).round() as u32).max(1),
            ((height as f32 * scale).round() as u32).max(1),
        );
        image.resize_exact(w, h, FilterType::Triangle)
    } else {
        image
    };
    let jpeg = encode_jpeg(&image, quality).ok()?;
    (jpeg.len() < stream.content.len()).then(|| (jpeg, image.width(), image.height()))
}

/// 3 for RGB and 1 for grey (device or ICC-based) colour spaces; `None` for the rest.
fn channels(doc: &lopdf::Document, space: &Object) -> Option<u8> {
    let (_, space) = doc.dereference(space).ok()?;
    match space {
        Object::Name(n) if n == b"DeviceRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" => Some(1),
        Object::Array(a) if a.first().and_then(|f| f.as_name().ok()) == Some(b"ICCBased") => {
            let (_, profile) = doc.dereference(a.get(1)?).ok()?;
            match profile
                .as_stream()
                .ok()?
                .dict
                .get(b"N")
                .and_then(Object::as_i64)
                .ok()?
            {
                3 => Some(3),
                1 => Some(1),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Every page rendered at `dpi` and stored as one JPEG on a page of the original size.
fn rasterise(
    pdfium: &Pdfium,
    input: &Path,
    out: &Path,
    settings: CompressSettings,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<(), JobError> {
    let src = open(pdfium, input).map_err(JobError::Failed)?;
    let mut dest = pdfium
        .create_new_pdf()
        .map_err(|e| JobError::Failed(format!("Couldn't create the PDF: {e}")))?;
    let count = src.pages().len().max(1);
    for (i, page) in src.pages().iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let failed =
            |e: PdfiumError| JobError::Failed(format!("Couldn't rebuild page {}: {e}", i + 1));
        let (w, h) = (page.width(), page.height());
        let image = DynamicImage::ImageRgb8(render(&page, settings.dpi).map_err(JobError::Failed)?);
        let jpeg = encode_jpeg(&image, settings.quality).map_err(JobError::Failed)?;
        let mut object =
            PdfPageImageObject::new_from_jpeg_reader(&dest, Cursor::new(jpeg)).map_err(failed)?;
        object.scale(w.value, h.value).map_err(failed)?;
        let mut target = dest
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(w, h))
            .map_err(failed)?;
        target
            .objects_mut()
            .add_image_object(object)
            .map_err(failed)?;
        progress((i + 1) as f32 / count as f32);
    }
    save(&dest, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: (f32, f32) = (612.0, 792.0);

    fn lib() -> &'static Pdfium {
        pdfium().expect("PDFium missing: run desktop/scripts/fetch-pdfium.sh")
    }

    #[test]
    fn library_search_starts_with_the_exes_own_folder() {
        let exe = std::env::current_exe().unwrap();
        assert_eq!(library_dirs().first().map(PathBuf::as_path), exe.parent());
    }

    fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("offkit-pdf-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A PDF whose pages have the given sizes in points, so tests can tell pages apart by size.
    fn sized_pdf(path: &Path, sizes: &[(f32, f32)]) -> PathBuf {
        let mut doc = lib().create_new_pdf().unwrap();
        for &(w, h) in sizes {
            doc.pages_mut()
                .create_page_at_end(PdfPagePaperSize::Custom(
                    PdfPoints::new(w),
                    PdfPoints::new(h),
                ))
                .unwrap();
        }
        doc.save_to_file(path).unwrap();
        path.to_path_buf()
    }

    /// Page widths of a PDF, in points, rounded.
    fn widths(path: &Path) -> Vec<u32> {
        let doc = open(lib(), path).unwrap();
        doc.pages()
            .iter()
            .map(|p| p.width().value.round() as u32)
            .collect()
    }

    fn rotations(path: &Path) -> Vec<u8> {
        let doc = open(lib(), path).unwrap();
        doc.pages()
            .iter()
            .map(|p| quarter_turns(p.rotation().unwrap()))
            .collect()
    }

    fn noisy_image(w: u32, h: u32) -> RgbImage {
        // A smooth gradient with deterministic grain: photo-like, so JPEG wins over Flate.
        RgbImage::from_fn(w, h, |x, y| {
            let grain = ((x * 7919 + y * 104_729) % 23) as u8;
            ::image::Rgb([
                (x % 256) as u8 ^ grain,
                (y % 256) as u8,
                ((x + y) % 256) as u8,
            ])
        })
    }

    /// A one-page Letter PDF holding a large photo stored losslessly, as scanners write them.
    fn image_heavy_pdf(path: &Path) -> PathBuf {
        let mut doc = lib().create_new_pdf().unwrap();
        let image = DynamicImage::ImageRgb8(noisy_image(2400, 3000));
        let mut object = PdfPageImageObject::new(&doc, &image).unwrap();
        object.scale(LETTER.0, LETTER.1).unwrap();
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(LETTER.0),
                PdfPoints::new(LETTER.1),
            ))
            .unwrap();
        page.objects_mut().add_image_object(object).unwrap();
        drop(page);
        doc.save_to_file(path).unwrap();
        path.to_path_buf()
    }

    /// A text-only PDF already saved as compactly as lopdf can: there's nothing left to win.
    fn text_only_pdf(path: &Path) -> PathBuf {
        use lopdf::{Stream, dictionary};
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 24 Tf 72 700 Td (Hello, world) Tj ET".to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
            ),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc.compress();
        let mut file = File::create(path).unwrap();
        let options = SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .build();
        doc.save_with_options(&mut file, options).unwrap();
        path.to_path_buf()
    }

    fn jpeg_file(path: &Path, w: u32, h: u32) -> PathBuf {
        DynamicImage::ImageRgb8(noisy_image(w, h))
            .save_with_format(path, ImageFormat::Jpeg)
            .unwrap();
        path.to_path_buf()
    }

    fn quiet() -> (CancelToken, impl Fn(f32)) {
        (CancelToken::default(), |_| {})
    }

    // ---- Ranges ----

    #[test]
    fn ranges_keep_the_typed_order_and_repeats() {
        let ranges = parse_ranges("3, 1-2, 3", 10).unwrap();

        assert_eq!(flatten(&ranges), [2, 0, 1, 2]);
    }

    #[test]
    fn open_ended_ranges_run_to_the_first_or_last_page() {
        let ranges = parse_ranges("8-, -2", 10).unwrap();

        assert_eq!(ranges, [8..=10, 1..=2]);
    }

    #[test]
    fn ranges_accept_dashes_pasted_from_documents() {
        let ranges = parse_ranges("2–4", 10).unwrap();

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], 2..=4);
    }

    #[test]
    fn a_backwards_range_is_named() {
        let error = parse_ranges("1, 9-4", 10).unwrap_err();

        assert_eq!(error, "“9-4” is backwards.");
    }

    #[test]
    fn a_page_past_the_end_is_named_with_the_last_page() {
        let error = parse_ranges("12", 10).unwrap_err();

        assert_eq!(error, "“12” is past the last page, 10.");
    }

    #[test]
    fn every_invalid_part_is_named() {
        let error = parse_ranges("0, x, 2-3", 3).unwrap_err();

        assert_eq!(
            error,
            "“0” isn't a page: pages start at 1. “x” isn't a page number or range."
        );
    }

    #[test]
    fn empty_ranges_ask_for_pages() {
        let error = parse_ranges(" , ", 3).unwrap_err();

        assert_eq!(error, "Type the pages to use, like 1-3, 5.");
    }

    #[test]
    fn selected_pages_format_as_sorted_runs() {
        let text = format_pages(&[4, 0, 2, 1, 4]);

        assert_eq!(text, "1-3, 5");
    }

    #[test]
    fn import_lists_shorten_consecutive_pages() {
        let list = import_list(&[2, 0, 1, 8, 9, 10]);

        assert_eq!(list, "3,1-2,9-11");
    }

    #[test]
    fn page_file_names_pad_to_the_page_count() {
        let name = page_file_name(6, 120, "png");

        assert_eq!(name, "Page 007.png");
    }

    // ---- The library ----

    #[test]
    fn the_library_is_not_found_in_a_folder_without_it() {
        let dir = temp_dir("no-library");

        let found = find_library(std::slice::from_ref(&dir));

        assert_eq!(found, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn opening_a_file_that_isnt_a_pdf_says_so() {
        let dir = temp_dir("not-pdf");
        let path = dir.join("notes.pdf");
        fs::write(&path, b"just text").unwrap();

        let error = open(lib(), &path).map(drop).unwrap_err();

        assert_eq!(error, "notes.pdf isn't a PDF, or it's damaged.");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn opening_a_folder_is_refused_before_pdfium_sees_it() {
        let dir = temp_dir("folder");

        let error = open(lib(), &dir).map(drop).unwrap_err();

        assert_eq!(error, format!("{} isn't a file.", file_name(&dir)));
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- Assembling ----

    #[test]
    fn merging_keeps_every_page_in_file_order() {
        let dir = temp_dir("merge");
        let a = sized_pdf(&dir.join("a.pdf"), &[(100.0, 500.0), (200.0, 500.0)]);
        let b = sized_pdf(&dir.join("b.pdf"), &[(300.0, 500.0)]);
        let out = dir.join("out.pdf");
        let pages = [all_pages(0, 2), all_pages(1, 1)].concat();
        let (cancel, progress) = quiet();

        assemble(lib(), &[a, b], &pages, &out, &cancel, &progress).unwrap();

        assert_eq!(widths(&out), [100, 200, 300]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extracting_keeps_the_typed_order_and_repeats() {
        let dir = temp_dir("extract");
        let src = sized_pdf(
            &dir.join("src.pdf"),
            &[(100.0, 500.0), (200.0, 500.0), (300.0, 500.0)],
        );
        let out = dir.join("out.pdf");
        let ranges = parse_ranges("3, 1, 3", 3).unwrap();
        let pages: Vec<PageRef> = flatten(&ranges)
            .into_iter()
            .map(|i| PageRef::new(0, i))
            .collect();
        let (cancel, progress) = quiet();

        assemble(lib(), &[src], &pages, &out, &cancel, &progress).unwrap();

        assert_eq!(widths(&out), [300, 100, 300]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn splitting_writes_each_range_to_its_own_file() {
        let dir = temp_dir("split");
        let src = sized_pdf(
            &dir.join("src.pdf"),
            &[
                (100.0, 500.0),
                (200.0, 500.0),
                (300.0, 500.0),
                (400.0, 500.0),
            ],
        );
        let (first, second) = (dir.join("Pages 2-3.pdf"), dir.join("Page 4.pdf"));
        let outputs = [
            (first.clone(), vec![PageRef::new(0, 1), PageRef::new(0, 2)]),
            (second.clone(), vec![PageRef::new(0, 3)]),
        ];
        let (cancel, progress) = quiet();

        assemble_each(lib(), &[src], &outputs, &cancel, &progress).unwrap();

        assert_eq!(
            [widths(&first), widths(&second)],
            [vec![200, 300], vec![400]]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reordering_copies_pages_in_the_new_order() {
        let dir = temp_dir("reorder");
        let src = sized_pdf(
            &dir.join("src.pdf"),
            &[(100.0, 500.0), (200.0, 500.0), (300.0, 500.0)],
        );
        let out = dir.join("out.pdf");
        let pages = [PageRef::new(0, 2), PageRef::new(0, 0), PageRef::new(0, 1)];
        let (cancel, progress) = quiet();

        assemble(lib(), &[src], &pages, &out, &cancel, &progress).unwrap();

        assert_eq!(widths(&out), [300, 100, 200]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotating_adds_quarter_turns_to_each_page() {
        let dir = temp_dir("rotate");
        let src = sized_pdf(&dir.join("src.pdf"), &[(100.0, 500.0), (200.0, 500.0)]);
        let out = dir.join("out.pdf");
        let pages = [
            PageRef {
                source: 0,
                index: 0,
                turns: 1,
            },
            PageRef {
                source: 0,
                index: 1,
                turns: 7,
            },
        ];
        let (cancel, progress) = quiet();

        assemble(lib(), &[src], &pages, &out, &cancel, &progress).unwrap();

        assert_eq!(rotations(&out), [1, 3]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancelled_assembly_stops_before_writing() {
        let dir = temp_dir("cancel");
        let src = sized_pdf(&dir.join("src.pdf"), &[(100.0, 500.0)]);
        let out = dir.join("out.pdf");
        let (cancel, progress) = quiet();
        cancel.cancel();

        let result = assemble(lib(), &[src], &all_pages(0, 1), &out, &cancel, &progress);

        assert!(matches!(result, Err(JobError::Cancelled)));
        assert!(!out.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- Images to PDF ----

    #[test]
    fn jpegs_go_into_the_pdf_untouched() {
        let dir = temp_dir("jpeg");
        let photo = jpeg_file(&dir.join("photo.jpg"), 400, 300);
        let out = dir.join("out.pdf");
        let layout = Layout {
            size: PageSize::Fit,
            orientation: Orientation::Auto,
            margin: 0.0,
        };
        let (cancel, progress) = quiet();

        images_to_pdf(lib(), &[photo], layout, &out, &cancel, &progress).unwrap();

        let bytes = fs::read(&out).unwrap();
        assert!(bytes.windows(9).any(|w| w == b"DCTDecode"));
        assert_eq!(widths(&out), [400]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_landscape_image_on_a4_gets_a_landscape_page_inside_the_margins() {
        let layout = Layout {
            size: PageSize::A4,
            orientation: Orientation::Auto,
            margin: 36.0,
        };

        let (page, (x, y, w, h)) = place(layout, 2000, 1000);

        assert_eq!(page, (841.89, 595.28));
        assert_eq!(
            (x.round(), y.round(), w.round(), h.round()),
            (36.0, 105.0, 770.0, 385.0)
        );
    }

    #[test]
    fn fit_pages_are_the_image_size_plus_margins() {
        let layout = Layout {
            size: PageSize::Fit,
            orientation: Orientation::Portrait,
            margin: 10.0,
        };

        let placed = place(layout, 300, 200);

        assert_eq!(placed, ((320.0, 220.0), (10.0, 10.0, 300.0, 200.0)));
    }

    // ---- Rendering ----

    #[test]
    fn pages_render_at_points_over_72_times_dpi() {
        let dir = temp_dir("render");
        let src = sized_pdf(&dir.join("letter.pdf"), &[LETTER]);
        let out = dir.join("images");
        fs::create_dir(&out).unwrap();
        let (cancel, progress) = quiet();

        render_to_folder(
            lib(),
            &src,
            &[0],
            PageImages {
                dpi: 150.0,
                format: Raster::Png,
            },
            &out,
            &cancel,
            &progress,
        )
        .unwrap();

        let image = ::image::open(out.join("Page 1.png")).unwrap();
        assert_eq!((image.width(), image.height()), (1275, 1650));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn huge_renders_are_capped_in_pixels() {
        let scale = render_scale(14_400.0, 14_400.0, 600.0).unwrap();

        let pixels = (14_400.0 * scale).powi(2);

        assert!(pixels <= MAX_RENDER_PIXELS * 1.001);
    }

    #[test]
    fn long_thin_renders_are_capped_in_edge_length() {
        let scale = render_scale(1.0, 14_400.0, 600.0).unwrap();

        assert!(14_400.0 * scale <= MAX_RENDER_EDGE * 1.001);
    }

    #[test]
    fn unreadable_page_sizes_and_scales_are_errors() {
        for (w, h, dpi) in [
            (f32::NAN, 100.0, 72.0),
            (100.0, f32::INFINITY, 72.0),
            (0.0, 100.0, 72.0),
            (100.0, 100.0, 0.0),
            (100.0, 100.0, f32::NAN),
        ] {
            assert_eq!(
                render_scale(w, h, dpi),
                Err("This page's size can't be read.".to_string())
            );
        }
    }

    #[test]
    fn thumbnails_stop_at_the_page_limit() {
        let dir = temp_dir("thumb-limit");
        let sizes = vec![(100.0, 100.0); MAX_THUMBNAIL_PAGES + 3];
        let src = sized_pdf(&dir.join("src.pdf"), &sizes);
        let mut drawn = 0;

        let left_out = thumbnails(lib(), &src, 16, &CancelToken::default(), |_, _| drawn += 1);

        assert_eq!((drawn, left_out), (MAX_THUMBNAIL_PAGES, Ok(3)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn thumbnails_fit_the_requested_edge() {
        let dir = temp_dir("thumbs");
        let src = sized_pdf(&dir.join("src.pdf"), &[(300.0, 600.0), (600.0, 300.0)]);
        let sizes = std::sync::Mutex::new(Vec::new());

        thumbnails(lib(), &src, 120, &CancelToken::default(), |_, img| {
            sizes.lock().unwrap().push(img.dimensions());
        })
        .unwrap();

        assert_eq!(sizes.into_inner().unwrap(), [(60, 120), (120, 60)]);
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- Compression ----

    #[test]
    fn compressing_images_shrinks_an_image_heavy_pdf() {
        let dir = temp_dir("compress");
        let src = image_heavy_pdf(&dir.join("scan.pdf"));
        let out = dir.join("out.pdf");
        let settings = CompressSettings {
            mode: CompressMode::Images,
            dpi: 100.0,
            quality: 60,
        };
        let (cancel, progress) = quiet();

        let result = compress(lib(), &src, &out, settings, &cancel, &progress).unwrap();

        let Compressed::Smaller { before, after } = result else {
            panic!("expected a smaller file, got {result:?}");
        };
        assert!(after * 4 < before, "{before} -> {after}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn compressing_a_text_only_pdf_never_grows_it() {
        let dir = temp_dir("compress-text");
        let src = text_only_pdf(&dir.join("letter.pdf"));
        let out = dir.join("out.pdf");
        let settings = CompressSettings {
            mode: CompressMode::Images,
            dpi: 150.0,
            quality: 70,
        };
        let (cancel, progress) = quiet();

        let result = compress(lib(), &src, &out, settings, &cancel, &progress).unwrap();

        assert_eq!(result, Compressed::NoGain);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rasterising_keeps_the_page_size() {
        let dir = temp_dir("rasterise");
        let src = image_heavy_pdf(&dir.join("scan.pdf"));
        let out = dir.join("out.pdf");
        let settings = CompressSettings {
            mode: CompressMode::Rasterise,
            dpi: 72.0,
            quality: 50,
        };
        let (cancel, progress) = quiet();

        compress(lib(), &src, &out, settings, &cancel, &progress).unwrap();

        assert_eq!(widths(&out), [612]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
