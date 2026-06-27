// Central Zustand store. Holds all UI state plus the reducers that the IPC event
// listeners (see api.ts) call as copy progress streams in.

import { create } from "zustand";
import type {
  BenchmarkInfo,
  Config,
  ConflictPolicy,
  DonePayload,
  Phase,
  QueueEntryDto,
  QueueRow,
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

  // --- plain setters ---
  setTree: (tree: Tree) => void;
  setDestination: (path: string, freeSpace: number | null) => void;
  setConfig: (config: Config) => void;
  setTheme: (theme: "dark" | "light") => void;
  setBenchmark: (info: BenchmarkInfo) => void;
  setConflictPolicy: (p: ConflictPolicy) => void;
  setSettingsOpen: (open: boolean) => void;
  showToast: (msg: string | null) => void;

  // --- copy lifecycle ---
  beginCopy: (entries: QueueEntryDto[]) => void;
  setPhase: (phase: Phase) => void;
  clearCopy: () => void;

  // --- event reducers ---
  onProgress: (index: number, bytesCopied: number) => void;
  onFileDone: (index: number) => void;
  onFileFailed: (index: number, error: string) => void;
  onFileSkipped: (index: number) => void;
  onThroughput: (p: ThroughputPayload) => void;
  onDone: (p: DonePayload) => void;
}

function updateRow(
  queue: QueueRow[],
  index: number,
  patch: Partial<QueueRow>,
): QueueRow[] {
  return queue.map((row) => (row.index === index ? { ...row, ...patch } : row));
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

  setTree: (tree) => set({ tree }),
  setDestination: (destination, freeSpace) => set({ destination, freeSpace }),
  setConfig: (config) =>
    set({ config, theme: config.theme === "light" ? "light" : "dark" }),
  setTheme: (theme) => set({ theme }),
  setBenchmark: (benchmark) => set({ benchmark }),
  setConflictPolicy: (conflictPolicy) => set({ conflictPolicy }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  showToast: (toast) => set({ toast }),

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

  onProgress: (index, bytesCopied) =>
    set((s) => ({
      queue: updateRow(s.queue, index, { status: "inProgress", bytesCopied }),
    })),
  onFileDone: (index) =>
    set((s) => ({
      queue: updateRow(s.queue, index, {
        status: "done",
        bytesCopied:
          s.queue.find((r) => r.index === index)?.size ?? 0,
      }),
    })),
  onFileFailed: (index, error) =>
    set((s) => ({
      queue: updateRow(s.queue, index, { status: "failed", error }),
    })),
  onFileSkipped: (index) =>
    set((s) => ({
      queue: updateRow(s.queue, index, { status: "skipped" }),
    })),
  onThroughput: (p) =>
    set((s) => ({
      progress: p,
      throughputHistory: [...s.throughputHistory, p.speed].slice(-HISTORY_LEN),
    })),
  onDone: (summary) => set({ phase: "done", summary }),
}));
