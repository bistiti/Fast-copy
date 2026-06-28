# Fast-copy

An adaptive file copy utility for Windows 10/11 x64 that dynamically selects
between buffered and unbuffered I/O on a per-file basis, aiming to outperform
both `copy` (for small files) and `robocopy /J` (for large files).

## How it works

Fast-copy uses the Win32 `CopyFileExW` API with different flags depending on
file size:

- **Small files** (below the threshold): copied with standard buffered I/O,
  parallelized across multiple threads. This competes with `robocopy /MT`
  which also uses multithreaded copies.
- **Large files** (at or above the threshold): copied with
  `COPY_FILE_NO_BUFFERING` to bypass the filesystem cache and avoid polluting
  memory, combined with `COPY_FILE_RESTARTABLE` for resume support.

The threshold between modes is determined by a real disk benchmark that
measures throughput at various file sizes on your actual source and destination
volumes. The benchmark result is cached per volume serial number and only
needs to run once.

## Features

- **Adaptive I/O mode**: automatically picks buffered or unbuffered per file
- **Disk benchmark**: real throughput measurement to find the optimal crossover
- **Multithreaded small-file copy**: thread pool parallelism (configurable)
- **Pause, cancel, and resume**: via CopyFileExW progress callback and journal
- **Long path support**: paths prefixed with `\\?\` for >260 character support
- **Drag-and-drop**: drop files/folders onto the window to add sources
- **Modern web UI** (Tauri 2 + React): sleek two-pane dashboard with a sticky
  progress dock, live throughput sparkline, per-file ETA, and a **light/dark
  theme toggle** (persisted)
- **Conflict handling**: choose Overwrite, Skip existing, or Keep both (rename)
- **Completion summary**: totals, duration, average speed, and an error list
- **Configurable**: all thresholds and buffer sizes tunable via the Settings
  dialog or the JSON file

## Screenshot layout

```
+-------------------------------------------------------+
| Destination: [C:\backup\        ] [Browse] Free: 1 TiB|
+------------------+------------------------------------+
| Sources          | Copy Queue                         |
| [+ Files][+ Dir] | Benchmark: Completed (16 MiB, 4t) |
| 42 files (3 GiB) | [Copy] [Pause] [Cancel]            |
|                  |                                    |
| [x] ProjectDir/  | v main.rs    [Buf]  12 KiB        |
|   [x] src/       | v config.rs  [Buf]  4 KiB         |
|     [x] main.rs  | > bigfile.iso [Unbuf] 4 GiB [45%] |
|   [x] bigfile.iso| . pending.txt [Buf]  128 B        |
|                  |                                    |
+------------------+------------------------------------+
| [============================        ] 67%            |
| 125.3 MiB/s | 2.01 GiB / 3.00 GiB | ETA: 8s | 3/42  |
+-------------------------------------------------------+
```

## Building

Fast-copy is a Tauri 2 app: a Rust backend (`src-tauri/`) plus a React + Vite
frontend. Build on **Windows** — Tauri apps cannot be cross-compiled to Windows
from Linux in this setup.

### Prerequisites

- Node.js 18+ and npm
- Rust toolchain via rustup (the pinned version in
  `src-tauri/rust-toolchain.toml` installs automatically)
- WebView2 runtime (ships with Windows 10/11)

### Commands

```bash
npm ci              # install frontend dependencies
npm run tauri dev   # run the app in development (hot reload)
npm run tauri build # produce the release artifacts
```

`npm run tauri build` produces:

- the portable executable at `src-tauri/target/release/fast-copy.exe`
- an NSIS installer at `src-tauri/target/release/bundle/nsis/*-setup.exe`

The helper script `./build.sh [deps|dev|build|test]` wraps these. CI
(`.github/workflows/`) builds and tests on `windows-latest` for every push and
publishes release artifacts on version tags.

## Usage

1. Launch `fast-copy.exe`.
2. Set the destination folder (Browse button or type the path).
3. Add source files/folders via the buttons or drag-and-drop.
4. (Optional) Run the benchmark to calibrate the buffered/unbuffered threshold
   for your specific disks.
5. Click **Copy**.
6. Use **Pause** to suspend, **Cancel** to abort. Progress is journaled, so
   re-launching and copying the same set will skip already-completed files.

### Configuration

Settings are stored in `fast-copy.json` next to the executable:

| Setting | Default | Description |
|---------|---------|-------------|
| `size_threshold_bytes` | 16 MiB | Files below this use buffered I/O |
| `thread_count` | CPU cores (2-8) | Worker threads for parallel copies |
| `unbuffered_buffer_bytes` | 8 MiB | Buffer size for unbuffered mode |
| `buffered_buffer_bytes` | 1 MiB | Buffer size for buffered mode |
| `max_memory_bytes` | 512 MiB | Total memory ceiling for buffers |

The benchmark overwrites `size_threshold_bytes` and `thread_count` with
measured values. You can always edit the JSON file or use the Settings panel
in the UI.

## Benchmark

The benchmark writes temporary files of sizes 256 KiB through 64 MiB on the
destination volume, copies each in both buffered and unbuffered mode, and
measures throughput. The first size where unbuffered I/O becomes faster than
buffered I/O becomes the threshold.

Results are cached in `fast-copy-benchmark.json` keyed by volume serial
number. The benchmark only runs when you click "Run Benchmark" -- it does not
run automatically on every launch.

If the benchmark cannot run (read-only volume, insufficient space, etc.), it
falls back to the default 16 MiB threshold and displays a warning.

**Important**: there is no universal threshold. The optimal crossover depends on
the specific hardware (SSD vs HDD, NVMe vs SATA, USB, network drives) and the
current system load. Always benchmark on your actual volumes for best results.

## Resume support

A journal file (`fast-copy-journal.log`) tracks completed files by destination
path. On re-launch, files already in the journal are skipped. For large files
in progress, `COPY_FILE_RESTARTABLE` allows CopyFileExW to resume from where
it left off.

**Note**: `COPY_FILE_RESTARTABLE` may reduce throughput slightly because the
OS writes restart-data bookkeeping alongside the copy. This is the tradeoff
for resume capability.

## Known limitations

- **Windows only**: the copy engine uses Win32 APIs (CopyFileExW,
  COPY_FILE_NO_BUFFERING) and the UI uses WebView2. On non-Windows the engine
  compiles as a stub for development; the shipped app targets Windows 10/11 x64.

- **Requires WebView2**: the UI renders in the system WebView2 runtime, which is
  preinstalled on Windows 10/11. The portable exe is not fully standalone in the
  sense that it relies on this evergreen runtime.

- **ACLs are NOT preserved**: `CopyFileExW` preserves timestamps and standard
  file attributes, but does not copy NTFS ACLs (Access Control Lists). If you
  need ACL preservation, use `robocopy /COPYALL` or `xcopy /O`.

- **No universal threshold**: the buffered vs. unbuffered crossover varies by
  hardware, driver, filesystem, and load. The benchmark measures your specific
  setup, but results may shift over time.

- **Robocopy /MT competition**: `robocopy /MT` (multithreaded mode) is already
  very fast for small files. Fast-copy's multithreaded buffered mode aims to
  match or exceed this, but robocopy has deep OS-level optimizations. The real
  advantage of Fast-copy is the adaptive per-file mode selection.

- **COPY_FILE_RESTARTABLE overhead**: enabling restart support adds I/O
  overhead. For maximum raw throughput on large files where resume is not
  needed, this flag could be removed (not currently exposed in the UI).

- **No encryption/compression awareness**: the tool does not account for
  NTFS-compressed or EFS-encrypted files differently.

## Project structure

```
src-tauri/                 -- Rust backend
  src/
    main.rs                -- Tauri builder; registers commands + state
    state.rs               -- Managed AppState (config, sources, dest, control)
    commands.rs            -- #[tauri::command] IPC handlers
    bridge.rs              -- Forwards engine events -> Tauri events + throughput
    dto.rs                 -- Serde DTOs shared with the frontend
    sources.rs             -- Source file/folder tree (data model)
    config.rs              -- Configuration (load/save/defaults)
    engine/
      copy_item.rs         -- CopyItem, CopyMode, CopyStatus
      journal.rs           -- Resume journal (completed file tracking)
      worker.rs            -- Thread pool orchestrator + ConflictPolicy
      win32.rs             -- Windows CopyFileExW implementation
      stub.rs              -- Non-Windows stub
    benchmark/
      runner.rs            -- Disk benchmark runner and cache
  tauri.conf.json, build.rs, capabilities/, icons/

src/                       -- React + Vite frontend
  main.tsx, App.tsx, styles.css
  store.ts                 -- Zustand store
  api.ts                   -- invoke() wrappers + event listeners
  types.ts, utils/format.ts
  components/               -- TopBar, SourcesPanel, SourceTree, QueuePanel,
                              BenchmarkChip, ActionBar, ProgressDock,
                              ThroughputChart, SettingsModal, CompletionSummary
```

## Testing

The copy engine is decoupled from the UI and from the OS so the orchestration is
unit-testable:

- All real copying sits behind the `FileCopier` trait (`engine/copier.rs`):
  `SystemCopier` wraps `CopyFileExW` on Windows / `std::fs::copy` elsewhere; a
  `MockCopier` replaces it in tests.
- The platform-independent file loop, cancellation checks, conflict policy,
  journal handling, and partial-file cleanup live in `engine/pipeline.rs`
  (`process_item` / `run_copy`) and are driven by the tests with temp directories.

Run the Rust tests (from `src-tauri/`) and the frontend tests:

```bash
cd src-tauri && cargo test     # engine, pipeline, scan estimator, enumeration
npm test                       # frontend formatters (Vitest)
```

Covered by automated tests (deterministic, mock copier, temp dirs):

1. Cancellation between files — cancel after file 1; asserts exactly one file is
   copied and the rest are never processed.
2. No corrupt destination on cancel — a simulated mid-file abort leaves the
   partial destination **removed** (not a truncated look-alike) and not
   journal-recorded.
3. Resume correctness — after a cancelled run, a resume skips the journaled file
   and copies only the remainder.
4. Cancel-immediately then restart — cancel before file 1 leaves no artifacts and
   nothing journaled; a fresh run then completes and contents match.
5. Buffered/unbuffered decision (`select_mode`, `faster_mode_by_throughput`) —
   pure functions tested on each side of the boundary, no I/O.
6. Enumeration counts — a known temp tree yields the expected file/folder totals.

**Not covered by unit tests** (stated explicitly, not implied): the real
`CopyFileExW` / `COPY_FILE_RESTARTABLE` cancellation behavior (the actual OS call
is mocked — only the orchestration around it is tested), and the live WebView/UI.
These require a Windows host and manual/integration testing of the built app.

## License

MIT
