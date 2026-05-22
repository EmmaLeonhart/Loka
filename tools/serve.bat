@echo off
REM Start the Loka engine on http://localhost:3030 (plain data endpoint).
REM Normally you don't need this directly: !studio.bat (in the repo root)
REM starts the engine for you. Kept here for headless / scripted use.
title Loka engine :3030
cd /d "%~dp0.."
echo Starting Loka on port 3030...  (press Ctrl+C to stop)
target\release\loka.exe serve --port 3030
pause
