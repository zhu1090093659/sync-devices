# install.ps1 - Install sync-devices from GitHub Releases
# Usage: irm https://raw.githubusercontent.com/zhu1090093659/sync-devices/master/install.ps1 | iex

$ErrorActionPreference = "Stop"

$repo = "zhu1090093659/sync-devices"
$binaryName = "sync-devices.exe"
$installDir = if ($env:SYNC_DEVICES_INSTALL_DIR) { $env:SYNC_DEVICES_INSTALL_DIR } else { Join-Path $env:USERPROFILE ".sync-devices\bin" }
$version = if ($env:SYNC_DEVICES_VERSION) { $env:SYNC_DEVICES_VERSION } else { "latest" }

function Write-Info($msg)  { Write-Host "info: " -ForegroundColor Green -NoNewline; Write-Host $msg }
function Write-Warn($msg)  { Write-Host "warn: " -ForegroundColor Yellow -NoNewline; Write-Host $msg }
function Write-Err($msg)   { Write-Host "error: " -ForegroundColor Red -NoNewline; Write-Host $msg; exit 1 }

# Check architecture
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64") {
    Write-Err "Unsupported architecture: $arch. Only AMD64 (x86_64) is supported."
}

$tmpFile = $null

try {
    Write-Host ""
    Write-Host "sync-devices installer" -ForegroundColor White -BackgroundColor DarkCyan
    Write-Host ""
    Write-Info "Detected platform: windows/x86_64"

    # Resolve version
    if ($version -eq "latest") {
        Write-Info "Fetching latest release version..."
        try {
            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ "User-Agent" = "sync-devices-installer" }
            $version = $release.tag_name
        } catch {
            Write-Err "Failed to fetch latest version. Set `$env:SYNC_DEVICES_VERSION to install a specific version."
        }
    }

    if (-not $version) {
        Write-Err "Could not determine version to install."
    }

    Write-Info "Installing version: $version"

    # Download
    $artifact = "sync-devices-windows-x86_64.exe"
    $url = "https://github.com/$repo/releases/download/$version/$artifact"
    Write-Info "Downloading $url"

    $tmpFile = Join-Path ([System.IO.Path]::GetTempPath()) "sync-devices-$([System.Guid]::NewGuid()).exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmpFile -UseBasicParsing
    } catch {
        Write-Err "Download failed. Check that version $version exists and has a Windows binary."
    }

    if (-not (Test-Path $tmpFile) -or (Get-Item $tmpFile).Length -eq 0) {
        Write-Err "Downloaded file is empty or missing."
    }

    # Install
    if (-not (Test-Path $installDir)) {
        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    }

    $dest = Join-Path $installDir $binaryName
    Move-Item -Path $tmpFile -Destination $dest -Force
    $tmpFile = $null
    Write-Info "Installed to $dest"

    # Verify
    try {
        $verOutput = & $dest --version 2>&1
        Write-Info "Verified: $verOutput"
    } catch {
        Write-Warn "Binary installed but verification failed. You may need to check compatibility."
    }

    # PATH management
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathEntries = $userPath -split ";"
    if ($pathEntries -notcontains $installDir) {
        Write-Warn "$installDir is not in your PATH."
        $newPath = "$userPath;$installDir"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Info "Added $installDir to your User PATH."
        Write-Warn "Restart your terminal for the PATH change to take effect."
    }

    Write-Host ""
    Write-Host "sync-devices has been installed successfully!" -ForegroundColor Green
    Write-Host ""

} finally {
    if ($tmpFile -and (Test-Path $tmpFile)) {
        Remove-Item -Path $tmpFile -Force -ErrorAction SilentlyContinue
    }
}
