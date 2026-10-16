//! Gradient Maker: build a linear, radial or conic gradient from stops, as the website did, and
//! copy it as CSS or Tailwind or export a 1600 × 900 PNG. Changes from the website:
//! - The preview and the PNG come from one renderer (engine::color::render) on a worker, so the
//!   preview is what exports, and the PNG keeps the radial shape and position (the website's
//!   was always a centred circle).
//! - A linear angle is any whole degree, not only the eight directions.
//! - Stops reorder with Move Up and Move Down instead of dragging, and each stop's colour has a
//!   picker beside its text field, which checks what's typed.
//!
//! The stops live in the Slint model, which the colour fields and sliders edit in place; Rust
//! reads it back to build the CSS.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread::JoinHandle;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::engine::color::{self, Gradient, Kind, Latest};
use crate::{AppWindow, GradientKind, GradientMaker, MakerStop, clipboard, files, jobs};

pub const SLUG: &str = "gradient-maker";
/// The website's radial shapes, in its order.
const SHAPES: [&str; 6] = [
    "circle at center",
    "circle at top",
    "circle at bottom right",
    "ellipse at center",
    "ellipse at top left",
    "ellipse at bottom",
];
const DEFAULT_STOPS: [(&str, f64); 3] = [("#2563EB", 0.0), ("#8B5CF6", 50.0), ("#EC4899", 100.0)];
/// The website's colour for a stop added to an empty list.
const NEW_STOP: &str = "#5B8CFF";
/// The website's file name for the export.
const EXPORT_NAME: &str = "gradient-maker-preview.png";

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct Stop {
    color: String,
    /// 0–100.
    position: f64,
}

/// What the controls describe, besides the stops.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shape {
    kind: Kind,
    angle: f64,
    radial: &'static str,
    from: f64,
}

/// JavaScript's `%` on angles, wrapped into 0–360.
fn normalize_angle(deg: f64) -> f64 {
    let n = deg % 360.0;
    if n < 0.0 { n + 360.0 } else { n }
}

/// The website's `buildGradientCss`: stops in list order (not sorted), each `<colour> <n>%`.
fn build_css(shape: Shape, stops: &[Stop]) -> String {
    let list: Vec<String> = stops
        .iter()
        .map(|s| format!("{} {}%", s.color.trim(), s.position.clamp(0.0, 100.0)))
        .collect();
    let list = list.join(", ");
    match shape.kind {
        Kind::Radial => format!("radial-gradient({}, {list})", shape.radial),
        Kind::Conic => format!(
            "conic-gradient(from {}deg at center, {list})",
            normalize_angle(shape.from)
        ),
        Kind::Linear => format!(
            "linear-gradient({}deg, {list})",
            normalize_angle(shape.angle)
        ),
    }
}

/// The website's `addStop`: the last colour again, halfway between the last two positions (or
/// 20 past a lone stop, at most 100).
fn added_stop(stops: &[Stop]) -> Stop {
    match stops {
        [] => Stop {
            color: NEW_STOP.into(),
            position: 0.0,
        },
        [.., last] => {
            let position = match stops {
                [.., previous, _] => ((previous.position + last.position) / 2.0 + 0.5).floor(),
                _ => (last.position + 20.0).min(100.0),
            };
            Stop {
                color: last.color.to_uppercase(),
                position,
            }
        }
    }
}

/// The controls and stops that rebuild `g`, for editing a library gradient. A conic start is
/// carried as a linear angle, the way the website converts between the two.
fn from_gradient(g: &Gradient) -> Option<(Shape, Vec<Stop>)> {
    let angle = match g.kind {
        Kind::Conic => Gradient::parse(&g.convert(Kind::Linear))?.orientation,
        _ => g.orientation.clone(),
    };
    let degrees = normalize_angle(color::linear_angle_degrees(&angle));
    let shape = Shape {
        kind: g.kind,
        angle: if g.kind == Kind::Linear { degrees } else { 135.0 },
        radial: SHAPES
            .iter()
            .find(|s| **s == g.orientation)
            .unwrap_or(&SHAPES[0]),
        from: if g.kind == Kind::Conic { degrees } else { 90.0 },
    };
    let stops = color::stops(g)
        .ok()?
        .into_iter()
        .map(|(c, at)| Stop {
            color: color::hex(c),
            position: (at * 100.0).round(),
        })
        .collect();
    Some((shape, stops))
}

