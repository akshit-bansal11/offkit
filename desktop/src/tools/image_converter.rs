//! Image Converter: a placeholder until the images family builds it. Its Slint side is
//! ui/tools/image-converter.slint; `tools` describes the interface every tool module has.

use std::path::PathBuf;
use std::thread::JoinHandle;

use crate::AppWindow;

pub const SLUG: &str = "image-converter";

pub fn init(_app: &AppWindow) {}

/// Takes no files yet, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files yet, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Nothing runs in the background yet.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}
