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

echo Starting EU Toolkit (UI debug mode: vite on 1430, CDP on 9222, hot reload ON)...
rem WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS exposes the WebView2 (Chromium) DevTools
rem protocol so UI tooling can attach to the running app (port 9222) to snapshot,
rem click, and screenshot the real UI; scripts/ui-screenshot.mjs is the
rem zero-dependency fallback.
rem
rem EU_TOOLKIT_UI_PORT=1430 (+ the tauri.uidebug.conf.json devUrl override) puts
rem this instance's dev server on its own port so it never collides with - or
rem silently loads - another Tauri project's vite already sitting on 1420.
rem
rem Unlike run.bat this runs in dev mode (watcher + HMR on) so frontend edits
rem apply live while iterating on the design; the session survives reloads via
rem sessionStorage. Don't use this for a long-lived note-taking instance -
rem that's run.bat's job.
set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
set EU_TOOLKIT_UI_PORT=1430
call npm run tauri -- dev --config src-tauri/tauri.uidebug.conf.json
