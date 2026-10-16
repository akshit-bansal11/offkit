//! Git Scaffold (shown as Directory Scaffold once tools.json renames it): a local folder's tree
//! as ASCII and as a Markdown list, to copy or save. It reads the chosen folder and nothing
//! else; it makes no network request. Its Slint side is ui/tools/git-scaffold.slint.
//!
//! The walk runs on a worker, never follows a link (a link shows as `name -> target`), stops at
//! `MAX_ENTRIES`, and can honour the folder's .gitignore files through the small matcher below:
//! nested files, `!` negation, `dir/` patterns, anchored `/patterns`, `*`, `?`, `[…]` and `**`.
//! It reads .gitignore files inside the chosen folder only, not those of a repository above it,
//! `.git/info/exclude` or a global excludes file.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use slint::ComponentHandle;

use crate::engine::archive::{plural, shown, thousands};
use crate::files::{self, file_name};
use crate::jobs::{self, CancelToken, JobError};
use crate::select::{self, index};
use crate::{AppWindow, GitScaffold, ScaffoldFormat, ScaffoldSort, clipboard};

pub const SLUG: &str = "git-scaffold";
/// Items listed at most; a bigger folder is cut off with a note.
const MAX_ENTRIES: usize = 20_000;
/// The most output text the pane shows; Copy and Save use all of it (see `tools::base64`).
const PREVIEW_BYTES: usize = 50_000;
/// A .gitignore bigger than this isn't a real one; it's left unread.
const MAX_GITIGNORE_BYTES: u64 = 1_000_000;
/// How long a walk runs before Stop replaces Choose Folder…, so quick walks don't flicker.
const SHOW_STOP_AFTER: Duration = Duration::from_millis(250);
const DEPTHS: [(&str, Option<usize>); 6] = [
    ("All levels", None),
    ("1 level", Some(1)),
    ("2 levels", Some(2)),
    ("3 levels", Some(3)),
    ("4 levels", Some(4)),
    ("5 levels", Some(5)),
];

// ---- Logic --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Options {
    /// Folder levels shown below the chosen one; `None` shows all.
    depth: Option<usize>,
    hidden: bool,
    folders_only: bool,
    folders_first: bool,
    gitignore: bool,
}

#[derive(Debug, PartialEq)]
enum Kind {
    Folder(Vec<Node>),
    File,
    /// Where the link points, as stored; never followed.
    Link(String),
}

#[derive(Debug, PartialEq)]
struct Node {
    name: String,
    kind: Kind,
}

impl Node {
    /// "src/", "main.rs", "latest -> v2.1".
    fn label(&self) -> String {
        let name = shown(&self.name);
        match &self.kind {
            Kind::Folder(_) => format!("{name}/"),
            Kind::File => name,
            Kind::Link(target) => format!("{name} -> {}", shown(target)),
        }
    }
}

#[derive(Debug, Default)]
struct Tree {
    name: String,
    nodes: Vec<Node>,
    folders: usize,
    files: usize,
    links: usize,
    /// Stopped at `MAX_ENTRIES`.
    truncated: bool,
    /// Folders (and .gitignore files) that couldn't be read, by path below the root.
    unreadable: Vec<String>,
}

impl Tree {
    /// "12 folders · 210 files · 1 link".
    fn summary(&self) -> String {
        let mut parts = vec![
            plural(self.folders, "folder"),
            plural(self.files, "file"),
        ];
        if self.links > 0 {
            parts.push(plural(self.links, "link"));
        }
        parts.join(" · ")
    }

    /// The tree as `tree` prints it, with real box-drawing characters.
    fn ascii(&self) -> String {
        fn walk(nodes: &[Node], prefix: &str, lines: &mut Vec<String>) {
            for (i, node) in nodes.iter().enumerate() {
                let last = i + 1 == nodes.len();
                let branch = if last { "└── " } else { "├── " };
                lines.push(format!("{prefix}{branch}{}", node.label()));
                if let Kind::Folder(children) = &node.kind {
                    let indent = if last { "    " } else { "│   " };
                    walk(children, &format!("{prefix}{indent}"), lines);
                }
            }
        }
        let mut lines = vec![format!("{}/", shown(&self.name))];
        walk(&self.nodes, "", &mut lines);
        self.finish(lines)
    }

