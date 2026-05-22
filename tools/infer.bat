@echo off
REM Loka world-model inference sidecar (port 8092) — powers the generative
REM double-click "expand a node into model-predicted triples" feature.
REM First run downloads the pinned checkpoint (~180 MB) from Hugging Face
REM (EmmaLeonhart/loka, public — no login needed).
REM
REM Start a data endpoint first: !studio.bat (then click "Load test data"),
REM or tools\retrieval.bat for the full vectorised graph.
title Loka world-model inference sidecar :8092
cd /d "%~dp0.."
echo Starting the Loka inference sidecar on port 8092...
python tools\infer_server.py --port 8092
pause
