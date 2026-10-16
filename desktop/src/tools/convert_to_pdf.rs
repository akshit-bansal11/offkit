//! Convert to PDF: images in order, made into one PDF with a page per image (through a save
//! dialog), or a PDF per image, on a chosen page size with margins (`engine::pdf::images_to_pdf`;
//! JPEGs go in without re-encoding). Images only: Markdown to PDF stays on the website. Wires
//! the `ConvertToPdf` global (ui/tools/convert-to-pdf.slint). Each image is read once on a
//! worker as it's added, for its size and its thumbnail.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use ::image::RgbaImage;
use slint::{ComponentHandle, Image, ModelRc, VecModel};

use super::pdf_toolkit::{Outcome, Task, TaskUpdate, count, get_pdfium, slint_image};
use crate::engine::pdf::{self, Layout, Orientation, PageSize};
use crate::files::{self, file_name, format_size, reserve_output, same_file};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, ConvertToPdf, PdfListItem};

pub const SLUG: &str = "convert-to-pdf";
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];
const CONCURRENT_READS: usize = 4;
/// Row thumbnails render at twice the row's thumbnail square.
const THUMB_EDGE: u32 = 72;
/// Margins in points: none, a quarter, half and one inch.
const MARGINS: [(&str, f32); 4] = [
    ("None", 0.0),
    ("Small", 18.0),
    ("Medium", 36.0),
    ("Large", 72.0),
];
const ONE_PDF: i32 = 0;

/// Whether a dropped path names an image this tool takes, by extension; it's decoded later.
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

struct Item {
    id: u64,
    path: PathBuf,
    size: u64,
    /// Pixel size once read, or why it can't be used.
    read: Option<Result<(u32, u32), String>>,
    thumb: Option<Image>,
}

#[derive(Default)]
struct State {
    items: Vec<Item>,
    next_id: u64,
    rows: Rc<VecModel<PdfListItem>>,
    task: Task,
    readers: Vec<JoinHandle<()>>,
    /// Where a PDF per image goes; `None` puts each next to its image.
    out_dir: Option<PathBuf>,
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
    let g = app.global::<ConvertToPdf>();
    g.set_pdfium_found(pdf::pdfium().is_some());
    g.set_page_sizes(select::options(PageSize::ALL.iter().map(|s| s.label())));
    g.set_orientations(select::options(Orientation::ALL.iter().map(|o| o.label())));
    g.set_margins(select::options(MARGINS.iter().map(|(label, _)| *label)));
    with(|s| g.set_images(ModelRc::from(s.rows.clone())));
    sync(app);

    let weak = app.as_weak();
    g.on_choose_files(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(paths) = files::choose_files("Choose Images", Some(("Image", IMAGE_EXTENSIONS)))
        {
            add_paths(&app, paths);
        }
    });
    let weak = app.as_weak();
    g.on_move(move |i, delta| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let (i, j) = (i as usize, (i + delta) as usize);
            if !s.task.is_running() && i < s.items.len() && j < s.items.len() {
                s.items.swap(i, j);
                s.task.clear();
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_remove(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            if !s.task.is_running() && (i as usize) < s.items.len() {
                s.items.remove(i as usize);
                s.task.clear();
            }
        });
        sync(&app);
    });
    let weak = app.as_weak();
    g.on_convert(move || {
        if let Some(app) = weak.upgrade() {
            convert(&app);
        }
    });
    let weak = app.as_weak();
    g.on_cancel(move || {
        with(|s| s.task.cancel());
        if let Some(app) = weak.upgrade() {
            sync(&app);
        }
    });
    g.on_reveal(|| {
        if let Some(output) = with(|s| s.task.output()) {
            files::reveal(&output);
        }
    });
    let weak = app.as_weak();
    g.on_choose_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Save PDFs To") {
            with(|s| s.out_dir = Some(dir));
            sync(&app);
        }
    });
    let weak = app.as_weak();
    g.on_reset_output_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.out_dir = None);
        sync(&app);
    });
    g.on_get_pdfium(get_pdfium);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    let g = app.global::<ConvertToPdf>();
    g.set_drop_hover(hover && g.get_pdfium_found());
}

