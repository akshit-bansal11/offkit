//! JSON Formatter: pretty-prints (2 spaces, 4 spaces or tabs) or minifies JSON, optionally with
//! every object's keys sorted, live as you type. Invalid JSON is named by line and column. Key
//! order is kept otherwise (serde_json's `preserve_order`). A big input formats in the background
//! (`text::Latest`), so a 10 MB file never stalls typing, and the output pane shows a preview
//! while Copy and Save use the whole output. The view is ui/tools/json-formatter.slint.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use serde::Serialize;
use serde_json::Value;
use serde_json::ser::{PrettyFormatter, Serializer};
use slint::ComponentHandle;

use crate::engine::text::{self, Latest};
use crate::files::{self, format_size};
use crate::{AppWindow, JsonFormatter, JsonIndent, JsonStyle, clipboard};

pub const SLUG: &str = "json-formatter";
const TOOL: &str = "JSON Formatter";
/// The website's sample, shown on first open and by Sample.
const SAMPLE: &str = r#"{
  "name": "offkit",
  "tool": "json-formatter",
  "features": ["pretty print", "minify", "validate"],
  "localFirst": true
}"#;

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Options {
    /// `None` minifies; otherwise the text of one indent level.
    indent: Option<&'static str>,
    sort_keys: bool,
}

#[derive(Debug, Default, PartialEq)]
enum Output {
    #[default]
    Empty,
    Json(String),
    Invalid(String),
}

fn format(input: &str, options: Options) -> Output {
    if input.trim().is_empty() {
        return Output::Empty;
    }
    let mut value: Value = match serde_json::from_str(input) {
        Ok(value) => value,
        Err(e) => return Output::Invalid(describe(&e, input)),
    };
    if options.sort_keys {
        value.sort_all_objects();
    }
    let Some(indent) = options.indent else {
        return Output::Json(value.to_string());
    };
    let mut out = Vec::new();
    let mut serializer =
        Serializer::with_formatter(&mut out, PrettyFormatter::with_indent(indent.as_bytes()));
    match value.serialize(&mut serializer) {
        // serde_json writes UTF-8 only.
        Ok(()) => Output::Json(String::from_utf8_lossy(&out).into_owned()),
        Err(e) => Output::Invalid(format!("Couldn't format it: {e}")),
    }
}

/// "Line 3, column 5: expected `,` or `}`." serde_json counts columns in bytes, so the column is
/// recounted in characters, the way an editor shows it.
fn describe(e: &serde_json::Error, input: &str) -> String {
    let (line, byte_column) = (e.line(), e.column());
    let full = e.to_string();
    let message = full
        .strip_suffix(&format!(" at line {line} column {byte_column}"))
        .unwrap_or(&full);
    let text = input.split('\n').nth(line.saturating_sub(1)).unwrap_or("");
    let column = text[..text.floor_char_boundary(byte_column)]
        .chars()
        .count();
    format!("Line {line}, column {column}: {message}.")
}

fn options(g: &JsonFormatter) -> Options {
    let indent = match (g.get_style(), g.get_indent()) {
        (JsonStyle::Minify, _) => None,
        (JsonStyle::Pretty, JsonIndent::Two) => Some("  "),
        (JsonStyle::Pretty, JsonIndent::Four) => Some("    "),
        (JsonStyle::Pretty, JsonIndent::Tab) => Some("\t"),
    };
    Options {
        indent,
        sort_keys: g.get_sort_keys(),
    }
}

// ---- State and wiring ---------------------------------------------------------------------

/// A file too long for the input pane, held here instead.
struct InputFile {
    name: String,
    text: Arc<str>,
}

#[derive(Default)]
struct State {
    file: Option<InputFile>,
    output: Output,
    formatting: Latest,
    typing: slint::Timer,
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
    let g = app.global::<JsonFormatter>();
    g.set_input(SAMPLE.into());
    let weak = app.as_weak();
    g.on_edited(move || {
        if let Some(app) = weak.upgrade() {
            with(|s| text::debounce(&s.typing, &app, refresh));
        }
    });
    let weak = app.as_weak();
    g.on_options_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_open_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Open JSON", Some(("JSON", &["json"]))) {
            open_file(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_remove_file(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.file = None);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_sample(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.file = None);
        app.global::<JsonFormatter>().set_input(SAMPLE.into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.file = None);
        app.global::<JsonFormatter>().set_input("".into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy_output(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let copied = with(|s| match &s.output {
            Output::Json(json) => Some(clipboard::copy_text(json)),
            _ => None,
        });
        match copied {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                app.global::<JsonFormatter>()
                    .set_error(format!("Couldn't copy: {e}").into());
                false
            }
            None => false,
        }
    });
    let weak = app.as_weak();
    g.on_save_output(move || {
        if let Some(app) = weak.upgrade() {
            save_output(&app);
        }
    });
    refresh(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<JsonFormatter>().set_drop_hover(hover);
}

/// A dropped file becomes the input; one at a time.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) => open_file(app, path),
        Err(paths) if paths.is_empty() => {}
        Err(_) => app
            .global::<JsonFormatter>()
            .set_error("Drop one file at a time.".into()),
    }
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.formatting.shutdown())
}

fn open_file(app: &AppWindow, path: PathBuf) {
    let g = app.global::<JsonFormatter>();
    let file = match text::read_text_file(&path, TOOL) {
        Ok(file) => file,
        Err(message) => return g.set_error(message.into()),
    };
    if text::fits_pane(&file.text) {
        with(|s| s.file = None);
        g.set_input(file.text.into());
    } else {
        g.set_file_label(format!("{} · {}", file.name, format_size(file.text.len() as u64)).into());
        with(|s| {
            s.file = Some(InputFile {
                name: file.name,
                text: file.text.into(),
            })
        });
    }
    refresh(app);
}

