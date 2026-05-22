@echo off
REM Start the vectorised retrieval graph on http://localhost:3031.
REM This is the data endpoint the :8092 inference sidecar (tools\infer.bat)
REM drives for the generative double-click demo. Open
REM http://localhost:3031/browse to test the KG-expand demo.
title Loka retrieval graph :3031
cd /d "%~dp0.."
echo Loka retrieval graph on port 3031 (vectorised Wikidata).
echo Open http://localhost:3031/browse to test the KG expand demo.
target\release\loka.exe serve --data-dir loka-retrieval-data --port 3031
pause
