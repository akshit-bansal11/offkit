"use client";

import {
  CheckCircle2,
  Copy,
  Eraser,
  FileJson2,
  Minimize2,
  Wand2,
} from "lucide-react";
import { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Textarea } from "@/components/ui/form/Textarea";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("json-formatter");

export default function JsonFormatterPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <JsonFormatter />
    </ToolPageShell>
  );
}

const SAMPLE_JSON = `{
  "name": "offkit",
  "tool": "json-formatter",
  "features": ["pretty print", "minify", "validate"],
  "localFirst": true
}`;

function JsonFormatter() {
  const [input, setInput] = useState(SAMPLE_JSON);
  const [output, setOutput] = useState(() => formatJson(SAMPLE_JSON, "pretty"));
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const inputStats = useMemo(() => {
    const characters = input.length;
    const lines = input ? input.split(/\r?\n/).length : 0;

    return { characters, lines };
  }, [input]);

  const runFormat = (mode: "pretty" | "minify") => {
    try {
      const nextOutput = formatJson(input, mode);
      setOutput(nextOutput);
      setError(null);
    } catch (formatError) {
      setError(
        formatError instanceof Error
          ? formatError.message
          : "Unable to parse the JSON input.",
      );
      setOutput("");
    }
  };

  const copyOutput = async () => {
    if (!output) {
      return;
    }

    await navigator.clipboard.writeText(output);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <Card className="flex h-full flex-col border-white/10 bg-card/70">
        <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
          <div>
            <CardTitle className="flex items-center gap-2">
              <FileJson2 className="size-5" />
              Input
            </CardTitle>
          </div>
          <Badge variant="secondary">
            {inputStats.characters} chars · {inputStats.lines} lines
          </Badge>
        </CardHeader>
        <CardContent className="flex flex-1 flex-col">
          <Textarea
            value={input}
            onChange={(event) => setInput(event.target.value)}
            spellCheck={false}
            className="flex-1 min-h-[400px] w-full resize-none overflow-auto rounded-2xl bg-background/70 p-4 font-mono text-sm transition focus:border-primary"
            placeholder='{"hello":"world"}'
          />

          <div className="mt-4 flex flex-wrap gap-3">
            <Button onClick={() => runFormat("pretty")} className="gap-2">
              <Wand2 className="size-4" />
              Pretty Print
            </Button>
            <Button
              onClick={() => runFormat("minify")}
              variant="outline"
              className="gap-2"
            >
              <Minimize2 className="size-4" />
              Minify
            </Button>
            <Button
              onClick={() => {
                setInput(SAMPLE_JSON);
                setOutput(formatJson(SAMPLE_JSON, "pretty"));
                setError(null);
              }}
              variant="outline"
            >
              Load Sample
            </Button>
            <Button
              onClick={() => {
                setInput("");
                setOutput("");
                setError(null);
              }}
              variant="ghost"
              className="gap-2"
            >
              <Eraser className="size-4" />
              Clear
            </Button>
          </div>

          {error && (
            <div className="mt-4 rounded-2xl border border-red-500/20 bg-red-500/10 p-4 text-sm text-red-300">
              {error}
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="flex h-full flex-col border-white/10 bg-card/70">
        <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
          <div>
            <CardTitle>Output</CardTitle>
          </div>
          <Button
            onClick={copyOutput}
            variant="outline"
            size="sm"
            className="gap-2"
            disabled={!output}
          >
            {copied ? (
              <CheckCircle2 className="size-4" />
            ) : (
              <Copy className="size-4" />
            )}
            {copied ? "Copied" : "Copy"}
          </Button>
        </CardHeader>
        <CardContent className="flex flex-1 flex-col">
          <pre className="flex-1 min-h-[400px] w-full overflow-auto rounded-2xl border bg-background/70 p-4 font-mono text-sm leading-relaxed text-white/80">
            {output || (
              <span className="text-muted-foreground">
                Formatted JSON will appear here.
              </span>
            )}
          </pre>
        </CardContent>
      </Card>
    </div>
  );
}

function formatJson(input: string, mode: "pretty" | "minify") {
  const parsed = JSON.parse(input);
  return mode === "pretty"
    ? JSON.stringify(parsed, null, 2)
    : JSON.stringify(parsed);
}
