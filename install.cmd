@echo off
REM Sald Installer for Windows (CMD)
REM Usage: powershell -c "irm https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.ps1 | iex"
REM Note: For the best experience, use the PowerShell installer (install.ps1)

setlocal EnableDelayedExpansion

set "REPO=starssxhfdmh/sald"
set "INSTALL_DIR=%USERPROFILE%\.sald"
set "BIN_DIR=%INSTALL_DIR%\bin"
set "TEMP_DIR=%LOCALAPPDATA%\Temp\sald-install-%RANDOM%"

REM Create temp directory
mkdir "%TEMP_DIR%" 2>nul

echo.
echo [32msald[0m installer
echo.
echo   [90mPlatform: windows-x86_64[0m

REM Get latest version using PowerShell
echo   [90mFetching latest version...[0m
for /f "delims=" %%v in ('powershell -NoProfile -Command "(Invoke-RestMethod -Uri 'https://api.github.com/repos/%REPO%/releases/latest').tag_name"') do set "VERSION=%%v"

if "%VERSION%"=="" (
    echo   [31mFailed to get latest version[0m
    exit /b 1
)

echo   [90mVersion: %VERSION%[0m
echo.

set "BASE_URL=https://github.com/%REPO%/releases/download/%VERSION%"

echo   [90mDownloading...[0m

REM Download binaries with progress indicator
echo   [90m[[0m          [90m]   0%%[0m [36msald[0m
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%BASE_URL%/sald-windows-x86_64.exe' -OutFile '%TEMP_DIR%\sald.exe'" 2>nul
if errorlevel 1 (
    echo   [31mFailed to download sald[0m
    exit /b 1
)

echo   [90m[[0m==========          [90m]  33%%[0m [36msald-lsp[0m
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%BASE_URL%/sald-lsp-windows-x86_64.exe' -OutFile '%TEMP_DIR%\sald-lsp.exe'" 2>nul
if errorlevel 1 (
    echo   [31mFailed to download sald-lsp[0m
    exit /b 1
)

echo   [90m[[0m====================          [90m]  66%%[0m [36msalad[0m
powershell -NoProfile -Command "Invoke-WebRequest -Uri '%BASE_URL%/salad-windows-x86_64.exe' -OutFile '%TEMP_DIR%\salad.exe'" 2>nul
if errorlevel 1 (
    echo   [31mFailed to download salad[0m
    exit /b 1
)

echo   [32mDownloaded[0m [90m3 binaries[0m

REM Create install directory
mkdir "%BIN_DIR%" 2>nul

REM Move binaries
echo   [90mInstalling...[0m
move /Y "%TEMP_DIR%\sald.exe" "%BIN_DIR%\sald.exe" >nul
move /Y "%TEMP_DIR%\sald-lsp.exe" "%BIN_DIR%\sald-lsp.exe" >nul
move /Y "%TEMP_DIR%\salad.exe" "%BIN_DIR%\salad.exe" >nul
echo   [32mInstalled[0m [90mto %BIN_DIR%[0m

REM Cleanup temp
rmdir /S /Q "%TEMP_DIR%" 2>nul

REM Add to PATH (user environment)
for /f "tokens=2*" %%a in ('reg query "HKCU\Environment" /v Path 2^>nul') do set "CURRENT_PATH=%%b"

echo %CURRENT_PATH% | find /i ".sald\bin" >nul
if errorlevel 1 (
    if "%CURRENT_PATH%"=="" (
        set "NEW_PATH=%BIN_DIR%"
    ) else (
        set "NEW_PATH=%CURRENT_PATH%;%BIN_DIR%"
    )
    reg add "HKCU\Environment" /v Path /t REG_EXPAND_SZ /d "!NEW_PATH!" /f >nul 2>&1
    powershell -NoProfile -Command "[Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';%BIN_DIR%', 'User')" 2>nul
    echo   [32mUpdated[0m [90mPATH[0m
)

echo.
echo [32mDone[0m
echo.
echo   [90mRestart your terminal to use sald[0m
echo.

endlocal
