"use client";

import { Copy, Download, Loader2, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ToolPageShell } from "@/components/common/ToolPageShell";
import { GitTreeNodeItem } from "@/components/tools/git-scaffold/GitTreeNodeItem";
import { Button } from "@/components/ui/Button";
import { Badge } from "@/components/ui/feedback/Badge";
import { Input } from "@/components/ui/form/Input";
import { Label } from "@/components/ui/form/Label";
import { ToggleSwitch } from "@/components/ui/interaction/ToggleSwitch";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/layout/Card";
import { getToolBySlug } from "@/config/tools";
import { fetchRepoTree } from "@/lib/tools/git-scaffold/github";
import type { TreeNode } from "@/lib/tools/git-scaffold/types";
import {
  buildTree,
  countNodes,
  parseRepoIdentity,
  treeToAscii,
} from "@/lib/tools/git-scaffold/utils";

const tool = getToolBySlug("git-scaffold");

export default function GitScaffoldPage() {
  if (!tool) {
    return null;
  }

  return (
    <ToolPageShell title={tool.name} description={tool.description}>
      <GitScaffoldTool />
    </ToolPageShell>
  );
}

// Versions before 16 October 2026 saved the token here in plain text.
const LEGACY_TOKEN_STORAGE_KEY = "git_scaffold_token";

function GitScaffoldTool() {
  const [repoInput, setRepoInput] = useState("");
  // The token lives in this component's memory only: it is never written to
  // storage, a URL or a log, and it is gone when the page closes.
  const [token, setToken] = useState("");
  const [isPrivateRepo, setIsPrivateRepo] = useState(false);
  const [treeRoot, setTreeRoot] = useState<TreeNode | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    try {
      window.localStorage.removeItem(LEGACY_TOKEN_STORAGE_KEY);
    } catch {
      // Storage is blocked, so an older version cannot have saved anything either.
    }
  }, []);

  const changePrivate = useCallback((checked: boolean) => {
    setIsPrivateRepo(checked);
    if (!checked) setToken("");
  }, []);

  const togglePath = useCallback((path: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const loadTree = useCallback(async () => {
    const parsed = parseRepoIdentity(repoInput);
    if (!parsed) {
      setErrorMessage(
        "Invalid repository input. Use https://github.com/owner/repo or owner/repo.",
      );
      return;
    }

    setIsLoading(true);
    setErrorMessage(null);
    setCopied(false);

    try {
      const result = await fetchRepoTree(
        parsed,
        isPrivateRepo ? token.trim() : "",
      );
      setTreeRoot(buildTree(result.entries));
      setTruncated(result.truncated);
      setCollapsed(new Set());
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : "Unable to fetch the repository tree.",
      );
    } finally {
      setIsLoading(false);
    }
  }, [isPrivateRepo, repoInput, token]);

  const nodeCounts = useMemo(
    () => (treeRoot ? countNodes(treeRoot) : { files: 0, folders: 0 }),
    [treeRoot],
  );

  const asciiTree = useMemo(
    () => (treeRoot ? treeToAscii(treeRoot) : ""),
    [treeRoot],
  );

  const copyAscii = useCallback(async () => {
    if (!asciiTree) return;

    await navigator.clipboard.writeText(asciiTree);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [asciiTree]);

  const downloadAscii = useCallback(() => {
    if (!asciiTree) return;

    const blob = new Blob([`\`\`\`\n${asciiTree}\n\`\`\``], {
      type: "text/plain;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "repository-tree.md";
    link.click();
    URL.revokeObjectURL(url);
  }, [asciiTree]);

  return (
    <div className="space-y-6">
      <Card>
        <CardContent className="space-y-5">
          <div className="space-y-2">
            <Label
              htmlFor="repository"
              className="text-xs text-muted-foreground"
            >
              URL or owner/repo
            </Label>
            <Input
              id="repository"
              value={repoInput}
              onChange={(event) => setRepoInput(event.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void loadTree();
              }}
              placeholder="https://github.com/owner/repo or owner/repo"
              disabled={isLoading}
            />
          </div>

          <div className="flex items-center gap-3">
            <ToggleSwitch
              checked={isPrivateRepo}
              onCheckedChange={changePrivate}
              size="sm"
              disabled={isLoading}
              id="private-repo"
            />
            <Label
              htmlFor="private-repo"
              className="text-sm font-medium text-foreground"
            >
              Private repository
            </Label>
          </div>

          {isPrivateRepo ? (
            <div className="space-y-2">
              <Label
                htmlFor="github-token"
                className="text-xs text-muted-foreground"
              >
                Personal access token
              </Label>
              <Input
                id="github-token"
                value={token}
                onChange={(event) => setToken(event.target.value)}
                placeholder="ghp_xxx"
                type="password"
                // Not a site password: keeps browsers from filling in a saved one,
                // and tells common password managers to leave the field alone.
                autoComplete="new-password"
                data-1p-ignore
                data-lpignore="true"
                spellCheck={false}
                aria-describedby="github-token-note"
                disabled={isLoading}
              />
              <p
                id="github-token-note"
                className="text-xs text-muted-foreground"
              >
                Kept in memory on this page only and sent only to
                api.github.com. It is never saved. A fine-grained token with
                read-only Contents access is enough.
              </p>
            </div>
          ) : null}

          <Button
            variant={"default"}
            size={"default"}
            onClick={() => void loadTree()}
            disabled={isLoading || !repoInput.trim()}
            className="w-full"
          >
            {isLoading ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <RefreshCw className="size-4" />
            )}
            {isLoading ? "Fetching..." : "Fetch tree"}
          </Button>

          {errorMessage ? (
            <div
              role="alert"
              className="rounded-lg border border-red-500/25 bg-red-500/10 px-4 py-3 text-sm text-red-200"
            >
              {errorMessage}
            </div>
          ) : null}

          {treeRoot && truncated ? (
            <p
              role="status"
              className="rounded-lg border border-amber-500/25 bg-amber-500/10 px-4 py-3 text-sm text-amber-200"
            >
              This repository is too large for GitHub to list in one request, so
              the tree below is incomplete.
            </p>
          ) : null}

          {treeRoot ? (
            <div className="flex flex-wrap items-center gap-2 pt-2">
              <Badge
                variant="outline"
                className="border-white/15 bg-white/[0.06] text-white/75"
              >
                {nodeCounts.folders} folders
              </Badge>
              <Badge
                variant="outline"
                className="border-blue-500/20 bg-blue-500/10 text-blue-300"
              >
                {nodeCounts.files} files
              </Badge>
              <div className="ml-auto flex gap-2">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void copyAscii()}
                >
                  <Copy className="size-4" />
                  {copied ? "Copied!" : "Copy"}
                </Button>
                <Button variant="secondary" size="sm" onClick={downloadAscii}>
                  <Download className="size-4" />
                  Download
                </Button>
              </div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      {treeRoot ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Directory structure</CardTitle>
          </CardHeader>
          <CardContent className="max-h-[70vh] overflow-auto p-4">
            <ul className="space-y-1 text-sm font-mono">
              {treeRoot.children.map((node) => (
                <GitTreeNodeItem
                  key={node.path}
                  node={node}
                  collapsed={collapsed}
                  onToggle={togglePath}
                />
              ))}
            </ul>
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
