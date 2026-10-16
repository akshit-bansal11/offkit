//! Markdown Editor: an edit pane with a formatting toolbar and a preview drawn natively. The
//! preview maps pulldown-cmark's events (GitHub-flavoured: tables, strikethrough, task lists)
//! to a flat list of blocks, each drawn by ui/tools/md-editor.slint; the text inside a block
//! goes through Slint's `StyledText`, so emphasis, code and links wrap with the paragraph. Raw
//! HTML is shown as text and never run, images are never loaded (their alt text is shown), and
//! only http and https links open (in the system browser). Open and Save use `.md` files; Export
//! HTML writes a standalone page whose raw HTML is escaped and whose links are limited to
//! http, https and mailto. The toolbar edits around the selection that `TextPane` exposes.

use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use slint::{ComponentHandle, ModelRc, StyledText, VecModel};

use crate::engine::text::{self, Latest};
use crate::{AppWindow, MdBlock, MdEditor, MdFormat, MdKind, files};

pub const SLUG: &str = "md-editor";
const TOOL: &str = "Markdown Editor";
const DEFAULT_NAME: &str = "document.md";
/// The website's starter document, minus its claims of PDF export and synced scrolling, which
/// were never true.
const DEFAULT_DOCUMENT: &str = "# Welcome to Markdown Editor

Write **bold**, *italic* and `inline code`, and see the result as you type.

## Features

- Live preview
- Toolbar shortcuts
- [x] Task lists
- Export to **.md** or **.html**

## Code Block

```javascript
function greet(name) {
  return `Hello, ${name}!`;
}
```

## Table

| Feature      | Status |
|--------------|--------|
| Live preview | Ready  |
| HTML export  | Ready  |

> The best tools are those that get out of your way.
> — Anonymous

---

Start writing, or use the toolbar above.
";
const HTML_SCHEMES: [&str; 3] = ["http", "https", "mailto"];
const OPEN_SCHEMES: [&str; 2] = ["http", "https"];
const TABLE_TEMPLATE: &str =
    "\n| Column 1 | Column 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |\n";
/// The top of Export HTML's page, up to the title. The page's policy allows no script and no
/// resource beyond its own inline style.
const HTML_PAGE_HEAD: &str = "<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<meta http-equiv=\"Content-Security-Policy\"
 content=\"default-src 'none'; style-src 'unsafe-inline'\">
