//! Every tool's module, and the only way the shell reaches them. Each tool module has the same
//! interface, so a family edits only its own tool files:
//!
//! - `pub const SLUG: &str`: the tool's slug in tools.json.
//! - `pub fn init(app: &AppWindow)`: wires the tool's Slint global; called once at start.
//! - `pub fn set_drop_hover(app: &AppWindow, hover: bool)`: files are being dragged over the
//!   window while this tool shows.
//! - `pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>)`: files were dropped (one batch per
//!   drop), or preloaded by the snapshot hook (`OFFKIT_FILES`).
//! - `pub fn shutdown() -> Vec<JoinHandle<()>>`: cancels background work and hands back its
//!   threads; called once the window closes. Every tool cancels first, then all are joined
//!   together, so no partial output outlives the app.
//!
//! A tool that takes no files does nothing in `set_drop_hover` and `add_paths`: no drop
//! highlight, and a drop is ignored. Only the tool showing gets drops.

use std::path::PathBuf;

use crate::{AppWindow, file_queue};

macro_rules! tools {
    ($($tool:ident,)*) => {
        $(pub mod $tool;)*

        /// Every tool's module name and slug, for the tests.
        #[cfg(test)]
        const MODULES: &[(&str, &str)] = &[$((stringify!($tool), self::$tool::SLUG)),*];

        pub fn init_all(app: &AppWindow) {
            $(self::$tool::init(app);)*
        }

        /// Cancels every tool's background work, then waits for all of it.
        pub fn shutdown_all() {
            let workers = [$(self::$tool::shutdown()),*];
            file_queue::join(workers.into_iter().flatten().collect());
        }

        /// Routes a drag over the window to the tool with `slug`.
        pub fn set_drop_hover(app: &AppWindow, slug: &str, hover: bool) {
            match slug {
                $(self::$tool::SLUG => self::$tool::set_drop_hover(app, hover),)*
                _ => {}
            }
        }

        /// Routes dropped or preloaded files to the tool with `slug`.
        pub fn add_paths(app: &AppWindow, slug: &str, paths: Vec<PathBuf>) {
            match slug {
                $(self::$tool::SLUG => self::$tool::add_paths(app, paths),)*
                _ => {}
            }
        }
    };
}

tools! {
    // audio
    audio_converter,
    audio_extractor,
    audio_editor,
    audio_joiner,

    // video
    video_converter,
    frames_extractor,
    video_editor,
    video_merger,

    // images
    image_converter,
    image_cropper,
    palette_extractor,

    // pdf
    pdf_toolkit,
    pdf_converter,
    pdf_merger,
    pdf_splitter,
    pdf_compressor,
    convert_to_pdf,
    convert_from_pdf,

    // svg
    svg_optimizer,
    svg_pattern,
    svg_animator,

    // colors
    color_converter,
    gradient_maker,
    gradient_library,
    gradient_converter,
    palette_library,

    // css
    blob_generator,
    glassmorphism,
    box_shadow,

    // text
    base64,
    lorem_generator,
    diff_checker,
    json_formatter,
    md_editor,

    // dev
    directory_scaffold,
    archive_extractor,
    archiver,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn every_registry_tool_has_exactly_one_module() {
        let registry = Registry::load().unwrap();

        let modules: HashSet<_> = MODULES.iter().map(|(_, slug)| *slug).collect();

        let slugs: HashSet<_> = registry
            .tools
            .iter()
            .filter(|t| t.on_desktop())
            .map(|t| t.slug.as_str())
            .collect();
        assert_eq!(modules, slugs);
        assert_eq!(MODULES.len(), slugs.len());
    }

    #[test]
    fn every_module_is_named_after_its_slug() {
        let misnamed: Vec<_> = MODULES
            .iter()
            .filter(|(module, slug)| *module != slug.replace('-', "_"))
            .collect();

        assert!(misnamed.is_empty(), "{misnamed:?}");
    }

    #[test]
    fn every_tool_has_a_view_file() {
        let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/tools");

        let missing: Vec<_> = MODULES
            .iter()
            .filter(|(_, slug)| !ui.join(format!("{slug}.slint")).is_file())
            .collect();

        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn app_slint_imports_and_shows_every_tool_exactly_once() {
        let app_slint = include_str!("../../ui/app.slint");
        let once = |pattern: String| app_slint.matches(&pattern).count() == 1;

        let unwired: Vec<_> = MODULES
            .iter()
            .filter(|(_, slug)| {
                !once(format!("from \"tools/{slug}.slint\";"))
                    || !once(format!("if root.current-slug == \"{slug}\":"))
            })
            .collect();

        assert!(unwired.is_empty(), "{unwired:?}");
    }
}
