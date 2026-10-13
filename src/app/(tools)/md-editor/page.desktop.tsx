"use client";

import DOMPurify from "dompurify";
import {
  Bold,
  Code,
  Download,
  FileCode2,
  Heading1,
  Heading2,
  Heading3,
  Image,
  Italic,
  Link,
  List,
  ListOrdered,
  MessageSquare,
  Minus,
  Printer,
  RotateCcw,
  Table,
} from "lucide-react";
import { marked } from "marked";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Input } from "@/components/ui/form/Input";
import { Card } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("md-editor");

const DEFAULT_MARKDOWN = `# Welcome to Markdown Editor

Write **bold**, *italic*, or \`inline code\` with live preview.

## Features
- Live split-pane preview
- Toolbar shortcuts
- Export to **.md** file or **PDF**

## Code Block

\`\`\`javascript
const greet = (name) => \`Hello, \${name}!\`;
console.log(greet("World"));
\`\`\`

## Table

| Feature | Status |
|---------|--------|
| Live preview | ✅ |
| PDF export | ✅ |
| Sync scroll | ✅ |

> "The best tools are those that get out of your way." — Anonymous

---

Start writing below or use the toolbar above!
`;

export default function MdEditorPage() {
  if (!tool) return null;
  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <MdEditorTool />
    </ToolPageShell>
  );
}

interface ToolbarButton {
  label: string;
  icon: React.ReactNode;
  action: (
    ta: HTMLTextAreaElement,
    val: string,
    set: (s: string) => void,
  ) => void;
}

function insertWrapped(
  ta: HTMLTextAreaElement,
  val: string,
  set: (s: string) => void,
  before: string,
  after: string,
  placeholder: string,
) {
  const start = ta.selectionStart;
  const end = ta.selectionEnd;
  const sel = val.slice(start, end) || placeholder;
  const next = val.slice(0, start) + before + sel + after + val.slice(end);
  set(next);
  setTimeout(() => {
    ta.selectionStart = start + before.length;
    ta.selectionEnd = start + before.length + sel.length;
    ta.focus();
  }, 0);
}

function insertLine(
  ta: HTMLTextAreaElement,
  val: string,
  set: (s: string) => void,
  prefix: string,
  placeholder: string,
) {
  const start = ta.selectionStart;
  // find start of line
  const lineStart = val.lastIndexOf("\n", start - 1) + 1;
  const lineEnd = val.indexOf("\n", start);
  const line = val.slice(lineStart, lineEnd === -1 ? undefined : lineEnd);
  const newLine = prefix + (line || placeholder);
  const next =
    val.slice(0, lineStart) +
    newLine +
    (lineEnd === -1 ? "" : val.slice(lineEnd));
  set(next);
  setTimeout(() => {
    ta.selectionStart = lineStart + prefix.length;
    ta.selectionEnd = lineStart + newLine.length;
    ta.focus();
  }, 0);
}

