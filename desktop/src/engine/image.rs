//! Images family (image-converter, image-cropper, palette-extractor): decoding, encoding and
//! pixel work on the `image` crate. Owner: images family.
//!
//! Every decode goes through `open` or `probe`, which cap an image's size (`MAX_SIDE`,
//! `MAX_ALLOC`) so a small file claiming to be enormous can't exhaust memory, and turn EXIF
//! orientation into real pixels, because no output keeps the metadata that says to rotate.
//! AVIF is written but not read (decoding needs the dav1d C library), and HEIC is not read at
//! all; both are refused with a message that says so.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use ::image::codecs::avif::AvifEncoder;
use ::image::codecs::gif::GifEncoder;
use ::image::codecs::ico::{IcoEncoder, IcoFrame};
use ::image::codecs::jpeg::JpegEncoder;
use ::image::error::LimitErrorKind;
use ::image::imageops::FilterType;
use ::image::metadata::Orientation;
use ::image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits,
    RgbImage, RgbaImage,
};
use anyhow::{Context, anyhow, bail};
use slint::{Rgba8Pixel, SharedPixelBuffer};


/// The widest or tallest image any tool opens. Large panoramas fit; a file claiming more is
/// refused before its pixels are allocated.
pub const MAX_SIDE: u32 = 32_768;
/// The most memory one decode may allocate: about 268 megapixels of 8-bit RGBA.
const MAX_ALLOC: u64 = 1 << 30;
/// What the open dialogs offer: everything `image` decodes here.
pub const INPUT_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "webp", "gif", "bmp", "tif", "tiff", "ico",
];
/// What the drop zones say the tools open.
pub const FORMATS_HINT: &str = "PNG, JPEG, WebP, GIF, BMP, TIFF and ICO. AVIF can be saved but not opened, and HEIC isn't supported.";
const AVIF_INPUT: &str =
    "AVIF images can be saved but not opened: decoding needs a library this build doesn't include";
/// AVIF encoder speed, 1 (slowest, smallest) to 10: 6 keeps a photo to a few seconds.
const AVIF_SPEED: u8 = 6;
/// The square sizes an icon holds, smallest first; each no larger than the source.
const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// The formats the tools write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    WebP,
    Avif,
    Tiff,
    Gif,
    Bmp,
    Ico,
}

impl Format {
    pub const ALL: [Format; 8] = [
        Self::Png,
        Self::Jpeg,
        Self::WebP,
        Self::Avif,
        Self::Tiff,
        Self::Gif,
        Self::Bmp,
        Self::Ico,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Avif => "AVIF",
            Self::Tiff => "TIFF",
            Self::Gif => "GIF",
            Self::Bmp => "BMP",
            Self::Ico => "ICO",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Avif => "avif",
            Self::Tiff => "tiff",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
        }
    }

    /// Whether the quality setting applies.
    pub fn is_lossy(self) -> bool {
        matches!(self, Self::Jpeg | Self::WebP | Self::Avif)
    }

    /// Whether transparent pixels stay transparent.
    pub fn keeps_transparency(self) -> bool {
        self != Self::Jpeg
    }

    /// What's worth knowing before saving in this format; "" when nothing is.
    pub fn note(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG has no transparency, so transparent areas turn white.",
            Self::Avif => "AVIF files are small, but saving one takes a few seconds per photo.",
            Self::Gif => "GIF holds at most 256 colours, so photos lose detail.",
            Self::Ico => "Holds every icon size from 16 to 256 pixels that the image is large enough for.",
            Self::Png | Self::WebP | Self::Tiff | Self::Bmp => "",
        }
    }

    /// The format that saves like `source`, if the tools write it.
    fn of(source: ImageFormat) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.image_format() == source)
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::WebP => ImageFormat::WebP,
            Self::Avif => ImageFormat::Avif,
            Self::Tiff => ImageFormat::Tiff,
            Self::Gif => ImageFormat::Gif,
            Self::Bmp => ImageFormat::Bmp,
            Self::Ico => ImageFormat::Ico,
        }
    }
}

/// What a file holds, read from its header.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    /// After EXIF orientation, so a portrait photo reads as portrait.
    pub width: u32,
    pub height: u32,
    /// "JPEG", "HEIC"
    pub kind: &'static str,
    /// The output format that matches the source, for "Same as original".
    pub format: Option<Format>,
}