    /// A nested Markdown list, each name a code span so `*`, `_` and `#` in names show as typed.
    fn markdown(&self) -> String {
        fn walk(nodes: &[Node], depth: usize, lines: &mut Vec<String>) {
            for node in nodes {
                lines.push(format!("{}- {}", "  ".repeat(depth), code(&node.label())));
                if let Kind::Folder(children) = &node.kind {
                    walk(children, depth + 1, lines);
                }
            }
        }
        let mut lines = vec![format!("- {}", code(&format!("{}/", shown(&self.name))))];
        walk(&self.nodes, 1, &mut lines);
        self.finish(lines)
    }

    fn finish(&self, mut lines: Vec<String>) -> String {
        if self.truncated {
            lines.push("…".into());
        }
        lines.join("\n")
    }
}

/// Walks `root` with `options`, polling `cancel` between folders.
fn walk(root: &Path, options: Options, cancel: &CancelToken) -> Result<Tree, JobError> {
    let mut walker = Walker {
        options,
        cancel,
        rules: Vec::new(),
        count: 0,
        tree: Tree {
            name: file_name(root),
            ..Tree::default()
        },
    };
    let nodes = walker.folder(root, &mut Vec::new(), 1)?;
    walker.tree.nodes = nodes;
    Ok(walker.tree)
}

struct Walker<'a> {
    options: Options,
    cancel: &'a CancelToken,
    /// The .gitignore files of the folders being walked, outermost first.
    rules: Vec<Rules>,
    count: usize,
    tree: Tree,
}

impl Walker<'_> {
    /// The listing of `dir`, which sits at `rel` below the root, `depth` levels down.
    fn folder(&mut self, dir: &Path, rel: &mut Vec<String>, depth: usize) -> Result<Vec<Node>, JobError> {
        if self.cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let loaded = self.options.gitignore && self.load_rules(dir, rel);
        let entries = match fs::read_dir(dir).and_then(|d| d.collect::<std::io::Result<Vec<_>>>()) {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("couldn't read {}: {e}", dir.display());
                let at = if rel.is_empty() { self.tree.name.clone() } else { rel.join("/") };
                self.tree.unreadable.push(at);
                Vec::new()
            }
        };
        let mut items: Vec<_> = entries
            .into_iter()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                // `DirEntry::file_type` doesn't follow links, so a link to a folder isn't one.
                let kind = entry.file_type().ok()?;
                let hidden = name.starts_with('.') || hidden_on_windows(&entry);
                if hidden && !self.options.hidden {
                    return None;
                }
                let ignored = self.options.gitignore
                    && ((kind.is_dir() && name == ".git") || self.ignored(rel, &name, kind.is_dir()));
                (!ignored).then_some((name, entry.path(), kind))
            })
            .collect();
        let folders_first = self.options.folders_first;
        items.sort_by(|(a, _, ka), (b, _, kb)| {
            let rank = |dir: bool| u8::from(!(folders_first && dir));
            rank(ka.is_dir())
                .cmp(&rank(kb.is_dir()))
                .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
                .then_with(|| a.cmp(b))
        });

        let mut nodes = Vec::new();
        for (name, path, file_type) in items {
            if self.count >= MAX_ENTRIES {
                self.tree.truncated = true;
                break;
            }
            // A link is never a folder here, even when it points at one.
            if self.options.folders_only && !file_type.is_dir() {
                continue;
            }
            self.count += 1;
            let kind = if file_type.is_symlink() {
                self.tree.links += 1;
                let target = fs::read_link(&path)
                    .map(|t| t.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| format!("(unreadable: {e})"));
                Kind::Link(target)
            } else if file_type.is_dir() {
                self.tree.folders += 1;
                let children = if self.options.depth.is_none_or(|max| depth < max) {
                    rel.push(name.clone());
                    let children = self.folder(&path, rel, depth + 1);
                    rel.pop();
                    children?
                } else {
                    Vec::new()
                };
                Kind::Folder(children)
            } else {
                self.tree.files += 1;
                Kind::File
            };
            nodes.push(Node { name, kind });
        }
        if loaded {
            self.rules.pop();
        }
        Ok(nodes)
    }

    /// Reads `dir`'s .gitignore onto the stack; false when there is none to read.
    fn load_rules(&mut self, dir: &Path, rel: &[String]) -> bool {
        let path = dir.join(".gitignore");
        // A link, a folder or an implausibly big file named .gitignore isn't read.
        let Ok(meta) = fs::symlink_metadata(&path) else {
            return false;
        };
        if !meta.is_file() || meta.len() > MAX_GITIGNORE_BYTES {
            return false;
        }
        match fs::read(&path) {
            Ok(bytes) => {
                self.rules.push(Rules {
                    base: rel.to_vec(),
                    patterns: parse_gitignore(&String::from_utf8_lossy(&bytes)),
                });
                true
            }
            Err(e) => {
                eprintln!("couldn't read {}: {e}", path.display());
                let mut at = rel.to_vec();
                at.push(".gitignore".into());
                self.tree.unreadable.push(at.join("/"));
                false
            }
        }
    }

    /// Whether `name`, inside the folder at `rel`, is ignored. As in git, the last pattern that
    /// matches decides, and a deeper .gitignore comes after the ones above it.
    fn ignored(&self, rel: &[String], name: &str, is_dir: bool) -> bool {
        let mut path = rel.to_vec();
        path.push(name.into());
        let mut ignored = false;
        for rules in &self.rules {
            let Some(below) = path.strip_prefix(rules.base.as_slice()) else {
                continue;
            };
            for pattern in &rules.patterns {
                if pattern.matches(below, is_dir) {
                    ignored = !pattern.negated;
                }
            }
        }
        ignored
    }
}

