# Fast-copy — Progress Feedback Overhaul Design

**Date:** 2026-06-28
**Status:** Approved (design); autonomous build delegated by user
**Branch:** `claude/windows-adaptive-copy-tool-ohxlac`
**Supersedes:** the progress UI shipped in v0.2.0–v0.2.1 (the earlier "progress block").

## Goal

Give the file-copy utility honest, continuous progress feedback in both the
**scan** and **copy** phases, and eliminate the "silent" gap when Copy is
pressed. Readouts use **icons instead of word labels** (folder icon for folder
counts, file icon for file counts, clock for time, etc.).

## Principles

- **Never silent, never frozen.** Every long operation shows immediate feedback;
  heavy work runs off the main thread.
- **Accurate widgets are primary; estimates are secondary and clearly marked.**
  Any estimate is de-emphasized and prefixed `~` / "approx." Never invent a
  total, percentage, or ETA by an unspecified method.

## Scan phase

### Accurate widgets (primary, always shown while scanning)
Indeterminate spinner plus live counters, updated continuously:
- files found (file icon)
- folders found (folder icon)
- total bytes found (storage icon) — accurate running sum
- elapsed time (clock icon)
- the path currently being scanned (short, truncated)

### Rough estimate (secondary, low confidence, optional)
An **approximate** remaining time and approximate totals, de-emphasized and
prefixed `~`/"approx.", computed **only** this way:
1. Read the immediate top-level subfolders of the added folder first — a single
   cheap `read_dir`, counting subdirectories only, no deep walk — to get **T**.
2. As each top-level subfolder is *fully* scanned, increment **C**.
3. Estimate `est_total_files = (files_found / C) * T` and
   `est_total_bytes = (bytes_found / C) * T`; derive ETA from current scan
   throughput (files/sec): `eta = (est_total_files − files_found) / throughput`.

**Visibility rules — show NO number (spinner + counters only) when:**
- T cannot be read, **or**
- C == 0, **or**
- T == 1 and that single subfolder is not yet partially scanned.

Because C only increments when a top-level subfolder *completes*, the `T == 1`
case shows no estimate until completion — consistent with the rule. No estimate
is ever produced by any other method.

### Backend mechanics
- `ScanProgress` (shared via `Arc`): atomics `files_found`, `folders_found`,
  `bytes_found`, `top_level_total (T)`, `top_level_done (C)`; a
  `Mutex<String>` `current_path`; a start `Instant`.
- `add_directory`: cheap `read_dir` → T; then deep-scan each top-level child
  (the existing cancellable `scan_directory`, extended to bump the atomics +
  update `current_path` per entry), incrementing C after each child completes.
- A ~150 ms sampler thread emits `scan://progress` with the counters + computed
  estimate (or no estimate, per the rules). It stops when the scan finishes.
- `add_directory`/`add_paths` keep returning the final `TreeDto` (off-thread via
  `spawn_blocking`); the sampler provides the live updates in between. These
  commands gain an `AppHandle` parameter to emit events.
- For `add_paths` (drag-drop of multiple items), T = total top-level subfolders
  across the dropped directories; loose files count toward `files_found`
  immediately. Same visibility rules.
- Cancellation (Stop) already wired via `scan_cancel`; unchanged.

### Event
`scan://progress`:
```
{ filesFound, foldersFound, bytesFound, elapsedSecs, currentPath,
  estimate: null | { etaSecs, totalFilesEst, totalBytesEst } }
```
`estimate` is `null` whenever the visibility rules say "no number."

## Copy phase

### Eliminate the silent state (priority bug)
Today `start_copy` is a synchronous command: pressing Copy builds the whole
queue on the main thread before any UI appears, so the app looks frozen/silent.
Fix:
- **Frontend:** on click, immediately enter a `preparing` phase — the Copy
  button switches to a pressed/disabled state and the progress UI appears at
  once (indeterminate) **before** the backend call.
- **Backend:** make `start_copy` an **async** command so queue construction runs
  off the main thread. When it returns, the determinate UI takes over with real
  totals; on error, revert to `idle` and toast.

### Determinate progress (totals known from the scan)
Single global determinate bar plus an icon readout:
- elapsed time (clock icon)
- approximate remaining time (clock/hourglass icon, prefixed `~`), recomputed
  live from actual copy throughput
- folders done / total (folder icon)
- files done / total (file icon)
- the file currently being copied (copy icon + short, truncated status line)

A small throughput **sparkline** is retained as a secondary visual; the
standalone "MiB/s" number is dropped (the icon readout above is the primary
data). [Reversible if undesired.]

### "folders done / total" definition (confirmed: Option 1)
A **folder** = any source directory containing ≥ 1 included file. **Total** =
such folders found during the scan. A folder is **done** when *all* its included
files have finished (copied, skipped, or failed).
- At queue-build time, group items by their **source parent directory**; assign
  each item a `folderId` and compute `foldersTotal` = distinct folders.
- The bridge tracks a per-folder remaining-file counter; on each
  done/failed/skipped it decrements, and increments `foldersDone` when a
  folder's counter hits 0.

### Event change
`copy://throughput` gains: `elapsedSecs`, `foldersDone`, `foldersTotal`,
`currentIndex` (since copies run on N threads, this is the **most recently
progressed** in-flight file's index; the frontend maps it to the queue row name
for the single status line). Existing fields (`totalCopied`, `totalBytes`, `eta`, file
counts) remain; the frontend formats `eta` with a `~` prefix.

## Components (frontend)

- `store`: add `scanProgress` slice and a `preparing` phase value.
- `api`: listen to `scan://progress`; `startCopy` sets `preparing` before the
  await; existing `scan` flow already sets `scanning`.
- `ScanProgress` view (replaces the bare spinner in the sources panel): spinner
  + icon counters + current path + optional `~` estimate.
- `ProgressDock`: icon-based readout (folder/file/clock icons), determinate bar,
  retained sparkline, current-file line; shows immediately in `preparing`.
- `icons.tsx`: add `IconClock` and `IconDrive` (storage); reuse `IconFolder`,
  `IconFile`, `IconCopy`.

## Error handling
Scan `read_dir`/metadata errors stay silently skipped (best-effort enumeration),
as today. Copy per-file failures continue to flow through `copy://file-failed`
into the queue and the completion summary. Command errors surface as toasts; a
failed `start_copy` reverts `preparing → idle`.

## Testing
- **Rust:** unit-test the pure scan estimator
  `estimate(files_found, bytes_found, C, T, elapsed) -> Option<{eta, totalFiles, totalBytes}>`,
  covering the no-number cases (T unreadable/0, C==0, T==1) and a normal case.
- **TS (Vitest):** unit-test copy "remaining time" derivation and the `~`
  formatting; existing `format` tests stay.
- Engine/config/sources Rust tests unchanged.
- No local end-to-end run (no local toolchain); CI builds + tests, user verifies
  the artifact.

## Out of scope
Per-file ETA in the queue rows (global ETA only), byte-accurate scan ETA
(file-count throughput is sufficient), reordering, and any estimate method other
than the top-level-subfolder extrapolation specified above.
