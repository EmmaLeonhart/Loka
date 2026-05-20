@echo off
title Loka retrieval data (vectorised Wikidata, port 3031)
cd /d "%~dp0"
echo.
echo   Loka retrieval graph  (port 3031)
echo.
echo   Vectorised real-Wikidata Loka — 22k triples + 3 vector indexes
echo   (nodeEmb / nameEmb / tripleEmb, 384-d cosine).
echo   The :8092 inference sidecar (!infer.bat) drives BFS+embedding
echo   retrieval against this endpoint; browse.html's double-click
echo   pipes through both.
echo.
echo   Open  http://localhost:3031/browse  to test the KG expand demo
echo   (the endpoint + infer fields auto-fill).
echo.
"%~dp0target\release\loka.exe" serve --data-dir loka-retrieval-data --port 3031
pause
