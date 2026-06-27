@echo off
REM Test + dev launcher for Fast-copy (Tauri) on Windows.
REM Requires Node.js + npm and the Rust toolchain.

echo ============================================
echo  Fast-copy - Tauri dev launcher
echo ============================================
echo.

if not exist "node_modules" (
    echo Installing frontend dependencies...
    call npm install || goto :error
)

echo Running frontend unit tests...
call npm test || goto :error

echo Running Rust tests...
pushd src-tauri
call cargo test || (popd & goto :error)
popd

echo.
echo Tests passed. Launching the app in dev mode...
echo (Close the window to stop. Use Ctrl+C in this console to exit Vite.)
echo.
call npm run tauri dev
goto :eof

:error
echo.
echo Build/test failed. See output above.
pause
exit /b 1
