//! PDF ↔ Image Converter: the website's two-tab converter, which duplicated Convert to PDF and
//! Convert from PDF. Here it is those two tools behind a direction switch
//! (ui/tools/pdf-converter.slint shows their views), so there is one implementation of each
//! direction and both entries share their files. A drop goes to the side that takes it.

use std::path::PathBuf;
use std::thread::JoinHandle;

use slint::ComponentHandle;

use super::pdf_toolkit::is_pdf;
use super::{convert_from_pdf, convert_to_pdf};
use crate::{AppWindow, PdfConverter};

pub const SLUG: &str = "pdf-converter";
const TO_PDF: i32 = 0;
const FROM_PDF: i32 = 1;

/// Both directions are wired by their own tools' `init`.
pub fn init(_app: &AppWindow) {}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    convert_to_pdf::set_drop_hover(app, hover);
    convert_from_pdf::set_drop_hover(app, hover);
}

/// PDFs go to Convert from PDF and images to Convert to PDF; the view follows the last kind
/// dropped.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let (pdfs, images): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| is_pdf(p));
    let g = app.global::<PdfConverter>();
    if !images.is_empty() {
        convert_to_pdf::add_paths(app, images);
        g.set_direction(TO_PDF);
    }
    if !pdfs.is_empty() {
        convert_from_pdf::add_paths(app, pdfs);
        g.set_direction(FROM_PDF);
    }
}

/// Its work belongs to the two tools it shows, which shut it down.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}
