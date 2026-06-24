@echo off
REM Test launcher for Fast-copy on Windows.
REM Run this from the project root after building with: cargo build
REM Or run the portable .exe directly: fast-copy.exe

echo ============================================
echo  Fast-copy Test Launcher
echo ============================================
echo.

REM Check if the exe exists in common locations
if exist "target\release\fast-copy.exe" (
    echo Found release build.
    set EXE=target\release\fast-copy.exe
) else if exist "target\debug\fast-copy.exe" (
    echo Found debug build.
    set EXE=target\debug\fast-copy.exe
) else if exist "fast-copy.exe" (
    echo Found portable exe.
    set EXE=fast-copy.exe
) else (
    echo ERROR: fast-copy.exe not found.
    echo Build first with: cargo build --release
    echo Or place fast-copy.exe in this directory.
    pause
    exit /b 1
)

echo Launching: %EXE%
echo.
echo --- Quick test checklist ---
echo  1. Set a destination folder (Browse or type path)
echo  2. Add source files/folders (drag-and-drop or buttons)
echo  3. Run Benchmark (optional, calibrates threshold)
echo  4. Click Copy and verify:
echo     - Small files show [Buffered] mode
echo     - Large files show [Unbuffered] mode
echo     - Progress bar, speed, ETA update correctly
echo  5. Test Pause / Resume / Cancel
echo  6. Close and relaunch to test resume (journal skip)
echo  7. Check Settings panel (threshold, threads)
echo ============================
echo.

start "" "%EXE%"
