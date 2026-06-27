import { useState } from "react";
import { toggleNode } from "../api";
import type { TreeNode } from "../types";
import { formatBytes } from "../utils/format";
import { IconChevron } from "./icons";

interface Props {
  node: TreeNode;
  depth: number;
}

export function SourceTree({ node, depth }: Props) {
  const [open, setOpen] = useState(depth === 0);

  return (
    <div className="tnode">
      <div className="tnode-row" style={{ paddingLeft: depth * 14 }}>
        {node.isDir ? (
          <button
            className={`twist ${open ? "open" : ""}`}
            onClick={() => setOpen((v) => !v)}
            aria-label={open ? "Collapse" : "Expand"}
          >
            <IconChevron size={13} />
          </button>
        ) : (
          <span className="twist-spacer" />
        )}

        <label className="tcheck">
          <input
            type="checkbox"
            checked={node.included}
            onChange={(e) => void toggleNode(node.path, e.target.checked)}
          />
          <span className="box" />
        </label>

        <span
          className={`tname ${node.included ? "" : "dim"}`}
          title={node.path}
        >
          {node.name}
        </span>

        {!node.isDir && (
          <span className="tsize mono">{formatBytes(node.size)}</span>
        )}
      </div>

      {node.isDir && open && node.children.length > 0 && (
        <div className="tchildren">
          {node.children.map((c, i) => (
            <SourceTree node={c} depth={depth + 1} key={c.id + i} />
          ))}
        </div>
      )}
    </div>
  );
}
