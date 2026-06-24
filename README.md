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
- **Dark theme GUI**: clean two-panel layout with monospace stats display
- **Portable**: single executable, no installer, no external dependencies
- **Configurable**: all thresholds and buffer sizes tunable via UI or JSON file

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

### Native Windows build

```
cargo build --release
```

The output is `target\release\fast-copy.exe`.

### Cross-compilation from Linux (Arch/CachyOS)

1. Install the MinGW-w64 cross-compiler:

```bash
sudo pacman -S mingw-w64-gcc
```

2. Add the Windows target to Rust:

```bash
rustup target add x86_64-pc-windows-gnu
```

3. Build:

```bash
cargo build --release --target x86_64-pc-windows-gnu
```

The output is `target/x86_64-pc-windows-gnu/release/fast-copy.exe`.

The `.cargo/config.toml` file already configures the MinGW linker for the
Windows target.

### Static CRT

The release profile uses LTO and strips symbols. On Windows with the `gnu`
target, the CRT is statically linked by default. For MSVC targets, add
`-C target-feature=+crt-static` to RUSTFLAGS.

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
  COPY_FILE_NO_BUFFERING). The project compiles on Linux for development and
  testing, but the copy engine is a stub that uses `std::fs::copy`.

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
src/
  main.rs              -- Entry point, eframe launch
  config.rs            -- Configuration (load/save/defaults)
  engine/
    mod.rs             -- Engine module root
    copy_item.rs       -- CopyItem, CopyMode, CopyStatus types
    journal.rs         -- Resume journal (completed file tracking)
    worker.rs          -- Thread pool orchestrator
    win32.rs           -- Windows CopyFileExW implementation
    stub.rs            -- Non-Windows stub for cross-compilation
  benchmark/
    mod.rs             -- Benchmark module root
    runner.rs          -- Disk benchmark runner and cache
  gui/
    mod.rs             -- GUI module root
    app.rs             -- Main eframe::App, UI layout and logic
    source_tree.rs     -- Source file/folder tree with checkboxes
    style.rs           -- Dark theme, colors, formatting helpers
```

## License

MIT
