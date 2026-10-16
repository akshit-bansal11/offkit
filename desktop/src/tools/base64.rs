//! Base64: text or a file to Base64 and back, standard or URL-safe. The reference editor-style
//! tool: pure logic first (`encode`, `decode`, `convert`, tested without a window), then the
//! `Base64` global (ui/tools/base64.slint) wired to it. Everything runs on the UI thread, which
//! the file size cap keeps short. Laying out megabytes of text is what's slow, so the output
//! pane shows a `preview` while Copy and Save use the whole output.

use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

use base64::alphabet;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::{DecodeError, Engine};
use slint::ComponentHandle;

use crate::files::{self, format_size};
use crate::{AppWindow, Base64, Base64Mode, clipboard};

pub const SLUG: &str = "base64";
/// Keeps the whole output (a third larger than the file, when encoding) cheap to hold and copy.
const MAX_FILE_BYTES: u64 = 10_000_000;
/// The most output text the pane shows. Measured in a debug build: 1.3 MB of Base64 took about
/// 11 s to lay out, so the pane stays well under that.
const PREVIEW_BYTES: usize = 50_000;

// ---- Logic --------------------------------------------------------------------------------

/// Decoding accepts what people paste: padded or not, and stray bits after the last byte.
const LENIENT: GeneralPurposeConfig = GeneralPurposeConfig::new()
    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
    .with_decode_allow_trailing_bits(true);
const DECODE_STANDARD: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, LENIENT);
const DECODE_URL_SAFE: GeneralPurpose = GeneralPurpose::new(&alphabet::URL_SAFE, LENIENT);

/// Standard Base64 with `=` padding, or the URL-safe alphabet (`-` and `_`) without padding,
/// as it appears in URLs and tokens.
fn encode(bytes: &[u8], url_safe: bool) -> String {
    if url_safe {
        URL_SAFE_NO_PAD.encode(bytes)
    } else {
        STANDARD.encode(bytes)
    }
}

/// Decodes either alphabet, ignoring whitespace (line-wrapped Base64 from mail or PEM files).
/// The error is the sentence a person sees.
fn decode(text: &[u8]) -> Result<Vec<u8>, String> {
    let compact: Vec<u8> = text
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    let engine = if compact.iter().any(|b| matches!(b, b'-' | b'_')) {
        &DECODE_URL_SAFE
    } else {
        &DECODE_STANDARD
    };
    engine.decode(&compact).map_err(|e| match e {
        DecodeError::InvalidByte(_, b'=') | DecodeError::InvalidPadding => {
            padding_error(&compact).into()
        }
        // The reported offset can point into a multi-byte character, so look for it instead.
        DecodeError::InvalidByte(..) => match String::from_utf8_lossy(&compact)
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || "+/-_=".contains(*c)))
        {
            Some(c) => format!("Not valid Base64: “{c}” isn't a Base64 character."),
            None => "Not valid Base64: it mixes “+ /” and “- _”, which belong to different \
                     Base64 alphabets."
                .into(),
        },
        DecodeError::InvalidLength(_) | DecodeError::InvalidLastSymbol { .. } => {
            "Not valid Base64: it ends partway through, so part of it is missing.".into()
        }
    })
}

/// Why the `=` padding is wrong: in the middle of the text, or the wrong amount at its end.
fn padding_error(compact: &[u8]) -> &'static str {
    let first = compact
        .iter()
        .position(|b| *b == b'=')
        .unwrap_or(compact.len());
    if compact[first..].iter().all(|b| *b == b'=') {
        "Not valid Base64: it ends with the wrong number of “=”."
    } else {
        "Not valid Base64: “=” can only come at the very end."
    }
}

/// What the output pane holds.
#[derive(Debug, Default, PartialEq)]
enum Output {
    #[default]
    Empty,
    Text(String),
    /// Decoded bytes that aren't text: they can only be saved.
    Binary(Vec<u8>),
    Invalid(String),
}

fn convert(input: &[u8], decoding: bool, url_safe: bool) -> Output {
    if input.is_empty() {
        return Output::Empty;
    }
    if !decoding {
        return Output::Text(encode(input, url_safe));
    }
    match decode(input) {
        Ok(bytes) if bytes.is_empty() => Output::Empty,
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Output::Text(text),
            Err(e) => Output::Binary(e.into_bytes()),
        },
        Err(message) => Output::Invalid(message),
    }
}