/// Lists the images in `paths` after the ones already there, skipping folders and duplicates,
/// and reads each in the background.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    if pdf::pdfium().is_none() {
        return;
    }
    let added = with(|s| {
        if s.task.is_running() {
            return Vec::new();
        }
        s.task.clear();
        let mut added = Vec::new();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() || s.items.iter().any(|e| same_file(&e.path, &path)) {
                continue;
            }
            if !is_image(&path) {
                s.task.show_error(format!(
                    "{} isn't an image this tool takes, so it wasn't added.",
                    file_name(&path)
                ));
                continue;
            }
            s.next_id += 1;
            s.items.push(Item {
                id: s.next_id,
                path: path.clone(),
                size: meta.len(),
                read: None,
                thumb: None,
            });
            added.push((s.next_id, path));
        }
        added
    });
    sync(app);

    let weak = app.as_weak();
    let readers = jobs::run_pool(added, CONCURRENT_READS, move |(id, path)| {
        let read = pdf::decode_image(&path).map(|image| {
            let size = (image.width(), image.height());
            (size, image.thumbnail(THUMB_EDGE, THUMB_EDGE).into_rgba8())
        });
        jobs::post(&weak, move |app| set_read(&app, id, read));
    });
    with(|s| {
        s.readers.retain(|r| !r.is_finished());
        s.readers.extend(readers);
    });
}

fn set_read(app: &AppWindow, id: u64, read: Result<((u32, u32), RgbaImage), String>) {
    with(|s| {
        if let Some(item) = s.items.iter_mut().find(|e| e.id == id) {
            match read {
                Ok((size, thumb)) => {
                    item.read = Some(Ok(size));
                    item.thumb = Some(slint_image(&thumb));
                }
                Err(message) => item.read = Some(Err(message)),
            }
        }
    });
    sync(app);
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        let mut workers = s.task.shutdown();
        workers.append(&mut s.readers);
        workers
    })
}

/// Why Create can't run yet; `None` when every image has been read.
fn blocked(items: &[Item]) -> Option<&'static str> {
    if items.is_empty() {
        return Some("Add images to convert.");
    }
    if items.iter().any(|e| e.read.is_none()) {
        return Some("Reading images…");
    }
    if items.iter().any(|e| matches!(e.read, Some(Err(_)))) {
        return Some("Remove the images that can't be read, then try again.");
    }
    None
}

fn layout(g: &ConvertToPdf) -> Layout {
    Layout {
        size: PageSize::ALL[index(g.get_page_size_index(), PageSize::ALL.len())],
        orientation: Orientation::ALL[index(g.get_orientation_index(), Orientation::ALL.len())],
        margin: MARGINS[index(g.get_margin_index(), MARGINS.len())].1,
    }
}

