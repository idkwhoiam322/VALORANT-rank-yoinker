<#
.SYNOPSIS
    Builds tauri_rewrite\src-tauri\target\release\vry-rust.exe.

.DESCRIPTION
    Equivalent to:
        cd tauri_rewrite\src-tauri
        cargo build --release
    with sanity checks (cargo on PATH) and clearer output, plus an optional
    -Clean switch to cargo clean first.

    Before building, force-stops any running vry-rust.exe process(es) -- a
    stale process can hold the exe file open, causing a linker "Access denied".

.PARAMETER Clean
    Run cargo clean before building (removes tauri_rewrite\src-tauri\target\).

.PARAMETER NoRestart
    Skip auto-restart even if vry-rust.exe was running before the build.

.PARAMETER Start
    Always start vry-rust.exe after building, regardless of whether it was running.

.EXAMPLE
    .\create_exe.ps1
.EXAMPLE
    .\create_exe.ps1 -Clean
#>

[CmdletBinding()]
param(
    [switch]$Clean,
    [switch]$NoRestart,
    [switch]$Start
)

$ErrorActionPreference = "Stop"

function Write-Step($msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

function Fail($msg) {
    Write-Host ""
    Write-Host "ERROR: $msg" -ForegroundColor Red
    exit 1
}

function Stop-VryProcesses {
    $procs = Get-Process -Name "vry-rust" -ErrorAction SilentlyContinue
    if (-not $procs) {
        return
    }

    Write-Step "Stopping running vry-rust.exe process(es) (pid: $($procs.Id -join ', '))"
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue

    for ($i = 0; $i -lt 10; $i++) {
        Start-Sleep -Milliseconds 300
        if (-not (Get-Process -Name "vry-rust" -ErrorAction SilentlyContinue)) {
            return
        }
    }

    if (Get-Process -Name "vry-rust" -ErrorAction SilentlyContinue) {
        Fail "Couldn't stop vry-rust.exe (it may be unresponsive) -- close it manually via Task Manager and re-run."
    }
}

# Always run relative to this script's own folder
Set-Location -Path $PSScriptRoot

if (-not (Test-Path ".\tauri_rewrite\src-tauri\Cargo.toml")) {
    Fail "tauri_rewrite\src-tauri\Cargo.toml not found in $PSScriptRoot. Run this script from the project root."
}

$wasRunning = $null -ne (Get-Process -Name "vry-rust" -ErrorAction SilentlyContinue)
Stop-VryProcesses

Write-Step "Checking for Rust / cargo"
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    Fail "cargo was not found on PATH. Install the Rust toolchain from https://rustup.rs and try again."
}
cargo --version

if ($Clean) {
    Write-Step "Cleaning old build artifacts (cargo clean)"
    Push-Location -Path ".\tauri_rewrite\src-tauri"
    cargo clean
    Pop-Location
}

Write-Step "Building vry-rust.exe (cargo build --release)"
Push-Location -Path ".\tauri_rewrite\src-tauri"
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    Fail "cargo build failed -- see output above."
}
Pop-Location

$exePath = Join-Path $PSScriptRoot "tauri_rewrite\src-tauri\target\release\vry-rust.exe"
if (-not (Test-Path $exePath)) {
    Fail "Build finished but $exePath wasn't produced. Check the cargo output above."
}

if ($Start -or ($wasRunning -and -not $NoRestart)) {
    Write-Step "Re-launching vry-rust.exe (was running before build)"
    Start-Process -FilePath $exePath
}

Write-Step "Done"
Write-Host "Built: $exePath" -ForegroundColor Green
