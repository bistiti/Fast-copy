import { useStore } from "../store";
import { formatBytes, formatDuration, formatSpeed } from "../utils/format";
import { ThroughputChart } from "./ThroughputChart";

export function ProgressDock() {
  const phase = useStore((s) => s.phase);
  const queue = useStore((s) => s.queue);
  const progress = useStore((s) => s.progress);
  const history = useStore((s) => s.throughputHistory);

  if (phase === "idle" && queue.length === 0) return null;

  const totalBytes = progress?.totalBytes ?? 0;
  const totalCopied = progress?.totalCopied ?? 0;
  const frac = totalBytes > 0 ? totalCopied / totalBytes : 0;
  const pct = Math.min(100, Math.round(frac * 100));
  const speed = progress?.speed ?? 0;
  const eta = progress?.eta ?? -1;

  const done = progress?.filesDone ?? 0;
  const failed = progress?.filesFailed ?? 0;
  const skipped = progress?.filesSkipped ?? 0;

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
          <span className="stat-speed mono">{formatSpeed(speed)}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Copied</span>
          <span className="mono">
            {formatBytes(totalCopied)} / {formatBytes(totalBytes)}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">ETA</span>
          <span className="mono">{formatDuration(eta)}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Files</span>
          <span className="mono">
            <span className="ok">{done}</span> ·{" "}
            <span className={failed ? "err" : ""}>{failed}</span> ·{" "}
            <span className="dim">{skipped}</span> / {queue.length}
          </span>
        </div>
      </div>
    </footer>
  );
}
