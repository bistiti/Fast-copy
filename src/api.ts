// Typed wrappers over Tauri IPC: commands (invoke), native dialogs, and the
// event subscriptions that feed live copy progress into the store.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "./store";
import type {
  BenchmarkInfo,
  Config,
  CopyBatchPayload,
  DonePayload,
  QueueEntryDto,
  ScanProgressPayload,
  Tree,
} from "./types";

/** Debug-gated client trace (Step 2). Enable with
 *  `localStorage.setItem("FASTCOPY_TRACE", "1")` then reload. Logs to the
 *  devtools console with a high-resolution timestamp; pairs with the Rust-side
 *  `%TEMP%\fastcopy-trace.log` to measure webview event-loop backpressure. */
function trace(msg: string) {
  try {
    if (localStorage.getItem("FASTCOPY_TRACE") === "1") {
      // eslint-disable-next-line no-console
      console.log(`[trace ${performance.now().toFixed(1)}ms] ${msg}`);
    }
  } catch {
    /* localStorage may be unavailable; tracing is best-effort. */
  }
}

const store = () => useStore.getState();

/** Run an async action, surfacing any backend error as a toast. */
async function guard<T>(fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    store().showToast(typeof e === "string" ? e : String(e));
    return undefined;
  }
}

// ---- config ----

export async function loadConfig() {
  const cfg = await guard(() => invoke<Config>("get_config"));
  if (cfg) store().setConfig(cfg);
}

export async function saveConfig(config: Config) {
  const cfg = await guard(() => invoke<Config>("set_config", { config }));
  if (cfg) store().setConfig(cfg);
}

export async function setTheme(theme: "dark" | "light") {
  const cfg = store().config;
  store().setTheme(theme);
  if (cfg) await saveConfig({ ...cfg, theme });
}

// ---- sources ----

export async function pickFiles() {
  const sel = await open({ multiple: true, title: "Add files" });
  if (!sel) return;
  const paths = Array.isArray(sel) ? sel : [sel];
  const tree = await guard(() => invoke<Tree>("add_sources", { paths }));
  if (tree) store().setTree(tree);
}

export async function pickFolder() {
  const dir = await open({ directory: true, title: "Add folder" });
  if (!dir || Array.isArray(dir)) return;
  await addDirectory(dir);
}

export async function addDirectory(path: string) {
  // The backend scans off the main thread; show the spinner while we wait.
  store().setScanning(true);
  try {
    const tree = await invoke<Tree>("add_directory", { path });
    store().setTree(tree);
  } catch (e) {
    store().showToast(typeof e === "string" ? e : String(e));
  } finally {
    store().setScanning(false);
    store().setScanProgress(null);
  }
}

export async function addPaths(paths: string[]) {
  if (paths.length === 0) return;
  store().setScanning(true);
  try {
    const tree = await invoke<Tree>("add_paths", { paths });
    store().setTree(tree);
  } catch (e) {
    store().showToast(typeof e === "string" ? e : String(e));
  } finally {
    store().setScanning(false);
    store().setScanProgress(null);
  }
}

export async function toggleNode(path: string, included: boolean) {
  const tree = await guard(() =>
    invoke<Tree>("toggle_node", { path, included }),
  );
  if (tree) store().setTree(tree);
}

export async function removeRoot(index: number) {
  const tree = await guard(() => invoke<Tree>("remove_root", { index }));
  if (tree) store().setTree(tree);
}

export async function clearSources() {
  const tree = await guard(() => invoke<Tree>("clear_sources"));
  if (tree) store().setTree(tree);
}

// ---- destination ----

export async function pickDestination() {
  const dir = await open({ directory: true, title: "Choose destination" });
  if (!dir || Array.isArray(dir)) return;
  await setDestination(dir);
}

export async function setDestination(path: string) {
  const info = await guard(() =>
    invoke<{ freeSpace: number | null }>("set_destination", { path }),
  );
  if (info) store().setDestination(path, info.freeSpace);
}

// ---- benchmark ----

export async function runBenchmark() {
  store().setBenchmark({ state: "running" });
  await guard(() => invoke("run_benchmark"));
}

// ---- copy control ----

export async function startCopy() {
  const conflictPolicy = store().conflictPolicy;
  // Show the progress UI + pressed button immediately, before the backend
  // builds the queue — no silent gap.
  store().setPhase("preparing");
  try {
    const entries = await invoke<QueueEntryDto[]>("start_copy", {
      conflictPolicy,
    });
    store().beginCopy(entries);
  } catch (e) {
    store().setPhase("idle");
    store().showToast(typeof e === "string" ? e : String(e));
  }
}

export async function pauseCopy() {
  await guard(() => invoke("pause"));
  store().setPhase("paused");
}

export async function resumeCopy() {
  await guard(() => invoke("resume"));
  store().setPhase("copying");
}

/// Stop whatever is running: scan, benchmark, or copy.
export async function stop() {
  trace("stop: invoking cancel");
  await guard(() => invoke("cancel"));
  trace("stop: cancel invoke returned");
}

// ---- event wiring ----

export async function setupListeners() {
  const s = store();

  // A single coalesced event carries all per-file row deltas plus the throughput
  // aggregate, at ~3×/sec. This replaces the former per-file event stream, which
  // flooded the webview event loop on large queues.
  await listen<CopyBatchPayload>("copy://batch", (e) => {
    trace(`batch: ${e.payload.rows.length} rows`);
    s.applyBatch(e.payload);
  });
  await listen<DonePayload>("copy://done", (e) => {
    trace("done");
    s.onDone(e.payload);
  });

  await listen<ScanProgressPayload>("scan://progress", (e) => {
    // Ignore late events after a scan has ended.
    if (store().scanning) store().setScanProgress(e.payload);
  });

  await listen<BenchmarkInfo>("benchmark://status", (e) => {
    s.setBenchmark(e.payload);
    if (e.payload.state === "completed") {
      // The backend auto-tuned threshold/threads; refresh the config view.
      void loadConfig();
    }
  });

  // Native drag-and-drop of files/folders onto the window.
  await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop") {
      void addPaths(event.payload.paths);
    }
  });
}
