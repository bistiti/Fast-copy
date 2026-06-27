// Shared types mirroring the Rust DTOs and event payloads.

export interface TreeNode {
  id: string;
  name: string;
  path: string;
  isDir: boolean;
  included: boolean;
  size: number;
  children: TreeNode[];
}

export interface Tree {
  roots: TreeNode[];
  totalFiles: number;
  totalSize: number;
}

export type CopyMode = "buffered" | "unbuffered";

export interface QueueEntryDto {
  index: number;
  name: string;
  size: number;
  mode: CopyMode;
}

export type RowStatus =
  | "pending"
  | "inProgress"
  | "done"
  | "failed"
  | "skipped";

export interface QueueRow extends QueueEntryDto {
  status: RowStatus;
  bytesCopied: number;
  error?: string;
}

export type ConflictPolicy = "overwrite" | "skip" | "rename";

// Matches the Rust `Config` struct (serde, snake_case).
export interface Config {
  size_threshold_bytes: number;
  unbuffered_buffer_bytes: number;
  buffered_buffer_bytes: number;
  thread_count: number;
  max_memory_bytes: number;
  theme: string;
}

export type Phase = "idle" | "preparing" | "copying" | "paused" | "done";

export interface ScanEstimate {
  etaSecs: number;
  totalFilesEst: number;
  totalBytesEst: number;
}

export interface ScanProgressPayload {
  filesFound: number;
  foldersFound: number;
  bytesFound: number;
  elapsedSecs: number;
  currentPath: string;
  estimate: ScanEstimate | null;
}

export type BenchmarkState =
  | "notRun"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export interface BenchmarkInfo {
  state: BenchmarkState;
  thresholdMib?: number;
  threads?: number;
  message?: string;
}

export interface ThroughputPayload {
  speed: number;
  totalCopied: number;
  totalBytes: number;
  eta: number;
  elapsedSecs: number;
  filesDone: number;
  filesFailed: number;
  filesSkipped: number;
  foldersDone: number;
  foldersTotal: number;
  currentIndex: number | null;
}

export interface DonePayload {
  totalCopied: number;
  totalBytes: number;
  elapsedSecs: number;
  avgSpeed: number;
  filesDone: number;
  filesFailed: number;
  filesSkipped: number;
  errors: string[];
}
