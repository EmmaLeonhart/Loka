@echo off
title Loka World-Model Inference Sidecar
cd /d "%~dp0"
echo.
echo   Loka world-model inference sidecar  (port 8092)
echo.
echo   - First run downloads the pinned checkpoint (~180 MB) from
echo     Hugging Face (EmmaLeonhart/loka, public — no login needed).
echo   - Also start the Loka data endpoint: !playground.bat (Shinto
echo     demo) or !serve.bat.
echo   - Then double-click a node in the :8091 Knowledge Graph tab.
echo.
python tools\infer_server.py --port 8092
pause
