// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine {
    pub mod ffmpeg;
}
mod jobs;
mod registry;
mod tools {
    pub mod video_converter;
}

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::winit::event::WindowEvent;
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::engine::ffmpeg::Ffmpeg;
use crate::registry::Registry;
use crate::tools::video_converter;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    // The winit backend is required: file drops and the Mica/vibrancy backdrop go through it.
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;

    let registry = Rc::new(Registry::load()?);
    let app = AppWindow::new()?;
    app.window()
        .set_size(slint::LogicalSize::new(1180.0, 760.0));
    app.set_app_version(env!("CARGO_PKG_VERSION").into());

    let rows = Rc::new(VecModel::from(sidebar_rows(&registry, "")));
    app.set_sidebar_rows(ModelRc::from(rows.clone()));
    app.on_search_changed({
        let (registry, rows) = (registry.clone(), rows.clone());
        move |query| rows.set_vec(sidebar_rows(&registry, &query))
    });
    app.on_select_adjacent({
        let (registry, weak) = (registry.clone(), app.as_weak());
        move |delta| {
            let Some(app) = weak.upgrade() else { return };
            let slugs: Vec<_> = rows.iter().filter(|r| !r.header).map(|r| r.slug).collect();
            let current = slugs.iter().position(|s| *s == app.get_current_slug());
            let next = match current {
                Some(i) => i
                    .saturating_add_signed(delta as isize)
                    .min(slugs.len().saturating_sub(1)),
                None => 0,
            };
            if let Some(slug) = slugs.get(next) {
                select_tool(&app, &registry, slug);
            }
        }
    });
    app.on_select_tool({
        let (registry, weak) = (registry.clone(), app.as_weak());
        move |slug| {
            if let Some(app) = weak.upgrade() {
                select_tool(&app, &registry, &slug);
            }
        }
    });

    video_converter::init(&app, Ffmpeg::locate());
    install_file_drop(&app);

    let snapshot = std::env::var_os("OPEN_TOOLS_SNAPSHOT").map(PathBuf::from);
    let tool = std::env::var("OPEN_TOOLS_TOOL").unwrap_or_else(|_| video_converter::SLUG.into());
    select_tool(&app, &registry, video_converter::SLUG);
    if tool != video_converter::SLUG {
        // Selecting once the sidebar has laid out lets it scroll the row into view.
        let (registry, weak) = (registry.clone(), app.as_weak());
        slint::Timer::single_shot(Duration::from_millis(100), move || {
            if let Some(app) = weak.upgrade() {
                select_tool(&app, &registry, &tool);
            }
        });
    }
    match &snapshot {
        Some(path) => prepare_snapshot(&app, path.clone()),
        None => apply_backdrop(&app),
    }

    app.run()?;
    video_converter::shutdown();
    Ok(())
}

fn select_tool(app: &AppWindow, registry: &Registry, slug: &str) {
    if let Some(tool) = registry.tool(slug) {
        app.set_current_slug(tool.slug.as_str().into());
        app.set_current_name(tool.name.as_str().into());
        app.set_current_description(tool.description.as_str().into());
    }
}

/// Category headers followed by their tools, in registry order. Categories with no tool
/// matching `query` (case-insensitive, on the tool name) are left out.
fn sidebar_rows(registry: &Registry, query: &str) -> Vec<SidebarRow> {
    let query = query.trim().to_lowercase();
    registry
        .categories
        .iter()
        .flat_map(|category| {
            let tools: Vec<SidebarRow> = registry
                .tools
                .iter()
                .filter(|t| t.category == category.name && t.name.to_lowercase().contains(&query))
                .map(|t| SidebarRow {
                    header: false,
                    label: t.name.as_str().into(),
                    slug: t.slug.as_str().into(),
                    icon: t.icon.as_str().into(),
                })
                .collect();
            let header = SidebarRow {
                header: true,
                label: category.name.as_str().into(),
                ..Default::default()
            };
            (!tools.is_empty())
                .then_some(header)
                .into_iter()
                .chain(tools)
        })
        .collect()
}

