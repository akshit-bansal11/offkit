//! Image Cropper: crop one image or a batch, as a rectangle (free or a fixed aspect ratio, with
//! an optional rule-of-thirds grid) or a circle with transparent corners. Wires the
//! `ImageCropper` global (ui/tools/image-cropper.slint).
//!
//! The crop geometry is the website's (src/lib/tools/image-cropper/utils.ts and the circular
//! editor), in source pixels of the upright image: the default crop, clamping with a 20 px
//! minimum, resizing from eight handles with the aspect ratio kept, the circle's radius
//! handles, and refitting to a new ratio. Two things differ on purpose:
//! - "All images" shares one crop as a fraction of the image (`to_relative`/`from_relative`),
//!   so it lands on the same part of images of different sizes; the website clamped one pixel
//!   rectangle to each image.
//! - Output keeps the source's format by default (the website always wrote PNG), except a
//!   circle from a format without transparency, which becomes PNG.
//!
//! Each image is decoded once when added, for its size and a ≤1024 px preview kept for the
//! editor; cropping decodes it again at full size on a worker.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use ::image::DynamicImage;
use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};

use crate::engine::image::{self, Format, INPUT_EXTENSIONS, Source};
use crate::file_queue::{self, FileQueue, Summary, Update};
use crate::files::{self, Reserved, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, CropField, CropHandle, CropScope, CropShape, FileStatus, ImageCropper};

pub const SLUG: &str = "image-cropper";
/// The smallest crop side, as on the website.
const MIN_CROP: f64 = 20.0;
/// The editor's preview: enough detail to place a crop, a few MB per image.
const PREVIEW_SIDE: u32 = 1024;
const CONCURRENT_PROBES: usize = 4;
const CONCURRENT_CROPS: usize = 3;
/// The website's presets: a label and width ÷ height (`None` is free).
const ASPECTS: [(&str, Option<f64>); 6] = [
    ("Free", None),
    ("1:1", Some(1.0)),
    ("4:3", Some(4.0 / 3.0)),
    ("16:9", Some(16.0 / 9.0)),
    ("3:2", Some(3.0 / 2.0)),
    ("2:3", Some(2.0 / 3.0)),
];

// ---- Geometry -----------------------------------------------------------------------------

/// A crop in source pixels (or, for `to_relative`, in fractions of the image).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// The centred 70% of the image.
fn default_crop(width: f64, height: f64) -> Rect {
    let w = MIN_CROP.max((width * 0.7).round());
    let h = MIN_CROP.max((height * 0.7).round());
    Rect {
        x: ((width - w) / 2.0).round(),
        y: ((height - h) / 2.0).round(),
        w,
        h,
    }
}

/// A centred circle (its square bounds) with a radius of 30% of the shorter side.
fn default_circle(width: f64, height: f64) -> Rect {
    let r = (width.min(height) * 0.3).round();
    Rect {
        x: (width / 2.0).round() - r,
        y: (height / 2.0).round() - r,
        w: r * 2.0,
        h: r * 2.0,
    }
}

/// Keeps a crop inside the image and at least `MIN_CROP` on a side (or the whole side, for an
/// image smaller than that).
fn clamp_rect(rect: Rect, width: f64, height: f64) -> Rect {
    let w = MIN_CROP.min(width).max(rect.w.min(width));
    let h = MIN_CROP.min(height).max(rect.h.min(height));
    Rect {
        x: rect.x.min(width - w).max(0.0),
        y: rect.y.min(height - h).max(0.0),
        w,
        h,
    }
}

/// The largest rectangle of `ratio` centred inside `crop`, kept in bounds.
fn fit_to_aspect(crop: Rect, ratio: f64, width: f64, height: f64) -> Rect {
    let w = crop.w.min(crop.h * ratio);
    let h = w / ratio;
    clamp_rect(
        Rect {
            x: (crop.x + (crop.w - w) / 2.0).round(),
            y: (crop.y + (crop.h - h) / 2.0).round(),
            w: w.round(),
            h: h.round(),
        },
        width,
        height,
    )
}

