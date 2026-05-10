@echo off
echo Opening Loka Browser...
echo Make sure Loka is running: loka serve --port 3030
start "" "%~dp0tools\browse.html"
