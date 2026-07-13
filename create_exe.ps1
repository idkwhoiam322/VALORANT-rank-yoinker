<#
.SYNOPSIS
    Builds tauri_rewrite\src-tauri\target\release\vry-rust.exe.

.DESCRIPTION
    Before building, force-stops any running vry-rust.exe process(es) -- a
    stale process can hold the exe file open, causing a linker "Access denied".

Auto-detects LLD (fast multi-threaded linker) on each run and places a
ld.exe copy on PATH so GCC's collect2 uses ld.lld instead of MinGW's
single-threaded ld.bfd. Builds are ~2.4x faster when LLVM is installed.
Falls back to the default MinGW linker when LLD is not available.

    Profile settings (opt-level, LTO, etc.) are applied via environment
    variables instead of rewriting Cargo.toml.

.PARAMETER Clean
    Run cargo clean before building (removes tauri_rewrite\src-tauri\target\).

.PARAMETER Release
    Build with maximum performance optimizations (LTO, opt-level 3, etc.)
    instead of the fast dev build. Takes longer but produces a faster binary.

.PARAMETER NoRestart
    Skip auto-restart even if vry-rust.exe was running before the build.

.PARAMETER Start
    Always start vry-rust.exe after building, regardless of whether it was running.

.EXAMPLE
    .\create_exe.ps1              # fast dev build
    .\create_exe.ps1 -Release     # max perf build (takes longer)
    .\create_exe.ps1 -Clean
#>

[CmdletBinding()]
param(
    [switch]$Clean,
    [switch]$Release,
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
    if (-not $procs) { return }

    Write-Step "Stopping running vry-rust.exe process(es) (pid: $($procs.Id -join ', '))"
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue

    for ($i = 0; $i -lt 10; $i++) {
        Start-Sleep -Milliseconds 300
        if (-not (Get-Process -Name "vry-rust" -ErrorAction SilentlyContinue)) { return }
    }

    if (Get-Process -Name "vry-rust" -ErrorAction SilentlyContinue) {
        Fail "Couldn't stop vry-rust.exe (it may be unresponsive) -- close it manually via Task Manager and re-run."
    }
}

function Find-Lld {
    $lld = Get-Command ld.lld -ErrorAction SilentlyContinue
    if ($lld) { return $lld.Source }
    $candidate = "C:\Program Files\LLVM\bin\ld.lld.exe"
    if (Test-Path $candidate) { return $candidate }
    return $null
}

function Invoke-Cargo {
    param([string]$Dir)
    Push-Location -Path $Dir
    $ErrorActionPreference = "Continue"
    cargo build --release 2>&1 | ForEach-Object { Write-Host $_ }
    $rc = $LASTEXITCODE
    Pop-Location
    if ($rc -ne 0) { Fail "cargo build failed (exit code $rc) -- see output above." }
}

# ── preamble ──────────────────────────────────────────────────────────
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
& $cargo.Source --version

# ── LLD auto-detection ────────────────────────────────────────────────
$lldPath = Find-Lld
$lldTempDir = $null
$lldBinsDir = $null

if ($lldPath) {
    Write-Step "LLD found at $lldPath -- enabling fast linker"

    $lldTempDir = Join-Path $PSScriptRoot "tauri_rewrite\src-tauri\.lld-bin"
    New-Item -ItemType Directory -Force -Path $lldTempDir | Out-Null

    # Place a ld.exe that is actually ld.lld and name the copy ld.exe so
    # that GCC's collect2 finds it instead of MinGW's single-threaded ld.
    $ldCopy = Join-Path $lldTempDir "ld.exe"
    if (-not (Test-Path $ldCopy)) {
        Copy-Item -Path $lldPath -Destination $ldCopy
    }

    # Prepend to PATH so collect2 finds our ld.exe before MinGW's ld.exe
    $lldBinsDir = $lldTempDir
    $env:PATH = "$lldBinsDir;$env:PATH"

    Write-Host "  Using LLD as the linker (replaced ld.exe in PATH)"
} else {
    Write-Step "LLD not found -- using default MinGW linker"
}

# ── profile via env vars (no Cargo.toml rewriting) ───────────────────
$profileLabel = if ($Release) { "release" } else { "dev" }
$envBackup = @{}

