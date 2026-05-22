@echo off
REM ==========================================================================
REM  Loka Studio — one-click UI opener.
REM
REM  This is the single launcher for the desktop app. It is self-sufficient:
REM    1. builds the Loka engine if it isn't built yet,
REM    2. starts the engine on http://localhost:3030 if nothing answers there,
REM    3. starts the world-model inference sidecar on :8092 (powers the
REM       generative double-click), and
REM    4. opens the Electron Studio window (Knowledge Graph + SPARQL + more).
REM
REM  Once it's open, click "Load test data" in the top bar to drop a small
REM  slice of the normalized corpus in, then double-click a node to see the
REM  world model expand it. No other .bat needed. (The sidecar's first run
REM  downloads the base model from Hugging Face; that can take a few minutes.)
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

REM 3. Start the world-model inference sidecar on :8092 unless it's already up.
REM    Runs in its own window (cmd /k keeps it open so you can see logs / errors);
REM    first run downloads the base model from Hugging Face, a few minutes.
powershell -NoProfile -Command "try{(New-Object Net.Sockets.TcpClient).Connect('localhost',8092);exit 0}catch{exit 1}" >nul 2>&1
if errorlevel 1 (
    echo   Starting the world-model inference sidecar on http://localhost:8092 ...
    start "Loka inference sidecar :8092" cmd /k python tools\infer_server.py --port 8092
) else (
    echo   Inference sidecar already running on :8092 - reusing it.
)

REM 4. Launch the Electron Studio UI (it talks to :3030 by default).
cd /d "%~dp0loka-studio\electron"
if not exist "node_modules" (
    echo   Installing Studio dependencies ^(first run only^)...
    call npm install
)
echo.
echo   Opening Loka Studio... Tip: click "Load test data" in the top bar.
echo.
call npm run studio:js
