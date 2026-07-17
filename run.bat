@echo off
setlocal
cd /d "%~dp0"

if not exist node_modules (
    echo Installing dependencies...
    call npm install
    if errorlevel 1 (
        echo npm install failed.
        pause
        exit /b 1
    )
)

echo Starting EU Toolkit (stable mode: no auto-reload; rerun to pick up changes)...
rem EU_TOOLKIT_NO_WATCH freezes the frontend dev server (no HMR, no file watching)
rem and --no-watch stops the Rust rebuild/restart watcher, so source edits made
rem while the app is open never break the running instance.
set EU_TOOLKIT_NO_WATCH=1
call npm run tauri -- dev --no-watch