function TOOLBAR_BUTTONS(): ToolbarButton[] {
  return [
    {
      label: "Bold",
      icon: <Bold className="size-3.5" />,
      action: (ta, val, set) =>
        insertWrapped(ta, val, set, "**", "**", "bold text"),
    },
    {
      label: "Italic",
      icon: <Italic className="size-3.5" />,
      action: (ta, val, set) =>
        insertWrapped(ta, val, set, "*", "*", "italic text"),
    },
    {
      label: "Code",
      icon: <Code className="size-3.5" />,
      action: (ta, val, set) => insertWrapped(ta, val, set, "`", "`", "code"),
    },
    { label: "divider", icon: null, action: () => {} },
    {
      label: "H1",
      icon: <Heading1 className="size-3.5" />,
      action: (ta, val, set) => insertLine(ta, val, set, "# ", "Heading 1"),
    },
    {
      label: "H2",
      icon: <Heading2 className="size-3.5" />,
      action: (ta, val, set) => insertLine(ta, val, set, "## ", "Heading 2"),
    },
    {
      label: "H3",
      icon: <Heading3 className="size-3.5" />,
      action: (ta, val, set) => insertLine(ta, val, set, "### ", "Heading 3"),
    },
    { label: "divider", icon: null, action: () => {} },
    {
      label: "Link",
      icon: <Link className="size-3.5" />,
      action: (ta, val, set) => {
        const start = ta.selectionStart;
        const end = ta.selectionEnd;
        const sel = val.slice(start, end) || "link text";
        const next = val.slice(0, start) + `[${sel}](url)` + val.slice(end);
        set(next);
        setTimeout(() => {
          ta.selectionStart = start + sel.length + 3;
          ta.selectionEnd = start + sel.length + 6;
          ta.focus();
        }, 0);
      },
    },
    {
      label: "Image",
      // eslint-disable-next-line jsx-a11y/alt-text
      icon: <Image className="size-3.5" />,
      action: (ta, val, set) => {
        const start = ta.selectionStart;
        const snippet = `![Alt text](image-url)`;
        const next = val.slice(0, start) + snippet + val.slice(start);
        set(next);
        setTimeout(() => {
          ta.selectionStart = start + 2;
          ta.selectionEnd = start + 10;
          ta.focus();
        }, 0);
      },
    },
    { label: "divider", icon: null, action: () => {} },
    {
      label: "Bullet list",
      icon: <List className="size-3.5" />,
      action: (ta, val, set) => insertLine(ta, val, set, "- ", "List item"),
    },
    {
      label: "Numbered list",
      icon: <ListOrdered className="size-3.5" />,
      action: (ta, val, set) => insertLine(ta, val, set, "1. ", "List item"),
    },
    {
      label: "Blockquote",
      icon: <MessageSquare className="size-3.5" />,
      action: (ta, val, set) => insertLine(ta, val, set, "> ", "Quote"),
    },
    {
      label: "HR",
      icon: <Minus className="size-3.5" />,
      action: (ta, val, set) => {
        const start = ta.selectionStart;
        const next = val.slice(0, start) + "\n---\n" + val.slice(start);
        set(next);
        setTimeout(() => {
          ta.selectionStart = ta.selectionEnd = start + 5;
          ta.focus();
        }, 0);
      },
    },
    {
      label: "Table",
      icon: <Table className="size-3.5" />,
      action: (ta, val, set) => {
        const start = ta.selectionStart;
        const snippet = `\n| Column 1 | Column 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |\n`;
        const next = val.slice(0, start) + snippet + val.slice(start);
        set(next);
        setTimeout(() => {
          ta.selectionStart = ta.selectionEnd = start + snippet.length;
          ta.focus();
        }, 0);
      },
    },
  ];
}

