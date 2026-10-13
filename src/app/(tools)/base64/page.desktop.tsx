"use client";

import {
  ArrowLeftRight,
  CheckCircle2,
  Copy,
  Eraser,
  Lock,
  Unlock,
} from "lucide-react";
import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Textarea } from "@/components/ui/form/Textarea";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("base64");

export default function Base64Page() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <Base64Tool />
    </ToolPageShell>
  );
}

const SAMPLE_TEXT = "open-tools keeps small utilities easy to use.";

type Mode = "encode" | "decode";

function Base64Tool() {
  const [mode, setMode] = useState<Mode>("encode");
  const [input, setInput] = useState(SAMPLE_TEXT);
  const [output, setOutput] = useState(() => encodeBase64(SAMPLE_TEXT));
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const transform = () => {
    try {
      const nextOutput =
        mode === "encode" ? encodeBase64(input) : decodeBase64(input);
      setOutput(nextOutput);
      setError(null);
    } catch (transformError) {
      setOutput("");
      setError(
        transformError instanceof Error
          ? transformError.message
          : "Base64 conversion failed.",
      );
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
    <div className="grid gap-6 lg:grid-cols-[1.1fr_0.9fr]">
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="space-y-4">
          <div className="flex flex-wrap gap-3">
            <Button
              onClick={() => setMode("encode")}
              variant={mode === "encode" ? "default" : "outline"}
              className="gap-2"
            >
              <Lock className="size-4" />
              Encode
            </Button>
            <Button
              onClick={() => setMode("decode")}
              variant={mode === "decode" ? "default" : "outline"}
              className="gap-2"
            >
              <Unlock className="size-4" />
              Decode
            </Button>
          </div>

          <div>
            <CardTitle>Input</CardTitle>
            <p className="mt-2 text-sm text-muted-foreground">
              {mode === "encode"
                ? "Turn normal text into a base64 string."
                : "Paste a base64 string and decode it back into readable text."}
            </p>
          </div>
        </CardHeader>
        <CardContent>
          <Textarea
            value={input}
            onChange={(event) => setInput(event.target.value)}
            spellCheck={false}
            className="min-h-[320px] w-full rounded-2xl bg-background/70 p-4 font-mono text-sm transition focus:border-primary"
            placeholder={
              mode === "encode"
                ? "Type text to encode"
                : "Paste a base64 value to decode"
            }
          />

          <div className="mt-4 flex flex-wrap gap-3">
            <Button onClick={transform}>
              {mode === "encode" ? "Encode Text" : "Decode Text"}
            </Button>
            <Button
              onClick={() => {
                setMode(mode === "encode" ? "decode" : "encode");
                setInput(output);
                setOutput("");
                setError(null);
              }}
              variant="outline"
              className="gap-2"
              disabled={!output}
            >
              <ArrowLeftRight className="size-4" />
              Swap Output to Input
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

      <Card className="border-white/10 bg-card/70">
        <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
          <div>
            <CardTitle>Output</CardTitle>
            <p className="mt-2 text-sm text-muted-foreground">
              Copy the transformed value directly from here.
            </p>
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
        <CardContent>
          <Textarea
            value={output}
            readOnly
            spellCheck={false}
            className="min-h-[320px] w-full rounded-2xl bg-background/70 p-4 font-mono text-sm text-white/80"
            placeholder="Base64 output will appear here."
          />
        </CardContent>
      </Card>
    </div>
  );
}

function encodeBase64(input: string) {
  const bytes = new TextEncoder().encode(input);
  let binary = "";

  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }

  return btoa(binary);
}

function decodeBase64(input: string) {
  const binary = atob(input.trim());
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}
