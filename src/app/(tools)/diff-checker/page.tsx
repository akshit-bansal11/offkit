"use client";

import * as diff from "diff";
import {
  AlignJustify,
  AlignLeft,
  ArrowRightLeft,
  CheckCircle2,
  Copy,
  FileDiff,
  Settings,
} from "lucide-react";
import React, { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Checkbox } from "@/components/ui/form/Checkbox";
import { Label } from "@/components/ui/form/Label";
import { Textarea } from "@/components/ui/form/Textarea";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { getToolBySlug } from "@/config/tools";

const tool = getToolBySlug("diff-checker");

export default function DiffCheckerPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <DiffCheckerTool />
    </ToolPageShell>
  );
}

function DiffCheckerTool() {
  const [original, setOriginal] = useState("");
  const [modified, setModified] = useState("");

  const [ignoreWhitespace, setIgnoreWhitespace] = useState(false);
  const [ignoreCase, setIgnoreCase] = useState(false);
  const [diffMode, setDiffMode] = useState<"chars" | "words">("chars");

  const [copiedId, setCopiedId] = useState<string | null>(null);

  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const diffResult = useMemo(() => {
    const opts = { ignoreCase, ignoreWhitespace };
    if (diffMode === "chars") {
      return diff.diffChars(original, modified, opts);
    }
    return diff.diffWords(original, modified, opts);
  }, [original, modified, ignoreWhitespace, ignoreCase, diffMode]);

  const unifiedPatch = useMemo(() => {
    return (
      diff.createPatch("document", original, modified, "Original", "Modified", {
        // @ts-expect-error - The diff package lacks type definitions for patch string configurations natively
        ignoreCase,
        ignoreWhitespace,
      }) || ""
    );
  }, [original, modified, ignoreWhitespace, ignoreCase]);

  const plainTextDiff = useMemo(() => {
    return diffResult
      .map((part) => {
        if (part.added) return `{+${part.value}+}`;
        if (part.removed) return `[-${part.value}-]`;
        return part.value;
      })
      .join("");
  }, [diffResult]);

  return (
    <div className="space-y-6 max-w-5xl mx-auto xl:max-w-7xl">
      {/* Top Configuration & Inputs Array */}
      <div className="grid gap-6 xl:grid-cols-[1fr_1fr]">
        {/* Left Side: Original Input & Settings */}
        <div className="space-y-4">
          <Card className="border-white/10 bg-card/70 overflow-hidden">
            <CardHeader className="border-b border-white/5 bg-background/50 px-4 py-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium flex items-center gap-2">
                  <AlignLeft className="size-4 text-muted-foreground" />
                  Original Text
                </CardTitle>
                <div className="flex gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-xs px-2"
                    onClick={() => setOriginal("")}
                  >
                    Clear
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    className="h-7 text-xs px-2"
                    onClick={async () =>
                      setOriginal(await navigator.clipboard.readText())
                    }
                  >
                    Paste
                  </Button>
                </div>
              </div>
            </CardHeader>
            <Textarea
              value={original}
              onChange={(e) => setOriginal(e.target.value)}
              placeholder="Paste the original text here."
              className="min-h-[240px] w-full resize-none border-0 bg-transparent p-4 font-mono text-sm focus-visible:ring-0 leading-relaxed rounded-none"
              spellCheck={false}
            />
          </Card>

          <Card className="border-white/10 bg-card/70 p-4">
            <div className="flex items-center gap-6">
              <p className="text-sm font-medium flex items-center gap-2">
                <Settings className="size-4 text-muted-foreground" />
                Diff Rules
              </p>
              <div className="flex items-center gap-4">
                <Label className="flex items-center gap-2 text-sm hover:cursor-pointer select-none">
                  <Checkbox
                    checked={ignoreWhitespace}
                    onCheckedChange={setIgnoreWhitespace}
                  />
                  Ignore Whitespace
                </Label>
                <Label className="flex items-center gap-2 text-sm hover:cursor-pointer select-none">
                  <Checkbox
                    checked={ignoreCase}
                    onCheckedChange={setIgnoreCase}
                  />
                  Ignore Case
                </Label>
              </div>
            </div>
            <div className="mt-4 flex items-center gap-6">
              <p className="text-sm font-medium text-muted-foreground">Mode</p>
              <SegmentedControl
                size="sm"
                value={diffMode}
                onValueChange={(value) =>
                  setDiffMode(value as "chars" | "words")
                }
                options={[
                  { label: "Character Level", value: "chars" },
                  { label: "Word Level", value: "words" },
                ]}
              />
            </div>
          </Card>
        </div>

        {/* Right Side: Modified Input */}
        <div className="space-y-4">
          <Card className="border-white/10 bg-card/70 overflow-hidden h-full">
            <CardHeader className="border-b border-white/5 bg-background/50 px-4 py-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium flex items-center gap-2">
                  <AlignJustify className="size-4 text-muted-foreground" />
                  Modified Text
                </CardTitle>
                <div className="flex gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 text-xs px-2"
                    onClick={() => setModified("")}
                  >
                    Clear
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    className="h-7 text-xs px-2"
                    onClick={async () =>
                      setModified(await navigator.clipboard.readText())
                    }
                  >
                    Paste
                  </Button>
                </div>
              </div>
            </CardHeader>
            <Textarea
              value={modified}
              onChange={(e) => setModified(e.target.value)}
              placeholder="Paste the modified text here."
              className="h-[calc(100%-48px)] w-full resize-none border-0 bg-transparent p-4 font-mono text-sm focus-visible:ring-0 leading-relaxed rounded-none"
              spellCheck={false}
            />
          </Card>
        </div>
      </div>

      {/* Visual Rich Preview */}
      <Card className="border-white/10 bg-card/70">
        <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 py-4">
          <CardTitle className="flex items-center gap-2 text-lg">
            <FileDiff className="size-5 text-primary" />
            Inline Visual Diff
          </CardTitle>
          <div className="flex gap-2">
            <Badge
              variant="outline"
              className="bg-destructive/10 text-destructive border-destructive/25 px-2 py-0.5"
            >
              Removed
            </Badge>
            <Badge
              variant="outline"
              className="bg-success/10 text-success border-success/25 px-2 py-0.5"
            >
              Added
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="p-6">
          {!original && !modified ? (
            <div className="flex h-[150px] items-center justify-center text-muted-foreground flex-col gap-3 opacity-50">
              <ArrowRightLeft className="size-8" />
              <p className="text-sm">
                Enter original and modified text to see differences.
              </p>
            </div>
          ) : (
            <div className="whitespace-pre-wrap font-mono text-sm leading-[1.8] tracking-wide break-words rounded-xl border border-white/5 bg-background/50 p-6 min-h-[150px] shadow-inner">
              {diffResult.map((part, index) => {
                if (part.added) {
                  return (
                    <ins
                      key={index}
                      className="rounded-sm bg-success/20 px-0.5 font-semibold text-success no-underline"
                    >
                      {part.value}
                    </ins>
                  );
                }
                if (part.removed) {
                  return (
                    <del
                      key={index}
                      className="bg-destructive/20 text-destructive line-through opacity-70 px-0.5 rounded-sm"
                    >
                      {part.value}
                    </del>
                  );
                }
                return (
                  <span key={index} className="text-muted-foreground/80">
                    {part.value}
                  </span>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Output Formats */}
      <div className="grid gap-6 xl:grid-cols-2">
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label>Unified Patch Format</Label>
            <Button
              size="sm"
              variant="outline"
              onClick={() => handleCopy(unifiedPatch, "patch")}
            >
              {copiedId === "patch" ? (
                <CheckCircle2 className="size-3.5 mr-2 text-primary" />
              ) : (
                <Copy className="size-3.5 mr-2" />
              )}
              Copy Patch
            </Button>
          </div>
          <Textarea
            readOnly
            value={unifiedPatch}
            variant="dark"
            className="min-h-[200px] border-white/5 bg-black/20 font-mono text-xs leading-relaxed focus-visible:ring-0"
            placeholder="Diff patch format will appear here..."
          />
        </div>

        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label>Plain Text (Markup)</Label>
            <Button
              size="sm"
              variant="outline"
              onClick={() => handleCopy(plainTextDiff, "plain")}
            >
              {copiedId === "plain" ? (
                <CheckCircle2 className="size-3.5 mr-2 text-primary" />
              ) : (
                <Copy className="size-3.5 mr-2" />
              )}
              Copy Markup
            </Button>
          </div>
          <Textarea
            readOnly
            value={plainTextDiff}
            variant="dark"
            className="min-h-[200px] border-white/5 bg-black/20 font-mono text-xs leading-relaxed focus-visible:ring-0"
            placeholder="[-removed-] {+added+}..."
          />
        </div>
      </div>
    </div>
  );
}
