# scripts/build.ps1 - Build dispatcher PowerShell pour apollia-os (Windows natif).
#
# Équivalent de scripts/build.sh pour les devs qui ne veulent pas passer par
# Git Bash / WSL. La table de presets est identique.
#
# Usage :
#   .\scripts\build.ps1 <preset> [-Debug] [-Check] [-List]
#
# Voir BUILD.md pour la matrice complète.

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Preset,
    [switch]$Debug,
    [switch]$Release,
    [switch]$Check,
    [switch]$List,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Extra
)

$ErrorActionPreference = "Stop"

# ─── Preset table ──────────────────────────────────────────────────────────────
$Presets = @{
    "macos-silicon"      = @{ Triple = "aarch64-apple-darwin";        Features = "cloud,local-metal,local-accelerate,stt-metal" }
    "linux-x86-cpu"      = @{ Triple = "x86_64-unknown-linux-gnu";    Features = "cloud,local-cpu,stt-whisper-cpp" }
    "linux-x86-cuda"     = @{ Triple = "x86_64-unknown-linux-gnu";    Features = "cloud,local-cuda,stt-cuda" }
    "linux-x86-rocm"     = @{ Triple = "x86_64-unknown-linux-gnu";    Features = "cloud,local-rocm,stt-rocm" }
    "linux-x86-vulkan"   = @{ Triple = "x86_64-unknown-linux-gnu";    Features = "cloud,local-vulkan,stt-whisper-cpp" }
    "linux-arm-cpu"      = @{ Triple = "aarch64-unknown-linux-gnu";   Features = "cloud,local-cpu,stt-whisper-cpp" }
    "linux-arm-cuda"     = @{ Triple = "aarch64-unknown-linux-gnu";   Features = "cloud,local-cuda,stt-cuda" }
    "windows-x86-cpu"    = @{ Triple = "x86_64-pc-windows-msvc";      Features = "cloud,local-cpu,stt-whisper-cpp" }
    "windows-x86-cuda"   = @{ Triple = "x86_64-pc-windows-msvc";      Features = "cloud,local-cuda,stt-cuda" }
    "windows-x86-rocm"   = @{ Triple = "x86_64-pc-windows-msvc";      Features = "cloud,local-rocm,stt-rocm" }
    "windows-x86-vulkan" = @{ Triple = "x86_64-pc-windows-msvc";      Features = "cloud,local-vulkan,stt-whisper-cpp" }
    "windows-arm-cpu"    = @{ Triple = "aarch64-pc-windows-msvc";     Features = "cloud,local-cpu,stt-whisper-cpp" }
    "windows-arm-cuda"   = @{ Triple = "aarch64-pc-windows-msvc";     Features = "cloud,local-cuda,stt-cuda" }
}

if ($List) {
    Write-Host "Available presets:"
    foreach ($key in ($Presets.Keys | Sort-Object)) {
        $p = $Presets[$key]
        Write-Host ("  {0,-22}  {1,-30}  features: {2}" -f $key, $p.Triple, $p.Features)
    }
    exit 0
}

if (-not $Preset) {
    Write-Host "Usage: .\scripts\build.ps1 <preset> [-Debug] [-Check] [-List]"
    Write-Host "Run with -List to see all presets."
    exit 1
}

if (-not $Presets.ContainsKey($Preset)) {
    Write-Host "ERROR: unknown preset '$Preset'. Use -List to see presets." -ForegroundColor Red
    exit 1
}

$Triple   = $Presets[$Preset].Triple
$Features = $Presets[$Preset].Features

# ─── Host sanity ───────────────────────────────────────────────────────────────
if ($Preset -like "macos-*") {
    Write-Host "ERROR: '$Preset' must be built on macOS." -ForegroundColor Red
    exit 1
}
if ($Preset -like "linux-*") {
    Write-Host "WARNING: '$Preset' is normally built on a Linux host. Cross-build
         from Windows MSVC → Linux requires a Linux sysroot. Continuing." -ForegroundColor Yellow
}
if ($Preset -eq "windows-arm-cuda") {
    Write-Host "WARNING: NVIDIA does not ship CUDA drivers for Windows-on-ARM today.
         This preset only makes sense for cross-build toward Jetson/Orin Linux." -ForegroundColor Yellow
    Start-Sleep -Seconds 5
}

# ─── rustup target ─────────────────────────────────────────────────────────────
if (Get-Command rustup -ErrorAction SilentlyContinue) {
    $installed = (rustup target list --installed 2>$null)
    if ($installed -notcontains $Triple) {
        Write-Host "→ Installing rustup target: $Triple"
        rustup target add $Triple
    }
}

# ─── Args ──────────────────────────────────────────────────────────────────────
$Verb = if ($Check) { "check" } else { "build" }
$Profile = if ($Debug) { "" } else { "--release" }
$ProfileDir = if ($Debug) { "debug" } else { "release" }

Write-Host "─── apollia-os build ───"
Write-Host "  preset    : $Preset"
Write-Host "  triple    : $Triple"
Write-Host "  features  : $Features"
Write-Host "  profile   : $(if ($Debug) { 'debug' } else { 'release' })"
Write-Host "  verb      : $Verb"
Write-Host

$CargoArgs = @(
    $Verb,
    "-p", "apollia-cli",
    "--target", $Triple,
    "--no-default-features",
    "--features", $Features
)
if ($Profile) { $CargoArgs += $Profile }
$CargoArgs += $Extra

& cargo $CargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# ─── Locate output ─────────────────────────────────────────────────────────────
$Ext = if ($Triple -match "windows") { ".exe" } else { "" }
$BinPath = "target\$Triple\$ProfileDir\apollia-os$Ext"

if ($Verb -eq "build" -and (Test-Path $BinPath)) {
    $Size = "{0:N1} MB" -f ((Get-Item $BinPath).Length / 1MB)
    Write-Host
    Write-Host "Built: $BinPath  ($Size)" -ForegroundColor Green
}
