//! Text family (lorem-generator, diff-checker, json-formatter, md-editor): what more than one of
//! those tools needs. Opening and saving text files, the cut-down preview a pane shows, the
//! "N characters · N lines" badge, and `Latest`, which formats or compares in the background
//! when the input is big and applies only the newest result. Owner: text family.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use slint::ComponentHandle;

use crate::files::{self, format_size};
use crate::{AppWindow, jobs};

/// The largest file a text tool opens: big enough for logs and data dumps, small enough to hold
/// twice (input and output) and to format or compare in a second or two.
pub const MAX_FILE_BYTES: u64 = 10_000_000;
/// The most text a read-only pane shows. Laying out text is what's slow (1.3 MB took about 11 s
/// in a debug build), so a longer output is cut here and Copy and Save use all of it.
const PREVIEW_BYTES: usize = 50_000;
/// TextPane refuses typed or pasted text past this many characters (ui/components/editor.slint);
/// a longer file is held aside instead of being put in an editable pane.
pub const PANE_MAX_CHARS: usize = 100_000;
/// How long typing pauses before a tool recomputes, so a burst of keystrokes costs one run.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// The start of `text` that a pane shows, cut on a character boundary.
pub fn preview(text: &str) -> &str {
    &text[..text.floor_char_boundary(PREVIEW_BYTES)]
}

/// Whether `text` fits in an editable pane.
pub fn fits_pane(text: &str) -> bool {
    text.chars().nth(PANE_MAX_CHARS).is_none()
}

/// The note that goes with a cut-down preview, or "" when `shown` is all of `full`.
pub fn preview_note(shown: &str, full: &str) -> String {
    if shown.len() < full.len() {
        format!(
            "Showing the first {} of {}. Copy and Save use all of it.",
            format_size(shown.len() as u64),
            format_size(full.len() as u64)
        )
    } else {
        String::new()
    }
}

/// "1,204 characters · 38 lines", as the website's badge counts them.
pub fn stats(text: &str) -> String {
    let characters = text.chars().count();
    let lines = if text.is_empty() {
        0
    } else {
        text.split('\n').count()
    };
    format!(
        "{} · {}",
        counted(characters, "character", "characters"),
        counted(lines, "line", "lines")
    )
}

/// "1 word", "1,234 words".
pub fn counted(n: usize, one: &str, many: &str) -> String {
    let digits = n.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {}", if n == 1 { one } else { many })
}

/// A text file read whole.
pub struct TextFile {
    pub name: String,
    pub text: String,
}

/// Reads a UTF-8 text file of at most `MAX_FILE_BYTES`, dropping a byte-order mark. The error is
/// the sentence a person sees; `tool` names who refuses a big file ("Diff Checker takes …").
pub fn read_text_file(path: &Path, tool: &str) -> Result<TextFile, String> {
    let name = files::file_name(path);
    let size = match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(_) => return Err(format!("{name} is a folder. Choose a file.")),
        Err(e) => return Err(format!("Couldn't open {name}: {e}")),
    };
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{name} is {}. {tool} takes files up to {}.",
            format_size(size),
            format_size(MAX_FILE_BYTES)
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {name}: {e}"))?;
    let Ok(mut text) = String::from_utf8(bytes) else {
        return Err(format!(
            "Couldn't open {name}: it isn't UTF-8 text. Open a text file."
        ));
    };
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(TextFile { name, text })
}

/// Writes `text` to `path`, a path from `files::choose_save_path`, through a temporary file so a
/// failed save never damages the file it replaces. Returns the note or error to show.
pub fn save_text(path: &Path, text: &str) -> Result<String, String> {
    let name = files::file_name(path);
    files::reserve_replacing(path)
        .and_then(|output| {
            std::fs::write(output.path(), text)?;
            output.keep()
        })
        .map(|_| format!("Saved as {name}"))
        .map_err(|e| format!("Couldn't save {name}: {e}"))
}

