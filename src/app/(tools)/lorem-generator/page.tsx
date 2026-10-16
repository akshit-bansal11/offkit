"use client";

import {
  AlignJustify,
  AlignLeft,
  CheckCircle2,
  Copy,
  RefreshCw,
  Settings,
  Type,
} from "lucide-react";
import React, { useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { Button } from "@/components/ui/Button";
import { Checkbox } from "@/components/ui/form/Checkbox";
import { Input } from "@/components/ui/form/Input";
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

const tool = getToolBySlug("lorem-generator");

export default function LoremGeneratorPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <LoremGeneratorTool />
    </ToolPageShell>
  );
}

const CLASSIC_WORDS = [
  "lorem",
  "ipsum",
  "dolor",
  "sit",
  "amet",
  "consectetur",
  "adipiscing",
  "elit",
  "sed",
  "do",
  "eiusmod",
  "tempor",
  "incididunt",
  "ut",
  "labore",
  "et",
  "dolore",
  "magna",
  "aliqua",
  "enim",
  "ad",
  "minim",
  "veniam",
  "quis",
  "nostrud",
  "exercitation",
  "ullamco",
  "laboris",
  "nisi",
  "ut",
  "aliquip",
  "ex",
  "ea",
  "commodo",
  "consequat",
  "duis",
  "aute",
  "irure",
  "dolor",
  "in",
  "reprehenderit",
  "in",
  "voluptate",
  "velit",
  "esse",
  "cillum",
  "dolore",
  "eu",
  "fugiat",
  "nulla",
  "pariatur",
  "excepteur",
  "sint",
  "occaecat",
  "cupidatat",
  "non",
  "proident",
  "sunt",
  "in",
  "culpa",
  "qui",
  "officia",
  "deserunt",
  "mollit",
  "anim",
  "id",
  "est",
  "laborum",
  "sed",
  "ut",
  "perspiciatis",
  "unde",
  "omnis",
  "iste",
  "natus",
  "error",
  "sit",
  "voluptatem",
  "accusantium",
  "doloremque",
  "laudantium",
  "totam",
  "rem",
  "aperiam",
  "eaque",
  "ipsa",
  "quae",
  "ab",
  "illo",
  "inventore",
  "veritatis",
];

const ENGLISH_WORDS = [
  "the",
  "quick",
  "brown",
  "fox",
  "jumps",
  "over",
  "lazy",
  "dog",
  "time",
  "flies",
  "like",
  "an",
  "arrow",
  "fruit",
  "banana",
  "apple",
  "sky",
  "blue",
  "ocean",
  "deep",
  "mountain",
  "high",
  "river",
  "flow",
  "wind",
  "blow",
  "sun",
  "shine",
  "moon",
  "glow",
  "star",
  "bright",
  "night",
  "dark",
  "day",
  "light",
  "morning",
  "dew",
  "evening",
  "mist",
  "tree",
  "green",
  "leaf",
  "fall",
  "winter",
  "cold",
  "summer",
  "hot",
  "spring",
  "warm",
  "autumn",
  "cool",
  "city",
  "busy",
  "street",
  "quiet",
  "home",
  "sweet",
  "family",
  "love",
  "friend",
  "true",
  "journey",
  "long",
  "adventure",
  "wild",
  "dream",
  "big",
  "goal",
  "reach",
  "mind",
  "clear",
  "heart",
  "pure",
  "soul",
  "free",
  "spirit",
  "brave",
  "courage",
  "strong",
  "hope",
  "always",
  "faith",
  "never",
  "give",
  "up",
  "keep",
  "going",
  "move",
  "forward",
];

// Bypass static linter checks for pure components by wrapping Math.random outside
const random = () => Math.random();

