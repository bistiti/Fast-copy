import {
  cancelCopy,
  pauseCopy,
  resumeCopy,
  startCopy,
} from "../api";
import { useStore } from "../store";
import type { ConflictPolicy } from "../types";
import { IconCopy, IconPause, IconPlay, IconStop } from "./icons";

const POLICIES: { value: ConflictPolicy; label: string }[] = [
  { value: "overwrite", label: "Overwrite" },
  { value: "skip", label: "Skip existing" },
  { value: "rename", label: "Keep both" },
];

export function ActionBar() {
  const phase = useStore((s) => s.phase);
  const tree = useStore((s) => s.tree);
  const destination = useStore((s) => s.destination);
  const conflictPolicy = useStore((s) => s.conflictPolicy);
  const setConflictPolicy = useStore((s) => s.setConflictPolicy);
  const clearCopy = useStore((s) => s.clearCopy);

  const canCopy =
    phase === "idle" && tree.totalFiles > 0 && destination.trim().length > 0;
  const running = phase === "copying" || phase === "paused";

  return (
    <div className="actionbar">
      {phase !== "done" && (
        <button
          className="btn primary"
          disabled={!canCopy}
          onClick={() => void startCopy()}
        >
          <IconCopy size={16} />
          Copy
        </button>
      )}

      {phase === "copying" && (
        <button className="btn" onClick={() => void pauseCopy()}>
          <IconPause size={15} />
          Pause
        </button>
      )}
      {phase === "paused" && (
        <button className="btn" onClick={() => void resumeCopy()}>
          <IconPlay size={15} />
          Resume
        </button>
      )}
      {running && (
        <button className="btn danger" onClick={() => void cancelCopy()}>
          <IconStop size={15} />
          Cancel
        </button>
      )}
      {phase === "done" && (
        <button className="btn" onClick={() => clearCopy()}>
          Clear
        </button>
      )}

      <div className="spacer" />

      <label className="policy">
        On conflict
        <select
          value={conflictPolicy}
          disabled={running}
          onChange={(e) =>
            setConflictPolicy(e.target.value as ConflictPolicy)
          }
        >
          {POLICIES.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
