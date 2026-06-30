import { memo, useEffect, useRef, useState } from "react";
import { useStore } from "../store";
import type { QueueRow } from "../types";
import { formatBytes, formatDuration } from "../utils/format";
import { ActionBar } from "./ActionBar";
import { BenchmarkChip } from "./BenchmarkChip";
import { IconAlert, IconCheck, IconCopy } from "./icons";

// Fixed row height in px. MUST match `.qrow { height }` in styles.css — the
// windowing math below assumes every row is exactly this tall.
const ROW_HEIGHT = 34;
// Extra rows rendered above/below the viewport to avoid blank edges on fast scroll.
const OVERSCAN = 8;

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

const Row = memo(function Row({ row, speed }: { row: QueueRow; speed: number }) {
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
});

export function QueuePanel() {
  const queue = useStore((s) => s.queue);
  const speed = useStore((s) => s.progress?.speed ?? 0);

  // Virtualized window: only the rows intersecting the viewport are rendered, so
  // a 100k+ entry queue keeps a constant DOM size and React reconciles a handful
  // of rows per frame instead of the whole list.
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setViewport(el.clientHeight);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const total = queue.length;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const visible = Math.ceil((viewport || ROW_HEIGHT) / ROW_HEIGHT) + OVERSCAN * 2;
  const last = Math.min(total, first + visible);
  const slice = queue.slice(first, last);

  return (
    <section className="panel queue">
      <div className="panel-head">
        <h2>Copy Queue</h2>
        <BenchmarkChip />
      </div>

      <ActionBar />

      <div
        className="scroll queue-scroll"
        ref={scrollRef}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      >
        {total === 0 ? (
          <div className="empty">
            <IconCopy size={24} />
            <p>Add sources, pick a destination, then hit Copy.</p>
          </div>
        ) : (
          <div className="qvirt" style={{ height: total * ROW_HEIGHT }}>
            <div
              className="qvirt-window"
              style={{ transform: `translateY(${first * ROW_HEIGHT}px)` }}
            >
              {slice.map((row) => (
                <Row key={row.index} row={row} speed={speed} />
              ))}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
