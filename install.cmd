@echo off
REM Sald Installer for Windows
REM Usage: curl -fsSL https://raw.githubusercontent.com/starssxhfdmh/sald/main/install.cmd -o install.cmd && install.cmd
REM Or: powershell -c "irm https://raw.githubusercontent.com/starssxhfdmh/sald/main/install.cmd -OutFile install.cmd; .\install.cmd"

setlocal EnableDelayedExpansion

set "REPO=starssxhfdmh/sald"
set "INSTALL_DIR=%USERPROFILE%\.sald"
set "BIN_DIR=%INSTALL_DIR%\bin"
set "TEMP_DIR=%LOCALAPPDATA%\Temp\sald-install-%RANDOM%"

REM Create temp directory
mkdir "%TEMP_DIR%" 2>nul

REM Get latest version using PowerShell
for /f "delims=" %%v in ('powershell -NoProfile -Command "(Invoke-RestMethod -Uri 'https://api.github.com/repos/%REPO%/releases/latest').tag_name"') do set "VERSION=%%v"

if "%VERSION%"=="" (
    echo Failed to get latest version
    exit /b 1
)

set "BASE_URL=https://github.com/%REPO%/releases/download/%VERSION%"

REM Download binaries using PowerShell
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%BASE_URL%/sald-windows-x86_64.exe' -OutFile '%TEMP_DIR%\sald.exe'" 2>nul
if errorlevel 1 (
    echo Failed to download sald
    exit /b 1
)

powershell -NoProfile -Command "Invoke-WebRequest -Uri '%BASE_URL%/sald-lsp-windows-x86_64.exe' -OutFile '%TEMP_DIR%\sald-lsp.exe'" 2>nul
if errorlevel 1 (
    echo Failed to download sald-lsp
    exit /b 1
)

powershell -NoProfile -Command "Invoke-WebRequest -Uri '%BASE_URL%/salad-windows-x86_64.exe' -OutFile '%TEMP_DIR%\salad.exe'" 2>nul
if errorlevel 1 (
    echo Failed to download salad
    exit /b 1
)

REM Create install directory
mkdir "%BIN_DIR%" 2>nul

REM Move binaries
move /Y "%TEMP_DIR%\sald.exe" "%BIN_DIR%\sald.exe" >nul
move /Y "%TEMP_DIR%\sald-lsp.exe" "%BIN_DIR%\sald-lsp.exe" >nul
move /Y "%TEMP_DIR%\salad.exe" "%BIN_DIR%\salad.exe" >nul

REM Cleanup temp
rmdir /S /Q "%TEMP_DIR%" 2>nul

REM Add to PATH (user environment)
for /f "tokens=2*" %%a in ('reg query "HKCU\Environment" /v Path 2^>nul') do set "CURRENT_PATH=%%b"

echo %CURRENT_PATH% | find /i ".sald\bin" >nul
if errorlevel 1 (
    REM Not in PATH yet, add it
    if "%CURRENT_PATH%"=="" (
        set "NEW_PATH=%BIN_DIR%"
    ) else (
        set "NEW_PATH=%CURRENT_PATH%;%BIN_DIR%"
    )
    reg add "HKCU\Environment" /v Path /t REG_EXPAND_SZ /d "!NEW_PATH!" /f >nul 2>&1
    
    REM Notify system of environment change
    powershell -NoProfile -Command "[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';%BIN_DIR%', 'User')" 2>nul
)

REM Success message with colors using PowerShell
powershell -NoProfile -Command ^
    "Write-Host ''; ^
     Write-Host 'sald' -ForegroundColor Green; ^
     Write-Host ''; ^
     Write-Host '  Installed ' -NoNewline; ^
     Write-Host 'sald' -ForegroundColor Cyan -NoNewline; ^
     Write-Host ', ' -NoNewline; ^
     Write-Host 'sald-lsp' -ForegroundColor Cyan -NoNewline; ^
     Write-Host ', ' -NoNewline; ^
     Write-Host 'salad' -ForegroundColor Cyan -NoNewline; ^
     Write-Host ' %VERSION%' -ForegroundColor DarkGray; ^
     Write-Host '  Location: %BIN_DIR%' -ForegroundColor DarkGray; ^
     Write-Host ''; ^
     Write-Host '  Restart your terminal to use sald' -ForegroundColor DarkGray; ^
     Write-Host ''; ^
     Write-Host 'Done' -ForegroundColor Green; ^
     Write-Host ''"

endlocal
