# Fast-copy — Tauri UI Rewrite Design

**Date:** 2026-06-27
**Status:** Approved (autonomous build delegated by user)
**Branch:** `claude/windows-adaptive-copy-tool-ohxlac`

## Goal

Replace the egui/eframe GUI with a modern **Tauri 2 + React + Vite** desktop UI,
styled as a **sleek dark dashboard with a light/dark toggle**. Keep full feature
parity with the current app and add high-value upgrades. The Rust copy engine is
reused essentially unchanged.

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Shell | Tauri 2 (WebView2 runtime, already present on the target machine) |
| Frontend | React 18 + TypeScript + Vite |
| Aesthetic | Sleek dark dashboard, accent gradient, light/dark toggle (persisted) |
| Layout | Two-pane dashboard + sticky bottom progress dock |
| Scope | Parity **+** throughput chart, per-file ETA, conflict policy, completion summary |
| State mgmt | Zustand |
| Toolchain | No local installs — build & verify via GitHub Actions only |

## Architecture

```
fast-copy/
├─ src-tauri/                 ← Rust backend (former crate, restructured)
│  ├─ src/
│  │  ├─ main.rs              ← Tauri builder, registers commands + managed state
│  │  ├─ state.rs            ← AppState (config, sources, dest, copy control)
│  │  ├─ commands.rs         ← #[tauri::command] IPC surface
│  │  ├─ bridge.rs           ← drains engine channel → Tauri events + throughput sampling
│  │  ├─ dto.rs              ← serde DTOs shared with the frontend
│  │  ├─ sources.rs          ← moved from gui/source_tree.rs (pure data)
│  │  ├─ config.rs           ← unchanged
│  │  ├─ engine/             ← unchanged except worker.rs gains ConflictPolicy
│  │  └─ benchmark/          ← unchanged
│  ├─ Cargo.toml             ← drops eframe/egui/rfd; adds tauri + tauri-plugin-dialog
│  ├─ build.rs               ← tauri_build::build()
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  └─ icons/                 ← app icons (generated)
├─ src/                       ← React frontend
│  ├─ main.tsx, App.tsx, styles.css
│  ├─ store.ts               ← Zustand store
│  ├─ api.ts                 ← typed invoke() wrappers + event subscriptions
│  ├─ types.ts, theme.ts
│  ├─ utils/format.ts (+ .test.ts)
│  └─ components/            ← TopBar, SourcesPanel, SourceTree, QueuePanel,
│                              BenchmarkChip, ActionBar, ProgressDock,
│                              ThroughputChart, SettingsModal, CompletionSummary, icons
├─ index.html, package.json, vite.config.ts, tsconfig*.json
└─ .github/workflows/release.yml  ← updated to build the Tauri app
```

## IPC surface

### Commands (frontend → Rust)
- `add_sources(paths: string[])` → `TreeDto` — add files
- `add_directory(path)` → `TreeDto` — add a recursively-scanned folder
- `remove_root(index)` → `TreeDto`
- `toggle_node(path, included)` → `TreeDto` — cascades to descendants for dirs
- `clear_sources()` → `TreeDto`
- `set_destination(path)` → `{ freeSpace: number | null }`
- `run_benchmark()` → `()` (result delivered via event)
- `get_config()` / `set_config(cfg)` → `Config`
- `start_copy(conflictPolicy)` → `QueueEntryDto[]`
- `pause()` / `resume()` / `cancel()` → `()`

Native file/folder dialogs are invoked in the frontend via
`@tauri-apps/plugin-dialog`, then paths are passed to `add_*`/`set_destination`.

### Events (Rust → frontend), emitted by the `bridge` thread
- `copy://progress` `{ index, bytesCopied }`
- `copy://file-done` `{ index }`
- `copy://file-failed` `{ index, error }`
- `copy://file-skipped` `{ index }`
- `copy://throughput` `{ speed, totalCopied, totalBytes, eta, filesDone, filesFailed, filesSkipped }` (~every 300 ms)
- `copy://done` `{ totalCopied, totalBytes, elapsedSecs, avgSpeed, filesDone, filesFailed, filesSkipped, errors }`
- `benchmark://status` `{ state: "running"|"completed"|"failed", thresholdMib?, threads?, message? }`

This preserves the engine's threading model: instead of egui polling the
`crossbeam` channel per frame, one forwarder thread drains it and re-emits as
Tauri events, while sampling throughput on a 300 ms `recv_timeout` tick.

## Engine change: conflict policy

`worker.rs` gains a `ConflictPolicy { Overwrite, Skip, Rename }` passed through
`CopyOrchestrator::new`. In the worker loop, after the journal check and before
copying:
- **Overwrite** — proceed (CopyFileExW overwrites by default).
- **Skip** — if the destination already exists on disk, emit `FileSkipped`.
- **Rename** — if it exists, retarget to `name (1).ext`, `name (2).ext`, … (unique).

`compute_destination` (relative-structure preservation) moves from the old
`gui/app.rs` into the backend.

## Frontend behavior

- **Two-pane dashboard**: left = sources tree + add buttons + summary; center =
  benchmark chip, action bar, queue list (status glyph, name, `[mode]`, size,
  per-file bar + ETA); bottom sticky dock = global bar, throughput sparkline,
  speed / copied-of-total / ETA / counts.
- **Throughput chart**: custom lightweight SVG sparkline over a rolling window of
  `copy://throughput` samples — no heavy charting dependency.
- **Theme**: CSS custom properties; dark default + light; toggle persisted in the
  config file (new `theme` field) so it survives restarts.
- **Settings modal**: threshold (MiB), threads, unbuffered buffer (MiB), buffered
  buffer (KiB), conflict policy — mirrors current config plus policy.
- **Completion summary**: card with totals, duration, average speed, and a
  collapsible error list.

## Error handling

Commands return `Result<_, String>`; the frontend surfaces failures as toasts.
Per-file failures flow through `copy://file-failed` into the queue rows and the
final summary's error list.

## Testing

- Existing Rust unit tests for `config`, `copy_item`, `sources`, `benchmark`
  remain (the `sources` tests come over with the file).
- Frontend: Vitest unit tests for `utils/format.ts` (bytes / speed / duration,
  ported from the old `style.rs`).
- No local end-to-end run is possible (no local toolchain); CI builds the bundle
  and the user verifies the artifact.

## CI

`release.yml` updated: checkout → setup Node + `npm ci` → setup Rust (rustup via
`rust-toolchain.toml`) → generate icons (`tauri icon`) → `npm run tauri build` →
publish the NSIS installer **and** the portable exe. WebView2 is evergreen, so the
runtime is not embedded (keeps size down).

## Out of scope (v1)

Per-file "ask on each conflict" dialog (global policy only), copy verification
(hash check), drag-to-reorder queue, multi-window.
