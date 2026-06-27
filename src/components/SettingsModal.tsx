import { useState, type ReactNode } from "react";
import { saveConfig } from "../api";
import { useStore } from "../store";
import { IconClose } from "./icons";

const MIB = 1024 * 1024;
const KIB = 1024;

export function SettingsModal() {
  const config = useStore((s) => s.config);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  const [threshold, setThreshold] = useState(
    config ? Math.round(config.size_threshold_bytes / MIB) : 16,
  );
  const [threads, setThreads] = useState(config?.thread_count ?? 4);
  const [unbuf, setUnbuf] = useState(
    config ? Math.round(config.unbuffered_buffer_bytes / MIB) : 8,
  );
  const [buf, setBuf] = useState(
    config ? Math.round(config.buffered_buffer_bytes / KIB) : 1024,
  );

  const close = () => setSettingsOpen(false);

  const apply = () => {
    if (!config) return close();
    void saveConfig({
      ...config,
      size_threshold_bytes: Math.max(1, threshold) * MIB,
      thread_count: Math.max(1, threads),
      unbuffered_buffer_bytes: Math.max(1, unbuf) * MIB,
      buffered_buffer_bytes: Math.max(1, buf) * KIB,
    });
    close();
  };

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h3>Settings</h3>
          <button className="icon-btn" onClick={close}>
            <IconClose size={16} />
          </button>
        </div>

        <div className="form">
          <Field label="Size threshold (MiB)" hint="Files ≥ this use unbuffered I/O">
            <input
              type="number"
              min={1}
              value={threshold}
              onChange={(e) => setThreshold(Number(e.target.value))}
            />
          </Field>
          <Field label="Worker threads" hint="Parallel small-file copies">
            <input
              type="number"
              min={1}
              value={threads}
              onChange={(e) => setThreads(Number(e.target.value))}
            />
          </Field>
          <Field label="Unbuffered buffer (MiB)" hint="Large-file copy buffer">
            <input
              type="number"
              min={1}
              value={unbuf}
              onChange={(e) => setUnbuf(Number(e.target.value))}
            />
          </Field>
          <Field label="Buffered buffer (KiB)" hint="Small-file copy buffer">
            <input
              type="number"
              min={1}
              value={buf}
              onChange={(e) => setBuf(Number(e.target.value))}
            />
          </Field>
        </div>

        <div className="modal-foot">
          <button className="btn ghost" onClick={close}>
            Cancel
          </button>
          <button className="btn primary" onClick={apply}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </label>
  );
}
