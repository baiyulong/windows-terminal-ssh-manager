@echo off
setlocal

echo.
echo  =========================================================
echo   wt-ssh-manager (Rust) — Windows Terminal SSH Manager
echo  =========================================================
echo.

set "INSTALL_DIR=%~dp0"
if "%INSTALL_DIR:~-1%"=="\" set "INSTALL_DIR=%INSTALL_DIR:~0,-1%"

REM ── Build release binary ───────────────────────────────────────────────────
echo [1/3] Building Rust release binary (first build may take ~2 minutes)...
cargo build --release --manifest-path "%INSTALL_DIR%\Cargo.toml"
if errorlevel 1 (
    echo ERROR: cargo build failed. Make sure Rust ^(rustup^) is installed.
    echo        Install Rust: https://rustup.rs
    pause & exit /b 1
)
echo       Done.

REM ── Copy binary to WindowsApps (on PATH) ──────────────────────────────────
set "DEST=%LOCALAPPDATA%\Microsoft\WindowsApps\ssh-manager.exe"
echo [2/3] Installing ssh-manager.exe to PATH...
copy /Y "%INSTALL_DIR%\target\release\ssh-manager.exe" "%DEST%" >nul
if errorlevel 1 (
    echo WARNING: Could not copy to %DEST%.
    echo          Run manually: ssh-manager --help
) else (
    echo       Installed: %DEST%
)

REM ── Generate initial Windows Terminal Fragment ─────────────────────────────
echo [3/3] Generating Windows Terminal Fragment profiles...
"%DEST%" sync
if errorlevel 1 (
    echo WARNING: Fragment generation failed. Run  ssh-manager sync  manually.
)

echo.
echo  ✅  Installation complete! (standalone Rust binary, no Python needed)
echo.
echo  Usage:
echo    ssh-manager add          — add a server
echo    ssh-manager list         — list all servers
echo    ssh-manager connect      — interactive picker
echo    ssh-manager --help       — full help
echo.
pause
endlocal
