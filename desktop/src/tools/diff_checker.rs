//! Diff Checker: compares two texts line by line, highlights the words that changed inside a
//! changed line, and shows the result side by side or as one unified column. Ignore whitespace
//! and ignore case compare on a normalised key and still show the text as written. Each text is
//! typed, pasted or opened from a file; a file too long for a pane is held aside and used as it
//! is. Comparing runs on a worker for big inputs (`text::Latest`) and only the changes with four
//! lines of context are kept, so the result list stays short and is drawn virtualised. Copy
//! Patch copies a unified patch in the website's format. The view is ui/tools/diff-checker.slint.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use similar::{Algorithm, DiffOp, capture_diff_slices, group_diff_ops};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::engine::text::{self, Latest};
use crate::files::{self, format_size};
use crate::{AppWindow, DiffCell, DiffChecker, DiffKind, DiffMode, DiffRow, DiffSeg, clipboard};

pub const SLUG: &str = "diff-checker";
const TOOL: &str = "Diff Checker";
/// Unchanged lines kept around a change, as in the website's patch.
const CONTEXT: usize = 4;
/// A longer line is cut when drawn (the patch keeps all of it): one very long line would
/// otherwise make the row, and the list's width, enormous.
const MAX_LINE_CHARS: usize = 2_000;
const ORIGINAL: usize = 0;
const MODIFIED: usize = 1;
/// The website's patch header: jsdiff's `createPatch("document", …, "Original", "Modified")`.
const PATCH_HEADER: &str = "Index: document\n\
===================================================================\n\
--- document\tOriginal\n\
+++ document\tModified\n";
const EMPTY_MESSAGE: &str = "Enter original and modified text to see differences.";
const IDENTICAL_MESSAGE: &str = "The two texts are identical.";

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Options {
    ignore_whitespace: bool,
    ignore_case: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Empty,
    Context,
    Added,
    Removed,
}

/// A run of text in a line; `hot` marks the words that changed.
#[derive(Clone, Debug, PartialEq)]
struct Seg {
    text: String,
    hot: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Cell {
    kind: Kind,
    number: String,
    number_b: String,
    segs: Vec<Seg>,
}

impl Cell {
    fn empty() -> Self {
        Self {
            kind: Kind::Empty,
            number: String::new(),
            number_b: String::new(),
            segs: Vec::new(),
        }
    }

    fn line(kind: Kind, number: Option<usize>, number_b: Option<usize>, segs: Vec<Seg>) -> Self {
        let show = |n: Option<usize>| n.map(|n| n.to_string()).unwrap_or_default();
        Self {
            kind,
            number: show(number),
            number_b: show(number_b),
            segs,
        }
    }

    fn chars(&self) -> usize {
        self.segs.iter().map(|s| s.text.chars().count()).sum()
    }
}

/// An added line as unified draws it: its number goes in the second gutter.
fn unified(mut cell: Cell) -> Cell {
    cell.number_b = std::mem::take(&mut cell.number);
    cell
}

/// One row of the result: a gap ("12 unchanged lines") or two cells. Unified rows use `left`.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    gap: String,
    left: Cell,
    right: Cell,
}

impl Row {
    fn gap(lines: usize) -> Self {
        Self {
            gap: text::counted(lines, "unchanged line", "unchanged lines"),
            left: Cell::empty(),
            right: Cell::empty(),
        }
    }