impl Source {
    /// "4032×3024 · JPEG"
    pub fn summary(&self) -> String {
        format!("{}×{} · {}", self.width, self.height, self.kind)
    }
}

pub struct Decoded {
    pub image: DynamicImage,
    pub source: Source,
}

fn reader(path: &Path) -> anyhow::Result<(ImageReader<BufReader<File>>, ImageFormat)> {
    let reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .context("Couldn't open it")?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match reader.format() {
        Some(ImageFormat::Avif) => bail!(AVIF_INPUT),
        Some(format) => Ok((reader, format)),
        None if ext == "avif" => bail!(AVIF_INPUT),
        None if matches!(ext.as_str(), "heic" | "heif" | "hif") => bail!(
            "HEIC images aren't supported yet. Convert the photo to JPEG or PNG first"
        ),
        None => bail!("Not an image Open Tools can read"),
    }
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

/// The sentence for a decode error; size limits get their own.
fn decode_error(e: ImageError) -> anyhow::Error {
    match e {
        ImageError::Limits(limit) => match limit.kind() {
            LimitErrorKind::InsufficientMemory => anyhow!(
                "It's too large to open: it needs more than 1 GB of memory, the most Open \
                 Tools gives one image"
            ),
            _ => too_large(),
        },
        ImageError::Unsupported(_) => anyhow!("This image type isn't supported"),
        e => anyhow!("Couldn't read it: {e}"),
    }
}

fn too_large() -> anyhow::Error {
    anyhow!(
        "It's too large to open: it's over {} pixels on a side, the most Open Tools opens",
        grouped(MAX_SIDE)
    )
}

/// 32768 as "32,768".
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn swaps_sides(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

fn native_source(format: ImageFormat, (w, h): (u32, u32), orientation: Orientation) -> Source {
    let (width, height) = if swaps_sides(orientation) { (h, w) } else { (w, h) };
    Source {
        width,
        height,
        // Every format this build decodes natively is one the tools also write.
        kind: Format::of(format).map_or("Image", Format::label),
        format: Format::of(format),
    }
}

/// Reads a file's size and type without decoding its pixels.
pub fn probe(path: &Path) -> anyhow::Result<Source> {
    let (mut reader, format) = reader(path)?;
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let orientation = decoder.orientation().map_err(decode_error)?;
    Ok(native_source(format, decoder.dimensions(), orientation))
}

/// Decodes a whole image, upright.
pub fn open(path: &Path) -> anyhow::Result<Decoded> {
    let (mut reader, format) = reader(path)?;
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let orientation = decoder.orientation().map_err(decode_error)?;
    let size = decoder.dimensions();
    let mut image = DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    image.apply_orientation(orientation);
    Ok(Decoded {
        image,
        source: native_source(format, size, orientation),
    })
}

/// Writes `image` to `path` as `format`. `quality` (1–100) applies to the lossy formats.
pub fn save(image: &DynamicImage, format: Format, quality: u8, path: &Path) -> anyhow::Result<()> {
    let file = File::create(path).context("Couldn't write the file")?;
    let mut out = BufWriter::new(file);
    let quality = quality.clamp(1, 100);
    let result = match format {
        Format::Png => high_depth(image).write_to(&mut out, ImageFormat::Png),
        Format::Tiff => high_depth(image).write_to(&mut out, ImageFormat::Tiff),
        Format::Bmp => DynamicImage::ImageRgba8(image.to_rgba8()).write_to(&mut out, ImageFormat::Bmp),
        Format::Jpeg => {
            JpegEncoder::new_with_quality(&mut out, quality).encode_image(&on_white(image))
        }
        Format::Gif => GifEncoder::new(&mut out)
            .encode_frame(::image::Frame::new(image.to_rgba8())),
        Format::Avif => {
            let encoder = AvifEncoder::new_with_speed_quality(&mut out, AVIF_SPEED, quality);
            if image.color().has_alpha() {
                DynamicImage::ImageRgba8(image.to_rgba8()).write_with_encoder(encoder)
            } else {
                DynamicImage::ImageRgb8(image.to_rgb8()).write_with_encoder(encoder)
            }
        }
        Format::WebP => {
            let rgba = image.to_rgba8();
            let encoded = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height())
                .encode(f32::from(quality));
            return out
                .write_all(&encoded)
                .and_then(|()| out.flush())
                .context("Couldn't write the file");
        }
        Format::Ico => write_ico(image, &mut out),
    };
    result.map_err(|e| anyhow!("Couldn't save it as {}: {e}", format.label()))?;
    out.flush().context("Couldn't write the file")
}

/// PNG and TIFF keep 8- and 16-bit pixels as they are; floating-point ones become 16-bit.
fn high_depth(image: &DynamicImage) -> std::borrow::Cow<'_, DynamicImage> {
    match image {
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            std::borrow::Cow::Owned(DynamicImage::ImageRgba16(image.to_rgba16()))
        }
        _ => std::borrow::Cow::Borrowed(image),
    }
}

