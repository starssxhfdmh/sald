# Sald Installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.ps1 | iex

$ErrorActionPreference = "Stop"
# Disable PowerShell's default progress bar
$ProgressPreference = 'SilentlyContinue'

$Repo = "starssxhfdmh/sald"
$InstallDir = "$env:USERPROFILE\.sald"
$BinDir = "$InstallDir\bin"
$TempDir = "$env:LOCALAPPDATA\Temp\sald-install-$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"

function Format-FileSize {
    param ([double]$Size)
    if ($Size -ge 1MB) {
        return "{0:N1} MB" -f ($Size / 1MB)
    } elseif ($Size -ge 1KB) {
        return "{0:N0} KB" -f ($Size / 1KB)
    } else {
        return "$([int]$Size) B"
    }
}

function Format-Speed {
    param ([double]$BytesPerSec)
    if ($BytesPerSec -ge 1MB) {
        return "{0:N1} MB/s" -f ($BytesPerSec / 1MB)
    } elseif ($BytesPerSec -ge 1KB) {
        return "{0:N0} KB/s" -f ($BytesPerSec / 1KB)
    } else {
        return "$([int]$BytesPerSec) B/s"
    }
}

function Format-ETA {
    param ([int]$Seconds)
    if ($Seconds -lt 0 -or $Seconds -gt 3600) {
        return "--:--"
    } elseif ($Seconds -ge 60) {
        return "{0}m {1}s" -f [math]::Floor($Seconds / 60), ($Seconds % 60)
    } else {
        return "{0}s" -f $Seconds
    }
}

function Clear-Line {
    Write-Host "`r$(' ' * 100)`r" -NoNewline
}