/// The first stop whose colour can't be read, as the sentence to show.
fn unreadable(stops: &[Stop]) -> Option<String> {
    stops.iter().enumerate().find_map(|(i, s)| {
        color::parse(&s.color).is_none().then(|| {
            format!(
                "Stop {}: couldn't read “{}” as a colour.",
                i + 1,
                s.color.trim()
            )
        })
    })
}

// ---- State and wiring ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    /// The stops as the view edits them.
    stops: Rc<VecModel<MakerStop>>,
    preview: Latest,
    /// Counts preview requests, so a slow render never replaces a newer one.
    generation: u64,
    exports: Vec<JoinHandle<()>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

fn row(stop: &Stop) -> MakerStop {
    let parsed = color::parse(&stop.color);
    MakerStop {
        text: stop.color.as_str().into(),
        color: parsed.map_or(slint::Color::default(), |(_, c)| color::slint_color(c)),
        invalid: parsed.is_none(),
        position: stop.position as f32,
    }
}

fn read_stops() -> Vec<Stop> {
    with(|s| {
        s.stops
            .iter()
            .map(|r| Stop {
                color: r.text.to_string(),
                position: f64::from(r.position),
            })
            .collect()
    })
}

fn kind_of(kind: GradientKind) -> Kind {
    match kind {
        GradientKind::Linear => Kind::Linear,
        GradientKind::Radial => Kind::Radial,
        GradientKind::Conic => Kind::Conic,
    }
}

fn read_shape(app: &AppWindow) -> Shape {
    let g = app.global::<GradientMaker>();
    Shape {
        kind: kind_of(g.get_kind()),
        angle: f64::from(g.get_angle()),
        radial: SHAPES[crate::select::index(g.get_shape(), SHAPES.len())],
        from: f64::from(g.get_conic_from()),
    }
}

