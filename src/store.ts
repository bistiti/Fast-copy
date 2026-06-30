// Central Zustand store. Holds all UI state plus the reducers that the IPC event
// listeners (see api.ts) call as copy progress streams in.

import { create } from "zustand";
import type {
  BenchmarkInfo,
  Config,
  ConflictPolicy,
  CopyBatchPayload,
  DonePayload,
  Phase,
  QueueEntryDto,
  QueueRow,
  ScanProgressPayload,
  ThroughputPayload,
  Tree,
} from "./types";

const HISTORY_LEN = 120;
const EMPTY_TREE: Tree = { roots: [], totalFiles: 0, totalSize: 0 };

interface AppStore {
  // --- data ---
  tree: Tree;
  destination: string;
  freeSpace: number | null;
  config: Config | null;
  theme: "dark" | "light";
  benchmark: BenchmarkInfo;
  phase: Phase;
  queue: QueueRow[];
  progress: ThroughputPayload | null;
  throughputHistory: number[];
  summary: DonePayload | null;
  conflictPolicy: ConflictPolicy;
  settingsOpen: boolean;
  toast: string | null;
  scanning: boolean;
  scanProgress: ScanProgressPayload | null;

  // --- plain setters ---
  setTree: (tree: Tree) => void;
  setDestination: (path: string, freeSpace: number | null) => void;
  setConfig: (config: Config) => void;
  setTheme: (theme: "dark" | "light") => void;
  setBenchmark: (info: BenchmarkInfo) => void;
  setConflictPolicy: (p: ConflictPolicy) => void;
  setSettingsOpen: (open: boolean) => void;
  showToast: (msg: string | null) => void;
  setScanning: (scanning: boolean) => void;
  setScanProgress: (p: ScanProgressPayload | null) => void;

  // --- copy lifecycle ---
  beginCopy: (entries: QueueEntryDto[]) => void;
  setPhase: (phase: Phase) => void;
  clearCopy: () => void;

  // --- event reducers ---
  applyBatch: (b: CopyBatchPayload) => void;
  onDone: (p: DonePayload) => void;
}

// Queue entries are built in index order (see `start_copy`), so a row's array
// position equals its `index`. We rely on that for O(1) lookup, with a guard.
function rowPosition(queue: QueueRow[], index: number): number {
  if (queue[index]?.index === index) return index;
  return queue.findIndex((r) => r.index === index);
}

export const useStore = create<AppStore>((set) => ({
  tree: EMPTY_TREE,
  destination: "",
  freeSpace: null,
  config: null,
  theme: "dark",
  benchmark: { state: "notRun" },
  phase: "idle",
  queue: [],
  progress: null,
  throughputHistory: [],
  summary: null,
  conflictPolicy: "overwrite",
  settingsOpen: false,
  toast: null,
  scanning: false,
  scanProgress: null,

  setTree: (tree) => set({ tree }),
  setDestination: (destination, freeSpace) => set({ destination, freeSpace }),
  setConfig: (config) =>
    set({ config, theme: config.theme === "light" ? "light" : "dark" }),
  setTheme: (theme) => set({ theme }),
  setBenchmark: (benchmark) => set({ benchmark }),
  setConflictPolicy: (conflictPolicy) => set({ conflictPolicy }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  showToast: (toast) => set({ toast }),
  setScanning: (scanning) => set({ scanning }),
  setScanProgress: (scanProgress) => set({ scanProgress }),

  beginCopy: (entries) =>
    set({
      phase: "copying",
      summary: null,
      progress: null,
      throughputHistory: [],
      queue: entries.map((e) => ({
        ...e,
        status: "pending",
        bytesCopied: 0,
      })),
    }),
  setPhase: (phase) => set({ phase }),
  clearCopy: () =>
    set({
      phase: "idle",
      queue: [],
      progress: null,
      throughputHistory: [],
      summary: null,
    }),

  // Apply one coalesced batch (rows + throughput) in a single store update, so a
  // copy of any size produces at most one React render per ~300 ms tick. The
  // queue array is shallow-copied once (cheap pointer copy); only changed rows
  // get a new object — O(changed rows), not O(queue) of object spreads.
  applyBatch: (b) =>
    set((s) => {
      let queue = s.queue;
      if (b.rows.length > 0) {
        queue = s.queue.slice();
        for (const d of b.rows) {
          const pos = rowPosition(queue, d.index);
          if (pos < 0) continue;
          const cur = queue[pos];
          queue[pos] = {
            ...cur,
            status: d.status,
            bytesCopied: d.bytesCopied,
            ...(d.error != null ? { error: d.error } : {}),
          };
        }
      }
      return {
        queue,
        progress: b.throughput,
        throughputHistory: [...s.throughputHistory, b.throughput.speed].slice(
          -HISTORY_LEN,
        ),
      };
    }),
  onDone: (summary) => set({ phase: "done", summary }),
}));
