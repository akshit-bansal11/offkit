// GitTreeNodeItem.tsx
// Recursive tree node item for the git scaffold viewer component.

import { File, Folder, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/Button";
import type { TreeNode } from "@/lib/tools/directory-scaffold/types";

interface GitTreeNodeItemProps {
  node: TreeNode;
  collapsed: Set<string>;
  onToggle: (path: string) => void;
}

export function GitTreeNodeItem({
  node,
  collapsed,
  onToggle,
}: GitTreeNodeItemProps) {
  const isFolder = node.type === "folder";
  const isCollapsed = collapsed.has(node.path);

  return (
    <li className="text-sm">
      <Button
        type="button"
        variant="ghost"
        className={`h-auto w-full justify-start gap-2 rounded-md px-2 py-1 text-left font-normal transition-colors ${
          isFolder ? "hover:bg-white/5" : ""
        }`}
        onClick={() => {
          if (isFolder) onToggle(node.path);
        }}
      >
        {isFolder ? (
          isCollapsed ? (
            <Folder className="size-4 text-amber-300" />
          ) : (
            <FolderOpen className="size-4 text-amber-300" />
          )
        ) : (
          <File className="size-4 text-sky-300" />
        )}
        <span className="truncate">{node.name}</span>
      </Button>

      {isFolder && !isCollapsed && node.children.length > 0 ? (
        <ul className="ml-4 border-l border-white/10 pl-2">
          {node.children.map((child) => (
            <GitTreeNodeItem
              key={child.path}
              node={child}
              collapsed={collapsed}
              onToggle={onToggle}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