#[cfg(windows)]
fn hidden_on_windows(entry: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    entry
        .metadata()
        .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn hidden_on_windows(_entry: &fs::DirEntry) -> bool {
    false
}

/// One .gitignore file: its folder (below the root) and its patterns, in file order.
struct Rules {
    base: Vec<String>,
    patterns: Vec<Pattern>,
}

#[derive(Debug, PartialEq)]
struct Pattern {
    /// The pattern split at `/`, each part a glob.
    parts: Vec<Vec<char>>,
    negated: bool,
    /// Ended in `/`: matches folders only.
    dir_only: bool,
    /// Had a `/` at its start or middle: matches the path from the .gitignore's folder, not a
    /// name at any depth.
    anchored: bool,
}

impl Pattern {
    /// `path` is the entry's path below the .gitignore's folder.
    fn matches(&self, path: &[String], is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }
        if self.anchored {
            match_parts(&self.parts, path)
        } else {
            path.last().is_some_and(|name| glob(&self.parts[0], name))
        }
    }
}

fn parse_gitignore(text: &str) -> Vec<Pattern> {
    text.lines().filter_map(parse_line).collect()
}

/// One .gitignore line, or `None` for a blank line or a comment. A backslash keeps a leading
/// `#` or `!`, or a trailing space, literal.
fn parse_line(line: &str) -> Option<Pattern> {
    if line.starts_with('#') {
        return None;
    }
    let mut end = line.len();
    while line[..end].ends_with(' ') && !line[..end - 1].ends_with('\\') {
        end -= 1;
    }
    let line = &line[..end];
    let (negated, line) = match line.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (dir_only, line) = match line.strip_suffix('/') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    if line.is_empty() {
        return None;
    }
    let anchored = line.contains('/');
    let line = line.strip_prefix('/').unwrap_or(line);
    Some(Pattern {
        parts: line.split('/').map(|p| p.chars().collect()).collect(),
        negated,
        dir_only,
        anchored,
    })
}

/// Pattern parts against path parts. A `**` part matches any number of folders when it leads
/// or sits between parts, and everything inside when it ends the pattern.
fn match_parts(parts: &[Vec<char>], path: &[String]) -> bool {
    match parts.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first.as_slice() == ['*', '*'] => {
            let least = usize::from(rest.is_empty());
            (least..=path.len()).any(|skip| match_parts(rest, &path[skip..]))
        }
        Some((first, rest)) => path
            .split_first()
            .is_some_and(|(name, below)| glob(first, name) && match_parts(rest, below)),
    }
}

/// One name against one glob part: `*` any run, `?` one character, `[…]` a class, `\` an
/// escape. Greedy with backtracking to the last `*`, so it's linear-ish, not exponential.
fn glob(pattern: &[char], name: &str) -> bool {
    let text: Vec<char> = name.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if pattern.get(p) == Some(&'*') {
            star = Some((p, t));
            p += 1;
            continue;
        }
        if let Some(len) = token(&pattern[p.min(pattern.len())..], text[t]) {
            p += len;
            t += 1;
            continue;
        }
        match star {
            Some((sp, st)) => {
                p = sp + 1;
                t = st + 1;
                star = Some((sp, st + 1));
            }
            None => return false,
        }
    }
    pattern[p.min(pattern.len())..].iter().all(|c| *c == '*')
}