fn convert(app: &AppWindow) {
    let Some(pdfium) = pdf::pdfium() else { return };
    let g = app.global::<ConvertToPdf>();
    let layout = layout(&g);
    let picked = with(|s| {
        (!s.task.is_running() && blocked(&s.items).is_none()).then(|| {
            let images: Vec<PathBuf> = s.items.iter().map(|e| e.path.clone()).collect();
            (images, s.out_dir.clone())
        })
    });
    let Some((images, out_dir)) = picked else {
        return;
    };

    if g.get_output_index() == ONE_PDF {
        let stem = images[0]
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "images".into());
        let Some(target) = files::choose_save_path("Save PDF", &format!("{stem}.pdf")) else {
            return;
        };
        with(|s| {
            s.task
                .start(app, "Creating the PDF", apply, move |cancel, progress| {
                    let out = files::reserve_replacing(&target).map_err(|e| {
                        JobError::Failed(format!("Couldn't create {}: {e}", file_name(&target)))
                    })?;
                    pdf::images_to_pdf(pdfium, &images, layout, out.path(), cancel, progress)?;
                    let saved = out.keep().map_err(|e| {
                        JobError::Failed(format!("Couldn't save {}: {e}", file_name(&target)))
                    })?;
                    Ok(Outcome {
                        message: format!(
                            "Saved {} · {}",
                            file_name(&saved),
                            count(images.len(), "page", "pages")
                        ),
                        reveal: saved,
                    })
                })
        });
    } else {
        with(|s| {
            s.task
                .start(app, "Creating PDFs", apply, move |cancel, progress| {
                    let mut first = None;
                    for (i, image) in images.iter().enumerate() {
                        let out =
                            reserve_output(image, out_dir.as_deref(), "pdf").map_err(|e| {
                                JobError::Failed(format!(
                                    "Couldn't create a PDF for {}: {e}",
                                    file_name(image)
                                ))
                            })?;
                        let share = |p: f32| progress((i as f32 + p) / images.len() as f32);
                        pdf::images_to_pdf(
                            pdfium,
                            std::slice::from_ref(image),
                            layout,
                            out.path(),
                            cancel,
                            &share,
                        )
                        .map_err(|e| match e {
                            // The PDFs already made stay: each is complete on its own.
                            JobError::Failed(m) if i > 0 => JobError::Failed(format!(
                                "{m} ({} saved before it.)",
                                count(i, "PDF was", "PDFs were")
                            )),
                            e => e,
                        })?;
                        let saved = out.keep().map_err(|e| {
                            JobError::Failed(format!(
                                "Couldn't save a PDF for {}: {e}",
                                file_name(image)
                            ))
                        })?;
                        first.get_or_insert(saved);
                    }
                    let reveal = first.ok_or(JobError::Cancelled)?;
                    Ok(Outcome {
                        message: format!("Saved {}", count(images.len(), "PDF", "PDFs")),
                        reveal,
                    })
                })
        });
    }
    sync(app);
}

fn apply(app: &AppWindow, token: &Arc<CancelToken>, update: TaskUpdate) {
    with(|s| s.task.update(token, update));
    sync(app);
}

fn row(e: &Item) -> PdfListItem {
    let size = format_size(e.size);
    let (detail, failed) = match &e.read {
        None => (format!("{size} · Reading…"), false),
        Some(Ok((w, h))) => (format!("{w}×{h} · {size}"), false),
        Some(Err(message)) => (message.clone(), true),
    };
    PdfListItem {
        name: file_name(&e.path).into(),
        detail: detail.into(),
        thumb: e.thumb.clone().unwrap_or_default(),
        failed,
    }
}

fn sync(app: &AppWindow) {
    let g = app.global::<ConvertToPdf>();
    with(|s| {
        s.rows.set_vec(s.items.iter().map(row).collect::<Vec<_>>());
        let bytes: u64 = s.items.iter().map(|e| e.size).sum();
        g.set_summary(
            format!(
                "{} · {}",
                count(s.items.len(), "image", "images"),
                format_size(bytes)
            )
            .into(),
        );
        let blocked = blocked(&s.items);
        g.set_can_convert(blocked.is_none() && !s.task.is_running());
        g.set_note(blocked.unwrap_or_default().into());
        g.set_running(s.task.is_running());
        g.set_status(s.task.status());
        g.set_custom_output(s.out_dir.is_some());
        g.set_output_folder(
            s.out_dir
                .as_deref()
                .map(file_name)
                .unwrap_or_else(|| "Same as original".into())
                .into(),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(read: Option<Result<(u32, u32), String>>) -> Item {
        Item {
            id: 0,
            path: PathBuf::from("a.png"),
            size: 0,
            read,
            thumb: None,
        }
    }

    #[test]
    fn images_are_recognised_by_extension_in_any_case() {
        let names = [Path::new("a.JPG"), Path::new("b.tiff"), Path::new("c.pdf")];

        let images: Vec<bool> = names.iter().map(|p| is_image(p)).collect();

        assert_eq!(images, [true, true, false]);
    }

    #[test]
    fn creating_waits_until_every_image_is_read() {
        let items = [item(Some(Ok((10, 10)))), item(None)];

        assert_eq!(blocked(&items), Some("Reading images…"));
    }

    #[test]
    fn an_unreadable_image_blocks_creating() {
        let items = [item(Some(Err("bad".into())))];

        assert_eq!(
            blocked(&items),
            Some("Remove the images that can't be read, then try again.")
        );
    }
}
