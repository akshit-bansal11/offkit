//! Image Converter: batch-converts images to PNG, JPEG, WebP (lossy), AVIF, TIFF, GIF, BMP or
//! ICO, with a quality setting for the lossy formats and an optional fit-within resize. Wires
//! the `ImageConverter` global (ui/tools/image-converter.slint) to `engine::image`, on the
//! Video Converter's file-queue pattern: headers are read in the background when files are
//! added, conversions run on `file_queue::run`.
//!
//! Unlike the website there are no size caps beyond the decoder's safety limits, and one set
//! of settings applies to the whole batch.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use slint::ComponentHandle;

use crate::engine::image::{self, Format, INPUT_EXTENSIONS, MAX_SIDE, Source};
use crate::file_queue::{self, FileQueue, Summary, Update};
use crate::files::{self, reserve_output};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, FileStatus, ImageConverter};

pub const SLUG: &str = "image-converter";
/// Encoding is single-threaded per image (AVIF aside), so a few run side by side.
const CONCURRENT_CONVERSIONS: usize = 3;
const CONCURRENT_PROBES: usize = 4;

/// A row's header: `None` until it's read, and for files that aren't images.
type Probe = Option<Source>;

impl Summary for Probe {
    fn summary(&self) -> String {
        self.as_ref().map(Source::summary).unwrap_or_default()
    }
}

#[derive(Default)]
struct State {
    queue: FileQueue<Probe>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

/// What every image in a batch is converted with.
#[derive(Clone, Copy)]
struct Settings {
    format: Format,
    quality: u8,
    /// Fit within this width and height, when set.
    fit: Option<(u32, u32)>,
}

pub fn init(app: &AppWindow) {
    let g = app.global::<ImageConverter>();
    g.set_formats_hint(image::FORMATS_HINT.into());
    g.set_formats(select::options(Format::ALL.iter().map(|f| f.label())));
    g.set_max_side(MAX_SIDE as f32);
    with(|s| g.set_files(s.queue.rows()));

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Images", Some(("Image", INPUT_EXTENSIONS)))
        {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.queue.remove(i as usize));
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
    g.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save Converted Images To") {
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
    g.on_convert_all(move || {
        if let Some(app) = weak.upgrade() {
            convert_all(&app);
        }
    });
    sync(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<ImageConverter>().set_drop_hover(hover);
}

/// Adds files (from a drop, the dialog or the snapshot hook), skipping folders and
/// duplicates, and reads the new ones' headers in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let added = with(|s| s.queue.add(paths, FileStatus::Reading, || None));
    sync(app);

    let weak = app.as_weak();
    jobs::run_pool(added, CONCURRENT_PROBES, move |(id, path)| {
        let result = image::probe(&path).map_err(|e| e.to_string());
        jobs::post(&weak, move |app| {
            with(|s| {
                s.queue.update(id, |e| match result {
                    Ok(source) => {
                        e.data = Some(source);
                        e.status = FileStatus::Ready;
                    }
                    Err(message) => {
                        e.status = FileStatus::Failed;
                        e.message = message;
                    }
                })
            });
            sync(&app);
        });
    });
}

/// Cancels running conversions and hands back their workers, for `tools::shutdown_all` to join.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.queue.shutdown())
}

fn settings(app: &AppWindow) -> Settings {
    let g = app.global::<ImageConverter>();
    let side = |v: f32| v.round().clamp(1.0, MAX_SIDE as f32) as u32;
    Settings {
        format: Format::ALL[index(g.get_format_index(), Format::ALL.len())],
        quality: g.get_quality().round().clamp(1.0, 100.0) as u8,
        fit: g
            .get_resize()
            .then(|| (side(g.get_max_width()), side(g.get_max_height()))),
    }
}

/// Converts one image: decode (upright, within the safety limits), fit, encode.
fn convert(
    input: &std::path::Path,
    settings: Settings,
    out_dir: Option<&std::path::Path>,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<files::Reserved, JobError> {
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
    let picture = match settings.fit {
        Some((w, h)) => image::fit_within(decoded.image, w, h),
        None => decoded.image,
    };
    let output = reserve_output(input, out_dir, settings.format.extension())
        .map_err(|e| JobError::Failed(format!("Couldn't create the output file: {e}")))?;
    image::save(&picture, settings.format, settings.quality, output.path()).map_err(failed)?;
    Ok(output)
}

fn convert_all(app: &AppWindow) {
    let settings = settings(app);
    let (jobs, out_dir) = with(|s| {
        let jobs = s.queue.start(|e| e.data.as_ref().map(|_| ()));
        (jobs, s.queue.out_dir().map(PathBuf::from))
    });
    sync(app);
    let workers = file_queue::run(
        app,
        jobs,
        CONCURRENT_CONVERSIONS,
        apply,
        move |job, progress| {
            convert(
                &job.input,
                settings,
                out_dir.as_deref(),
                &job.cancel,
                progress,
            )
        },
    );
    with(|s| s.queue.track(workers));
}

fn apply(app: &AppWindow, id: u64, token: &Arc<CancelToken>, update: Update) {
    with(|s| s.queue.apply(id, token, update));
    sync(app);
}

/// Recomputes the list-level and format-dependent properties.
fn sync(app: &AppWindow) {
    let g = app.global::<ImageConverter>();
    let format = Format::ALL[index(g.get_format_index(), Format::ALL.len())];
    g.set_quality_applies(format.is_lossy());
    g.set_format_note(format.note().into());
    with(|s| {
        let q = &s.queue;
        let running = q.is_running();
        g.set_running(running);
        g.set_can_convert(!running && q.can_start(|e| e.data.is_some()));
        g.set_list_summary(q.count_label("image", "images").into());
        g.set_custom_output(q.out_dir().is_some());
        g.set_output_folder(q.out_dir_label().into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, RgbaImage};
    use std::fs;

    fn temp_dir(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("open-tools-convert-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn photo(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("photo.png");
        DynamicImage::ImageRgba8(RgbaImage::new(400, 300))
            .save(&path)
            .unwrap();
        path
    }

    #[test]
    fn a_conversion_fits_the_image_and_saves_beside_the_source() {
        let dir = temp_dir("fit");
        let input = photo(&dir);
        let settings = Settings {
            format: Format::WebP,
            quality: 80,
            fit: Some((100, 100)),
        };

        let output = convert(&input, settings, None, &CancelToken::default(), &|_| {})
            .unwrap()
            .keep()
            .unwrap();

        assert_eq!(output, dir.join("photo.webp"));
        assert_eq!(::image::image_dimensions(&output).unwrap(), (100, 75));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancelled_conversion_leaves_no_output() {
        let dir = temp_dir("cancel");
        let input = photo(&dir);
        let cancel = CancelToken::default();
        cancel.cancel();
        let settings = Settings {
            format: Format::Png,
            quality: 80,
            fit: None,
        };

        let result = convert(&input, settings, None, &cancel, &|_| {});

        assert!(matches!(result, Err(JobError::Cancelled)));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