/// How many pattern characters match `c` as one token, or `None` if they don't.
fn token(pattern: &[char], c: char) -> Option<usize> {
    match *pattern.first()? {
        '?' => Some(1),
        '\\' => (pattern.get(1) == Some(&c)).then_some(2),
        // An unclosed `[` is a literal.
        '[' => match class(pattern, c) {
            Some((len, hit)) => hit.then_some(len),
            None => (c == '[').then_some(1),
        },
        literal => (literal == c).then_some(1),
    }
}

/// A `[…]` class at the start of `pattern`: its length, and whether `c` is in it. `!` or `^`
/// first negates it, `a-z` is a range, and a `]` straight after the opening is literal.
fn class(pattern: &[char], c: char) -> Option<(usize, bool)> {
    let mut i = 1;
    let negated = matches!(pattern.get(i), Some('!' | '^'));
    if negated {
        i += 1;
    }
    let start = i;
    let mut hit = false;
    loop {
        let ch = *pattern.get(i)?;
        if ch == ']' && i > start {
            return Some((i + 1, hit != negated));
        }
        let (low, next) = match ch {
            '\\' => (*pattern.get(i + 1)?, i + 2),
            _ => (ch, i + 1),
        };
        match (pattern.get(next), pattern.get(next + 1)) {
            (Some('-'), Some(&high)) if high != ']' => {
                hit |= (low..=high).contains(&c);
                i = next + 2;
            }
            _ => {
                hit |= low == c;
                i = next;
            }
        }
    }
}

/// `text` as a Markdown code span, its backtick fence longer than any run inside it.
fn code(text: &str) -> String {
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// The start of `text` that the pane shows, cut on a line break when there is one.
fn preview(text: &str) -> &str {
    if text.len() <= PREVIEW_BYTES {
        return text;
    }
    let cut = &text[..text.floor_char_boundary(PREVIEW_BYTES)];
    cut.rfind('\n').map_or(cut, |end| &cut[..end])
}

// ---- State and wiring ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    folder: Option<PathBuf>,
    ascii: String,
    markdown: String,
    /// The walk showing or running; a new walk cancels it and takes its place.
    walk: Arc<CancelToken>,
    worker: Option<JoinHandle<()>>,
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
    let g = app.global::<GitScaffold>();
    g.set_depths(select::options(DEPTHS.iter().map(|(label, _)| *label)));
    let weak = app.as_weak();
    g.on_choose_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(dir) = files::choose_folder("Choose a Folder") {
            open_folder(&app, dir);
        }
    });
    let weak = app.as_weak();
    g.on_options_changed(move || {
        if let Some(app) = weak.upgrade() {
            start_walk(&app);
        }
    });
    let weak = app.as_weak();
    g.on_format_changed(move || {
        if let Some(app) = weak.upgrade() {
            show_output(&app);
        }
    });
    let weak = app.as_weak();
    g.on_stop(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.walk.cancel());
        app.global::<GitScaffold>().set_walking(false);
    });
    let weak = app.as_weak();
    g.on_copy_output(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        let g = app.global::<GitScaffold>();
        let text = with(|s| current(s, g.get_format()).to_string());
        match clipboard::copy_text(&text) {
            Ok(()) => true,
            Err(e) => {
                g.set_error(format!("Couldn't copy: {e}").into());
                false
            }
        }
    });
    let weak = app.as_weak();
    g.on_save_output(move || {
        if let Some(app) = weak.upgrade() {
            save_output(&app);
        }
    });
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<GitScaffold>().set_drop_hover(hover);
}

/// A dropped folder becomes the one shown; a file or several items are refused with a reason.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    let g = app.global::<GitScaffold>();
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) if path.is_dir() => open_folder(app, path),
        Ok([path]) => g.set_error(
            format!(
                "{} is a file. Drop a folder to see its tree.",
                file_name(&path)
            )
            .into(),
        ),
        Err(paths) if paths.is_empty() => {}
        Err(_) => g.set_error("Drop one folder at a time.".into()),
    }
}