fn has(handle: CropHandle, side: char) -> bool {
    let name = match handle {
        CropHandle::Move => "",
        CropHandle::N => "n",
        CropHandle::Ne => "ne",
        CropHandle::E => "e",
        CropHandle::Se => "se",
        CropHandle::S => "s",
        CropHandle::Sw => "sw",
        CropHandle::W => "w",
        CropHandle::Nw => "nw",
    };
    name.contains(side)
}

/// Drags `handle` of `start` by (`dx`, `dy`); with a `ratio`, an edge handle recentres the
/// other axis and a corner follows the larger of the two movements.
fn resize_rect(start: Rect, dx: f64, dy: f64, handle: CropHandle, ratio: Option<f64>) -> Rect {
    if handle == CropHandle::Move {
        return Rect {
            x: start.x + dx,
            y: start.y + dy,
            ..start
        };
    }
    let (right, bottom) = (start.x + start.w, start.y + start.h);
    let mut next = start;
    if has(handle, 'e') {
        next.w = start.w + dx;
    }
    if has(handle, 'w') {
        next.x = start.x + dx;
        next.w = right - next.x;
    }
    if has(handle, 's') {
        next.h = start.h + dy;
    }
    if has(handle, 'n') {
        next.y = start.y + dy;
        next.h = bottom - next.y;
    }
    let Some(ratio) = ratio else { return next };

    let horizontal = has(handle, 'e') || has(handle, 'w');
    let vertical = has(handle, 'n') || has(handle, 's');
    if horizontal && !vertical {
        next.h = next.w / ratio;
        next.y = start.y + (start.h - next.h) / 2.0;
        return next;
    }
    if vertical && !horizontal {
        next.w = next.h * ratio;
        next.x = start.x + (start.w - next.w) / 2.0;
        return next;
    }
    if dx.abs() >= dy.abs() {
        next.h = next.w / ratio;
    } else {
        next.w = next.h * ratio;
    }
    if has(handle, 'w') {
        next.x = right - next.w;
    }
    if has(handle, 'n') {
        next.y = bottom - next.h;
    }
    next
}

/// The circle editor: the centre moves, or a radius handle (N, E, S, W) grows or shrinks the
/// circle around its centre, between 10 px and half the shorter side.
fn drag_circle(start: Rect, dx: f64, dy: f64, handle: CropHandle, width: f64, height: f64) -> Rect {
    let r = start.w / 2.0;
    let (cx, cy) = (start.x + r, start.y + r);
    if handle == CropHandle::Move {
        let moved = Rect {
            x: cx + dx - r,
            y: cy + dy - r,
            ..start
        };
        return clamp_rect(moved, width, height);
    }
    let grown = match handle {
        CropHandle::N => r - dy,
        CropHandle::S => r + dy,
        CropHandle::E => r + dx,
        CropHandle::W => r - dx,
        _ => r,
    };
    let r = grown.min(width.min(height) / 2.0).max(MIN_CROP / 2.0);
    clamp_rect(
        Rect {
            x: (cx - r).round(),
            y: (cy - r).round(),
            w: (r * 2.0).round(),
            h: (r * 2.0).round(),
        },
        width,
        height,
    )
}

/// A crop as fractions of its image, for sharing one crop across images of different sizes.
fn to_relative(rect: Rect, width: f64, height: f64) -> Rect {
    Rect {
        x: rect.x / width,
        y: rect.y / height,
        w: rect.w / width,
        h: rect.h / height,
    }
}

/// A shared crop placed on an image: scaled to its size, then refitted to `ratio` (a square
/// for a circle), since scaling by a different width and height changes the shape.
fn from_relative(rel: Rect, width: f64, height: f64, ratio: Option<f64>) -> Rect {
    let scaled = clamp_rect(
        Rect {
            x: (rel.x * width).round(),
            y: (rel.y * height).round(),
            w: (rel.w * width).round(),
            h: (rel.h * height).round(),
        },
        width,
        height,
    );
    match ratio {
        Some(ratio) => fit_to_aspect(scaled, ratio, width, height),
        None => scaled,
    }
}

/// Whole pixels inside an image of `width` × `height`: x, y, width, height.
fn pixels(rect: Rect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x = (rect.x.round().max(0.0) as u32).min(width.saturating_sub(1));
    let y = (rect.y.round().max(0.0) as u32).min(height.saturating_sub(1));
    let w = (rect.w.round().max(1.0) as u32).min(width - x);
    let h = (rect.h.round().max(1.0) as u32).min(height - y);
    (x, y, w, h)
}

