"use client";

import {
  ArrowDown,
  ArrowUp,
  Download,
  Lock,
  LockOpen,
  Plus,
  Shuffle,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { ColorInput } from "@/components/ui/form/ColorInput";
import { Input } from "@/components/ui/form/Input";
import { Card, CardHeader, CardTitle } from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import {
  MAX_COLORS,
  MIN_COLORS,
  moveItem,
  normalizeHex,
  randomHex,
  randomizeColors,
  toCssVariables,
  toJson,
  toTailwindTheme,
} from "@/lib/design-tools/palette-tools";

const tool = getToolBySlug("palette-maker");

interface Swatch {
  id: number;
  hex: string;
  locked: boolean;
  /** What the text field shows while it holds something that is not a hex. */
  draft: string | null;
}

let nextId = 0;

function createSwatch(hex: string): Swatch {
  nextId += 1;
  return { id: nextId, hex, locked: false, draft: null };
}

const exportFormats = [
  { label: "CSS custom properties", file: "palette.css", build: toCssVariables },
  { label: "Tailwind theme JSON", file: "palette.tailwind.json", build: toTailwindTheme },
  { label: "Plain JSON", file: "palette.json", build: toJson },
] as const;

export default function PaletteMakerPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <PaletteMakerTool />
    </ToolPageShell>
  );
}

function PaletteMakerTool() {
  const [swatches, setSwatches] = useState<Swatch[]>(() =>
    ["#2563eb", "#8b5cf6", "#ec4899", "#f59e0b"].map(createSwatch),
  );
  const [status, setStatus] = useState("");

  const colors = swatches.map((swatch) => swatch.hex);

  function update(id: number, patch: Partial<Swatch>) {
    setSwatches((current) =>
      current.map((swatch) =>
        swatch.id === id ? { ...swatch, ...patch } : swatch,
      ),
    );
  }

  function handleText(id: number, value: string) {
    const hex = normalizeHex(value);
    update(id, hex ? { hex, draft: null } : { draft: value });
  }

  function move(index: number, delta: number) {
    setSwatches((current) => moveItem(current, index, index + delta));
  }

  function remove(id: number) {
    setSwatches((current) => current.filter((swatch) => swatch.id !== id));
  }

  function add() {
    setSwatches((current) => [...current, createSwatch(randomHex())]);
  }

  function randomise() {
    setSwatches((current) => {
      const next = randomizeColors(
        current.map((swatch) => swatch.hex),
        current.map((swatch) => swatch.locked),
      );
      return current.map((swatch, i) => ({
        ...swatch,
        hex: next[i] ?? swatch.hex,
        draft: swatch.locked ? swatch.draft : null,
      }));
    });
    setStatus("Randomised unlocked colors");
  }

  async function copy(label: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setStatus(`Copied ${label}`);
    } catch {
      setStatus(`Could not copy ${label}`);
    }
  }

  function download(file: string, text: string) {
    const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = file;
    link.click();
    URL.revokeObjectURL(url);
    setStatus(`Downloaded ${file}`);
  }

  return (
    <div className="space-y-8">
      <p role="status" className="sr-only">
        {status}
      </p>

      <Card className="border-white/10 bg-card/70">
        <CardHeader className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <CardTitle>
              Palette ({swatches.length} of {MAX_COLORS})
            </CardTitle>
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="outline" onClick={randomise}>
                <Shuffle /> Randomise
              </Button>
              <Button
                type="button"
                onClick={add}
                disabled={swatches.length >= MAX_COLORS}
              >
                <Plus /> Add color
              </Button>
            </div>
          </div>

          <ul className="space-y-3">
            {swatches.map((swatch, index) => {
              const position = index + 1;
              const errorId = `hex-error-${swatch.id}`;
              const invalid = swatch.draft !== null;
              return (
                <li key={swatch.id} className="space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <ColorInput
                      aria-label={`Color ${position} picker`}
                      value={swatch.hex}
                      onChange={(event) =>
                        update(swatch.id, {
                          hex: event.target.value,
                          draft: null,
                        })
                      }
                      className="h-10 w-14"
                    />
                    <Input
                      aria-label={`Color ${position} hex`}
                      aria-invalid={invalid}
                      aria-describedby={invalid ? errorId : undefined}
                      value={swatch.draft ?? swatch.hex}
                      onChange={(event) =>
                        handleText(swatch.id, event.target.value)
                      }
                      onBlur={() => update(swatch.id, { draft: null })}
                      spellCheck={false}
                      className="w-32 font-mono"
                    />
                    <Button
                      type="button"
                      size="icon"
                      variant="outline"
                      aria-pressed={swatch.locked}
                      aria-label={`Lock color ${position}`}
                      onClick={() =>
                        update(swatch.id, { locked: !swatch.locked })
                      }
                    >
                      {swatch.locked ? <Lock /> : <LockOpen />}
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="outline"
                      aria-label={`Move color ${position} up`}
                      disabled={index === 0}
                      onClick={() => move(index, -1)}
                    >
                      <ArrowUp />
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="outline"
                      aria-label={`Move color ${position} down`}
                      disabled={index === swatches.length - 1}
                      onClick={() => move(index, 1)}
                    >
                      <ArrowDown />
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="outline"
                      aria-label={`Remove color ${position}`}
                      disabled={swatches.length <= MIN_COLORS}
                      onClick={() => remove(swatch.id)}
                    >
                      <Trash2 />
                    </Button>
                  </div>
                  {invalid && (
                    <p id={errorId} className="text-sm text-destructive">
                      Enter a hex color like #1a2b3c.
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
        </CardHeader>
      </Card>

      <div className="flex h-24 overflow-hidden rounded-2xl border border-white/10">
        {swatches.map((swatch) => (
          <div
            key={swatch.id}
            className="flex-1"
            style={{ backgroundColor: swatch.hex }}
            aria-hidden="true"
          />
        ))}
      </div>

      <div className="grid gap-6 lg:grid-cols-3">
        {exportFormats.map((format) => {
          const text = format.build(colors);
          return (
            <Card key={format.file} className="border-white/10 bg-card/70">
              <CardHeader className="space-y-3">
                <CardTitle className="text-base">{format.label}</CardTitle>
                <pre
                  aria-label={format.label}
                  className="overflow-x-auto rounded-lg bg-background/60 p-3 font-mono text-xs"
                >
                  {text}
                </pre>
                <div className="flex gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => copy(format.label, text)}
                  >
                    Copy
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => download(format.file, text)}
                  >
                    <Download /> Download
                  </Button>
                </div>
              </CardHeader>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
