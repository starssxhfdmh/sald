# Sald Installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/starssxhfdmh/sald/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "starssxhfdmh/sald"
$InstallDir = "$env:USERPROFILE\.sald"
$BinDir = "$InstallDir\bin"
$TempDir = "$env:LOCALAPPDATA\Temp\sald-install-$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"

function Main {
    # Get latest version
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $Release.tag_name
    
    if (-not $Version) {
        Write-Host "Failed to get latest version" -ForegroundColor Red
        exit 1
    }

    # Create temp directory
    New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

    try {
        $BaseUrl = "https://github.com/$Repo/releases/download/$Version"
        
        # Download binaries
        Invoke-WebRequest -Uri "$BaseUrl/sald-windows-x86_64.exe" -OutFile "$TempDir\sald.exe" -UseBasicParsing
        Invoke-WebRequest -Uri "$BaseUrl/sald-lsp-windows-x86_64.exe" -OutFile "$TempDir\sald-lsp.exe" -UseBasicParsing
        Invoke-WebRequest -Uri "$BaseUrl/salad-windows-x86_64.exe" -OutFile "$TempDir\salad.exe" -UseBasicParsing

        # Create install directory
        New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

        # Move binaries
        Move-Item -Path "$TempDir\sald.exe" -Destination "$BinDir\sald.exe" -Force
        Move-Item -Path "$TempDir\sald-lsp.exe" -Destination "$BinDir\sald-lsp.exe" -Force
        Move-Item -Path "$TempDir\salad.exe" -Destination "$BinDir\salad.exe" -Force
    }
    finally {
        # Cleanup temp
        Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    # Add to PATH
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($CurrentPath -notlike "*$BinDir*") {
        $NewPath = if ($CurrentPath) { "$CurrentPath;$BinDir" } else { $BinDir }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:Path = "$env:Path;$BinDir"
    }

    # Success message
    Write-Host ""
    Write-Host "sald" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Installed " -NoNewline
    Write-Host "sald" -ForegroundColor Cyan -NoNewline
    Write-Host ", " -NoNewline
    Write-Host "sald-lsp" -ForegroundColor Cyan -NoNewline
    Write-Host ", " -NoNewline
    Write-Host "salad" -ForegroundColor Cyan -NoNewline
    Write-Host " $Version" -ForegroundColor DarkGray
    Write-Host "  Location: $BinDir" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  Restart your terminal to use sald" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "Done" -ForegroundColor Green
    Write-Host ""
}

Main