/// Cuts the crop out; a circle keeps its anti-aliased disc (diameter = the crop's width) and
/// turns the corners transparent.
fn crop(image: &DynamicImage, (x, y, w, h): (u32, u32, u32, u32), shape: CropShape) -> DynamicImage {
    let cut = image.crop_imm(x, y, w, h);
    if shape == CropShape::Rectangle {
        return cut;
    }
    let mut rgba = cut.to_rgba8();
    let r = f64::from(w) / 2.0;
    for (px, py, pixel) in rgba.enumerate_pixels_mut() {
        let d = ((f64::from(px) + 0.5 - r).powi(2) + (f64::from(py) + 0.5 - r).powi(2)).sqrt();
        let coverage = (r - d + 0.5).clamp(0.0, 1.0);
        pixel.0[3] = (f64::from(pixel.0[3]) * coverage).round() as u8;
    }
    DynamicImage::ImageRgba8(rgba)
}

/// The format a crop is saved in: the chosen one, or for "Same as original" the source's,
/// falling back to PNG for sources the tools can't write (HEIC) and for a circle from a format
/// without transparency.
fn output_format(chosen: Option<Format>, source: Option<Format>, shape: CropShape) -> Format {
    if let Some(format) = chosen {
        return format;
    }
    match source {
        Some(f) if shape == CropShape::Rectangle || f.keeps_transparency() => f,
        _ => Format::Png,
    }
}

// ---- State --------------------------------------------------------------------------------

/// What the cropper knows about one listed image.
#[derive(Default)]
struct Item {
    source: Option<Source>,
    preview: Option<Image>,
    /// This image's own crop, used when each image is cropped separately.
    crop: Option<Rect>,
}

impl Summary for Item {
    fn summary(&self) -> String {
        self.source.as_ref().map(Source::summary).unwrap_or_default()
    }
}

/// A drag in progress, in source pixels.
struct Drag {
    handle: CropHandle,
    from: (f64, f64),
    start: Rect,
}

#[derive(Default)]
struct State {
    queue: FileQueue<Item>,
    /// One per row of `queue`, in the same order.
    thumbs: Rc<VecModel<Image>>,
    /// The row shown in the editor.
    current: Option<u64>,
    /// The crop every image shares in "All images", as fractions of the image.
    shared: Option<Rect>,
    drag: Option<Drag>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

/// The editor's settings, read from the global.
#[derive(Clone, Copy)]
struct Options {
    shape: CropShape,
    ratio: Option<f64>,
    shared: bool,
}

fn options(app: &AppWindow) -> Options {
    let g = app.global::<ImageCropper>();
    let shape = g.get_shape();
    Options {
        shape,
        ratio: match shape {
            CropShape::Circle => Some(1.0),
            CropShape::Rectangle => ASPECTS[index(g.get_aspect_index(), ASPECTS.len())].1,
        },
        shared: g.get_scope() == CropScope::All,
    }
}

fn default_for(source: &Source, o: Options) -> Rect {
    let (w, h) = (f64::from(source.width), f64::from(source.height));
    match (o.shape, o.ratio) {
        (CropShape::Circle, _) => default_circle(w, h),
        (CropShape::Rectangle, Some(ratio)) => fit_to_aspect(default_crop(w, h), ratio, w, h),
        (CropShape::Rectangle, None) => default_crop(w, h),
    }
}

impl State {
    fn position(&self, id: u64) -> Option<usize> {
        self.queue.entries().iter().position(|e| e.id == id)
    }

    /// The crop an image gets with the current options, in its own pixels.
    fn crop_of(&self, item: &Item, o: Options) -> Option<Rect> {
        let source = item.source.as_ref()?;
        let (w, h) = (f64::from(source.width), f64::from(source.height));
        match (o.shared, self.shared) {
            (true, Some(rel)) => Some(from_relative(rel, w, h, o.ratio)),
            _ => Some(item.crop.unwrap_or_else(|| default_for(source, o))),
        }
    }

