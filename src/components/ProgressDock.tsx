import { useStore } from "../store";
import { formatDuration } from "../utils/format";
import { ThroughputChart } from "./ThroughputChart";
import { IconClock, IconCopy, IconFile, IconFolder } from "./icons";

export function ProgressDock() {
  const phase = useStore((s) => s.phase);
  const queue = useStore((s) => s.queue);
  const progress = useStore((s) => s.progress);
  const history = useStore((s) => s.throughputHistory);

  if (phase === "idle" && queue.length === 0) return null;

  // Preparing: queue not built yet — show an indeterminate bar, never silent.
  if (phase === "preparing") {
    return (
      <footer className="dock">
        <div className="dock-bar">
          <div className="gbar">
            <div className="gbar-indet" />
          </div>
          <span className="gpct">Preparing…</span>
        </div>
      </footer>
    );
  }

  const totalBytes = progress?.totalBytes ?? 0;
  const totalCopied = progress?.totalCopied ?? 0;
  const pct =
    phase === "done"
      ? 100
      : totalBytes > 0
        ? Math.min(100, Math.round((totalCopied / totalBytes) * 100))
        : 0;

  const elapsed = progress?.elapsedSecs ?? 0;
  const eta = progress?.eta ?? -1;
  const foldersDone = progress?.foldersDone ?? 0;
  const foldersTotal = progress?.foldersTotal ?? 0;
  const filesResolved =
    (progress?.filesDone ?? 0) +
    (progress?.filesFailed ?? 0) +
    (progress?.filesSkipped ?? 0);
  const filesTotal = queue.length;

  const currentIndex = progress?.currentIndex ?? null;
  const currentFile =
    currentIndex != null
      ? queue.find((r) => r.index === currentIndex)?.name
      : undefined;

  return (
    <footer className="dock">
      <div className="dock-bar">
        <div className="gbar">
          <div
            className={`gbar-fill ${phase === "copying" ? "live" : ""}`}
            style={{ width: `${pct}%` }}
          />
        </div>
        <span className="gpct mono">{pct}%</span>
      </div>

      <div className="dock-stats">
        <div className="stat chart-stat">
          <ThroughputChart data={history} />
        </div>

        <span className="readout mono" title="elapsed">
          <IconClock size={15} />
          {formatDuration(elapsed)}
        </span>
        <span className="readout mono dim" title="approximate time remaining">
          <IconClock size={15} />~{formatDuration(eta)}
        </span>
        <span className="readout mono" title="folders done / total">
          <IconFolder size={15} />
          {foldersDone}/{foldersTotal}
        </span>
        <span className="readout mono" title="files done / total">
          <IconFile size={15} />
          {filesResolved}/{filesTotal}
        </span>

        {currentFile && (
          <span className="readout current" title={currentFile}>
            <IconCopy size={15} />
            <span className="current-name">{currentFile}</span>
          </span>
        )}
      </div>
    </footer>
  );
}
