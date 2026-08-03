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

echo Building EU Toolkit (release)...
rem --no-bundle: produce just the portable exe, skipping the MSI/NSIS installer
rem bundles (run "npm run tauri -- build" without it to get installers too).
call npm run tauri -- build --no-bundle
if errorlevel 1 (
    echo Build failed.
    pause
    exit /b 1
)

if not exist dist mkdir dist
copy /y "src-tauri\target\release\eu-toolkit.exe" "dist\eu-toolkit.exe" >nul
echo.
echo Done: %~dp0dist\eu-toolkit.exe
endlocal
