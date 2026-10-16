import type { GitTreeEntry, NodeType, RepoIdentity, TreeNode } from "./types";

// GitHub's own character rules. Holding names to them keeps "?", "#", "%" and ".."
// out of the API path the names are placed in.
const OWNER_PATTERN = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$/;
const REPO_PATTERN = /^[A-Za-z0-9._-]{1,100}$/;

export function parseRepoIdentity(input: string): RepoIdentity | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  const match =
    trimmed.match(
      /^https?:\/\/(?:www\.)?github\.com\/([^/]+)\/([^/#?]+)(?:[/?#].*)?$/i,
    ) ?? trimmed.match(/^([^/\s]+)\/([^/\s]+)$/);
  if (!match) return null;

  const owner = match[1];
  const repo = match[2].replace(/\.git$/i, "");
  if (!OWNER_PATTERN.test(owner) || !REPO_PATTERN.test(repo)) return null;
  if (repo === "." || repo === "..") return null;
  return { owner, repo };
}

export function buildTree(entries: GitTreeEntry[]): TreeNode {
  const root: TreeNode = {
    name: "root",
    path: "",
    type: "folder",
    children: [],
  };

  for (const entry of entries) {
    const segments = entry.path.split("/").filter(Boolean);
    let cursor = root;

    segments.forEach((segment, index) => {
      const isLast = index === segments.length - 1;
      const nextPath = cursor.path ? `${cursor.path}/${segment}` : segment;
      const type: NodeType =
        isLast && entry.type === "blob" ? "file" : "folder";

      let child = cursor.children.find((node) => node.name === segment);
      if (!child) {
        child = { name: segment, path: nextPath, type, children: [] };
        cursor.children.push(child);
      }

      if (isLast && type === "file") child.type = "file";
      cursor = child;
    });
  }

  const sortNode = (node: TreeNode) => {
    node.children.sort((a, b) => {
      if (a.type !== b.type) return a.type === "folder" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    node.children.forEach(sortNode);
  };

  sortNode(root);
  return root;
}

export function countNodes(root: TreeNode) {
  let files = 0;
  let folders = 0;

  const walk = (node: TreeNode) => {
    if (node.path) {
      if (node.type === "file") files += 1;
      else folders += 1;
    }

    node.children.forEach(walk);
  };

  walk(root);
  return { files, folders };
}

export function treeToAscii(root: TreeNode): string {
  const lines: string[] = [];

  const walk = (nodes: TreeNode[], prefix: string) => {
    nodes.forEach((node, index) => {
      const isLast = index === nodes.length - 1;
      const connector = isLast ? "└──" : "├──";
      lines.push(
        `${prefix}${connector} ${node.name}${node.type === "folder" ? "/" : ""}`,
      );
      if (node.type === "folder" && node.children.length > 0) {
        walk(node.children, `${prefix}${isLast ? "    " : "│   "}`);
      }
    });
  };

  lines.push(".");
  walk(root.children, "");
  return lines.join("\n");
}