<title>";
/// From the end of the title to the start of the body.
const HTML_PAGE_STYLE: &str = "</title>
<style>
:root { color-scheme: light dark; }
body { font: 16px/1.7 system-ui, sans-serif; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; }
h1, h2 { border-bottom: 1px solid #8884; padding-bottom: .3em; }
code, pre { font-family: ui-monospace, Consolas, monospace; background: #8882; border-radius: 4px; }
code { padding: .1em .3em; }
pre { padding: .8em 1em; overflow-x: auto; }
pre code { padding: 0; background: none; }
blockquote { margin-left: 0; padding-left: 1em; border-left: 3px solid #8886; opacity: .85; }
table { border-collapse: collapse; }
th, td { border: 1px solid #8886; padding: .3em .7em; }
th { background: #8882; }
hr { border: 0; border-top: 1px solid #8886; }
</style>
</head>
<body>
";
const HTML_PAGE_TAIL: &str = "</body>\n</html>\n";

// ---- Preview blocks -----------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Kind {
    #[default]
    Paragraph,
    Heading,
    Code,
    Rule,
    TableRow,
    Item,
}

/// One drawn block. `text` and `cells` are markdown that `StyledText` reads, built from the
/// parsed events with every literal character escaped (`text::styled_literal`).
#[derive(Clone, Debug, Default, PartialEq)]
struct Block {
    kind: Kind,
    level: i32,
    /// How many block quotes it sits in.
    quote: i32,
    /// How many lists deep it is.
    indent: i32,
    marker: String,
    /// 0 for no checkbox, 1 unchecked, 2 checked.
    task: i32,
    text: String,
    /// The block as plain text, for screen readers.
    label: String,
    code: String,
    cells: Vec<String>,
    header: bool,
}

/// The scheme of `url`, lower-cased, or `None` for a relative address. Whitespace and control
/// characters are ignored first, as a browser does (`java\tscript:` is still `javascript:`).
fn scheme(url: &str) -> Option<String> {
    let compact: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let (scheme, _) = compact.split_once(':')?;
    let tail_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.');
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(tail_ok);
    valid.then(|| scheme.to_ascii_lowercase())
}

fn is_safe_link(url: &str, allowed: &[&str]) -> bool {
    scheme(url).is_some_and(|s| allowed.contains(&s.as_str()))
}

/// `url` as a markdown link destination: the characters that would end it are percent-encoded.
fn markdown_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        match c {
            ' ' | '(' | ')' | '<' | '>' | '\\' => out.push_str(&format!("%{:02X}", c as u32)),
            '\u{e541}' => out.push_str("%EE%95%81"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// `code` as an inline code span: fenced by one more backtick than it holds, padded so that a
/// backtick or space at an end survives.
fn inline_code(code: &str) -> String {
    let code = code.replace(['\n', '\r'], " ").replace('\u{e541}', "\u{fffd}");
    let longest = code
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    let pad = if code.starts_with([' ', '`']) || code.ends_with([' ', '`']) {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{code}{pad}{fence}")
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    inline: String,
    plain: String,
    quote: i32,
    /// For each open list, the number of its next item, or `None` for a bullet list.
    lists: Vec<Option<u64>>,
    /// The marker and checkbox state of the item whose text is being read.
    item: Option<(String, i32)>,
    heading: Option<i32>,
    /// The text of the open code block (or HTML block, shown as code).
    code: Option<String>,
    in_header: bool,
    cells: Vec<String>,
    cell_labels: Vec<String>,
    /// For each open link, its destination if it is allowed to open.
    links: Vec<Option<String>>,
}

impl Builder {
    /// A heading or header cell is bold already; a bold span inside it would clash.
    fn bold_already(&self) -> bool {
        self.heading.is_some() || self.in_header
    }

    fn depth(&self) -> i32 {
        self.lists.len() as i32
    }

    /// Ends the paragraph or list item being read.
    fn flush(&mut self) {
        let text = std::mem::take(&mut self.inline);
        let label = std::mem::take(&mut self.plain);
        if text.trim().is_empty() {
            return;
        }
        let mut block = Block {
            text,
            label,
            quote: self.quote,
            indent: self.depth(),
            ..Block::default()
        };
        if let Some((marker, task)) = self.item.take() {
            block.kind = Kind::Item;
            block.marker = marker;
            block.task = task;
            block.indent -= 1;
        }
        self.blocks.push(block);
    }

    fn push(&mut self, block: Block) {
        self.blocks.push(Block {
            quote: self.quote,
            indent: self.depth(),
            ..block
        });
    }

    fn push_row(&mut self) {
        let label = std::mem::take(&mut self.cell_labels).join(" | ");
        let cells = std::mem::take(&mut self.cells);
        self.push(Block {
            kind: Kind::TableRow,
            cells,
            label,
            header: self.in_header,
            ..Block::default()
        });
    }

    fn text(&mut self, text: &str) {
        if let Some(code) = &mut self.code {
            code.push_str(text);
        } else {
            self.plain.push_str(text);
            self.inline.push_str(&text::styled_literal(text));
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => self.text(&t),
            Event::Code(t) => {
                self.plain.push_str(&t);
                self.inline.push_str(&inline_code(&t));
            }
            Event::SoftBreak => {
                self.plain.push(' ');
                self.inline.push(' ');
            }
            Event::HardBreak => {
                self.plain.push(' ');
                self.inline.push_str("\\\n");
            }
            Event::Rule => {
                self.flush();
                self.push(Block {
                    kind: Kind::Rule,
                    ..Block::default()
                });
            }
            Event::TaskListMarker(done) => {
                if let Some((marker, task)) = &mut self.item {
                    marker.clear();
                    *task = if done { 2 } else { 1 };
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.heading = Some(level as i32);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote += 1;
            }
            Tag::CodeBlock(_) | Tag::HtmlBlock => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::List(first) => {
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush();
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}.", *n - 1)
                    }
                    _ => "•".to_string(),
                };
                self.item = Some((marker, 0));
            }
            Tag::Table(_) => self.flush(),
            Tag::TableHead => self.in_header = true,
            Tag::TableCell => {
                self.inline.clear();
                self.plain.clear();
            }
            Tag::Emphasis => self.inline.push('*'),
            Tag::Strong if !self.bold_already() => self.inline.push_str("**"),
            Tag::Strikethrough => self.inline.push_str("~~"),
            Tag::Link { dest_url, .. } => {
                if is_safe_link(&dest_url, &OPEN_SCHEMES) {
                    self.inline.push('[');
                    self.links.push(Some(markdown_url(&dest_url)));
                } else {
                    self.links.push(None);
                }
            }
            // Images are never loaded; the alt text is shown in their place.
            Tag::Image { .. } => self.text("[Image: "),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(_) => {
                let text = std::mem::take(&mut self.inline);
                let label = std::mem::take(&mut self.plain);
                let level = self.heading.take().unwrap_or(1);
                self.push(Block {
                    kind: Kind::Heading,
                    level,
                    text: format!("**{text}**"),
                    label,
                    ..Block::default()
                });
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote -= 1;
            }
            TagEnd::CodeBlock | TagEnd::HtmlBlock => {
                let code = self.code.take().unwrap_or_default();
                let code = code.trim_end_matches('\n').to_string();
                self.push(Block {
                    kind: Kind::Code,
                    label: code.clone(),
                    code,
                    ..Block::default()
                });
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                self.item = None;
            }
            TagEnd::TableHead => {
                self.push_row();
                self.in_header = false;
            }
            TagEnd::TableRow => self.push_row(),
            TagEnd::TableCell => {
                let text = std::mem::take(&mut self.inline);
                self.cell_labels.push(std::mem::take(&mut self.plain));
                self.cells.push(if self.in_header && !text.is_empty() {
                    format!("**{text}**")
                } else {
                    text
                });
            }
            TagEnd::Emphasis => self.inline.push('*'),
            TagEnd::Strong if !self.bold_already() => self.inline.push_str("**"),
            TagEnd::Strikethrough => self.inline.push_str("~~"),
            TagEnd::Link => {
                if let Some(Some(url)) = self.links.pop() {
                    self.inline.push_str(&format!("]({url})"));
                }
            }
            TagEnd::Image => self.text("]"),
            _ => {}
        }
    }
}

fn parser(markdown: &str) -> Parser<'_> {
    Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
}

fn blocks(markdown: &str) -> Vec<Block> {
    let mut builder = Builder::default();
    for event in parser(markdown) {
        builder.event(event);
    }
    builder.flush();
    builder.blocks
}

// ---- HTML export --------------------------------------------------------------------------

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A standalone page for `markdown`. Raw HTML in it is escaped, not passed through; links keep
/// only the http, https and mailto schemes (the others become plain text); images become their
/// alt text. The page also carries a policy that allows no scripts and no outside resources.
fn to_html(markdown: &str, title: &str) -> String {
    let mut kept_links: Vec<bool> = Vec::new();
    let events = parser(markdown).filter_map(move |event| {
        if let Event::Start(Tag::Link { dest_url, .. }) = &event {
            let keep = is_safe_link(dest_url, &HTML_SCHEMES);
            kept_links.push(keep);
            return keep.then_some(event);
        }
        match event {
            Event::Html(t) | Event::InlineHtml(t) => Some(Event::Text(t)),
            Event::Start(Tag::HtmlBlock) => {
                Some(Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)))
            }
            Event::End(TagEnd::HtmlBlock) => Some(Event::End(TagEnd::CodeBlock)),
            Event::Start(Tag::Image { .. }) => Some(Event::Text("[Image: ".into())),
            Event::End(TagEnd::Image) => Some(Event::Text("]".into())),
            Event::End(TagEnd::Link) => kept_links.pop().unwrap_or(false).then_some(event),
            event => Some(event),
        }
    });
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, events);
    format!(
        "{HTML_PAGE_HEAD}{}{HTML_PAGE_STYLE}{body}{HTML_PAGE_TAIL}",
        escape_html(title)
    )
}

// ---- Toolbar edits ------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Format {
    Bold,
    Italic,
    Code,
    Heading(usize),
    Link,
    Quote,
    Bullet,
    Numbered,
    Rule,
    Table,
}

/// The text after a toolbar action and the selection to put on it, in UTF-8 byte offsets.
#[derive(Debug, PartialEq)]
struct Edit {
    text: String,
    anchor: usize,
    cursor: usize,
}

/// The nearest character boundary at or before `offset`, kept inside `text`.
fn boundary(text: &str, offset: i32) -> usize {
    text.floor_char_boundary(offset.max(0) as usize)
}

fn apply(format: Format, text: &str, anchor: i32, cursor: i32) -> Edit {
    let (a, b) = (boundary(text, anchor), boundary(text, cursor));
    let (start, end) = (a.min(b), a.max(b));
    match format {
        Format::Bold => wrap(text, start, end, "**", "**", "bold text"),
        Format::Italic => wrap(text, start, end, "*", "*", "italic text"),
        Format::Code => wrap(text, start, end, "`", "`", "code"),
        Format::Link => {
            let wrapped = wrap(text, start, end, "[", "](url)", "link text");
            // Select the address, which is what the person types next.
            let url = wrapped.cursor + 2;
            Edit {
                anchor: url,
                cursor: url + "url".len(),
                ..wrapped
            }
        }
        Format::Heading(level) => {
            let prefix = format!("{} ", "#".repeat(level));
            let placeholder = format!("Heading {level}");
            prefix_lines(text, start, end, |_| prefix.clone(), true, &placeholder)
        }
        Format::Quote => prefix_lines(text, start, end, |_| "> ".into(), false, "Quote"),
        Format::Bullet => prefix_lines(text, start, end, |_| "- ".into(), false, "List item"),
        Format::Numbered => {
            prefix_lines(text, start, end, |n| format!("{}. ", n + 1), false, "List item")
        }
        Format::Rule => insert(text, end, "\n---\n"),
        Format::Table => insert(text, end, TABLE_TEMPLATE),
    }
}

/// `open` and `close` around the selection, or around `placeholder` when nothing is selected;
/// the text between them ends up selected.
fn wrap(text: &str, start: usize, end: usize, open: &str, close: &str, placeholder: &str) -> Edit {
    let inner = if start == end {
        placeholder
    } else {
        &text[start..end]
    };
    let anchor = start + open.len();
    Edit {
        text: format!("{}{open}{inner}{close}{}", &text[..start], &text[end..]),
        anchor,
        cursor: anchor + inner.len(),
    }
}

/// A heading's text without its `#` marks, or the line itself when it isn't a heading.
fn heading_content(line: &str) -> &str {
    let rest = line.trim_start_matches('#');
    if rest.len() < line.len() && rest.starts_with(' ') {
        rest.trim_start()
    } else {
        line
    }
}

/// Puts `prefix(n)` before the n-th non-blank line of every line the selection touches (an
/// empty line gets the prefix and `placeholder`, selected). A heading replaces any marks the
/// line has.
fn prefix_lines(
    text: &str,
    start: usize,
    end: usize,
    prefix: impl Fn(usize) -> String,
    replace_heading: bool,
    placeholder: &str,
) -> Edit {
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    // A selection that ends just after a line break doesn't reach the next line.
    let last = if end > start && text[..end].ends_with('\n') {
        end - 1
    } else {
        end
    };
    let line_end = text[last..].find('\n').map_or(text.len(), |i| last + i);
    let block = &text[line_start..line_end];
    let (changed, anchor, cursor) = if start == end && block.trim().is_empty() {
        let lead = prefix(0);
        let anchor = line_start + lead.len();
        (format!("{lead}{placeholder}"), anchor, anchor + placeholder.len())
    } else {
        let mut n = 0;
        let changed = block
            .split('\n')
            .map(|line| {
                if line.trim().is_empty() {
                    return line.to_string();
                }
                n += 1;
                let body = if replace_heading {
                    heading_content(line)
                } else {
                    line
                };
                format!("{}{body}", prefix(n - 1))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let cursor = line_start + changed.len();
        (changed, line_start, cursor)
    };
    Edit {
        text: format!("{}{changed}{}", &text[..line_start], &text[line_end..]),
        anchor,
        cursor,
    }
}

/// `snippet` after the selection, with the caret after it.
fn insert(text: &str, at: usize, snippet: &str) -> Edit {
    let caret = at + snippet.len();
    Edit {
        text: format!("{}{snippet}{}", &text[..at], &text[at..]),
        anchor: caret,
        cursor: caret,
    }
}

// ---- Slint models -------------------------------------------------------------------------

/// `markdown` as styled text. It is built by `Builder` from escaped pieces and should always
/// parse; if it ever doesn't, the person sees the markup instead of nothing.
fn styled(markdown: &str) -> StyledText {
    StyledText::from_markdown(markdown).unwrap_or_else(|_| StyledText::from_plain_text(markdown))
}

fn slint_blocks(blocks: &[Block]) -> ModelRc<MdBlock> {
    let blocks: Vec<MdBlock> = blocks
        .iter()
        .map(|b| {
            let cells: Vec<StyledText> = b.cells.iter().map(|c| styled(c)).collect();
            MdBlock {
                kind: match b.kind {
                    Kind::Paragraph => MdKind::Paragraph,
                    Kind::Heading => MdKind::Heading,
                    Kind::Code => MdKind::Code,
                    Kind::Rule => MdKind::Rule,
                    Kind::TableRow => MdKind::TableRow,
                    Kind::Item => MdKind::Item,
                },
                level: b.level,
                quote: b.quote,
                indent: b.indent,
                marker: b.marker.as_str().into(),
                task: b.task,
                text: styled(&b.text),
                label: b.label.as_str().into(),
                code: b.code.as_str().into(),
                cells: ModelRc::new(VecModel::from(cells)),
                header: b.header,
            }
        })
        .collect();
    ModelRc::new(VecModel::from(blocks))
}

// ---- State and wiring ---------------------------------------------------------------------

struct State {
    /// The opened file's name, which Save and Export HTML suggest.
    name: String,
    rendering: Latest,
    typing: slint::Timer,
}

impl Default for State {
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.into(),
            rendering: Latest::default(),
            typing: slint::Timer::default(),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

fn format_of(format: MdFormat) -> Format {
    match format {
        MdFormat::Bold => Format::Bold,
        MdFormat::Italic => Format::Italic,
        MdFormat::Code => Format::Code,
        MdFormat::Heading1 => Format::Heading(1),
        MdFormat::Heading2 => Format::Heading(2),
        MdFormat::Heading3 => Format::Heading(3),
        MdFormat::Link => Format::Link,
        MdFormat::Quote => Format::Quote,
        MdFormat::Bullet => Format::Bullet,
        MdFormat::Numbered => Format::Numbered,
        MdFormat::Rule => Format::Rule,
        MdFormat::Table => Format::Table,
    }
}

pub fn init(app: &AppWindow) {
    let g = app.global::<MdEditor>();
    g.set_text(DEFAULT_DOCUMENT.into());
    let weak = app.as_weak();
    g.on_edited(move || {
        if let Some(app) = weak.upgrade() {
            with(|s| text::debounce(&s.typing, &app, refresh));
        }
    });
    let weak = app.as_weak();
    g.on_format(move |format, anchor, cursor| {
        let Some(app) = weak.upgrade() else { return };
        let g = app.global::<MdEditor>();
        let edit = apply(format_of(format), g.get_text().as_str(), anchor, cursor);
        g.set_text(edit.text.as_str().into());
        g.set_select_anchor(edit.anchor as i32);
        g.set_select_cursor(edit.cursor as i32);
        g.set_select_count(g.get_select_count() + 1);
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_open_file(move || {
        let Some(app) = weak.upgrade() else { return };
        let filter = Some(("Markdown", &["md", "markdown", "txt"][..]));
        if let Some(path) = files::choose_file("Open Markdown", filter) {
            open_file(&app, path);
        }
    });
    let weak = app.as_weak();
    g.on_save_file(move || {
        if let Some(app) = weak.upgrade() {
            let name = with(|s| s.name.clone());
            save_as(&app, "Save Markdown", &name, |markdown, _| markdown.to_string());
        }
    });
    let weak = app.as_weak();
    g.on_export_html(move || {
        if let Some(app) = weak.upgrade() {
            let stem = with(|s| s.name.trim_end_matches(".md").to_string());
            save_as(&app, "Export HTML", &format!("{stem}.html"), to_html);
        }
    });
    let weak = app.as_weak();
    g.on_reset(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.name = DEFAULT_NAME.into());
        app.global::<MdEditor>().set_text(DEFAULT_DOCUMENT.into());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_open_link(move |url| {
        let Some(app) = weak.upgrade() else { return };
        // The preview only makes http and https links, but the address is checked again here:
        // this is the one place that hands text to the operating system.
        if !is_safe_link(&url, &OPEN_SCHEMES) {
            return app
                .global::<MdEditor>()
                .set_error("Only http and https links open.".into());
        }
        if let Err(e) = open::that_detached(url.as_str()) {
            app.global::<MdEditor>()
                .set_error(format!("Couldn't open the link: {e}").into());
        }
    });
    refresh(app);
}

pub fn set_drop_hover(app: &AppWindow, hover: bool) {
    app.global::<MdEditor>().set_drop_hover(hover);
}

/// A dropped Markdown file replaces the text; one at a time.
pub fn add_paths(app: &AppWindow, paths: Vec<PathBuf>) {
    match <[PathBuf; 1]>::try_from(paths) {
        Ok([path]) => open_file(app, path),
        Err(paths) if paths.is_empty() => {}
        Err(_) => app
            .global::<MdEditor>()
            .set_error("Drop one file at a time.".into()),
    }
}

pub fn shutdown() -> Vec<JoinHandle<()>> {
    with(|s| s.rendering.shutdown())
}

fn open_file(app: &AppWindow, path: PathBuf) {
    let g = app.global::<MdEditor>();
    let file = match text::read_text_file(&path, TOOL) {
        Ok(file) => file,
        Err(message) => return g.set_error(message.into()),
    };
    if !text::fits_pane(&file.text) {
        return g.set_error(
            format!(
                "{} is too long to edit here. {TOOL} takes up to {}.",
                file.name,
                text::counted(text::PANE_MAX_CHARS, "character", "characters")
            )
            .into(),
        );
    }
    with(|s| s.name = file.name);
    g.set_text(file.text.into());
    refresh(app);
}

/// Saves `make(text, title)` to a file the person picks.
fn save_as(app: &AppWindow, title: &str, suggested: &str, make: impl Fn(&str, &str) -> String) {
    let g = app.global::<MdEditor>();
    let Some(path) = files::choose_save_path(title, suggested) else {
        return;
    };
    let heading = files::file_name(&path);
    let contents = make(g.get_text().as_str(), &heading);
    match text::save_text(&path, &contents) {
        Ok(note) => {
            g.set_error("".into());
            g.set_note(note.into());
        }
        Err(error) => g.set_error(error.into()),
    }
}

/// Re-reads the text and redraws the preview, in the background when the text is long.
fn refresh(app: &AppWindow) {
    let g = app.global::<MdEditor>();
    g.set_error("".into());
    g.set_note("".into());
    let markdown = g.get_text().to_string();
    g.set_stats(text::stats(&markdown).into());
    let rendering = with(|s| s.rendering.clone());
    rendering.run(app, markdown.len(), move || blocks(&markdown), show);
}

fn show(app: &AppWindow, blocks: Vec<Block>) {
    app.global::<MdEditor>().set_blocks(slint_blocks(&blocks));
}

#[cfg(test)]
mod tests {
    use super::*;

    const GFM: &str = "# Title

Some **bold**, *em*, ~~gone~~ and `code` with [link](https://example.com) and [bad](javascript:alert(1)).

> quoted *text*

- one
- [x] done
- [ ] todo
  - nested

1. first
2. second

```rust
fn main() {}
```

| A | B |
|---|---|
| 1 | 2 |

---
";

    fn kinds(blocks: &[Block]) -> Vec<Kind> {
        blocks.iter().map(|b| b.kind).collect()
    }

    #[test]
    fn a_gfm_document_maps_to_blocks_in_order() {
        let blocks = blocks(GFM);

        assert_eq!(
            kinds(&blocks),
            [
                Kind::Heading,
                Kind::Paragraph,
                Kind::Paragraph,
                Kind::Item,
                Kind::Item,
                Kind::Item,
                Kind::Item,
                Kind::Item,
                Kind::Item,
                Kind::Code,
                Kind::TableRow,
                Kind::TableRow,
                Kind::Rule,
            ]
        );
    }

    #[test]
    fn headings_carry_their_level_and_are_bold() {
        let blocks = blocks("## Two");

        assert_eq!((blocks[0].level, blocks[0].text.as_str()), (2, "**Two**"));
    }

    #[test]
    fn quotes_lists_and_tasks_keep_their_structure() {
        let blocks = blocks(GFM);

        assert_eq!(blocks[2].quote, 1);
        let items: Vec<_> = blocks[3..9]
            .iter()
            .map(|b| (b.marker.as_str(), b.task, b.indent))
            .collect();
        assert_eq!(
            items,
            [
                ("•", 0, 0),
                ("", 2, 0),
                ("", 1, 0),
                ("•", 0, 1),
                ("1.", 0, 0),
                ("2.", 0, 0),
            ]
        );
    }

    #[test]
    fn code_blocks_keep_their_text_and_tables_their_cells() {
        let blocks = blocks(GFM);

        assert_eq!(blocks[9].code, "fn main() {}");
        assert_eq!(blocks[10].cells, ["**A**", "**B**"]);
        assert!(blocks[10].header && !blocks[11].header);
        assert_eq!(blocks[11].cells, ["1", "2"]);
        assert_eq!(blocks[11].label, "1 | 2");
    }

    #[test]
    fn inline_markup_becomes_styled_text_and_only_web_links_stay_links() {
        let paragraph = &blocks(GFM)[1];

        assert!(paragraph.text.contains("**bold**"));
        assert!(paragraph.text.contains("~~gone~~"));
        assert!(paragraph.text.contains("[link](https://example.com)"));
        assert!(!paragraph.text.contains("javascript"));
        assert!(paragraph.label.contains("bad"));
    }

    #[test]
    fn every_block_parses_as_styled_text() {
        let nasty = concat!(
            "# <b>x</b> & [a](http://x.y/a(b)) \u{e541} `` ` ``\n\n",
            "*a*b **c**_d_ \\* \\\n\n",
            "![alt *x*](http://e/i.png) <script>1</script>",
        );

        for block in blocks(GFM).iter().chain(&blocks(nasty)) {
            assert!(StyledText::from_markdown(&block.text).is_ok(), "{block:?}");
            assert!(block.cells.iter().all(|c| StyledText::from_markdown(c).is_ok()));
        }
    }

    #[test]
    fn images_show_their_alt_text_and_raw_html_shows_as_text() {
        let blocks = blocks("![a logo](http://e/i.png)\n\n<div>hi</div>");

        assert_eq!(blocks[0].label, "[Image: a logo]");
        assert_eq!(blocks[1].kind, Kind::Code);
        assert_eq!(blocks[1].code, "<div>hi</div>");
    }

    #[test]
    fn schemes_ignore_hidden_whitespace_and_case() {
        assert!(is_safe_link("HTTPS://a.b", &OPEN_SCHEMES));
        assert!(!is_safe_link("java\tscript:alert(1)", &HTML_SCHEMES));
        assert!(!is_safe_link("/relative", &HTML_SCHEMES));
        assert!(!is_safe_link("mailto:a@b.c", &OPEN_SCHEMES));
        assert!(is_safe_link("mailto:a@b.c", &HTML_SCHEMES));
    }

    #[test]
    fn exported_html_escapes_raw_html_and_drops_unsafe_links() {
        let html = to_html(
            "<script>alert(1)</script>\n\nhi <img src=x onerror=y> [bad](javascript:alert(1)) \
             [ok](https://a.b) ![pic](http://e/p.png)",
            "a<b>.md",
        );

        assert!(!html.contains("<script>") && !html.contains("<img"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("<a href=\"https://a.b\">ok</a>"));
        assert!(html.contains("[Image: pic]"));
        assert!(html.contains("<title>a&lt;b&gt;.md</title>"));
    }

    #[test]
    fn bold_wraps_the_selection_or_a_placeholder() {
        assert_eq!(
            apply(Format::Bold, "ab cd", 3, 5),
            Edit {
                text: "ab **cd**".into(),
                anchor: 5,
                cursor: 7
            }
        );
        let empty = apply(Format::Bold, "x", 1, 1);
        assert_eq!(empty.text, "x**bold text**");
        assert_eq!(&empty.text[empty.anchor..empty.cursor], "bold text");
    }

    #[test]
    fn edits_on_multi_byte_text_stay_on_character_boundaries() {
        let wrapped = apply(Format::Bold, "é", 0, 2);
        assert_eq!(wrapped.text, "**é**");
        assert_eq!((wrapped.anchor, wrapped.cursor), (2, 4));

        // Offset 1 is inside “é”; it moves back to the start instead of panicking.
        let inside = apply(Format::Italic, "éa", 1, 1);
        assert_eq!(inside.text, "*italic text*éa");

        let past_the_end = apply(Format::Code, "é", 99, -4);
        assert_eq!(past_the_end.text, "`é`");

        let heading = apply(Format::Heading(2), "éa", 1, 1);
        assert_eq!(heading.text, "## éa");
    }

    #[test]
    fn a_reversed_selection_edits_the_same_text() {
        assert_eq!(
            apply(Format::Italic, "ab", 2, 0),
            apply(Format::Italic, "ab", 0, 2)
        );
    }

    #[test]
    fn a_link_selects_its_address() {
        let edit = apply(Format::Link, "é", 0, 2);

        assert_eq!(edit.text, "[é](url)");
        assert_eq!(&edit.text[edit.anchor..edit.cursor], "url");
    }

    #[test]
    fn headings_replace_existing_marks_and_empty_lines_get_a_placeholder() {
        assert_eq!(apply(Format::Heading(1), "## old", 0, 0).text, "# old");
        assert_eq!(apply(Format::Heading(1), "#tag", 0, 0).text, "# #tag");

        let empty = apply(Format::Heading(3), "a\n\nb", 2, 2);
        assert_eq!(empty.text, "a\n### Heading 3\nb");
        assert_eq!(&empty.text[empty.anchor..empty.cursor], "Heading 3");
    }

    #[test]
    fn lists_prefix_every_line_touched_and_number_them() {
        let text = "a\nb\n\nc\nd";

        assert_eq!(apply(Format::Numbered, text, 0, 6).text, "1. a\n2. b\n\n3. c\nd");
        assert_eq!(apply(Format::Bullet, text, 0, 3).text, "- a\n- b\n\nc\nd");
        assert_eq!(apply(Format::Quote, "é\nx", 0, 3).text, "> é\nx");
    }

    #[test]
    fn rules_and_tables_are_inserted_after_the_selection() {
        let rule = apply(Format::Rule, "ab", 0, 1);
        assert_eq!(rule.text, "a\n---\nb");
        assert_eq!(rule.cursor, 6);

        assert!(apply(Format::Table, "", 0, 0).text.starts_with("\n| Column 1 |"));
    }
}
