@echo off
REM Standalone playground server (the small Shinto demo). Superseded for most
REM purposes by !studio.bat + "Load test data", but kept for the canned demo.
title Loka Playground (Shinto demo)
cd /d "%~dp0.."
echo Building playground_server...
cargo build --release --example playground_server -p loka-proto 2>nul
if errorlevel 1 (
    echo Release build failed; trying debug...
    cargo build --example playground_server -p loka-proto
    if errorlevel 1 ( echo Build failed. Run cargo build to see errors. & pause & exit /b 1 )
    target\debug\examples\playground_server.exe
) else (
    target\release\examples\playground_server.exe
)
pause