function MdEditorTool() {
  const [markdown, setMarkdown] = useState(DEFAULT_MARKDOWN);
  const [renderedHtml, setRenderedHtml] = useState("");
  const [fileName, setFileName] = useState("document");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    (async () => {
      const raw = await marked.parse(markdown);
      if (typeof window !== "undefined") {
        setRenderedHtml(DOMPurify.sanitize(raw));
      } else {
        setRenderedHtml(raw);
      }
    })();
  }, [markdown]);

  const handleToolbarAction = useCallback(
    (action: ToolbarButton["action"]) => {
      const ta = textareaRef.current;
      if (!ta) return;
      action(ta, markdown, setMarkdown);
    },
    [markdown],
  );

  const downloadMd = () => {
    const blob = new Blob([markdown], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${fileName || "document"}.md`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const printPdf = () => {
    window.print();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Tab") {
      e.preventDefault();
      const ta = e.currentTarget;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const next = markdown.slice(0, start) + "  " + markdown.slice(end);
      setMarkdown(next);
      setTimeout(() => {
        ta.selectionStart = ta.selectionEnd = start + 2;
      }, 0);
    }
  };

  const buttons = TOOLBAR_BUTTONS();

  return (
    <div className="space-y-4">
      <style>{`
        @media print {
          body > * { display: none; }
          .md-print-target { display: block !important; }
        }
        .md-print-target { display: none; }
        .prose h1 { font-size: 2rem; font-weight: 700; margin-bottom: 0.5rem; border-bottom: 1px solid rgba(255,255,255,0.08); padding-bottom: 0.5rem; }
        .prose h2 { font-size: 1.5rem; font-weight: 600; margin-top: 1.5rem; margin-bottom: 0.5rem; }
        .prose h3 { font-size: 1.25rem; font-weight: 600; margin-top: 1.25rem; margin-bottom: 0.25rem; }
        .prose p { margin-bottom: 0.75rem; line-height: 1.7; }
        .prose ul, .prose ol { padding-left: 1.5rem; margin-bottom: 0.75rem; }
        .prose li { margin-bottom: 0.25rem; }
        .prose ul li { list-style-type: disc; }
        .prose ol li { list-style-type: decimal; }
        .prose code { background: rgba(255,255,255,0.08); border-radius: 4px; padding: 0.1em 0.4em; font-family: monospace; font-size: 0.875em; }
        .prose pre { background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.08); border-radius: 8px; padding: 1rem; overflow-x: auto; margin-bottom: 1rem; }
        .prose pre code { background: none; padding: 0; }
        .prose blockquote { border-left: 3px solid rgba(255,255,255,0.15); padding-left: 1rem; color: #9ca3af; margin-bottom: 0.75rem; }
        .prose table { width: 100%; border-collapse: collapse; margin-bottom: 1rem; font-size: 0.875rem; }
        .prose th, .prose td { border: 1px solid rgba(255,255,255,0.1); padding: 0.5rem 0.75rem; }
        .prose th { background: rgba(255,255,255,0.05); font-weight: 600; }
        .prose hr { border: none; border-top: 1px solid rgba(255,255,255,0.1); margin: 1.5rem 0; }
        .prose a { color: #60a5fa; text-decoration: underline; }
        .prose strong { font-weight: 700; }
        .prose em { font-style: italic; }
        .prose img { max-width: 100%; border-radius: 8px; }
      `}</style>

      {/* Toolbar */}
      <Card className="border-white/10 bg-white/[0.015] px-3 py-2">
        <div className="flex flex-wrap items-center gap-0.5">
          {buttons.map((btn, i) => {
            if (btn.label === "divider") {
              return <div key={i} className="mx-1 h-5 w-px bg-white/10" />;
            }
            return (
              <button
                key={i}
                title={btn.label}
                onClick={() => handleToolbarAction(btn.action)}
                className="flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground"
              >
                {btn.icon}
              </button>
            );
          })}

          <div className="ml-auto flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="h-7 gap-1.5 border border-white/10 px-2 text-[11px] text-muted-foreground"
              onClick={() => setMarkdown(DEFAULT_MARKDOWN)}
            >
              <RotateCcw className="size-3" />
              Reset
            </Button>
          </div>
        </div>
      </Card>

      {/* Split pane */}
      <div className="grid gap-4 lg:grid-cols-2">
        {/* Editor */}
        <div className="flex flex-col">
          <div className="mb-1.5 flex items-center gap-2 px-1">
            <FileCode2 className="size-3.5 text-muted-foreground" />
            <p className="text-xs font-medium text-muted-foreground">Editor</p>
            <Badge
              variant="outline"
              className="ml-auto border-white/10 bg-white/[0.04] px-1.5 py-0 text-[10px]"
            >
              {markdown.length} chars
            </Badge>
          </div>
          <textarea
            ref={textareaRef}
            value={markdown}
            onChange={(e) => setMarkdown(e.target.value)}
            onKeyDown={handleKeyDown}
            spellCheck={false}
            className="min-h-[600px] flex-1 resize-none rounded-2xl border border-white/10 bg-black/30 p-4 font-mono text-sm leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-white/20 focus:bg-black/40"
            placeholder="# Start writing..."
          />
        </div>

        {/* Preview */}
        <div className="flex flex-col">
          <div className="mb-1.5 flex items-center gap-2 px-1">
            <FileCode2 className="size-3.5 text-muted-foreground" />
            <p className="text-xs font-medium text-muted-foreground">Preview</p>
          </div>
          <div
            className="prose max-w-none min-h-[600px] overflow-auto rounded-2xl border border-white/10 bg-black/30 p-6 text-foreground"
            dangerouslySetInnerHTML={{ __html: renderedHtml }}
          />
        </div>
      </div>

      {/* Printable version (hidden, only visible on print) */}
      <div
        className="md-print-target prose max-w-none"
        dangerouslySetInnerHTML={{ __html: renderedHtml }}
      />

      {/* Footer */}
      <Card className="border-white/10 bg-white/[0.015] p-4">
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2">
            <Input
              value={fileName}
              onChange={(e) => setFileName(e.target.value)}
              placeholder="document"
              className="h-8 w-40 border-white/10 bg-white/[0.04] text-sm"
            />
            <span className="text-xs text-muted-foreground">.md</span>
          </div>

          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-8 border-white/10 bg-white/[0.04] text-xs"
              onClick={printPdf}
            >
              <Printer className="size-3.5" />
              Print / PDF
            </Button>
            <Button
              size="sm"
              className="h-8 gap-1.5 text-xs"
              onClick={downloadMd}
            >
              <Download className="size-3.5" />
              Download .md
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