    fn pair(left: Cell, right: Cell) -> Self {
        Self {
            gap: String::new(),
            left,
            right,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
struct Computed {
    side_by_side: Vec<Row>,
    unified: Vec<Row>,
    patch: String,
    /// The longest line drawn, in characters.
    max_chars: usize,
    summary: String,
    message: &'static str,
}

/// The text a line is compared by.
fn key(text: &str, options: Options) -> String {
    let mut key = if options.ignore_whitespace {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        text.to_string()
    };
    if options.ignore_case {
        key = key.to_lowercase();
    }
    key
}

fn line_keys(lines: &[&str], options: Options) -> Vec<String> {
    lines.iter().map(|l| key(l, options)).collect()
}

/// A line as drawn: tabs spaced out, a very long line cut.
fn shown(line: &str) -> String {
    let line = line.replace('\t', "    ");
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line,
    }
}

/// Words, runs of spaces, and single marks, so a change to one word of `a, b` doesn't light up
/// the rest of the line.
fn tokens(line: &str) -> Vec<&str> {
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut previous = None;
    for (i, c) in line.char_indices() {
        let now = class(c);
        if previous.is_some_and(|p| p != now || now == 2) {
            out.push(&line[start..i]);
            start = i;
        }
        previous = Some(now);
    }
    if start < line.len() {
        out.push(&line[start..]);
    }
    out
}

fn token_key(token: &str, options: Options) -> String {
    if options.ignore_whitespace && token.starts_with(char::is_whitespace) {
        " ".to_string()
    } else if options.ignore_case {
        token.to_lowercase()
    } else {
        token.to_string()
    }
}

fn token_keys(tokens: &[&str], options: Options) -> Vec<String> {
    tokens.iter().map(|t| token_key(t, options)).collect()
}

fn push_run(segs: &mut Vec<Seg>, words: &[&str], hot: bool) {
    if words.is_empty() {
        return;
    }
    let text = words.concat();
    match segs.last_mut() {
        Some(last) if last.hot == hot => last.text.push_str(&text),
        _ => segs.push(Seg { text, hot }),
    }
}

fn plain(line: &str) -> Vec<Seg> {
    if line.is_empty() {
        Vec::new()
    } else {
        vec![Seg {
            text: line.to_string(),
            hot: false,
        }]
    }
}

/// The runs of `old` and of `new`, with the words that differ marked.
fn word_segments(old: &str, new: &str, options: Options) -> (Vec<Seg>, Vec<Seg>) {
    let (a, b) = (tokens(old), tokens(new));
    let (keys_a, keys_b) = (token_keys(&a, options), token_keys(&b, options));
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for op in capture_diff_slices(Algorithm::Myers, &keys_a, &keys_b) {
        let hot = !matches!(op, DiffOp::Equal { .. });
        push_run(&mut left, &a[op.old_range()], hot);
        push_run(&mut right, &b[op.new_range()], hot);
    }
    (left, right)
}

fn compare(old: &str, new: &str, options: Options) -> Computed {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let (keys_a, keys_b) = (line_keys(&a, options), line_keys(&b, options));
    let ops = capture_diff_slices(Algorithm::Myers, &keys_a, &keys_b);
    let (mut added, mut removed) = (0, 0);
    for op in &ops {
        if !matches!(op, DiffOp::Equal { .. }) {
            removed += op.old_range().len();
            added += op.new_range().len();
        }
    }
    let groups = group_diff_ops(ops, CONTEXT);
    if groups.is_empty() {
        return Computed {
            message: if old.is_empty() && new.is_empty() {
                EMPTY_MESSAGE
            } else {
                IDENTICAL_MESSAGE
            },
            ..Computed::default()
        };
    }

    let mut out = Computed {
        patch: PATCH_HEADER.to_string(),
        summary: format!("+{added} −{removed} lines"),
        ..Computed::default()
    };
    let mut max_chars = 0;
    let mut drawn = 0;
    for group in &groups {
        let gap = group[0].old_range().start - drawn;
        if gap > 0 {
            out.side_by_side.push(Row::gap(gap));
            out.unified.push(Row::gap(gap));
        }
        drawn = group[group.len() - 1].old_range().end;
        out.patch.push_str(&format!(
            "{}\n",
            similar::udiff::UnifiedHunkHeader::new(group)
        ));
        for op in group {
            let (old_lines, new_lines) = (op.old_range(), op.new_range());
            if matches!(op, DiffOp::Equal { .. }) {
                for (i, j) in old_lines.zip(new_lines) {
                    let segs = plain(&shown(a[i]));
                    let left = Cell::line(Kind::Context, Some(i + 1), None, segs.clone());
                    let right = Cell::line(Kind::Context, Some(j + 1), None, segs.clone());
                    max_chars = max_chars.max(left.chars());
                    out.side_by_side.push(Row::pair(left, right));
                    let both = Cell::line(Kind::Context, Some(i + 1), Some(j + 1), segs);
                    out.unified.push(Row::pair(both, Cell::empty()));
                    out.patch.push_str(&format!(" {}\n", a[i]));
                }
                continue;
            }
            // The first lines of a replaced block are paired to mark the words that changed.
            let paired = old_lines.len().min(new_lines.len());
            let words: Vec<(Vec<Seg>, Vec<Seg>)> = (0..paired)
                .map(|k| {
                    let (i, j) = (old_lines.start + k, new_lines.start + k);
                    word_segments(&shown(a[i]), &shown(b[j]), options)
                })
                .collect();
            for i in old_lines.clone() {
                out.patch.push_str(&format!("-{}\n", a[i]));
            }
            for j in new_lines.clone() {
                out.patch.push_str(&format!("+{}\n", b[j]));
            }
            let removed_cells: Vec<Cell> = old_lines
                .enumerate()
                .map(|(k, i)| {
                    let segs = words
                        .get(k)
                        .map_or_else(|| plain(&shown(a[i])), |w| w.0.clone());
                    Cell::line(Kind::Removed, Some(i + 1), None, segs)
                })
                .collect();
            let added_cells: Vec<Cell> = new_lines
                .enumerate()
                .map(|(k, j)| {
                    let segs = words
                        .get(k)
                        .map_or_else(|| plain(&shown(b[j])), |w| w.1.clone());
                    Cell::line(Kind::Added, Some(j + 1), None, segs)
                })
                .collect();
            for k in 0..removed_cells.len().max(added_cells.len()) {
                let left = removed_cells.get(k).cloned().unwrap_or_else(Cell::empty);
                let right = added_cells.get(k).cloned().unwrap_or_else(Cell::empty);
                max_chars = max_chars.max(left.chars()).max(right.chars());
                out.side_by_side.push(Row::pair(left, right));
            }
            // Unified shows the removed lines, then the added ones, an added line numbered in
            // the second gutter.
            for cell in removed_cells
                .into_iter()
                .chain(added_cells.into_iter().map(unified))
            {
                out.unified.push(Row::pair(cell, Cell::empty()));
            }
        }
    }
    let tail = a.len() - drawn;
    if tail > 0 {
        out.side_by_side.push(Row::gap(tail));
        out.unified.push(Row::gap(tail));
    }
    out.max_chars = max_chars;
    out
}

// ---- Slint models -------------------------------------------------------------------------

fn slint_cell(cell: &Cell) -> DiffCell {
    let segs: Vec<DiffSeg> = cell
        .segs
        .iter()
        .map(|s| DiffSeg {
            text: s.text.as_str().into(),
            hot: s.hot,
        })
        .collect();
    DiffCell {
        kind: match cell.kind {
            Kind::Empty => DiffKind::Empty,
            Kind::Context => DiffKind::Context,
            Kind::Added => DiffKind::Added,
            Kind::Removed => DiffKind::Removed,
        },
        number: cell.number.as_str().into(),
        number_b: cell.number_b.as_str().into(),
        text: cell
            .segs
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
            .into(),
        segs: ModelRc::new(VecModel::from(segs)),
    }
}

fn slint_rows(rows: &[Row]) -> ModelRc<DiffRow> {
    let rows: Vec<DiffRow> = rows
        .iter()
        .map(|row| DiffRow {
            gap: row.gap.as_str().into(),
            left: slint_cell(&row.left),
            right: slint_cell(&row.right),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

// ---- State and wiring ---------------------------------------------------------------------

/// A file too long for its pane, held here instead.
struct InputFile {
    name: String,
    text: Arc<str>,
}

#[derive(Default)]
struct State {
    files: [Option<InputFile>; 2],
    result: Computed,
    comparing: Latest,
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

fn pane_text(g: &DiffChecker, side: usize) -> String {
    if side == ORIGINAL {
        g.get_original().to_string()
    } else {
        g.get_modified().to_string()
    }
}

fn set_pane_text(g: &DiffChecker, side: usize, text: &str) {
    if side == ORIGINAL {
        g.set_original(text.into());
    } else {
        g.set_modified(text.into());
    }
}

pub fn init(app: &AppWindow) {
    let g = app.global::<DiffChecker>();
    let weak = app.as_weak();
    g.on_edited(move || {
        if let Some(app) = weak.upgrade() {
            with(|s| text::debounce(&s.typing, &app, refresh));
        }
    });
    let weak = app.as_weak();
    g.on_options_changed(move || {
        if let Some(app) = weak.upgrade() {
            render(&app);
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_open_file(move |side| {
        let Some(app) = weak.upgrade() else { return };
        let title = if side as usize == ORIGINAL {
            "Open Original"
        } else {
            "Open Modified"
        };
        if let Some(path) = files::choose_file(title, None) {
            open_file(&app, side as usize, path);
        }
    });
    let weak = app.as_weak();
    g.on_remove_file(move |side| {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.files[side as usize] = None);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_swap(move || {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<DiffChecker>();
        let (original, modified) = (g.get_original(), g.get_modified());
        g.set_original(modified);
        g.set_modified(original);
        with(|s| s.files.swap(ORIGINAL, MODIFIED));
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_clear(move || {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<DiffChecker>();
        g.set_original("".into());
        g.set_modified("".into());
        with(|s| s.files = Default::default());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy_patch(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let copied = with(|s| match s.result.patch.as_str() {
            "" => None,
            patch => Some(clipboard::copy_text(patch)),
        });
        match copied {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                app.global::<DiffChecker>()
                    .set_error(format!("Couldn't copy: {e}").into());
                false
            }
            None => false,
        }
    });
    refresh(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<DiffChecker>().set_drop_hover(hover);
}

/// Two dropped files become the original and the modified text; one fills the first empty side
/// (the modified one if both are taken).
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let g = app.global::<DiffChecker>();
    match <[PathBuf; 2]>::try_from(paths) {
        Ok([original, modified]) => {
            open_file(app, ORIGINAL, original);
            open_file(app, MODIFIED, modified);
        }
        Err(paths) => match <[PathBuf; 1]>::try_from(paths) {
            Ok([path]) => {
                let original_taken =
                    !g.get_original().is_empty() || with(|s| s.files[ORIGINAL].is_some());
                open_file(app, if original_taken { MODIFIED } else { ORIGINAL }, path);
            }
            Err(paths) if paths.is_empty() => {}
            Err(_) => g.set_error("Drop one or two files.".into()),
        },
    }
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.comparing.shutdown())
}

fn open_file(app: &AppWindow, side: usize, path: PathBuf) {
    let g = app.global::<DiffChecker>();
    let file = match text::read_text_file(&path, TOOL) {
        Ok(file) => file,
        Err(message) => return g.set_error(message.into()),
    };
    if text::fits_pane(&file.text) {
        with(|s| s.files[side] = None);
        set_pane_text(&g, side, &file.text);
    } else {
        with(|s| {
            s.files[side] = Some(InputFile {
                name: format!("{} · {}", file.name, format_size(file.text.len() as u64)),
                text: file.text.into(),
            })
        });
    }
    refresh(app);
}

/// Compares the current texts, in the background when they are big, then shows the result.
fn refresh(app: &AppWindow) {
    let g = app.global::<DiffChecker>();
    g.set_error("".into());
    let options = Options {
        ignore_whitespace: g.get_ignore_whitespace(),
        ignore_case: g.get_ignore_case(),
    };
    let texts: [Arc<str>; 2] = [ORIGINAL, MODIFIED].map(|side| {
        with(|s| s.files[side].as_ref().map(|f| f.text.clone()))
            .unwrap_or_else(|| pane_text(&g, side).into())
    });
    let labels =
        with(|s| [ORIGINAL, MODIFIED].map(|side| s.files[side].as_ref().map(|f| f.name.clone())));
    g.set_original_file(labels[ORIGINAL].clone().unwrap_or_default().into());
    g.set_modified_file(labels[MODIFIED].clone().unwrap_or_default().into());
    g.set_original_stats(text::stats(&texts[ORIGINAL]).into());
    g.set_modified_stats(text::stats(&texts[MODIFIED]).into());
    let size = texts[ORIGINAL].len() + texts[MODIFIED].len();
    let comparing = with(|s| s.comparing.clone());
    comparing.run(
        app,
        size,
        move || compare(&texts[ORIGINAL], &texts[MODIFIED], options),
        show,
    );
}

fn show(app: &AppWindow, computed: Computed) {
    with(|s| s.result = computed);
    render(app);
}

/// Puts the stored result into the view, in the chosen layout.
fn render(app: &AppWindow) {
    let g = app.global::<DiffChecker>();
    with(|s| {
        let rows = match g.get_mode() {
            DiffMode::SideBySide => &s.result.side_by_side,
            DiffMode::Unified => &s.result.unified,
        };
        g.set_rows(slint_rows(rows));
        g.set_max_chars(s.result.max_chars as i32);
        g.set_summary(s.result.summary.as_str().into());
        g.set_message(s.result.message.into());
        g.set_can_copy(!s.result.patch.is_empty());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Options = Options {
        ignore_whitespace: false,
        ignore_case: false,
    };

    fn texts(cell: &Cell) -> Vec<(&str, bool)> {
        cell.segs.iter().map(|s| (s.text.as_str(), s.hot)).collect()
    }

    #[test]
    fn a_changed_line_is_a_removal_and_an_addition_in_a_hunk() {
        let diff = compare("a\nb\nc\n", "a\nB\nc\n", PLAIN);

        assert_eq!(
            diff.patch,
            format!("{PATCH_HEADER}@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n")
        );
        assert_eq!(diff.summary, "+1 −1 lines");
    }

    #[test]
    fn side_by_side_pairs_the_removed_line_with_the_added_one() {
        let diff = compare("a\nb\nc", "a\nB\nc", PLAIN);

        let kinds: Vec<_> = diff
            .side_by_side
            .iter()
            .map(|r| (r.left.kind, r.right.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                (Kind::Context, Kind::Context),
                (Kind::Removed, Kind::Added),
                (Kind::Context, Kind::Context),
            ]
        );
    }

    #[test]
    fn unified_lists_removed_lines_then_added_ones_with_both_numbers() {
        let diff = compare("a\nb", "a\nx\ny", PLAIN);

        let rows: Vec<_> = diff
            .unified
            .iter()
            .map(|r| {
                (
                    r.left.kind,
                    r.left.number.as_str(),
                    r.left.number_b.as_str(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (Kind::Context, "1", "1"),
                (Kind::Removed, "2", ""),
                (Kind::Added, "", "2"),
                (Kind::Added, "", "3"),
            ]
        );
    }

    #[test]
    fn only_the_changed_word_is_marked() {
        let diff = compare("the quick fox", "the slow fox", PLAIN);

        assert_eq!(
            texts(&diff.side_by_side[0].left),
            [("the ", false), ("quick", true), (" fox", false)]
        );
        assert_eq!(
            texts(&diff.side_by_side[0].right),
            [("the ", false), ("slow", true), (" fox", false)]
        );
    }

    #[test]
    fn identical_texts_say_so_and_have_no_patch() {
        let diff = compare("a\nb", "a\nb", PLAIN);

        assert_eq!(diff.message, IDENTICAL_MESSAGE);
        assert!(diff.patch.is_empty() && diff.unified.is_empty());
    }

    #[test]
    fn two_empty_texts_ask_for_input() {
        assert_eq!(compare("", "", PLAIN).message, EMPTY_MESSAGE);
    }

    #[test]
    fn ignoring_case_treats_a_recased_line_as_unchanged() {
        let options = Options {
            ignore_case: true,
            ..PLAIN
        };

        assert_eq!(
            compare("Hello", "hello", options).message,
            IDENTICAL_MESSAGE
        );
        assert_ne!(compare("Hello", "hello", PLAIN).message, IDENTICAL_MESSAGE);
    }

    #[test]
    fn ignoring_whitespace_ignores_indentation_and_spacing() {
        let options = Options {
            ignore_whitespace: true,
            ..PLAIN
        };

        assert_eq!(
            compare("a  b\n  c", "a b\nc", options).message,
            IDENTICAL_MESSAGE
        );
    }

    #[test]
    fn long_runs_of_unchanged_lines_collapse_to_a_gap() {
        let old: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        let new = old.replace("line 15\n", "changed\n");

        let diff = compare(&old, &new, PLAIN);

        let gaps: Vec<_> = diff.unified.iter().filter(|r| !r.gap.is_empty()).collect();
        assert_eq!(
            gaps.iter().map(|r| r.gap.as_str()).collect::<Vec<_>>(),
            ["10 unchanged lines", "11 unchanged lines"]
        );
        assert!(diff.patch.contains("@@ -11,9 +11,9 @@"));
    }

    #[test]
    fn multi_byte_text_is_cut_on_character_boundaries() {
        let long = "é".repeat(MAX_LINE_CHARS + 5);

        let diff = compare("", &long, PLAIN);

        assert_eq!(diff.max_chars, MAX_LINE_CHARS + 1);
    }

    #[test]
    fn tokens_split_words_spaces_and_marks() {
        assert_eq!(tokens("a, b  é_1"), ["a", ",", " ", "b", "  ", "é_1"]);
    }
}
