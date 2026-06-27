import { useState } from "react";
import { useStore } from "../store";
import type { DonePayload } from "../types";
import { formatBytes, formatDuration, formatSpeed } from "../utils/format";
import { IconCheck, IconClose } from "./icons";

export function CompletionSummary({ summary }: { summary: DonePayload }) {
  const clearCopy = useStore((s) => s.clearCopy);
  const [showErrors, setShowErrors] = useState(false);

  const hasErrors = summary.filesFailed > 0;

  return (
    <div className="modal-overlay">
      <div className="modal summary">
        <div className="modal-head">
          <h3>
            <span className={`summary-badge ${hasErrors ? "warn" : "ok"}`}>
              <IconCheck size={16} />
            </span>
            Copy {hasErrors ? "finished with errors" : "complete"}
          </h3>
          <button className="icon-btn" onClick={() => clearCopy()}>
            <IconClose size={16} />
          </button>
        </div>

        <div className="summary-grid">
          <Metric label="Copied" value={formatBytes(summary.totalCopied)} />
          <Metric label="Duration" value={formatDuration(summary.elapsedSecs)} />
          <Metric label="Avg speed" value={formatSpeed(summary.avgSpeed)} />
          <Metric label="Succeeded" value={String(summary.filesDone)} />
          <Metric label="Failed" value={String(summary.filesFailed)} tone={hasErrors ? "err" : undefined} />
          <Metric label="Skipped" value={String(summary.filesSkipped)} />
        </div>

        {hasErrors && (
          <div className="summary-errors">
            <button className="link" onClick={() => setShowErrors((v) => !v)}>
              {showErrors ? "Hide" : "Show"} {summary.errors.length} error
              {summary.errors.length === 1 ? "" : "s"}
            </button>
            {showErrors && (
              <ul className="error-list">
                {summary.errors.map((err, i) => (
                  <li key={i}>{err}</li>
                ))}
              </ul>
            )}
          </div>
        )}

        <div className="modal-foot">
          <button className="btn primary" onClick={() => clearCopy()}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

function Metric({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "err";
}) {
  return (
    <div className="metric">
      <span className="metric-label">{label}</span>
      <span className={`metric-value mono ${tone ?? ""}`}>{value}</span>
    </div>
  );
}
