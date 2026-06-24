#!/bin/bash
# Build script for Fast-copy.
# Supports native Linux build (for testing) and cross-compilation to Windows.
#
# Usage:
#   ./build.sh              # Build for Linux (dev/test only)
#   ./build.sh windows      # Cross-compile for Windows x64
#   ./build.sh release      # Linux release build
#   ./build.sh windows-release  # Windows release build (portable .exe)

set -e

case "${1:-linux}" in
    linux)
        echo "Building for Linux (debug)..."
        cargo build
        echo "Done: target/debug/fast-copy"
        ;;
    release)
        echo "Building for Linux (release)..."
        cargo build --release
        echo "Done: target/release/fast-copy"
        ;;
    windows)
        echo "Building for Windows x64 (debug)..."
        echo "Checking prerequisites..."
        if ! rustup target list --installed | grep -q x86_64-pc-windows-gnu; then
            echo "Adding Windows target..."
            rustup target add x86_64-pc-windows-gnu
        fi
        if ! command -v x86_64-w64-mingw32-gcc &>/dev/null; then
            echo "ERROR: mingw-w64-gcc not found."
            echo "Install it with:"
            echo "  Arch/CachyOS: sudo pacman -S mingw-w64-gcc"
            echo "  Ubuntu/Debian: sudo apt install gcc-mingw-w64-x86-64"
            echo "  Fedora: sudo dnf install mingw64-gcc"
            exit 1
        fi
        cargo build --target x86_64-pc-windows-gnu
        echo "Done: target/x86_64-pc-windows-gnu/debug/fast-copy.exe"
        ;;
    windows-release)
        echo "Building for Windows x64 (release)..."
        echo "Checking prerequisites..."
        if ! rustup target list --installed | grep -q x86_64-pc-windows-gnu; then
            echo "Adding Windows target..."
            rustup target add x86_64-pc-windows-gnu
        fi
        if ! command -v x86_64-w64-mingw32-gcc &>/dev/null; then
            echo "ERROR: mingw-w64-gcc not found."
            echo "Install it with:"
            echo "  Arch/CachyOS: sudo pacman -S mingw-w64-gcc"
            echo "  Ubuntu/Debian: sudo apt install gcc-mingw-w64-x86-64"
            echo "  Fedora: sudo dnf install mingw64-gcc"
            exit 1
        fi
        cargo build --release --target x86_64-pc-windows-gnu
        EXE="target/x86_64-pc-windows-gnu/release/fast-copy.exe"
        SIZE=$(du -h "$EXE" | cut -f1)
        echo "Done: $EXE ($SIZE)"
        echo ""
        echo "Copy this single .exe to your Windows machine to run it."
        echo "No installer or dependencies needed."
        ;;
    *)
        echo "Usage: $0 [linux|release|windows|windows-release]"
        exit 1
        ;;
esac
