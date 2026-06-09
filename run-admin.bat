@echo off
:: AnyN2N - 以管理员身份启动
title AnyN2N Launcher
echo Starting AnyN2N with admin privileges...
echo.

set "PROJECT_DIR=%~dp0"
set "BUN_PATH=%USERPROFILE%\.bun\bin"
set "CARGO_PATH=%USERPROFILE%\.cargo\bin"
set "NODE_PATH=D:\dev\nvm-nodejs\nodejs"

set "PATH=%BUN_PATH%;%CARGO_PATH%;%NODE_PATH%;%PATH%"

cd /d "%PROJECT_DIR%"

:: 检查是否已管理员
net session >nul 2>&1
if %errorlevel% equ 0 (
    echo [OK] Running as administrator
    goto :run
)

echo Requesting administrator privileges...
powershell -Command "Start-Process -FilePath '%0' -Verb runas"
exit /b

:run
echo Starting Tauri dev server...
bun run tauri dev
pause
