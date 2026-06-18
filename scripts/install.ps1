# termchat installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/LHagfoss/termchat/main/scripts/install.ps1 | iex

param(
    [string]$InstallDir = "$env:USERPROFILE\.local\bin"
)

$ErrorActionPreference = "Stop"
$Repo = "LHagfoss/termchat"
$BinaryName = "termchat"
$Arch = if ($Env:PROCESSOR_ARCHITECTURE -eq "AMD64") { "x86_64" } else { $null }

if (-not $Arch) {
    Write-Host "Unsupported architecture: $Env:PROCESSOR_ARCHITECTURE (only x86_64/AMD64 supported)" -ForegroundColor Red
    exit 1
}

Write-Host "Detecting system: Windows / $Arch" -ForegroundColor Cyan

# Fetch latest release from GitHub API
$apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
try {
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{"Accept"="application/vnd.github.v3+json"}
} catch {
    Write-Host "Failed to fetch release info: $_" -ForegroundColor Red
    exit 1
}

$assetName = "termchat-${Arch}-windows.exe"
$asset = $release.assets | Where-Object { $_.name -eq $assetName }

if (-not $asset) {
    Write-Host "No release asset found: $assetName" -ForegroundColor Red
    Write-Host "Check https://github.com/$Repo/releases for available releases." -ForegroundColor Yellow
    exit 1
}

$downloadUrl = $asset.browser_download_url
$tmpFile = Join-Path $env:TEMP "$BinaryName_install.exe"

Write-Host "Downloading $downloadUrl ..." -ForegroundColor Cyan

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $tmpFile -UseBasicParsing
} catch {
    Write-Host "Download failed: $_" -ForegroundColor Red
    exit 1
}

# Create install directory if needed
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}

$dest = Join-Path $InstallDir "$BinaryName.exe"
Move-Item -Path $tmpFile -Destination $dest -Force

Write-Host "[✓] Installed $BinaryName.exe to $dest" -ForegroundColor Green
Write-Host "Run '$BinaryName --help' to get started!" -ForegroundColor Green

# Check if install dir is in PATH
$envPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if (-not ($envPath -split ';' | Where-Object { $_ -eq $InstallDir })) {
    Write-Host "[!] $InstallDir is not in your PATH." -ForegroundColor Yellow
    Write-Host "Add it with:" -ForegroundColor Yellow
    Write-Host "  [Environment]::SetEnvironmentVariable('PATH', `"$envPath;$InstallDir``, 'User')" -ForegroundColor Yellow
}
