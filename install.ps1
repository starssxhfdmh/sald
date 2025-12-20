# Sald Installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "starssxhfdmh/sald"
$InstallDir = "$env:USERPROFILE\.sald"
$BinDir = "$InstallDir\bin"
$TempDir = "$env:LOCALAPPDATA\Temp\sald-install-$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"

function Write-Progress-Bar {
    param (
        [int]$Current,
        [int]$Total,
        [string]$Name
    )
    
    $Width = 30
    $Percent = [math]::Floor(($Current / $Total) * 100)
    $Filled = [math]::Floor(($Current / $Total) * $Width)
    $Empty = $Width - $Filled
    
    $Bar = "=" * $Filled + " " * $Empty
    
    Write-Host "`r  [" -NoNewline -ForegroundColor DarkGray
    Write-Host $Bar -NoNewline
    Write-Host "] " -NoNewline -ForegroundColor DarkGray
    Write-Host ("{0,3}%" -f $Percent) -NoNewline
    Write-Host " $Name" -NoNewline -ForegroundColor Cyan
}

function Clear-Line {
    Write-Host "`r$(' ' * 80)`r" -NoNewline
}

function Main {
    Write-Host ""
    Write-Host "sald" -ForegroundColor Green -NoNewline
    Write-Host " installer"
    Write-Host ""
    Write-Host "  Platform: windows-x86_64" -ForegroundColor DarkGray

    # Get latest version
    Write-Host "  Fetching latest version..." -ForegroundColor DarkGray
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $Release.tag_name
    
    if (-not $Version) {
        Write-Host "  Failed to get latest version" -ForegroundColor Red
        exit 1
    }
    
    Clear-Line
    Write-Host "  Version: $Version" -ForegroundColor DarkGray
    Write-Host ""

    # Create temp directory
    New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

    try {
        $BaseUrl = "https://github.com/$Repo/releases/download/$Version"
        
        Write-Host "  Downloading..." -ForegroundColor DarkGray
        
        # Download sald
        Write-Progress-Bar -Current 0 -Total 3 -Name "sald"
        Invoke-WebRequest -Uri "$BaseUrl/sald-windows-x86_64.exe" -OutFile "$TempDir\sald.exe" -UseBasicParsing
        Clear-Line
        
        # Download sald-lsp
        Write-Progress-Bar -Current 1 -Total 3 -Name "sald-lsp"
        Invoke-WebRequest -Uri "$BaseUrl/sald-lsp-windows-x86_64.exe" -OutFile "$TempDir\sald-lsp.exe" -UseBasicParsing
        Clear-Line
        
        # Download salad
        Write-Progress-Bar -Current 2 -Total 3 -Name "salad"
        Invoke-WebRequest -Uri "$BaseUrl/salad-windows-x86_64.exe" -OutFile "$TempDir\salad.exe" -UseBasicParsing
        Clear-Line
        
        Write-Host "  " -NoNewline
        Write-Host "Downloaded" -ForegroundColor Green -NoNewline
        Write-Host " 3 binaries" -ForegroundColor DarkGray

        # Create install directory
        New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

        # Move binaries
        Write-Host "  Installing..." -ForegroundColor DarkGray
        Move-Item -Path "$TempDir\sald.exe" -Destination "$BinDir\sald.exe" -Force
        Move-Item -Path "$TempDir\sald-lsp.exe" -Destination "$BinDir\sald-lsp.exe" -Force
        Move-Item -Path "$TempDir\salad.exe" -Destination "$BinDir\salad.exe" -Force
        Clear-Line
        Write-Host "  " -NoNewline
        Write-Host "Installed" -ForegroundColor Green -NoNewline
        Write-Host " to $BinDir" -ForegroundColor DarkGray
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
        Write-Host "  " -NoNewline
        Write-Host "Updated" -ForegroundColor Green -NoNewline
        Write-Host " PATH" -ForegroundColor DarkGray
    }

    # Success message
    Write-Host ""
    Write-Host "Done" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Restart your terminal to use sald" -ForegroundColor DarkGray
    Write-Host ""
}

Main
