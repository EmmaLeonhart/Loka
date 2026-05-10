@echo off
echo Starting Loka on port 3030...
echo Press Ctrl+C to stop.
"%~dp0target\release\loka.exe" serve --port 3030
pause
