import type { GitTreeEntry, NodeType, RepoIdentity, TreeNode } from "./types";

export function parseRepoIdentity(input: string): RepoIdentity | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  const urlMatch = trimmed.match(
    /^https?:\/\/github\.com\/([^/]+)\/([^/#?]+)(?:[/?#].*)?$/i,
  );
  if (urlMatch) {
    return { owner: urlMatch[1], repo: urlMatch[2].replace(/\.git$/i, "") };
  }

  const shortMatch = trimmed.match(/^([^/\s]+)\/([^/\s]+)$/);
  return shortMatch
    ? { owner: shortMatch[1], repo: shortMatch[2].replace(/\.git$/i, "") }
    : null;
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
      const connector = isLast ? "â””â”€â”€" : "â”œâ”€â”€";
      lines.push(
        `${prefix}${connector} ${node.name}${node.type === "folder" ? "/" : ""}`,
      );
      if (node.type === "folder" && node.children.length > 0) {
        walk(node.children, `${prefix}${isLast ? "    " : "â”‚   "}`);
      }
    });
  };

  lines.push(".");
  walk(root.children, "");
  return lines.join("\n");
}