/// Cancels a walk in progress and hands back its thread.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| {
        s.walk.cancel();
        s.worker.take().into_iter().collect()
    })
}

fn open_folder(app: &AppWindow, dir: PathBuf) {
    let g = app.global::<GitScaffold>();
    g.set_folder(file_name(&dir).into());
    with(|s| s.folder = Some(dir));
    start_walk(app);
}

fn options(g: &GitScaffold) -> Options {
    Options {
        depth: DEPTHS[index(g.get_depth_index(), DEPTHS.len())].1,
        hidden: g.get_show_hidden(),
        folders_only: g.get_folders_only(),
        folders_first: g.get_sort() == ScaffoldSort::FoldersFirst,
        gitignore: g.get_use_gitignore(),
    }
}

/// Walks the chosen folder again with the current options, replacing any walk in progress.
fn start_walk(app: &AppWindow) {
    let g = app.global::<GitScaffold>();
    let options = options(&g);
    let Some((dir, token)) = with(|s| {
        let dir = s.folder.clone()?;
        s.walk.cancel();
        s.walk = Arc::default();
        Some((dir, s.walk.clone()))
    }) else {
        return;
    };
    g.set_error("".into());
    let weak = app.as_weak();
    let walking = token.clone();
    slint::Timer::single_shot(SHOW_STOP_AFTER, move || {
        let Some(app) = weak.upgrade() else { return };
        if with(|s| Arc::ptr_eq(&s.walk, &walking) && s.worker.as_ref().is_some_and(|w| !w.is_finished())) {
            app.global::<GitScaffold>().set_walking(true);
        }
    });
    let weak = app.as_weak();
    let worker = thread::spawn(move || {
        let result = walk(&dir, options, &token).map(|tree| {
            let (ascii, markdown) = (tree.ascii(), tree.markdown());
            (tree, ascii, markdown)
        });
        jobs::post(&weak, move |app| finish_walk(&app, &token, result));
    });
    with(|s| s.worker = Some(worker));
}

fn finish_walk(app: &AppWindow, token: &Arc<CancelToken>, result: Result<(Tree, String, String), JobError>) {
    if !with(|s| Arc::ptr_eq(&s.walk, token)) {
        return; // A newer walk replaced this one.
    }
    let g = app.global::<GitScaffold>();
    g.set_walking(false);
    let (tree, ascii, markdown) = match result {
        Ok(done) => done,
        Err(JobError::Cancelled) => return g.set_note("Stopped. Change an option to read the folder again.".into()),
        Err(JobError::Failed(message)) => return g.set_error(message.into()),
    };
    g.set_summary(format!("{} · {}", shown(&tree.name), tree.summary()).into());
    let mut notes = Vec::new();
    if tree.truncated {
        notes.push(format!(
            "Showing the first {} items; the folder holds more.",
            thousands(MAX_ENTRIES)
        ));
    }
    if !tree.unreadable.is_empty() {
        let names: Vec<_> = tree.unreadable.iter().map(|n| format!("“{}”", shown(n))).collect();
        notes.push(format!("Couldn't read {}.", names.join(", ")));
    }
    g.set_note(notes.join(" ").into());
    with(|s| {
        s.ascii = ascii;
        s.markdown = markdown;
    });
    show_output(app);
}

fn current(s: &State, format: ScaffoldFormat) -> &str {
    match format {
        ScaffoldFormat::Tree => &s.ascii,
        ScaffoldFormat::Markdown => &s.markdown,
    }
}

/// Shows the chosen format, cut to `PREVIEW_BYTES`.
fn show_output(app: &AppWindow) {
    let g = app.global::<GitScaffold>();
    with(|s| {
        let full = current(s, g.get_format());
        let shown = preview(full);
        g.set_output(shown.into());
        g.set_cut(shown.len() < full.len());
    });
}