/// Files dragged in from the OS file manager. winit sends one `DroppedFile` per file, so the
/// paths are collected and handed over as one batch on the next event-loop turn: one probe
/// pool for the whole drop, not one per file.
fn install_file_drop(app: &AppWindow) {
    let weak = app.as_weak();
    let pending: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    app.window().on_winit_window_event(move |_, event| {
        let Some(app) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        if app.get_current_slug() != video_converter::SLUG {
            return EventResult::Propagate;
        }
        match event {
            WindowEvent::HoveredFile(_) => video_converter::set_drop_hover(&app, true),
            WindowEvent::HoveredFileCancelled => video_converter::set_drop_hover(&app, false),
            WindowEvent::DroppedFile(path) => {
                video_converter::set_drop_hover(&app, false);
                let mut batch = pending.borrow_mut();
                if batch.is_empty() {
                    let (pending, weak) = (pending.clone(), weak.clone());
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        let paths = pending.take();
                        if let Some(app) = weak.upgrade() {
                            video_converter::add_paths(&app, paths);
                        }
                    });
                }
                batch.push(path.clone());
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

/// Mica on Windows 11, the sidebar material on macOS. The UI keeps its opaque look when the
/// platform refuses (Windows 10, Linux), because `Backdrop.vibrancy` stays false.
fn apply_backdrop(app: &AppWindow) {
    let weak = app.as_weak();
    let task = slint::spawn_local(async move {
        let Some(app) = weak.upgrade() else { return };
        let Ok(window) = app.window().winit_window().await else {
            return;
        };
        #[cfg(windows)]
        let applied = window_vibrancy::apply_mica(&*window, None).is_ok();
        #[cfg(target_os = "macos")]
        let applied = window_vibrancy::apply_vibrancy(
            &*window,
            window_vibrancy::NSVisualEffectMaterial::Sidebar,
            None,
            None,
        )
        .is_ok();
        #[cfg(not(any(windows, target_os = "macos")))]
        let applied = {
            let _ = window;
            false
        };
        app.global::<Backdrop>().set_vibrancy(applied);
    });
    if let Err(e) = task {
        eprintln!("couldn't schedule the window backdrop: {e}");
    }
}

// ---- Verification hook --------------------------------------------------------------------
// OPEN_TOOLS_SNAPSHOT=<png> renders the window to a PNG ~1.5 s after start, then exits.
// Optional: OPEN_TOOLS_TOOL=<slug>, OPEN_TOOLS_THEME=light|dark,
// OPEN_TOOLS_FILES=<path;path> (preloaded into the Video Converter) and OPEN_TOOLS_CONVERT=1
// (start converting once probes finish), OPEN_TOOLS_ABOUT=1 (open the About sheet).
// Lets an agent check the UI without a human.

fn prepare_snapshot(app: &AppWindow, path: PathBuf) {
    if let Ok(theme) = std::env::var("OPEN_TOOLS_THEME") {
        use slint::language::ColorScheme;
        let scheme = if theme == "dark" {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        app.global::<Palette>().set_color_scheme(scheme);
    }
    if let Some(files) = std::env::var_os("OPEN_TOOLS_FILES") {
        video_converter::add_paths(app, std::env::split_paths(&files).collect());
    }
    if std::env::var_os("OPEN_TOOLS_CONVERT").is_some() {
        video_converter::convert_when_ready(app);
    }
    app.set_about_open(std::env::var_os("OPEN_TOOLS_ABOUT").is_some());
    // Count the delay from the first event-loop turn, not from before window creation.
    let weak = app.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        slint::Timer::single_shot(Duration::from_millis(1500), move || {
            if let Some(app) = weak.upgrade()
                && let Err(e) = write_snapshot(&app, &path)
            {
                eprintln!("snapshot failed: {e:#}");
            }
            let _ = slint::quit_event_loop();
        });
    });
}

fn write_snapshot(app: &AppWindow, path: &Path) -> anyhow::Result<()> {
    let buffer = app.window().take_snapshot()?;
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, buffer.width(), buffer.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()?
        .write_image_data(buffer.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_groups_by_category_and_filters() {
        let registry = Registry::load().unwrap();
        let all = sidebar_rows(&registry, "");
        assert_eq!(all.iter().filter(|r| r.header).count(), 9);
        assert_eq!(all.iter().filter(|r| !r.header).count(), 37);

        let found = sidebar_rows(&registry, "  VIDEO conv ");
        let labels: Vec<_> = found.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Video", "Video Converter"]);
        assert!(sidebar_rows(&registry, "no such tool").is_empty());
    }
}