fn save_output(app: &AppWindow) {
    let g = app.global::<JsonFormatter>();
    let suggested = with(|s| match (&s.output, &s.file) {
        (Output::Json(_), Some(file)) => Some(file.name.clone()),
        (Output::Json(_), None) if g.get_style() == JsonStyle::Minify => {
            Some("minified.json".to_string())
        }
        (Output::Json(_), None) => Some("formatted.json".into()),
        _ => None,
    });
    let Some(suggested) = suggested else { return };
    let Some(path) = files::choose_save_path("Save JSON", &suggested) else {
        return;
    };
    let saved = with(|s| match &s.output {
        Output::Json(json) => text::save_text(&path, json),
        _ => Err("Nothing to save: the JSON changed while the dialog was open.".into()),
    });
    match saved {
        Ok(note) => g.set_note(note.into()),
        Err(error) => g.set_error(error.into()),
    }
}

/// Formats the current input in the background, then shows the result.
fn refresh(app: &AppWindow) {
    let g = app.global::<JsonFormatter>();
    let options = options(&g);
    let input: Arc<str> = with(|s| match &s.file {
        Some(file) => file.text.clone(),
        None => g.get_input().as_str().into(),
    });
    with(|s| {
        if s.file.is_none() {
            g.set_file_label("".into());
        }
    });
    g.set_stats(text::stats(&input).into());
    let formatting = with(|s| s.formatting.clone());
    formatting.run(app, input.len(), move || format(&input, options), show);
}

fn show(app: &AppWindow, output: Output) {
    let g = app.global::<JsonFormatter>();
    with(|s| {
        s.output = output;
        let (shown, error, note) = match &s.output {
            Output::Json(json) => {
                let shown = text::preview(json);
                let note = text::preview_note(shown, json);
                let note = if note.is_empty() {
                    "Valid JSON.".to_string()
                } else {
                    note
                };
                (shown, "", note)
            }
            Output::Invalid(message) => ("", message.as_str(), String::new()),
            Output::Empty => ("", "", String::new()),
        };
        let has_json = matches!(s.output, Output::Json(_));
        g.set_output(shown.into());
        g.set_error(error.into());
        g.set_note(note.into());
        g.set_can_copy(has_json);
        g.set_can_save(has_json);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRETTY_2: Options = Options {
        indent: Some("  "),
        sort_keys: false,
    };

    fn json(text: &str) -> Output {
        Output::Json(text.into())
    }

    #[test]
    fn pretty_indents_by_two_spaces() {
        assert_eq!(
            format(r#"{"a":[1,2]}"#, PRETTY_2),
            json("{\n  \"a\": [\n    1,\n    2\n  ]\n}")
        );
    }

    #[test]
    fn pretty_indents_by_four_spaces() {
        let options = Options {
            indent: Some("    "),
            ..PRETTY_2
        };

        assert_eq!(format(r#"{"a":1}"#, options), json("{\n    \"a\": 1\n}"));
    }

    #[test]
    fn pretty_indents_by_tabs() {
        let options = Options {
            indent: Some("\t"),
            ..PRETTY_2
        };

        assert_eq!(format(r#"{"a":1}"#, options), json("{\n\t\"a\": 1\n}"));
    }

    #[test]
    fn minify_drops_every_space_outside_strings() {
        let options = Options {
            indent: None,
            ..PRETTY_2
        };

        assert_eq!(
            format("{\n  \"a b\": [1, 2],\n  \"c\": null\n}", options),
            json(r#"{"a b":[1,2],"c":null}"#)
        );
    }

    #[test]
    fn key_order_is_kept_when_not_sorting() {
        let options = Options {
            indent: None,
            ..PRETTY_2
        };

        assert_eq!(
            format(r#"{"zebra":1,"apple":2,"mango":3}"#, options),
            json(r#"{"zebra":1,"apple":2,"mango":3}"#)
        );
    }

    #[test]
    fn sorting_orders_keys_at_every_depth() {
        let options = Options {
            indent: None,
            sort_keys: true,
        };

        assert_eq!(
            format(r#"{"b":{"y":1,"x":2},"a":[{"d":1,"c":2}]}"#, options),
            json(r#"{"a":[{"c":2,"d":1}],"b":{"x":2,"y":1}}"#)
        );
    }

    #[test]
    fn non_ascii_and_slashes_stay_unescaped() {
        let options = Options {
            indent: None,
            ..PRETTY_2
        };

        assert_eq!(
            format(r#"{"url":"a/b","w":"héllo"}"#, options),
            json(r#"{"url":"a/b","w":"héllo"}"#)
        );
    }

    #[test]
    fn errors_name_the_line_and_column() {
        assert_eq!(
            format("{\n  \"a\": 1\n  \"b\": 2\n}", PRETTY_2),
            Output::Invalid("Line 3, column 3: expected `,` or `}`.".into())
        );
    }

    #[test]
    fn error_columns_count_characters_not_bytes() {
        assert_eq!(
            format(r#"{"é": x}"#, PRETTY_2),
            Output::Invalid("Line 1, column 7: expected value.".into())
        );
    }

    #[test]
    fn a_trailing_comma_is_invalid() {
        assert_eq!(
            format("[1, 2,]", PRETTY_2),
            Output::Invalid("Line 1, column 7: trailing comma.".into())
        );
    }

    #[test]
    fn top_level_scalars_are_valid() {
        assert_eq!(format(" 42 ", PRETTY_2), json("42"));
    }

    #[test]
    fn blank_input_is_empty() {
        assert_eq!(format(" \n\t", PRETTY_2), Output::Empty);
    }
}