/// Transparent pixels composited over white, as JPEG has no alpha.
fn on_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let a = u16::from(a);
        let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a) + 127) / 255) as u8;
        ::image::Rgb([blend(r), blend(g), blend(b)])
    })
}

/// One PNG-compressed entry per size in `ICO_SIZES` up to the image's longest side (ICO caps
/// entries at 256 pixels), each fitted inside its square with the aspect ratio kept.
fn write_ico(image: &DynamicImage, out: &mut impl Write) -> Result<(), ImageError> {
    let longest = image.width().max(image.height());
    let mut sides: Vec<u32> = ICO_SIZES.into_iter().filter(|s| *s < longest).collect();
    sides.push(longest.min(*ICO_SIZES.last().unwrap_or(&longest)));
    let frames = sides
        .into_iter()
        .map(|side| {
            let fitted = if side == longest {
                image.to_rgba8()
            } else {
                image.resize(side, side, FilterType::Lanczos3).to_rgba8()
            };
            IcoFrame::as_png(fitted.as_raw(), fitted.width(), fitted.height(), ExtendedColorType::Rgba8)
        })
        .collect::<Result<Vec<_>, _>>()?;
    IcoEncoder::new(out).encode_images(&frames)
}

/// Fits `image` inside `max_width` × `max_height`, keeping its aspect ratio; never enlarges.
pub fn fit_within(image: DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
    if image.width() <= max_width && image.height() <= max_height {
        return image;
    }
    image.resize(max_width.max(1), max_height.max(1), FilterType::Lanczos3)
}

/// A downscaled copy for the screen, at most `max_side` pixels on its longer side.
pub fn preview(image: &DynamicImage, max_side: u32) -> RgbaImage {
    if image.width().max(image.height()) <= max_side {
        image.to_rgba8()
    } else {
        image.thumbnail(max_side, max_side).to_rgba8()
    }
}

