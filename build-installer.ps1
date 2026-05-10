# KitsuneEngine - Release Build + Windows Installer
# Run from the repository root:
#   .\build-installer.ps1
#
# Output: target\wix\kitsune-engine-<version>-x86_64.msi

$ErrorActionPreference = 'Stop'

function Step($msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

function Ok($msg) {
    Write-Host "    OK: $msg" -ForegroundColor Green
}

function Warn($msg) {
    Write-Host "    WARN: $msg" -ForegroundColor Yellow
}

function Fail($msg) {
    Write-Host ""
    Write-Host "FAIL: $msg" -ForegroundColor Red
    exit 1
}

# 0. Verify we are at the repo root
if (-not (Test-Path "Cargo.toml")) {
    Fail "Run this script from the kitsune-engine repository root."
}

# 1. Build release binary
Step "Building release binary (cargo build --release -p kitsune-ui)"
cargo build --release -p kitsune-ui
if ($LASTEXITCODE -ne 0) {
    Fail "cargo build failed."
}

$exePath = "target\release\kitsune.exe"
if (-not (Test-Path $exePath)) {
    Fail "Expected binary not found: $exePath"
}
$exeSize = [math]::Round((Get-Item $exePath).Length / 1MB, 1)
Ok "Binary: $exePath ($exeSize MB)"

# 2. Ensure cargo-wix is installed
Step "Checking cargo-wix"
$wixCmd = Get-Command "cargo-wix" -ErrorAction SilentlyContinue
if (-not $wixCmd) {
    Warn "cargo-wix not found -- installing..."
    cargo install cargo-wix
    if ($LASTEXITCODE -ne 0) {
        Fail "Failed to install cargo-wix."
    }
}
Ok "cargo-wix is available"

# 3. Ensure WiX Toolset is on PATH
Step "Checking WiX Toolset (candle / light)"
$candle = Get-Command "candle.exe" -ErrorAction SilentlyContinue
if (-not $candle) {
    Warn "WiX Toolset not found on PATH."
    Warn "Download from: https://github.com/wixtoolset/wix3/releases"
    Warn "Install WiX 3.x and ensure it is on your PATH, then re-run."
    Fail "WiX Toolset required."
}
Ok "WiX Toolset: $($candle.Source)"

# 4. Build the MSI
Step "Building MSI installer (cargo wix -p kitsune-ui --no-build)"
cargo wix -p kitsune-ui --no-build --nocapture
if ($LASTEXITCODE -ne 0) {
    Fail "cargo wix failed."
}

# 5. Report output
$msi = Get-ChildItem -Path "target\wix" -Filter "*.msi" -ErrorAction SilentlyContinue |
       Sort-Object LastWriteTime -Descending |
       Select-Object -First 1

if (-not $msi) {
    Fail "MSI not found in target\wix -- check cargo wix output above."
}

$sizeMb = [math]::Round($msi.Length / 1MB, 1)

Write-Host ""
Write-Host "--------------------------------------------------------------" -ForegroundColor Green
Write-Host "  Build complete!" -ForegroundColor Green
Write-Host "  Installer : $($msi.Name)" -ForegroundColor Green
Write-Host "  Path      : $($msi.FullName)" -ForegroundColor Green
Write-Host "  Size      : $sizeMb MB" -ForegroundColor Green
Write-Host "--------------------------------------------------------------" -ForegroundColor Green
Write-Host ""

# 6. Open output folder
$open = Read-Host "Open output folder? [Y/n]"
if ($open -ne 'n' -and $open -ne 'N') {
    Start-Process explorer.exe -ArgumentList "/select,`"$($msi.FullName)`""
}