/// The start of `text` that the output pane shows, cut on a character boundary.
fn preview(text: &str) -> &str {
    &text[..text.floor_char_boundary(PREVIEW_BYTES)]
}

// ---- State and wiring ---------------------------------------------------------------------

struct InputFile {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct State {
    /// A file opened or dropped in place of typed text.
    file: Option<InputFile>,
    output: Output,
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
    let g = app.global::<Base64>();
    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_open_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = files::choose_file("Choose a File", None) {
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
    g.on_swap(move || {
        let Some(app) = weak.upgrade() else { return };
        let Some(text) = with(|s| match &s.output {
            Output::Text(text) => Some(text.clone()),
            _ => None,
        }) else {
            return;
        };
        let g = app.global::<Base64>();
        with(|s| s.file = None);
        g.set_input(text.into());
        g.set_mode(match g.get_mode() {
            Base64Mode::Encode => Base64Mode::Decode,
            Base64Mode::Decode => Base64Mode::Encode,
        });
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.file = None);
        app.global::<Base64>().set_input("".into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy_output(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let result = with(|s| match &s.output {
            Output::Text(text) => Some(clipboard::copy_text(text)),
            _ => None,
        });
        match result {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                let g = app.global::<Base64>();
                g.set_error(format!("Couldn't copy: {e}").into());
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
    app.global::<Base64>().set_drop_hover(hover);
}

/// A dropped file becomes the input; Base64 takes one file at a time.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) => open_file(app, path),
        Err(paths) if paths.is_empty() => {}
        Err(_) => app
            .global::<Base64>()
            .set_error("Drop one file at a time.".into()),
    }
}

/// Nothing runs in the background.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}

fn open_file(app: &AppWindow, path: PathBuf) {
    let g = app.global::<Base64>();
    let name = files::file_name(&path);
    let size = match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(_) => return g.set_error(format!("{name} is a folder. Choose a file.").into()),
        Err(e) => return g.set_error(format!("Couldn't open {name}: {e}").into()),
    };
    if size > MAX_FILE_BYTES {
        let message = format!(
            "{name} is {}. Base64 takes files up to {}.",
            format_size(size),
            format_size(MAX_FILE_BYTES)
        );
        return g.set_error(message.into());
    }
    match std::fs::read(&path) {
        Ok(bytes) => {
            g.set_file_label(format!("{name} · {}", format_size(size)).into());
            with(|s| s.file = Some(InputFile { name, bytes }));
            refresh(app);
        }
        Err(e) => g.set_error(format!("Couldn't read {name}: {e}").into()),
    }
}

fn save_output(app: &AppWindow) {
    let g = app.global::<Base64>();
    let suggested = with(|s| match (&s.output, &s.file) {
        (Output::Binary(_), _) => Some("decoded.bin".to_string()),
        (Output::Text(_), _) if g.get_mode() == Base64Mode::Decode => Some("decoded.txt".into()),
        (Output::Text(_), Some(file)) => Some(format!("{}.txt", file.name)),
        (Output::Text(_), None) => Some("encoded.txt".into()),
        _ => None,
    });
    let Some(suggested) = suggested else { return };
    let Some(path) = files::choose_save_path("Save Output", &suggested) else {
        return;
    };
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        with(|s| match &s.output {
            Output::Text(text) => std::fs::write(output.path(), text),
            Output::Binary(bytes) => std::fs::write(output.path(), bytes),
            Output::Empty | Output::Invalid(_) => Ok(()),
        })?;
        output.keep()
    });
    let name = files::file_name(&path);
    match result {
        Ok(_) => g.set_note(format!("Saved as {name}").into()),
        Err(e) => g.set_error(format!("Couldn't save {name}: {e}").into()),
    }
}

/// Converts the current input and shows the result.
fn refresh(app: &AppWindow) {
    let g = app.global::<Base64>();
    let typed = g.get_input();
    let decoding = g.get_mode() == Base64Mode::Decode;
    let url_safe = g.get_url_safe();
    with(|s| {
        let input = s.file.as_ref().map_or(typed.as_bytes(), |f| &f.bytes);
        s.output = convert(input, decoding, url_safe);
        if s.file.is_none() {
            g.set_file_label("".into());
        }
        let (text, error, placeholder) = match &s.output {
            Output::Text(text) => (preview(text), "", String::new()),
            Output::Invalid(message) => ("", message.as_str(), String::new()),
            Output::Binary(bytes) => (
                "",
                "",
                format!(
                    "Binary data · {}, not text. Save it to a file to use it.",
                    format_size(bytes.len() as u64)
                ),
            ),
            Output::Empty if decoding => ("", "", "The decoded text appears here.".into()),
            Output::Empty => ("", "", "Base64 appears here.".into()),
        };
        let note = match &s.output {
            Output::Text(full) if text.len() < full.len() => format!(
                "Showing the first {} of {}. Copy and Save use all of it.",
                format_size(text.len() as u64),
                format_size(full.len() as u64)
            ),
            _ => String::new(),
        };
        let has_text = matches!(s.output, Output::Text(_));
        // Swapping a cut output would put megabytes into the input pane.
        g.set_can_swap(has_text && note.is_empty());
        g.set_output(text.into());
        g.set_error(error.into());
        g.set_output_placeholder(placeholder.into());
        g.set_note(note.into());
        g.set_can_copy(has_text);
        g.set_can_save(has_text || matches!(s.output, Output::Binary(_)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_pads_the_standard_alphabet() {
        assert_eq!(encode(b"hi?>", false), "aGk/Pg==");
    }

    #[test]
    fn encode_url_safe_swaps_symbols_and_drops_padding() {
        assert_eq!(encode(b"hi?>", true), "aGk_Pg");
    }

    #[test]
    fn decode_accepts_either_alphabet_with_or_without_padding() {
        let hi = Ok(b"hi?>".to_vec());

        assert_eq!(decode(b"aGk/Pg=="), hi);
        assert_eq!(decode(b"aGk/Pg"), hi);
        assert_eq!(decode(b"aGk_Pg"), hi);
        assert_eq!(decode(b"aGk_Pg=="), hi);
    }

    #[test]
    fn decode_ignores_line_breaks_and_spaces() {
        assert_eq!(
            decode(b"aGVs\r\nbG8g\n d29y bGQ="),
            Ok(b"hello world".to_vec())
        );
    }

    #[test]
    fn decode_names_a_character_outside_the_alphabet() {
        assert_eq!(
            decode("aGVsbG8é".as_bytes()),
            Err("Not valid Base64: “é” isn't a Base64 character.".into())
        );
    }

    #[test]
    fn decode_explains_mixed_alphabets() {
        assert_eq!(
            decode(b"aGk+Pg-_"),
            Err(
                "Not valid Base64: it mixes “+ /” and “- _”, which belong to different \
                 Base64 alphabets."
                    .into()
            )
        );
    }

    #[test]
    fn decode_accepts_partial_padding() {
        assert_eq!(decode(b"QQ="), Ok(b"A".to_vec()));
    }

    #[test]
    fn decode_explains_padding_after_a_lone_character() {
        assert_eq!(
            decode(b"a==="),
            Err("Not valid Base64: it ends with the wrong number of “=”.".into())
        );
    }

    #[test]
    fn decode_explains_too_much_padding() {
        assert_eq!(
            decode(b"aGk==="),
            Err("Not valid Base64: it ends with the wrong number of “=”.".into())
        );
    }

    #[test]
    fn decode_explains_padding_in_the_middle() {
        assert_eq!(
            decode(b"aGk=aGk="),
            Err("Not valid Base64: “=” can only come at the very end.".into())
        );
    }

    #[test]
    fn decode_explains_a_truncated_input() {
        assert_eq!(
            decode(b"aGVsb"),
            Err("Not valid Base64: it ends partway through, so part of it is missing.".into())
        );
    }

    #[test]
    fn convert_decodes_text_as_text() {
        assert_eq!(
            convert(b"8J+RiyBoaQ==", true, false),
            Output::Text("👋 hi".into())
        );
    }

    #[test]
    fn convert_keeps_decoded_binary_as_bytes() {
        assert_eq!(
            convert(b"iVBORw0KGgo=", true, false),
            Output::Binary(vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'])
        );
    }

    #[test]
    fn preview_keeps_short_text_whole() {
        assert_eq!(preview("aGk="), "aGk=");
    }

    #[test]
    fn preview_cuts_long_text_on_a_character_boundary() {
        let text = format!("{}é", "a".repeat(PREVIEW_BYTES - 1));

        let shown = preview(&text);

        assert_eq!(shown.len(), PREVIEW_BYTES - 1);
    }

    #[test]
    fn convert_of_nothing_is_empty_both_ways() {
        assert_eq!(convert(b"", false, false), Output::Empty);
        assert_eq!(convert(b" \n", true, false), Output::Empty);
    }
}