pub fn init(app: &AppWindow) {
    let g = app.global::<GradientMaker>();
    g.set_shapes(crate::select::options(SHAPES.into_iter()));
    let model = with(|s| s.stops.clone());
    g.set_stops(ModelRc::from(model.clone()));
    model.set_vec(
        DEFAULT_STOPS
            .iter()
            .map(|(c, p)| {
                row(&Stop {
                    color: (*c).into(),
                    position: *p,
                })
            })
            .collect::<Vec<_>>(),
    );

    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_stop_edited(move |i| {
        let Some(app) = weak.upgrade() else { return };
        update_row(i, |r| {
            let text = r.text.to_string();
            *r = row(&Stop {
                color: text,
                position: f64::from(r.position),
            });
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_stop_picked(move |i, c| {
        let Some(app) = weak.upgrade() else { return };
        update_row(i, |r| {
            *r = row(&Stop {
                color: color::hex(color::from_slint(c)),
                position: f64::from(r.position),
            });
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_move_stop(move |from, to| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
                return;
            };
            if let (Some(a), Some(b)) = (s.stops.row_data(from), s.stops.row_data(to)) {
                s.stops.set_row_data(from, b);
                s.stops.set_row_data(to, a);
            }
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_remove_stop(move |i| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| {
            if let Ok(i) = usize::try_from(i)
                && i < s.stops.row_count()
                && s.stops.row_count() > 2
            {
                s.stops.remove(i);
            }
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_add_stop(move || {
        let Some(app) = weak.upgrade() else { return };
        let next = row(&added_stop(&read_stops()));
        with(|s| s.stops.push(next));
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy(move |text| {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        match clipboard::copy_text(&text) {
            Ok(()) => true,
            Err(e) => {
                app.global::<GradientMaker>()
                    .set_error(format!("Couldn't copy: {e}").into());
                false
            }
        }
    });
    let weak = app.as_weak();
    g.on_export(move || {
        if let Some(app) = weak.upgrade() {
            export(&app);
        }
    });
    refresh(app);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// The preview thread finishes its render and ends; exports still writing finish too.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        let mut threads = s.preview.shutdown();
        threads.append(&mut s.exports);
        threads
    })
}

/// Loads `g` for editing (Gradient Library's Edit), replacing the current gradient.
pub fn edit(app: &AppWindow, g: &Gradient) {
    let Some((shape, stops)) = from_gradient(g) else {
        return;
    };
    let global = app.global::<GradientMaker>();
    global.set_kind(match shape.kind {
        Kind::Linear => GradientKind::Linear,
        Kind::Radial => GradientKind::Radial,
        Kind::Conic => GradientKind::Conic,
    });
    global.set_angle(shape.angle as f32);
    global.set_conic_from(shape.from as f32);
    let shape_index = SHAPES.iter().position(|s| *s == shape.radial).unwrap_or(0);
    global.set_shape(shape_index as i32);
    global.set_note("".into());
    with(|s| s.stops.set_vec(stops.iter().map(row).collect::<Vec<_>>()));
    refresh(app);
}

fn update_row(i: i32, f: impl FnOnce(&mut MakerStop)) {
    with(|s| {
        let Ok(i) = usize::try_from(i) else { return };
        if let Some(mut r) = s.stops.row_data(i) {
            f(&mut r);
            s.stops.set_row_data(i, r);
        }
    });
}

/// Rebuilds the CSS from the controls and stops, and asks for a new preview.
fn refresh(app: &AppWindow) {
    let g = app.global::<GradientMaker>();
    let stops = read_stops();
    let css = build_css(read_shape(app), &stops);
    g.set_gradient(css.as_str().into());
    g.set_background(color::background_property(&css).into());
    g.set_tailwind(color::tailwind_arbitrary(&css).into());
    let problem = unreadable(&stops);
    g.set_can_export(problem.is_none());
    g.set_error(problem.clone().unwrap_or_default().into());
    let (Some(gradient), None) = (Gradient::parse(&css), problem) else {
        return;
    };
    let weak = app.as_weak();
    with(|s| {
        s.generation += 1;
        let id = s.generation;
        s.preview.preview(gradient, move |result| {
            jobs::post(&weak, move |app| {
                if with(|s| s.generation) != id {
                    return;
                }
                let g = app.global::<GradientMaker>();
                match result {
                    Ok(pixels) => g.set_preview(slint::Image::from_rgba8_premultiplied(pixels)),
                    Err(e) => g.set_error(e.into()),
                }
            });
        });
    });
}

fn export(app: &AppWindow) {
    let Some(gradient) = Gradient::parse(&app.global::<GradientMaker>().get_gradient()) else {
        return;
    };
    let Some(path) = files::choose_save_path("Export PNG", EXPORT_NAME) else {
        return;
    };
    let g = app.global::<GradientMaker>();
    g.set_note(format!("Exporting {}…", files::file_name(&path)).into());
    let weak = app.as_weak();
    let handle = color::export_png(gradient, path, move |result| {
        jobs::post(&weak, move |app| {
            let g = app.global::<GradientMaker>();
            match result {
                Ok(note) => g.set_note(note.into()),
                Err(error) => g.set_error(error.into()),
            }
        });
    });
    with(|s| {
        s.exports.retain(|h| !h.is_finished());
        s.exports.push(handle);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(color: &str, position: f64) -> Stop {
        Stop {
            color: color.into(),
            position,
        }
    }

    fn defaults() -> Vec<Stop> {
        DEFAULT_STOPS.iter().map(|(c, p)| stop(c, *p)).collect()
    }

    fn shape(kind: Kind) -> Shape {
        Shape {
            kind,
            angle: 135.0,
            radial: SHAPES[0],
            from: 90.0,
        }
    }

    // Expected strings come from running the website's buildGradientCss and addStop.

    #[test]
    fn build_css_linear_wraps_the_angle() {
        let s = Shape {
            angle: -45.0,
            ..shape(Kind::Linear)
        };

        assert_eq!(
            build_css(s, &defaults()),
            "linear-gradient(315deg, #2563EB 0%, #8B5CF6 50%, #EC4899 100%)"
        );
    }

    #[test]
    fn build_css_radial_uses_the_shape() {
        let s = Shape {
            radial: SHAPES[4],
            ..shape(Kind::Radial)
        };

        assert_eq!(
            build_css(s, &defaults()),
            "radial-gradient(ellipse at top left, #2563EB 0%, #8B5CF6 50%, #EC4899 100%)"
        );
    }

    #[test]
    fn build_css_conic_wraps_the_start() {
        let s = Shape {
            from: 400.0,
            ..shape(Kind::Conic)
        };

        assert_eq!(
            build_css(s, &defaults()),
            "conic-gradient(from 40deg at center, #2563EB 0%, #8B5CF6 50%, #EC4899 100%)"
        );
    }

    #[test]
    fn build_css_trims_colours_and_keeps_list_order() {
        let stops = [stop(" #EC4899 ", 100.0), stop("red", 12.5)];

        assert_eq!(
            build_css(shape(Kind::Linear), &stops),
            "linear-gradient(135deg, #EC4899 100%, red 12.5%)"
        );
    }

    #[test]
    fn added_stop_goes_halfway_between_the_last_two() {
        assert_eq!(added_stop(&defaults()), stop("#EC4899", 75.0));
    }

    #[test]
    fn added_stop_rounds_half_up() {
        assert_eq!(
            added_stop(&[stop("#fff", 10.0), stop("#000", 15.0)]),
            stop("#000", 13.0)
        );
    }

    #[test]
    fn added_stop_after_a_lone_stop_is_20_further_at_most_100() {
        assert_eq!(added_stop(&[stop("#fff", 95.0)]), stop("#FFF", 100.0));
    }

    #[test]
    fn added_stop_to_nothing_is_the_website_blue() {
        assert_eq!(added_stop(&[]), stop("#5B8CFF", 0.0));
    }

    #[test]
    fn from_gradient_reads_a_library_preset() {
        let g = Gradient::parse("linear-gradient(to right, #0f2027, #203a43, #2c5364)").unwrap();

        let (s, stops) = from_gradient(&g).unwrap();

        assert_eq!(s.angle, 90.0);
        assert_eq!(
            stops,
            [stop("#0F2027", 0.0), stop("#203A43", 50.0), stop("#2C5364", 100.0)]
        );
    }

    #[test]
    fn from_gradient_carries_a_conic_start() {
        let g = Gradient::parse("conic-gradient(from 45deg at center, red, blue)").unwrap();

        let (s, _) = from_gradient(&g).unwrap();

        assert_eq!(s.from, 45.0);
    }

    #[test]
    fn from_gradient_keeps_a_known_radial_shape() {
        let g = Gradient::parse("radial-gradient(ellipse at bottom, red, blue)").unwrap();

        let (s, _) = from_gradient(&g).unwrap();

        assert_eq!(s.radial, "ellipse at bottom");
    }

    #[test]
    fn unreadable_names_the_stop_and_its_text() {
        assert_eq!(
            unreadable(&[stop("#fff", 0.0), stop(" #zz ", 100.0)]),
            Some("Stop 2: couldn't read “#zz” as a colour.".into())
        );
    }
}