    fn current_item(&self) -> Option<&Item> {
        let i = self.position(self.current?)?;
        Some(&self.queue.entries()[i].data)
    }

    /// Stores an edit of the current image's crop: its own, or the shared one.
    fn set_current_crop(&mut self, rect: Rect, o: Options) {
        let Some(id) = self.current else { return };
        let Some(source) = self.current_item().and_then(|i| i.source.clone()) else {
            return;
        };
        if o.shared {
            self.shared = Some(to_relative(
                rect,
                f64::from(source.width),
                f64::from(source.height),
            ));
            self.mark_stale(None);
        } else {
            self.queue.update(id, |e| e.data.crop = Some(rect));
            self.mark_stale(Some(id));
        }
    }

    /// A finished crop no longer matches its settings: back to Ready so Crop All redoes it.
    /// `only` limits that to one row.
    fn mark_stale(&mut self, only: Option<u64>) {
        let ids: Vec<u64> = self
            .queue
            .entries()
            .iter()
            .filter(|e| e.status == FileStatus::Done && only.is_none_or(|id| id == e.id))
            .map(|e| e.id)
            .collect();
        for id in ids {
            self.queue.update(id, |e| {
                e.status = FileStatus::Ready;
                e.message.clear();
                e.output = None;
            });
        }
    }
}

// ---- Wiring -------------------------------------------------------------------------------

pub fn init(app: &AppWindow) {
    let g = app.global::<ImageCropper>();
    g.set_formats_hint(image::FORMATS_HINT.into());
    g.set_aspects(select::options(ASPECTS.iter().map(|(label, _)| *label)));
    g.set_formats(select::options(
        std::iter::once("Same as original").chain(Format::ALL.iter().map(|f| f.label())),
    ));
    with(|s| {
        g.set_files(s.queue.rows());
        g.set_thumbs(ModelRc::from(s.thumbs.clone()));
    });

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Images", Some(("Image", INPUT_EXTENSIONS)))
        {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_select(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            if let Some(e) = s.queue.entries().get(i as usize) {
                s.current = Some(e.id);
            }
        });
        show_current(&app);
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let before = s.queue.entries().len();
            s.queue.remove(i as usize);
            if s.queue.entries().len() < before {
                s.thumbs.remove(i as usize);
                let next = s.queue.entries().get(i as usize).or(s.queue.entries().last());
                s.current = next.map(|e| e.id);
            }
        });
        show_current(&app);
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_cancel(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.cancel(i as usize));
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_cancel_all(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.cancel_all());
        sync(&app);
    });
    g.on_reveal(|i| {
        if let Some(output) = with(|s| s.queue.output(i as usize)) {
            files::reveal(&output);
        }
    });

    let weak = app.as_weak();
    g.on_begin_drag(move |handle, x, y| {
        let Some(app) = weak.upgrade() else { return };
        let o = options(&app);
        with(|s| {
            let start = s.current_item().and_then(|item| s.crop_of(item, o));
            s.drag = start.map(|start| Drag {
                handle,
                from: (f64::from(x), f64::from(y)),
                start,
            });
        });
    });
    let weak = app.as_weak();
    g.on_drag_to(move |x, y| {
        let Some(app) = weak.upgrade() else { return };
        let o = options(&app);
        with(|s| {
            let Some(drag) = &s.drag else { return };
            let Some(source) = s.current_item().and_then(|i| i.source.clone()) else {
                return;
            };
            let (w, h) = (f64::from(source.width), f64::from(source.height));
            let (dx, dy) = (f64::from(x) - drag.from.0, f64::from(y) - drag.from.1);
            let next = match o.shape {
                CropShape::Circle => drag_circle(drag.start, dx, dy, drag.handle, w, h),
                CropShape::Rectangle => {
                    let r = clamp_rect(resize_rect(drag.start, dx, dy, drag.handle, o.ratio), w, h);
                    match o.ratio {
                        Some(ratio) if drag.handle != CropHandle::Move => {
                            fit_to_aspect(r, ratio, w, h)
                        }
                        _ => r,
                    }
                }
            };
            s.set_current_crop(next, o);
        });
        show_crop(&app);
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_end_drag(move || {
        with(|s| s.drag = None);
        if let Some(app) = weak.upgrade() {
            show_crop(&app);
        }
    });
    let weak = app.as_weak();
    g.on_nudge(move |dx, dy| {
        let Some(app) = weak.upgrade() else { return };
        edit_current(&app, |rect, _| Rect {
            x: rect.x + f64::from(dx),
            y: rect.y + f64::from(dy),
            ..rect
        });
    });
    let weak = app.as_weak();
    g.on_field_edited(move |field, value| {
        let Some(app) = weak.upgrade() else { return };
        let v = f64::from(value);
        edit_current(&app, |rect, o| match (field, o.ratio) {
            (CropField::X, _) => Rect { x: v, ..rect },
            (CropField::Y, _) => Rect { y: v, ..rect },
            (CropField::Width, Some(ratio)) => Rect { w: v, h: v / ratio, ..rect },
            (CropField::Width, None) => Rect { w: v, ..rect },
            (CropField::Height, Some(ratio)) => Rect { w: v * ratio, h: v, ..rect },
            (CropField::Height, None) => Rect { h: v, ..rect },
        });
    });
    let weak = app.as_weak();
    g.on_shape_changed(move || {
        if let Some(app) = weak.upgrade() {
            crop_settings_changed(&app, true);
        }
    });
    let weak = app.as_weak();
    g.on_aspect_changed(move || {
        if let Some(app) = weak.upgrade() {
            crop_settings_changed(&app, false);
        }
    });
    let weak = app.as_weak();
    g.on_scope_changed(move || {
        let Some(app) = weak.upgrade() else { return };
        scope_changed(&app);
    });
    let weak = app.as_weak();
    g.on_output_settings_changed(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.mark_stale(None));
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Cropped Images To") {
            with(|s| s.queue.set_out_dir(Some(dir)));
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_reset_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.set_out_dir(None));
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_crop_all(move || {
        if let Some(app) = weak.upgrade() {
            crop_all(&app);
        }
    });
    show_current(app);
    sync(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<ImageCropper>().set_drop_hover(hover);
}

/// Adds images (from a drop, the dialog or the snapshot hook), skipping folders and
/// duplicates, and decodes the new ones in the background for their size and preview.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let added = with(|s| {
        let added = s.queue.add(paths, FileStatus::Reading, Item::default);
        for _ in &added {
            s.thumbs.push(Image::default());
        }
        if s.current.is_none() {
            s.current = added.first().map(|(id, _)| *id);
        }
        added
    });
    show_current(app);
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        let result = image::open(&path).map(|decoded| {
            let preview = image::pixel_buffer(&image::preview(&decoded.image, PREVIEW_SIDE));
            (decoded.source, preview)
        });
        jobs::post(&weak, move |app| {
            let o = options(&app);
            with(|s| {
                let Some(i) = s.position(id) else { return };
                match result {
                    Ok((source, preview)) => {
                        let preview = Image::from_rgba8(preview);
                        s.thumbs.set_row_data(i, preview.clone());
                        s.queue.update(id, |e| {
                            e.data.crop = Some(default_for(&source, o));
                            e.data.source = Some(source);
                            e.data.preview = Some(preview);
                            e.status = FileStatus::Ready;
                        });
                    }
                    Err(e) => s.queue.update(id, |entry| {
                        entry.status = FileStatus::Failed;
                        entry.message = e.to_string();
                    }),
                }
            });
            show_current(&app);
            sync(&app);
        });
    });
}

