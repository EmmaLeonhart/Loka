@echo off
title Loka Playground
echo.
echo   Loka Playground
echo   Building...
echo.

cd /d "%~dp0"

:: Build release for speed (first run compiles, subsequent runs are instant)
cargo build --release --example playground_server -p loka-proto 2>nul
if errorlevel 1 (
    echo   Build failed. Trying debug build...
    cargo build --example playground_server -p loka-proto
    if errorlevel 1 (
        echo   Build failed. Run 'cargo build' manually to see errors.
        pause
        exit /b 1
    )
    set "EXE=%~dp0target\debug\examples\playground_server.exe"
) else (
    set "EXE=%~dp0target\release\examples\playground_server.exe"
)

echo.
"%EXE%"
pause