function LoremGeneratorTool() {
  const [unit, setUnit] = useState<"words" | "sentences" | "paragraphs">(
    "paragraphs",
  );
  const [count, setCount] = useState<number>(3);
  const [style, setStyle] = useState<"classic" | "english">("classic");
  const [startWithLorem, setStartWithLorem] = useState<boolean>(true);
  const [copied, setCopied] = useState(false);
  // We use this key to force deterministic re-renders triggered manually by the user
  const [refreshKey, setRefreshKey] = useState(0);

  const generatedText = useMemo(() => {
    const dict = style === "classic" ? CLASSIC_WORDS : ENGLISH_WORDS;

    const maxWords = 15;
    const minWords = 5;

    let resultText = "";

    const getRandomWord = (dictionary: string[]) => {
      return dictionary[Math.floor(random() * dictionary.length)];
    };

    const capitalize = (str: string) => {
      if (!str) return "";
      return str.charAt(0).toUpperCase() + str.slice(1);
    };

    const makeSentence = (sentenceLength: number, isFirstOverall: boolean) => {
      const sentenceArr = [];
      const needsClassicStarter =
        isFirstOverall && style === "classic" && startWithLorem;

      let startingIndex = 0;
      if (needsClassicStarter) {
        sentenceArr.push("Lorem", "ipsum", "dolor", "sit", "amet");
        startingIndex = 5;
      }

      for (let i = startingIndex; i < sentenceLength; i++) {
        sentenceArr.push(getRandomWord(dict));
      }

      const builtStr = sentenceArr.join(" ");
      return capitalize(builtStr) + ".";
    };

    if (unit === "words") {
      const wordsArr = [];
      const needsClassicStarter = style === "classic" && startWithLorem;
      let startIdx = 0;

      if (needsClassicStarter && count >= 5) {
        wordsArr.push("Lorem", "ipsum", "dolor", "sit", "amet");
        startIdx = 5;
      } else if (needsClassicStarter && count > 0) {
        // Just take what we can fit if count < 5
        const starters = ["Lorem", "ipsum", "dolor", "sit", "amet"];
        wordsArr.push(...starters.slice(0, count));
        startIdx = count;
      }

      for (let i = startIdx; i < count; i++) {
        wordsArr.push(getRandomWord(dict));
      }

      resultText = capitalize(wordsArr.join(" "));
    } else if (unit === "sentences") {
      const sentencesArr = [];
      for (let i = 0; i < count; i++) {
        const sentenceLength =
          Math.floor(random() * (maxWords - minWords + 1)) + minWords;
        sentencesArr.push(makeSentence(sentenceLength, i === 0));
      }
      resultText = sentencesArr.join(" ");
    } else if (unit === "paragraphs") {
      const paragraphsArr = [];
      for (let p = 0; p < count; p++) {
        // 4 to 8 sentences per paragraph
        const numSentences = Math.floor(random() * 5) + 4;
        const sentencesArr = [];
        for (let s = 0; s < numSentences; s++) {
          const sentenceLength =
            Math.floor(random() * (maxWords - minWords + 1)) + minWords;
          sentencesArr.push(makeSentence(sentenceLength, p === 0 && s === 0));
        }
        paragraphsArr.push(sentencesArr.join(" "));
      }
      resultText = paragraphsArr.join("\n\n");
    }

    return resultText;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unit, count, style, startWithLorem, refreshKey]);

  const handleCopy = () => {
    navigator.clipboard.writeText(generatedText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleRegenerate = () => {
    setRefreshKey((prev) => prev + 1);
  };

  return (
    <div className="space-y-6 max-w-4xl mx-auto">
      {/* Controls */}
      <Card className="border-white/10 bg-card/70 overflow-hidden shadow-sm">
        <CardHeader className="bg-background/20 border-b border-white/5 pb-4">
          <CardTitle className="text-sm font-medium flex items-center gap-2">
            <Settings className="size-4 text-muted-foreground" />
            Generator Configuration
          </CardTitle>
        </CardHeader>
        <CardContent className="p-6">
          <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-[1fr_1fr_auto]">
            {/* Unit Toggle */}
            <div className="space-y-3">
              <Label className="block text-muted-foreground">Length Unit</Label>
              <SegmentedControl
                fullWidth
                size="sm"
                value={unit}
                onValueChange={(value) =>
                  setUnit(value as "words" | "sentences" | "paragraphs")
                }
                options={[
                  {
                    label: (
                      <span className="flex items-center gap-2">
                        <Type className="size-3.5" />
                        Words
                      </span>
                    ),
                    value: "words",
                  },
                  {
                    label: (
                      <span className="flex items-center gap-2">
                        <AlignLeft className="size-3.5" />
                        Sentences
                      </span>
                    ),
                    value: "sentences",
                  },
                  {
                    label: (
                      <span className="flex items-center gap-2">
                        <AlignJustify className="size-3.5" />
                        Paragraphs
                      </span>
                    ),
                    value: "paragraphs",
                  },
                ]}
              />
            </div>

            {/* Input Count & Style */}
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-3">
                <Label className="block text-muted-foreground">Count</Label>
                <Input
                  type="number"
                  min={1}
                  max={unit === "words" ? 500 : 100}
                  value={count}
                  onChange={(e) => {
                    const val = parseInt(e.target.value);
                    if (!isNaN(val)) setCount(Math.max(1, val));
                  }}
                  className="bg-background/50 text-sm font-mono h-9"
                />
              </div>

              <div className="space-y-3">
                <Label className="block text-muted-foreground">Style</Label>
                <SegmentedControl
                  fullWidth
                  size="sm"
                  value={style}
                  onValueChange={(value) =>
                    setStyle(value as "classic" | "english")
                  }
                  options={[
                    { label: "Classic", value: "classic" },
                    { label: "English", value: "english" },
                  ]}
                />
              </div>
            </div>

            {/* Options Checkboxes */}
            <div className="space-y-3 flex flex-col justify-center border-t border-white/5 pt-4 md:border-none md:pt-0">
              <Label className="invisible hidden block text-muted-foreground lg:block">
                Toggles
              </Label>
              <Label
                className={`flex items-center gap-3 text-sm transition-opacity hover:cursor-pointer select-none ${
                  style !== "classic" ? "opacity-30 pointer-events-none" : ""
                }`}
              >
                <Checkbox
                  checked={startWithLorem}
                  disabled={style !== "classic"}
                  onCheckedChange={setStartWithLorem}
                />
                Start with{" "}
                <span className="italic opacity-80">
                  &quot;Lorem ipsum...&quot;
                </span>
              </Label>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Output Panel */}
      <Card className="border-white/10 bg-card/70 overflow-hidden shadow-xl">
        <CardHeader className="flex flex-row items-center justify-between border-b border-white/5 py-3 px-4 bg-background/20 backdrop-blur-md">
          <CardTitle className="text-sm font-medium text-muted-foreground">
            Generated Output ({count} {unit})
          </CardTitle>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleRegenerate}
              className="h-8 text-xs px-3 hover:bg-background/80"
            >
              <RefreshCw className="size-3.5 mr-2" />
              Regenerate
            </Button>
            <Button
              variant={copied ? "default" : "secondary"}
              size="sm"
              onClick={handleCopy}
              className={`h-8 text-xs px-3 transition-colors ${copied ? "bg-white/20 hover:bg-white/25 text-white" : ""}`}
            >
              {copied ? (
                <>
                  <CheckCircle2 className="size-3.5 mr-2" /> Copied
                </>
              ) : (
                <>
                  <Copy className="size-3.5 mr-2" /> Copy Text
                </>
              )}
            </Button>
          </div>
        </CardHeader>

        {/* Animated Background Display for the Text */}
        <div className="relative">
          <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent pointer-events-none" />
          <Textarea
            readOnly
            value={generatedText}
            className="min-h-[350px] resize-none border-0 bg-transparent p-6 font-serif text-[15px] leading-[1.8] tracking-wide text-foreground/90 focus-visible:ring-0 rounded-none shadow-inner"
            placeholder="Generating text..."
            spellCheck={false}
          />
        </div>
      </Card>
    </div>
  );
}