/// Cancels running crops and hands back their workers, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

/// Applies `edit` to the current image's crop, then keeps the result in bounds and in ratio.
fn edit_current(app: &AppWindow, edit: impl FnOnce(Rect, Options) -> Rect) {
    let o = options(app);
    with(|s| {
        let Some(item) = s.current_item() else { return };
        let (Some(rect), Some(source)) = (s.crop_of(item, o), item.source.clone()) else {
            return;
        };
        let (w, h) = (f64::from(source.width), f64::from(source.height));
        let mut next = clamp_rect(edit(rect, o), w, h);
        if let Some(ratio) = o.ratio {
            next = fit_to_aspect(next, ratio, w, h);
        }
        s.set_current_crop(next, o);
    });
    show_crop(app);
    sync(app);
}

/// The shape or aspect ratio changed. A new shape starts every crop afresh, as on the website;
/// a new ratio refits each crop inside itself.
fn crop_settings_changed(app: &AppWindow, shape_changed: bool) {
    let o = options(app);
    with(|s| {
        let ids: Vec<u64> = s.queue.entries().iter().map(|e| e.id).collect();
        for id in ids {
            s.queue.update(id, |e| {
                let Some(source) = &e.data.source else { return };
                let (w, h) = (f64::from(source.width), f64::from(source.height));
                e.data.crop = Some(match (shape_changed, e.data.crop, o.ratio) {
                    (false, Some(crop), Some(ratio)) => fit_to_aspect(crop, ratio, w, h),
                    (false, Some(crop), None) => crop,
                    _ => default_for(source, o),
                });
            });
        }
        let current = s.current_item().and_then(|item| {
            let source = item.source.as_ref()?;
            Some((item.crop?, f64::from(source.width), f64::from(source.height)))
        });
        s.shared = match (s.shared, current) {
            (Some(rel), Some((_, w, h))) if !shape_changed => {
                Some(to_relative(from_relative(rel, w, h, o.ratio), w, h))
            }
            (_, Some((crop, w, h))) if s.shared.is_some() => Some(to_relative(crop, w, h)),
            (shared, _) => shared,
        };
        s.mark_stale(None);
    });
    show_crop(app);
    sync(app);
}

