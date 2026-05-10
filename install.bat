@echo off
REM Install Loka CLI to user's local bin directory.
REM Requires Rust toolchain (cargo) to be installed.

setlocal

echo Building Loka (release)...
cargo build --release -p loka-cli
if errorlevel 1 (
    echo Build failed!
    exit /b 1
)

REM Create install directory if needed
set INSTALL_DIR=%USERPROFILE%\.loka\bin
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

echo Installing to %INSTALL_DIR%\loka.exe ...
copy /y target\release\loka.exe "%INSTALL_DIR%\"
if errorlevel 1 (
    echo Install failed!
    exit /b 1
)

echo.
echo Done! Add %INSTALL_DIR% to your PATH if not already there:
echo   set PATH=%INSTALL_DIR%;%%PATH%%
echo.
echo Usage:
echo   loka serve                    Start the HTTP server
echo   loka query "SELECT ..."       Run a SPARQL query
echo   loka import data.nt           Import N-Triples file
echo   loka export -o dump.nt        Export all triples
echo   loka info                     Show database statistics
echo.
