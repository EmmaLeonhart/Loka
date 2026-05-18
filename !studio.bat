@echo off
REM Loka Studio (JS/HTML build) in an Electron window.
REM Needs a Loka endpoint running (default http://localhost:3030) —
REM e.g. run !serve.bat or !playground.bat first.
cd /d "%~dp0loka-studio\electron"
echo Launching Loka Studio (JS) in Electron...
npm run studio:js
pause