/// Switching to "All images" shares the current image's crop; switching back gives every image
/// the shared crop as its own, so nothing jumps.
fn scope_changed(app: &AppWindow) {
    let o = options(app);
    with(|s| {
        if o.shared {
            let current = s.current_item().and_then(|item| {
                let source = item.source.as_ref()?;
                Some(to_relative(
                    s.crop_of(item, Options { shared: false, ..o })?,
                    f64::from(source.width),
                    f64::from(source.height),
                ))
            });
            s.shared = current.or(s.shared);
        } else if let Some(rel) = s.shared {
            let ids: Vec<u64> = s.queue.entries().iter().map(|e| e.id).collect();
            for id in ids {
                s.queue.update(id, |e| {
                    if let Some(source) = &e.data.source {
                        let (w, h) = (f64::from(source.width), f64::from(source.height));
                        e.data.crop = Some(from_relative(rel, w, h, o.ratio));
                    }
                });
            }
        }
        s.mark_stale(None);
    });
    show_crop(app);
    sync(app);
}

/// Shows the current image in the editor.
fn show_current(app: &AppWindow) {
    let g = app.global::<ImageCropper>();
    with(|s| {
        let i = s.current.and_then(|id| s.position(id));
        g.set_current(i.map_or(-1, |i| i as i32));
        let item = s.current_item();
        let source = item.and_then(|item| item.source.as_ref());
        g.set_preview(item.and_then(|i| i.preview.clone()).unwrap_or_default());
        g.set_source_width(source.map_or(0.0, |s| s.width as f32));
        g.set_source_height(source.map_or(0.0, |s| s.height as f32));
    });
    show_crop(app);
}

/// Shows the current image's crop, rounded to whole pixels.
fn show_crop(app: &AppWindow) {
    let g = app.global::<ImageCropper>();
    let o = options(app);
    let rect = with(|s| s.current_item().and_then(|item| s.crop_of(item, o)));
    g.set_editable(rect.is_some());
    let rect = rect.unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    });
    g.set_crop_x(rect.x.round() as f32);
    g.set_crop_y(rect.y.round() as f32);
    g.set_crop_w(rect.w.round() as f32);
    g.set_crop_h(rect.h.round() as f32);
}

/// One image's share of a batch.
#[derive(Clone, Copy)]
struct Work {
    rect: (u32, u32, u32, u32),
    shape: CropShape,
    format: Format,
    quality: u8,
}

