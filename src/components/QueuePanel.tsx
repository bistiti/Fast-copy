import { useStore } from "../store";
import type { QueueRow } from "../types";
import { formatBytes, formatDuration } from "../utils/format";
import { ActionBar } from "./ActionBar";
import { BenchmarkChip } from "./BenchmarkChip";
import { IconAlert, IconCheck, IconCopy } from "./icons";

function StatusGlyph({ status }: { status: QueueRow["status"] }) {
  switch (status) {
    case "done":
      return (
        <span className="glyph ok">
          <IconCheck size={14} />
        </span>
      );
    case "failed":
      return (
        <span className="glyph err">
          <IconAlert size={14} />
        </span>
      );
    case "inProgress":
      return <span className="glyph run spin" />;
    case "skipped":
      return <span className="glyph skip">–</span>;
    default:
      return <span className="glyph pending" />;
  }
}

function Row({ row, speed }: { row: QueueRow; speed: number }) {
  const frac = row.size > 0 ? row.bytesCopied / row.size : 1;
  const pct = Math.min(100, Math.round(frac * 100));
  const eta =
    row.status === "inProgress" && speed > 0
      ? formatDuration((row.size - row.bytesCopied) / speed)
      : null;

  return (
    <div className={`qrow ${row.status}`}>
      <StatusGlyph status={row.status} />
      <span className="qname" title={row.name}>
        {row.name}
      </span>
      <span className={`tag tag-${row.mode}`}>{row.mode}</span>
      <span className="qsize mono">{formatBytes(row.size)}</span>
      {row.status === "inProgress" ? (
        <span className="qprogress">
          <span className="qbar">
            <span className="qbar-fill" style={{ width: `${pct}%` }} />
          </span>
          <span className="qpct mono">{pct}%</span>
          {eta && <span className="qeta mono">{eta}</span>}
        </span>
      ) : row.status === "failed" ? (
        <span className="qerr" title={row.error}>
          {row.error}
        </span>
      ) : (
        <span className="qprogress" />
      )}
    </div>
  );
}

export function QueuePanel() {
  const queue = useStore((s) => s.queue);
  const speed = useStore((s) => s.progress?.speed ?? 0);

  return (
    <section className="panel queue">
      <div className="panel-head">
        <h2>Copy Queue</h2>
        <BenchmarkChip />
      </div>

      <ActionBar />

      <div className="scroll queue-scroll">
        {queue.length === 0 ? (
          <div className="empty">
            <IconCopy size={24} />
            <p>Add sources, pick a destination, then hit Copy.</p>
          </div>
        ) : (
          queue.map((row) => <Row key={row.index} row={row} speed={speed} />)
        )}
      </div>
    </section>
  );
}
