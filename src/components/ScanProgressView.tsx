import { useStore } from "../store";
import { formatBytes, formatDuration } from "../utils/format";
import { Spinner } from "./Spinner";
import { IconClock, IconDrive, IconFile, IconFolder } from "./icons";

export function ScanProgressView() {
  const p = useStore((s) => s.scanProgress);

  // Before the first sample arrives, just show the spinner.
  if (!p) {
    return <Spinner block size={28} label="Scanning files…" />;
  }

  return (
    <div className="scanview">
      <div className="scanview-head">
        <Spinner size={18} />
        <span className="scanview-title">Scanning files…</span>
        {p.estimate && (
          <span className="scanview-eta" title="Rough estimate — may be very wrong">
            ~{formatDuration(p.estimate.etaSecs)} remaining (approx.)
          </span>
        )}
      </div>

      <div className="scanview-stats">
        <span className="stat-chip" title="files found">
          <IconFile size={14} />
          {p.filesFound.toLocaleString()}
        </span>
        <span className="stat-chip" title="folders found">
          <IconFolder size={14} />
          {p.foldersFound.toLocaleString()}
        </span>
        <span className="stat-chip mono" title="bytes found">
          <IconDrive size={14} />
          {formatBytes(p.bytesFound)}
        </span>
        <span className="stat-chip mono" title="elapsed">
          <IconClock size={14} />
          {formatDuration(p.elapsedSecs)}
        </span>
      </div>

      {p.currentPath && (
        <div className="scanview-path mono" title={p.currentPath}>
          {p.currentPath}
        </div>
      )}
    </div>
  );
}