fn crop_file(
    input: &Path,
    work: Work,
    out_dir: Option<&Path>,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<Reserved, JobError> {
    let failed = |e: anyhow::Error| {
        if cancel.is_cancelled() {
            JobError::Cancelled
        } else {
            JobError::failed(e)
        }
    };
    let decoded = image::open(input).map_err(failed)?;
    if cancel.is_cancelled() {
        return Err(JobError::Cancelled);
    }
    progress(0.5);
    let cut = crop(&decoded.image, work.rect, work.shape);
    let output = reserve_output(input, out_dir, work.format.extension())
        .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
    image::save(&cut, work.format, work.quality, output.path()).map_err(failed)?;
    Ok(output)
}

fn crop_all(app: &AppWindow) {
    let g = app.global::<ImageCropper>();
    let o = options(app);
    let chosen = match index(g.get_format_index(), Format::ALL.len() + 1) {
        0 => None,
        i => Some(Format::ALL[i - 1]),
    };
    let quality = g.get_quality().round().clamp(1.0, 100.0) as u8;
    let (jobs, out_dir) = with(|s| {
        let shared = s.shared;
        let jobs = s.queue.start(|e| {
            let source = e.data.source.as_ref()?;
            let (w, h) = (f64::from(source.width), f64::from(source.height));
            let rect = match (o.shared, shared) {
                (true, Some(rel)) => from_relative(rel, w, h, o.ratio),
                _ => e.data.crop?,
            };
            Some(Work {
                rect: pixels(rect, source.width, source.height),
                shape: o.shape,
                format: output_format(chosen, source.format, o.shape),
                quality,
            })
        });
        (jobs, s.queue.out_dir().map(PathBuf::from))
    });
    sync(app);
    let workers = file_queue::run(app, jobs, CONCURRENT_CROPS, apply, move |job, progress| {
        crop_file(&job.input, job.work, out_dir.as_deref(), &job.cancel, progress)
    });
    with(|s| s.queue.track(workers));
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
}

/// Recomputes the list-level and format-dependent properties.
fn sync(app: &AppWindow) {
    let g = app.global::<ImageCropper>();
    let chosen = match index(g.get_format_index(), Format::ALL.len() + 1) {
        0 => None,
        i => Some(Format::ALL[i - 1]),
    };
    let circle = g.get_shape() == CropShape::Circle;
    // "Same as original" may meet a JPEG, WebP or AVIF source, so its quality can apply too.
    g.set_quality_applies(chosen.is_none_or(Format::is_lossy));
    let note = match chosen {
        Some(Format::Jpeg) if circle => "JPEG has no transparency, so the corners turn white.",
        Some(format) => format.note(),
        None if circle => "Circles from JPEG and HEIC images are saved as PNG, for their transparent corners.",
        None => "HEIC images are saved as PNG.",
    };
    g.set_format_note(note.into());
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        g.set_running(running);
        g.set_can_crop(!running && q.can_start(|e| e.data.source.is_some()));
        g.set_list_summary(q.count_label("image", "images").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{GenericImageView, RgbaImage};

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn the_default_crop_is_the_centred_70_percent() {
        assert_eq!(default_crop(1000.0, 800.0), rect(150.0, 120.0, 700.0, 560.0));
    }

    #[test]
    fn the_default_circle_has_a_radius_of_30_percent_of_the_shorter_side() {
        assert_eq!(default_circle(1000.0, 800.0), rect(260.0, 160.0, 480.0, 480.0));
    }

    #[test]
    fn clamping_keeps_the_crop_inside_and_at_least_20_px() {
        assert_eq!(
            clamp_rect(rect(-30.0, 790.0, 5.0, 50.0), 1000.0, 800.0),
            rect(0.0, 750.0, 20.0, 50.0)
        );
    }

    #[test]
    fn an_image_smaller_than_the_minimum_is_cropped_whole() {
        assert_eq!(
            clamp_rect(rect(0.0, 0.0, 5.0, 5.0), 12.0, 8.0),
            rect(0.0, 0.0, 12.0, 8.0)
        );
    }

    #[test]
    fn a_preset_fits_the_largest_rectangle_of_its_ratio_inside_the_crop() {
        let fitted = fit_to_aspect(rect(150.0, 120.0, 700.0, 560.0), 16.0 / 9.0, 1000.0, 800.0);

        assert_eq!(fitted, rect(150.0, 203.0, 700.0, 394.0));
    }

    #[test]
    fn a_free_corner_drag_moves_two_edges() {
        let start = rect(100.0, 100.0, 200.0, 100.0);

        let next = resize_rect(start, -10.0, 20.0, CropHandle::Sw, None);

        assert_eq!(next, rect(90.0, 100.0, 210.0, 120.0));
    }

    #[test]
    fn a_locked_edge_drag_recentres_the_other_axis() {
        let start = rect(100.0, 100.0, 200.0, 100.0);

        let next = resize_rect(start, 40.0, 0.0, CropHandle::E, Some(2.0));

        assert_eq!(next, rect(100.0, 90.0, 240.0, 120.0));
    }

    #[test]
    fn a_locked_corner_drag_follows_the_larger_movement() {
        let start = rect(100.0, 100.0, 200.0, 100.0);

        let next = resize_rect(start, -40.0, -10.0, CropHandle::Nw, Some(2.0));

        assert_eq!(next, rect(60.0, 80.0, 240.0, 120.0));
    }

    #[test]
    fn a_circle_handle_changes_the_radius_around_the_centre() {
        let start = rect(400.0, 300.0, 200.0, 200.0);

        let next = drag_circle(start, 50.0, 0.0, CropHandle::E, 1000.0, 800.0);

        assert_eq!(next, rect(350.0, 250.0, 300.0, 300.0));
    }

    #[test]
    fn a_circle_never_outgrows_the_shorter_side() {
        let start = rect(400.0, 300.0, 200.0, 200.0);

        let next = drag_circle(start, 0.0, 900.0, CropHandle::S, 1000.0, 800.0);

        assert_eq!(next, rect(100.0, 0.0, 800.0, 800.0));
    }

    #[test]
    fn a_shared_crop_lands_on_the_same_part_of_a_larger_image() {
        let rel = to_relative(rect(100.0, 50.0, 400.0, 300.0), 1000.0, 500.0);

        assert_eq!(from_relative(rel, 2000.0, 1000.0, None), rect(200.0, 100.0, 800.0, 600.0));
    }

    #[test]
    fn a_shared_crop_keeps_its_ratio_on_an_image_of_another_shape() {
        let rel = to_relative(rect(100.0, 100.0, 400.0, 400.0), 1000.0, 1000.0);

        let placed = from_relative(rel, 2000.0, 1000.0, Some(1.0));

        assert_eq!(placed, rect(400.0, 100.0, 400.0, 400.0));
    }

    #[test]
    fn pixels_round_and_stay_inside_the_image() {
        assert_eq!(pixels(rect(10.4, 20.6, 99.7, 500.0), 100, 300), (10, 21, 90, 279));
    }

    #[test]
    fn a_circle_crop_has_transparent_corners_and_an_opaque_centre() {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(40, 40, ::image::Rgba([9, 9, 9, 255])));

        let cut = crop(&image, (0, 0, 40, 40), CropShape::Circle);

        assert_eq!(cut.get_pixel(0, 0).0[3], 0);
        assert_eq!(cut.get_pixel(20, 20).0[3], 255);
    }

    #[test]
    fn same_as_original_keeps_the_source_format() {
        assert_eq!(
            output_format(None, Some(Format::Jpeg), CropShape::Rectangle),
            Format::Jpeg
        );
    }

    #[test]
    fn a_circle_from_a_jpeg_becomes_a_png() {
        assert_eq!(output_format(None, Some(Format::Jpeg), CropShape::Circle), Format::Png);
    }

    #[test]
    fn a_heic_source_is_saved_as_png() {
        assert_eq!(output_format(None, None, CropShape::Rectangle), Format::Png);
    }

    #[test]
    fn cropping_a_file_writes_the_crop_beside_it() {
        let dir = std::env::temp_dir().join(format!("open-tools-crop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("photo.png");
        DynamicImage::ImageRgba8(RgbaImage::new(64, 48)).save(&input).unwrap();
        let work = Work {
            rect: (8, 4, 32, 24),
            shape: CropShape::Rectangle,
            format: Format::Png,
            quality: 85,
        };

        let output = crop_file(&input, work, None, &CancelToken::default(), &|_| {})
            .unwrap()
            .keep()
            .unwrap();

        assert_eq!(output, dir.join("photo (1).png"));
        assert_eq!(::image::image_dimensions(&output).unwrap(), (32, 24));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