try {
    if ($Release) {
        Write-Step "Applying maximum performance profile"
        $envBackup.CARGO_PROFILE_RELEASE_OPT_LEVEL = $env:CARGO_PROFILE_RELEASE_OPT_LEVEL
        $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = "3"
        $envBackup.CARGO_PROFILE_RELEASE_LTO = $env:CARGO_PROFILE_RELEASE_LTO
        $env:CARGO_PROFILE_RELEASE_LTO = "true"
        $envBackup.CARGO_PROFILE_RELEASE_CODEGEN_UNITS = $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS
        $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"
        $envBackup.CARGO_PROFILE_RELEASE_STRIP = $env:CARGO_PROFILE_RELEASE_STRIP
        $env:CARGO_PROFILE_RELEASE_STRIP = "true"
    } else {
        Write-Step "Applying fast dev profile (opt-level = 0)"
        $envBackup.CARGO_PROFILE_RELEASE_OPT_LEVEL = $env:CARGO_PROFILE_RELEASE_OPT_LEVEL
        $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = "0"
    }

    if ($Clean) {
        Write-Step "Cleaning old build artifacts (cargo clean)"
        Push-Location -Path ".\tauri_rewrite\src-tauri"
        $ErrorActionPreference = "Continue"
        cargo clean 2>&1 | ForEach-Object { Write-Host $_ }
        Pop-Location
    }

    Write-Step "Building vry-rust.exe"
    Invoke-Cargo -Dir ".\tauri_rewrite\src-tauri"

    $exePath = Join-Path $PSScriptRoot "tauri_rewrite\src-tauri\target\release\vry-rust.exe"
    if (-not (Test-Path $exePath)) {
        Fail "Build finished but no exe at $exePath -- check cargo output above."
    }

    if ($Start -or ($wasRunning -and -not $NoRestart)) {
        Write-Step "Re-launching vry-rust.exe (was running before build)"
        Start-Process -FilePath $exePath
    }

    Write-Step "Done"
    Write-Host "Built: $exePath ($profileLabel)" -ForegroundColor Green
}
finally {
    # Restore profile env vars
    if ($null -ne $envBackup.CARGO_PROFILE_RELEASE_OPT_LEVEL) { $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = $envBackup.CARGO_PROFILE_RELEASE_OPT_LEVEL }
    else { Remove-Item -Path env:CARGO_PROFILE_RELEASE_OPT_LEVEL -ErrorAction SilentlyContinue }
    if ($envBackup.ContainsKey("CARGO_PROFILE_RELEASE_LTO")) {
        if ($null -ne $envBackup.CARGO_PROFILE_RELEASE_LTO) { $env:CARGO_PROFILE_RELEASE_LTO = $envBackup.CARGO_PROFILE_RELEASE_LTO }
        else { Remove-Item -Path env:CARGO_PROFILE_RELEASE_LTO -ErrorAction SilentlyContinue }
    }
    if ($envBackup.ContainsKey("CARGO_PROFILE_RELEASE_CODEGEN_UNITS")) {
        if ($null -ne $envBackup.CARGO_PROFILE_RELEASE_CODEGEN_UNITS) { $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = $envBackup.CARGO_PROFILE_RELEASE_CODEGEN_UNITS }
        else { Remove-Item -Path env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS -ErrorAction SilentlyContinue }
    }
    if ($envBackup.ContainsKey("CARGO_PROFILE_RELEASE_STRIP")) {
        if ($null -ne $envBackup.CARGO_PROFILE_RELEASE_STRIP) { $env:CARGO_PROFILE_RELEASE_STRIP = $envBackup.CARGO_PROFILE_RELEASE_STRIP }
        else { Remove-Item -Path env:CARGO_PROFILE_RELEASE_STRIP -ErrorAction SilentlyContinue }
    }
    # Clean up temp LLD binaries
    if ($lldBinsDir -and ($env:PATH -like "$lldBinsDir*")) {
        $env:PATH = $env:PATH -replace [regex]::Escape("$lldBinsDir;"), ""
    }
    if ($lldTempDir -and (Test-Path $lldTempDir)) {
        Remove-Item -Path $lldTempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