function Download-WithProgress {
    param (
        [string]$Url,
        [string]$OutFile,
        [string]$Name,
        [int64]$ExpectedSize
    )
    
    $request = [System.Net.HttpWebRequest]::Create($Url)
    $request.Method = "GET"
    $request.AllowAutoRedirect = $true
    $request.UserAgent = "PowerShell"
    
    try {
        $response = $request.GetResponse()
        $totalBytes = $ExpectedSize
        if ($totalBytes -le 0) {
            $totalBytes = $response.ContentLength
        }
        
        $responseStream = $response.GetResponseStream()
        $fileStream = [System.IO.File]::Create($OutFile)
        
        $buffer = New-Object byte[] 8192
        $bytesRead = 0
        $totalRead = 0
        $startTime = Get-Date
        $lastUpdate = $startTime
        $lastBytes = 0
        $speed = 0
        
        while (($bytesRead = $responseStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $fileStream.Write($buffer, 0, $bytesRead)
            $totalRead += $bytesRead
            
            $now = Get-Date
            $elapsed = ($now - $lastUpdate).TotalSeconds
            
            # Update display every 100ms
            if ($elapsed -ge 0.1) {
                $speed = ($totalRead - $lastBytes) / $elapsed
                $lastBytes = $totalRead
                $lastUpdate = $now
                
                $percent = if ($totalBytes -gt 0) { [math]::Floor(($totalRead / $totalBytes) * 100) } else { 0 }
                $width = 30
                $filled = if ($totalBytes -gt 0) { [math]::Floor(($totalRead / $totalBytes) * $width) } else { 0 }
                $empty = $width - $filled
                $bar = "=" * $filled + " " * $empty
                
                $sizeStr = "$(Format-FileSize $totalRead)/$(Format-FileSize $totalBytes)"
                $speedStr = Format-Speed $speed
                $remaining = if ($speed -gt 0) { [int](($totalBytes - $totalRead) / $speed) } else { -1 }
                $etaStr = Format-ETA $remaining
                
                Write-Host "`r  [$bar] " -NoNewline
                Write-Host ("{0,3}%" -f $percent) -NoNewline
                Write-Host " $Name " -NoNewline -ForegroundColor Cyan
                Write-Host "($sizeStr @ $speedStr, ETA $etaStr)    " -NoNewline -ForegroundColor Gray
            }
        }
        
        $fileStream.Close()
        $responseStream.Close()
        $response.Close()
        
        # Final update
        $percent = 100
        $bar = "=" * 30
        $sizeStr = Format-FileSize $totalBytes
        $totalTime = ((Get-Date) - $startTime).TotalSeconds
        $avgSpeed = if ($totalTime -gt 0) { $totalBytes / $totalTime } else { 0 }
        
        Write-Host "`r  [$bar] 100% " -NoNewline
        Write-Host "$Name " -NoNewline -ForegroundColor Cyan
        Write-Host "($sizeStr @ $(Format-Speed $avgSpeed))        " -NoNewline -ForegroundColor Gray
        
        return $true
    }
    catch {
        Write-Host "`r  Failed to download $Name : $_" -ForegroundColor Red
        return $false
    }
}

function Main {
    Write-Host ""
    Write-Host "sald" -ForegroundColor Green -NoNewline
    Write-Host " installer"
    Write-Host ""
    Write-Host "  Platform: windows-x86_64" -ForegroundColor Gray

    # Get latest version and asset info
    Write-Host "  Fetching latest version..." -NoNewline -ForegroundColor Gray
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $Release.tag_name
    
    if (-not $Version) {
        Write-Host ""
        Write-Host "  Failed to get latest version" -ForegroundColor Red
        exit 1
    }
    
    # Get file sizes from assets
    $Assets = @{}
    foreach ($Asset in $Release.assets) {
        $Assets[$Asset.name] = @{
            Size = $Asset.size
            Url = $Asset.browser_download_url
        }
    }
    
    Clear-Line
    Write-Host "  Version: $Version" -ForegroundColor Gray
    Write-Host ""

    # Create temp directory
    New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

    try {
        Write-Host "  Downloading..." -ForegroundColor Gray
        
        # Download sald
        $saldAsset = $Assets["sald-windows-x86_64.exe"]
        $success = Download-WithProgress -Url $saldAsset.Url -OutFile "$TempDir\sald.exe" -Name "sald" -ExpectedSize $saldAsset.Size
        if (-not $success) { exit 1 }
        Write-Host ""
        
        # Download sald-lsp
        $lspAsset = $Assets["sald-lsp-windows-x86_64.exe"]
        $success = Download-WithProgress -Url $lspAsset.Url -OutFile "$TempDir\sald-lsp.exe" -Name "sald-lsp" -ExpectedSize $lspAsset.Size
        if (-not $success) { exit 1 }
        Write-Host ""
        
        # Download salad
        $saladAsset = $Assets["salad-windows-x86_64.exe"]
        $success = Download-WithProgress -Url $saladAsset.Url -OutFile "$TempDir\salad.exe" -Name "salad" -ExpectedSize $saladAsset.Size
        if (-not $success) { exit 1 }
        Write-Host ""
        
        $totalSize = $saldAsset.Size + $lspAsset.Size + $saladAsset.Size
        Write-Host "  " -NoNewline
        Write-Host "Downloaded" -ForegroundColor Green -NoNewline
        Write-Host " 3 binaries ($(Format-FileSize $totalSize))" -ForegroundColor Gray

        # Create install directory
        New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

        # Move binaries
        Write-Host "  Installing..." -NoNewline -ForegroundColor Gray
        Move-Item -Path "$TempDir\sald.exe" -Destination "$BinDir\sald.exe" -Force
        Move-Item -Path "$TempDir\sald-lsp.exe" -Destination "$BinDir\sald-lsp.exe" -Force
        Move-Item -Path "$TempDir\salad.exe" -Destination "$BinDir\salad.exe" -Force
        Clear-Line
        Write-Host "  " -NoNewline
        Write-Host "Installed" -ForegroundColor Green -NoNewline
        Write-Host " to $BinDir" -ForegroundColor Gray
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
        Write-Host " PATH" -ForegroundColor Gray
    }

    # Success message
    Write-Host ""
    Write-Host "Done" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Restart your terminal to use sald" -ForegroundColor Gray
    Write-Host ""
}

Main