/// Pixels for `slint::Image::from_rgba8`: built on a worker (the buffer is `Send`), turned
/// into an image on the UI thread.
pub fn pixel_buffer(image: &RgbaImage) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::clone_from_slice(image.as_raw(), image.width(), image.height())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("open-tools-image-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An 8×6 gradient whose top-left corner is transparent.
    fn tiny() -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(8, 6, |x, y| {
            let alpha = if x == 0 && y == 0 { 0 } else { 255 };
            ::image::Rgba([(x * 30) as u8, (y * 40) as u8, 128, alpha])
        }))
    }

    fn open_ok(path: &Path) -> Decoded {
        open(path).unwrap()
    }

    /// Saves `tiny()` as `from`, opens it, saves that as `to` and returns the result's size,
    /// read back with `image` (AVIF, which it can't decode, through FFmpeg when present).
    fn convert(from: Format, to: Format) -> (u32, u32) {
        let dir = temp_dir(&format!("{}-{}", from.extension(), to.extension()));
        let input = dir.join(format!("in.{}", from.extension()));
        save(&tiny(), from, 90, &input).unwrap();
        let decoded = open_ok(&input);
        let output = dir.join(format!("out.{}", to.extension()));

        save(&decoded.image, to, 90, &output).unwrap();

        let size = if to == Format::Avif {
            let bytes = fs::read(&output).unwrap();
            assert_eq!(&bytes[4..12], b"ftypavif");
            (8, 6)
        } else {
            let back = open_ok(&output).image;
            (back.width(), back.height())
        };
        fs::remove_dir_all(&dir).unwrap();
        size
    }

    macro_rules! converts_to_every_format {
        ($module:ident, $from:expr) => {
            mod $module {
                use super::*;

                #[test]
                fn to_png() {
                    assert_eq!(convert($from, Format::Png), (8, 6));
                }
                #[test]
                fn to_jpeg() {
                    assert_eq!(convert($from, Format::Jpeg), (8, 6));
                }
                #[test]
                fn to_webp() {
                    assert_eq!(convert($from, Format::WebP), (8, 6));
                }
                #[test]
                fn to_avif() {
                    assert_eq!(convert($from, Format::Avif), (8, 6));
                }
                #[test]
                fn to_tiff() {
                    assert_eq!(convert($from, Format::Tiff), (8, 6));
                }
                #[test]
                fn to_gif() {
                    assert_eq!(convert($from, Format::Gif), (8, 6));
                }
                #[test]
                fn to_bmp() {
                    assert_eq!(convert($from, Format::Bmp), (8, 6));
                }
                #[test]
                fn to_ico() {
                    assert_eq!(convert($from, Format::Ico), (8, 6));
                }
            }
        };
    }

    converts_to_every_format!(from_png, Format::Png);
    converts_to_every_format!(from_jpeg, Format::Jpeg);
    converts_to_every_format!(from_webp, Format::WebP);
    converts_to_every_format!(from_tiff, Format::Tiff);
    converts_to_every_format!(from_gif, Format::Gif);
    converts_to_every_format!(from_bmp, Format::Bmp);
    converts_to_every_format!(from_ico, Format::Ico);

    #[test]
    fn avif_input_is_refused_with_a_reason() {
        let dir = temp_dir("avif-in");
        let input = dir.join("photo.avif");
        save(&tiny(), Format::Avif, 90, &input).unwrap();

        let error = open(&input).err().unwrap();

        assert_eq!(error.to_string(), AVIF_INPUT);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn heic_input_is_refused_with_a_reason() {
        let dir = temp_dir("heic-in");
        let input = dir.join("photo.heic");
        fs::write(&input, b"   ftypheic").unwrap();

        let error = open(&input).err().unwrap();

        assert_eq!(
            error.to_string(),
            "HEIC images aren't supported yet. Convert the photo to JPEG or PNG first"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_image_wider_than_the_limit_is_refused_before_decoding() {
        let dir = temp_dir("limit");
        let input = dir.join("wide.png");
        DynamicImage::ImageLuma8(::image::GrayImage::new(MAX_SIDE + 1, 1))
            .save(&input)
            .unwrap();

        let error = open(&input).err().unwrap();

        assert_eq!(
            error.to_string(),
            "It's too large to open: it's over 32,768 pixels on a side, the most Open Tools opens"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn jpeg_flattens_transparency_onto_white() {
        let flat = on_white(&tiny());

        assert_eq!(flat.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(flat.get_pixel(1, 0).0, [30, 0, 128]);
    }

    #[test]
    fn an_icon_holds_each_size_up_to_the_source() {
        let dir = temp_dir("ico");
        let path = dir.join("icon.ico");
        let square = DynamicImage::ImageRgba8(RgbaImage::new(40, 40));

        save(&square, Format::Ico, 90, &path).unwrap();

        let bytes = fs::read(&path).unwrap();
        // ICONDIR: reserved, type 1, then the entry count: 16, 24, 32 and the 40-pixel original.
        assert_eq!(&bytes[..6], &[0, 0, 1, 0, 4, 0]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fit_within_keeps_the_aspect_ratio_and_never_enlarges() {
        let wide = DynamicImage::ImageRgba8(RgbaImage::new(400, 200));

        let fitted = fit_within(wide.clone(), 100, 100);
        let untouched = fit_within(wide, 1000, 1000);

        assert_eq!((fitted.width(), fitted.height()), (100, 50));
        assert_eq!((untouched.width(), untouched.height()), (400, 200));
    }

    #[test]
    fn probe_reads_the_size_and_type_without_decoding() {
        let dir = temp_dir("probe");
        let path = dir.join("a.jpg");
        save(&tiny(), Format::Jpeg, 80, &path).unwrap();

        let source = probe(&path).unwrap();

        assert_eq!(source.summary(), "8×6 · JPEG");
        assert_eq!(source.format, Some(Format::Jpeg));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_isnt_an_image_is_named_as_such() {
        let dir = temp_dir("not-image");
        let path = dir.join("notes.png");
        fs::write(&path, b"just text").unwrap();

        let error = probe(&path).unwrap_err();

        assert_eq!(error.to_string(), "Not an image Open Tools can read");
        fs::remove_dir_all(&dir).unwrap();
    }
}
