import { clearSources, pickFiles, pickFolder, removeRoot } from "../api";
import { useStore } from "../store";
import { formatBytes } from "../utils/format";
import { SourceTree } from "./SourceTree";
import { Spinner } from "./Spinner";
import { IconClose, IconFile, IconFolder, IconPlus } from "./icons";

export function SourcesPanel() {
  const tree = useStore((s) => s.tree);
  const scanning = useStore((s) => s.scanning);

  return (
    <section className="panel sources">
      <div className="panel-head">
        <h2>Sources</h2>
        <div className="panel-head-actions">
          <button
            className="btn small"
            disabled={scanning}
            onClick={() => void pickFiles()}
          >
            <IconFile size={14} />
            Files
          </button>
          <button
            className="btn small"
            disabled={scanning}
            onClick={() => void pickFolder()}
          >
            <IconFolder size={14} />
            Folder
          </button>
        </div>
      </div>

      <div className="sources-summary">
        <span>
          <strong>{tree.totalFiles.toLocaleString()}</strong> files
        </span>
        <span className="mono">{formatBytes(tree.totalSize)}</span>
        {tree.roots.length > 0 && (
          <button className="link" onClick={() => void clearSources()}>
            Clear
          </button>
        )}
      </div>

      <div className="scroll tree-scroll">
        {scanning ? (
          <Spinner block size={28} label="Scanning files…" />
        ) : tree.roots.length === 0 ? (
          <div className="empty">
            <IconPlus size={22} />
            <p>Add files or folders, or drop them here.</p>
          </div>
        ) : (
          tree.roots.map((root, i) => (
            <div className="root-row" key={root.id + i}>
              <button
                className="icon-btn tiny remove"
                title="Remove"
                onClick={() => void removeRoot(i)}
              >
                <IconClose size={13} />
              </button>
              <SourceTree node={root} depth={0} />
            </div>
          ))
        )}
      </div>
    </section>
  );
}
