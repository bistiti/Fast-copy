#!/bin/bash
# Developer helper for the Tauri build of Fast-copy.
#
# Prerequisites (Windows):
#   - Node.js 18+ and npm
#   - Rust toolchain (rustup; the pinned version installs automatically)
#   - WebView2 runtime (ships with Windows 10/11)
#
# Usage:
#   ./build.sh deps      # install frontend dependencies (npm ci)
#   ./build.sh dev       # run the app in dev mode (Vite + Tauri)
#   ./build.sh build     # produce the release bundle (exe + NSIS installer)
#   ./build.sh test      # run frontend + Rust tests
#
# Cross-compiling a Tauri app from Linux to Windows is not supported here;
# build on Windows (locally or via the GitHub Actions workflow).

set -e

case "${1:-build}" in
    deps)
        npm install
        ;;
    dev)
        npm run tauri dev
        ;;
    build)
        npm install
        npm run tauri build
        echo "Artifacts under src-tauri/target/release/ (exe) and"
        echo "src-tauri/target/release/bundle/nsis/ (installer)."
        ;;
    test)
        npm test
        ( cd src-tauri && cargo test )
        ;;
    *)
        echo "Usage: $0 [deps|dev|build|test]"
        exit 1
        ;;
esac