/// Calls `refresh` once typing has paused. Restarting a running `timer` postpones it.
pub fn debounce(timer: &slint::Timer, app: &AppWindow, refresh: fn(&AppWindow)) {
    let weak = app.as_weak();
    timer.start(slint::TimerMode::SingleShot, DEBOUNCE, move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
}

/// Inputs up to this size are worked on at once, on the UI thread: a few milliseconds, and no
/// flash of the old result. Bigger ones go to a thread.
const INLINE_BYTES: usize = 64_000;

/// Work where only the newest request matters (formatting, comparing, rendering what was just
/// typed). Each `run` supersedes the ones before it: their results, when they arrive, are
/// dropped instead of overwriting newer ones. A cheap handle: a tool keeps one in its state and
/// clones it out, because `run` may call `apply` (which uses the state) before returning.
#[derive(Clone, Default)]
pub struct Latest {
    generation: Arc<AtomicU64>,
    threads: Rc<RefCell<Vec<JoinHandle<()>>>>,
}

impl Latest {
    /// Runs `work`, then `apply` on the UI thread with its result, unless a newer `run` (or
    /// `shutdown`) came first. `size` is the input's length in bytes: small work runs at once,
    /// anything bigger on its own thread. Never call it inside the tool's `with()`.
    pub fn run<T: Send + 'static>(
        &self,
        app: &AppWindow,
        size: usize,
        work: impl FnOnce() -> T + Send + 'static,
        apply: impl FnOnce(&AppWindow, T) + Send + 'static,
    ) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if size <= INLINE_BYTES {
            return apply(app, work());
        }
        let current = self.generation.clone();
        let weak = app.as_weak();
        let mut threads = self.threads.borrow_mut();
        threads.retain(|t| !t.is_finished());
        threads.push(thread::spawn(move || {
            let result = work();
            jobs::post(&weak, move |app| {
                if current.load(Ordering::SeqCst) == generation {
                    apply(&app, result);
                }
            });
        }));
    }

    /// Drops every result still on its way and hands back the threads, without joining them.
    pub fn shutdown(&self) -> Vec<JoinHandle<()>> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.threads.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file in a folder of its own, which the test removes.
    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("open-tools-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn stats_count_characters_and_lines_like_the_website() {
        assert_eq!(stats("é\nb\n"), "4 characters · 3 lines");
    }

    #[test]
    fn stats_of_nothing_are_zero() {
        assert_eq!(stats(""), "0 characters · 0 lines");
    }

    #[test]
    fn counts_are_grouped_by_thousands_and_singular_for_one() {
        assert_eq!(counted(1_234_567, "word", "words"), "1,234,567 words");
        assert_eq!(counted(1, "word", "words"), "1 word");
    }

    #[test]
    fn a_pane_holds_exactly_the_limit() {
        assert!(fits_pane(&"é".repeat(PANE_MAX_CHARS)));
        assert!(!fits_pane(&"a".repeat(PANE_MAX_CHARS + 1)));
    }

    #[test]
    fn preview_cuts_on_a_character_boundary() {
        let text = format!("{}é", "a".repeat(PREVIEW_BYTES - 1));

        assert_eq!(preview(&text).len(), PREVIEW_BYTES - 1);
    }

    #[test]
    fn preview_note_is_empty_for_a_whole_text() {
        assert_eq!(preview_note("abc", "abc"), "");
    }

    #[test]
    fn reading_drops_a_byte_order_mark() {
        let path = temp_file("bom.txt", "\u{feff}{}".as_bytes());

        let file = read_text_file(&path, "JSON Formatter").unwrap();

        assert_eq!((file.name.as_str(), file.text.as_str()), ("bom.txt", "{}"));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn reading_refuses_binary_files() {
        let path = temp_file("photo.png", &[0x89, b'P', b'N', b'G', 0xff]);

        let error = read_text_file(&path, "JSON Formatter").err();

        assert_eq!(
            error.as_deref(),
            Some("Couldn't open photo.png: it isn't UTF-8 text. Open a text file.")
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn saving_writes_the_whole_text() {
        let path = temp_file("out.json", b"old");

        let note = save_text(&path, "[1]");

        assert_eq!(note.as_deref(), Ok("Saved as out.json"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[1]");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
