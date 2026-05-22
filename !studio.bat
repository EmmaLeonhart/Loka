@echo off
REM ==========================================================================
REM  Loka Studio — one-click UI opener.
REM
REM  This is the single launcher for the desktop app. It is self-sufficient:
REM    1. builds the Loka engine if it isn't built yet,
REM    2. starts the engine on http://localhost:3030 if nothing answers there,
REM    3. opens the Electron Studio window (Knowledge Graph + SPARQL + more).
REM
REM  Once it's open, click "Load test data" in the top bar to drop a small
REM  demo graph in so you have something to explore. No other .bat needed.
REM  (The advanced generative double-click stack lives in tools\: tools\infer.bat
REM   + tools\retrieval.bat.)
REM ==========================================================================
title Loka Studio
cd /d "%~dp0"

echo.
echo   Loka Studio  -  one-click launcher
echo.

REM 1. Ensure the engine binary exists (compiles once; instant afterwards).
if not exist "target\release\loka.exe" (
    echo   Building the Loka engine ^(first run only^)...
    cargo build --release -p loka-cli
    if errorlevel 1 (
        echo   Build failed. Run "cargo build --release -p loka-cli" to see errors.
        pause
        exit /b 1
    )
)

REM 2. Start the backend on :3030 unless something already answers there.
powershell -NoProfile -Command "try{(New-Object Net.Sockets.TcpClient).Connect('localhost',3030);exit 0}catch{exit 1}" >nul 2>&1
if errorlevel 1 (
    echo   Starting the Loka engine on http://localhost:3030 ...
    start "Loka engine :3030" /min "%~dp0target\release\loka.exe" serve --port 3030
    powershell -NoProfile -Command "Start-Sleep -Milliseconds 1500" >nul 2>&1
) else (
    echo   Loka engine already running on :3030 - reusing it.
)

REM 3. Launch the Electron Studio UI (it talks to :3030 by default).
cd /d "%~dp0loka-studio\electron"
if not exist "node_modules" (
    echo   Installing Studio dependencies ^(first run only^)...
    call npm install
)
echo.
echo   Opening Loka Studio... Tip: click "Load test data" in the top bar.
echo.
call npm run studio:js