fn save_output(app: &AppWindow) {
    let g = app.global::<GitScaffold>();
    let format = g.get_format();
    let ext = match format {
        ScaffoldFormat::Tree => "txt",
        ScaffoldFormat::Markdown => "md",
    };
    let suggested = format!("{} tree.{ext}", g.get_folder());
    let Some(path) = files::choose_save_path("Save Tree", &suggested) else {
        return;
    };
    // Written beside the chosen file first, so a failed save never damages the one it replaces.
    let result = files::reserve_replacing(&path).and_then(|output| {
        with(|s| fs::write(output.path(), current(s, format)))?;
        output.keep()
    });
    let name = file_name(&path);
    match result {
        Ok(_) => g.set_note(format!("Saved as {name}").into()),
        Err(e) => g.set_error(format!("Couldn't save {name}: {e}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "open-tools-scaffold-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Creates `files` (paths ending in `/` are folders) under a fresh `dir/project`.
    fn project(test: &str, files: &[&str]) -> (PathBuf, PathBuf) {
        let dir = temp_dir(test);
        let root = dir.join("project");
        for file in files {
            let path = root.join(file);
            if file.ends_with('/') {
                fs::create_dir_all(&path).unwrap();
            } else {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, b"").unwrap();
            }
        }
        (dir, root)
    }

    fn defaults() -> Options {
        Options {
            depth: None,
            hidden: false,
            folders_only: false,
            folders_first: true,
            gitignore: true,
        }
    }

    fn ascii(root: &Path, options: Options) -> String {
        walk(root, options, &CancelToken::default()).unwrap().ascii()
    }

    const FIXTURE: &[&str] = &["src/main.rs", "src/lib/mod.rs", "README.md", "docs/", "Cargo.toml"];

    #[test]
    fn the_tree_draws_real_box_characters_with_folders_first() {
        let (dir, root) = project("ascii", FIXTURE);

        let text = ascii(&root, defaults());

        assert_eq!(
            text,
            "project/\n\
             ├── docs/\n\
             ├── src/\n\
             │   ├── lib/\n\
             │   │   └── mod.rs\n\
             │   └── main.rs\n\
             ├── Cargo.toml\n\
             └── README.md"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_markdown_list_nests_by_two_spaces() {
        let (dir, root) = project("markdown", &["src/main.rs", "notes_v1.md"]);

        let text = walk(&root, defaults(), &CancelToken::default()).unwrap().markdown();

        assert_eq!(
            text,
            "- `project/`\n  - `src/`\n    - `main.rs`\n  - `notes_v1.md`"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sorting_by_name_mixes_folders_and_files() {
        let (dir, root) = project("by-name", &["b/", "a.txt", "C.txt"]);

        let text = ascii(&root, Options { folders_first: false, ..defaults() });

        assert_eq!(text, "project/\n├── a.txt\n├── b/\n└── C.txt");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hidden_files_are_left_out_unless_asked_for() {
        let (dir, root) = project("hidden", &[".env", "app.rs"]);

        let hidden = ascii(&root, defaults());
        let shown = ascii(&root, Options { hidden: true, ..defaults() });

        assert_eq!(hidden, "project/\n└── app.rs");
        assert_eq!(shown, "project/\n├── .env\n└── app.rs");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_only_leaves_out_files() {
        let (dir, root) = project("folders-only", FIXTURE);

        let text = ascii(&root, Options { folders_only: true, ..defaults() });

        assert_eq!(text, "project/\n├── docs/\n└── src/\n    └── lib/");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn depth_stops_below_the_chosen_level() {
        let (dir, root) = project("depth", FIXTURE);

        let text = ascii(&root, Options { depth: Some(1), folders_only: true, ..defaults() });

        assert_eq!(text, "project/\n├── docs/\n└── src/");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gitignore_patterns_negation_and_folder_only_rules_apply() {
        let (dir, root) = project(
            "gitignore",
            &["debug.log", "keep.log", "build/out.o", "build.rs", "src/build", "notes.txt"],
        );
        fs::write(root.join(".gitignore"), "*.log\n!keep.log\nbuild/\n# a comment\n\n").unwrap();

        let text = ascii(&root, defaults());

        assert_eq!(
            text,
            "project/\n├── src/\n│   └── build\n├── build.rs\n├── keep.log\n└── notes.txt"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_anchored_pattern_matches_only_at_its_own_level() {
        let (dir, root) = project("anchored", &["todo.txt", "docs/todo.txt"]);
        fs::write(root.join(".gitignore"), "/todo.txt\n").unwrap();

        let text = ascii(&root, defaults());

        assert_eq!(text, "project/\n└── docs/\n    └── todo.txt");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_nested_gitignore_applies_only_below_its_folder() {
        let (dir, root) = project("nested", &["a/x.tmp", "b/x.tmp"]);
        fs::write(root.join("a/.gitignore"), "*.tmp\n").unwrap();

        let text = ascii(&root, defaults());

        assert_eq!(text, "project/\n├── a/\n└── b/\n    └── x.tmp");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_nested_gitignore_can_re_include_what_an_outer_one_ignored() {
        let (dir, root) = project("reinclude", &["a/x.tmp", "b/x.tmp"]);
        fs::write(root.join(".gitignore"), "*.tmp\n").unwrap();
        fs::write(root.join("b/.gitignore"), "!x.tmp\n").unwrap();

        let text = ascii(&root, defaults());

        assert_eq!(text, "project/\n├── a/\n└── b/\n    └── x.tmp");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn double_star_patterns_cross_folders() {
        let (dir, root) = project(
            "globstar",
            &["cache/a", "deep/er/cache/b", "docs/guide.md", "docs/api/ref.md", "docs/api/ref.txt"],
        );
        fs::write(root.join(".gitignore"), "**/cache/\ndocs/**/*.md\n").unwrap();

        let text = ascii(&root, defaults());

        assert_eq!(
            text,
            "project/\n├── deep/\n│   └── er/\n└── docs/\n    └── api/\n        └── ref.txt"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_git_folder_is_left_out_with_gitignore_on() {
        let (dir, root) = project("dot-git", &[".git/HEAD", "app.rs"]);

        let text = ascii(&root, Options { hidden: true, ..defaults() });

        assert_eq!(text, "project/\n└── app.rs");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gitignore_is_ignored_when_switched_off() {
        let (dir, root) = project("gitignore-off", &["debug.log"]);
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();

        let text = ascii(&root, Options { gitignore: false, ..defaults() });

        assert_eq!(text, "project/\n└── debug.log");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_symlink_shows_its_target_and_is_not_followed() {
        let (dir, root) = project("symlink", &["real/inside.txt"]);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(root.join("real"), root.join("link"));
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(root.join("real"), root.join("link"));
        let Ok(()) = made.map_err(|e| eprintln!("skipped: creating a symlink failed ({e}); needs Developer Mode on Windows")) else {
            fs::remove_dir_all(&dir).unwrap();
            return;
        };

        let tree = walk(&root, defaults(), &CancelToken::default()).unwrap();

        let target = root.join("real").to_string_lossy().into_owned();
        assert_eq!(
            tree.ascii(),
            format!("project/\n├── real/\n│   └── inside.txt\n└── link -> {target}")
        );
        assert_eq!(tree.summary(), "1 folder · 1 file · 1 link");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancelled_walk_reports_cancelled() {
        let (dir, root) = project("cancelled", FIXTURE);
        let cancel = CancelToken::default();
        cancel.cancel();

        let result = walk(&root, defaults(), &cancel);

        assert!(matches!(result, Err(JobError::Cancelled)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn globs_match_stars_classes_and_escapes() {
        let pattern = |p: &str| p.chars().collect::<Vec<_>>();

        assert!(glob(&pattern("*.rs"), "main.rs"));
        assert!(!glob(&pattern("*.rs"), "main.rsx"));
        assert!(glob(&pattern("file?.[ch]"), "file1.h"));
        assert!(!glob(&pattern("file[!0-9]"), "file7"));
        assert!(glob(&pattern("\\#notes"), "#notes"));
        assert!(glob(&pattern("a*b*c"), "aXXbYYbc"));
    }

    #[test]
    fn a_trailing_escaped_space_is_kept_and_others_dropped() {
        assert_eq!(parse_line("trail   "), parse_line("trail"));
        assert_eq!(parse_line("trail\\ ").unwrap().parts, [['t', 'r', 'a', 'i', 'l', '\\', ' '].to_vec()]);
    }

    #[test]
    fn code_spans_grow_their_fence_past_backticks_inside() {
        assert_eq!(code("plain.md"), "`plain.md`");
        assert_eq!(code("a`b"), "``a`b``");
        assert_eq!(code("`x"), "`` `x ``");
    }

    #[test]
    fn a_long_preview_is_cut_at_a_line_break() {
        let text = format!("{}\n{}", "a".repeat(PREVIEW_BYTES - 10), "b".repeat(100));

        let shown = preview(&text);

        assert_eq!(shown.len(), PREVIEW_BYTES - 10);
    }
}
