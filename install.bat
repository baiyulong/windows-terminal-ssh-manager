@echo off
setlocal

echo.
echo  ==========================================================
echo   wt-ssh-manager — Windows Terminal SSH Manager  (install)
echo  ==========================================================
echo.

REM ── Locate this script's directory ────────────────────────────────────────
set "INSTALL_DIR=%~dp0"
REM Strip trailing backslash
if "%INSTALL_DIR:~-1%"=="\" set "INSTALL_DIR=%INSTALL_DIR:~0,-1%"

REM ── Install Python dependencies ────────────────────────────────────────────
echo [1/3] Installing Python dependencies...
pip install -r "%INSTALL_DIR%\requirements.txt" --quiet
if errorlevel 1 (
    echo ERROR: pip install failed. Make sure Python 3.8+ and pip are available.
    pause & exit /b 1
)
echo       Done.

REM ── Write the ssh-manager.bat wrapper to WindowsApps (on PATH by default) ─
echo [2/3] Installing ssh-manager command to PATH...
powershell -NoProfile -ExecutionPolicy Bypass -File "%INSTALL_DIR%\install-wrapper.ps1" "%INSTALL_DIR%"
if errorlevel 1 (
    echo WARNING: Could not install wrapper. You can still run:
    echo          python "%INSTALL_DIR%\main.py" ^<command^>
)

REM ── Generate initial (empty) Fragment so WT picks up the extension ─────────
echo [3/3] Generating initial Windows Terminal Fragment...
python "%INSTALL_DIR%\main.py" sync
if errorlevel 1 (
    echo WARNING: Fragment generation failed. Run  ssh-manager sync  manually.
)

echo.
echo  ✅  Installation complete!
echo.
echo  Usage:
echo    ssh-manager add          — add a server
echo    ssh-manager list         — list all servers
echo    ssh-manager connect ^<n^>  — connect (or click the profile in Windows Terminal)
echo    ssh-manager --help       — full help
echo.
pause
endlocal
