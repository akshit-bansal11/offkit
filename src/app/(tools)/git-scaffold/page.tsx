"use client";

import { Copy, Download, Loader2, RefreshCw } from "lucide-react";
import React, {
  useCallback,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";
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
import type { GitTreeResponse, TreeNode } from "@/lib/tools/git-scaffold/types";
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

const TOKEN_STORAGE_KEY = "git_scaffold_token";
const subscribeToNothing = () => () => {};

function GitScaffoldTool() {
  const [repoInput, setRepoInput] = useState("");
  // The saved token is read with useSyncExternalStore so the first client
  // render matches the server's, then picks up localStorage without an effect.
  const storedToken = useSyncExternalStore(
    subscribeToNothing,
    () => window.localStorage.getItem(TOKEN_STORAGE_KEY) ?? "",
    () => "",
  );
  const [tokenDraft, setTokenInput] = useState<string | null>(null);
  const [privateChoice, setIsPrivateRepo] = useState<boolean | null>(null);
  const tokenInput = tokenDraft ?? storedToken;
  const isPrivateRepo = privateChoice ?? storedToken !== "";
  const [treeRoot, setTreeRoot] = useState<TreeNode | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

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
      if (isPrivateRepo && tokenInput.trim()) {
        window.localStorage.setItem(TOKEN_STORAGE_KEY, tokenInput.trim());
      } else {
        window.localStorage.removeItem(TOKEN_STORAGE_KEY);
      }

      const headers: Record<string, string> = {
        Accept: "application/vnd.github+json",
      };

      if (isPrivateRepo && tokenInput.trim()) {
        headers.Authorization = `Bearer ${tokenInput.trim()}`;
      }

      const endpoint = `https://api.github.com/repos/${parsed.owner}/${parsed.repo}/git/trees/HEAD?recursive=1`;
      const response = await fetch(endpoint, { headers });

      if (!response.ok) {
        if (response.status === 403) {
          setErrorMessage(
            "GitHub API rate limit reached (403). Add a personal access token to continue.",
          );
          return;
        }

        if (response.status === 404) {
          setErrorMessage(
            "Repository not found or tree endpoint is unavailable for this branch.",
          );
          return;
        }

        setErrorMessage(
          `GitHub request failed with status ${response.status}.`,
        );
        return;
      }

      const payload = (await response.json()) as GitTreeResponse;
      const filtered = payload.tree.filter(
        (entry) => entry.type === "blob" || entry.type === "tree",
      );
      setTreeRoot(buildTree(filtered));
      setCollapsed(new Set());
    } catch {
      setErrorMessage(
        "Unable to fetch repository tree. Check your network connection and try again.",
      );
    } finally {
      setIsLoading(false);
    }
  }, [isPrivateRepo, repoInput, tokenInput]);

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
            <Label className="text-xs text-muted-foreground">
              URL or owner/repo
            </Label>
            <Input
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
              onCheckedChange={setIsPrivateRepo}
              size="sm"
              disabled={isLoading}
              id="private-repo"
            />
            <Label
              htmlFor="private-repo"
              className="text-sm font-medium text-foreground"
            >
              Private repo?
            </Label>
          </div>

          {isPrivateRepo ? (
            <div className="space-y-2">
              <Label className="text-xs text-muted-foreground">
                Personal access token
              </Label>
              <Input
                value={tokenInput}
                onChange={(event) => setTokenInput(event.target.value)}
                placeholder="ghp_xxx"
                type="password"
                disabled={isLoading}
              />
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
            <div className="rounded-lg border border-red-500/25 bg-red-500/10 px-4 py-3 text-sm text-red-200">
              {errorMessage}
            </div>
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
