<#
.SYNOPSIS
    Builds dist\vry\vry.exe from this project.

.DESCRIPTION
    Equivalent to running, from the project root:
        pip install -r requirements.txt
        pyinstaller vry.spec --noconfirm
    but with a few sanity checks (python/pip on PATH, script run from the
    right folder) and clearer output, plus an optional -Clean switch to
    remove old build\ and dist\ folders first (recommended after pulling
    upstream changes, so you're not shipping stale files from a previous
    build).

    Before building, this also force-stops any running vry.exe process(es)
    -- both the GUI and its "--vry-backend" child re-launch the same exe
    under the same process name, so a window you "closed" (e.g. minimized
    to tray, or a child that didn't exit cleanly) can still be holding
    dist\vry\vry.exe open, which is what causes:
        PermissionError: [WinError 5] Access is denied: '...\vry.exe'

.PARAMETER Clean
    Delete build\ and dist\ before building.

.EXAMPLE
    .\create_exe.ps1
.EXAMPLE
    .\create_exe.ps1 -Clean
#>

[CmdletBinding()]
param(
    [switch]$Clean
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
    # Both the GUI and its "--vry-backend" child process run as vry.exe, so
    # this one Get-Process call catches either/both. Matches by process name
    # only (not path), so this also cleans up an old build's vry.exe if the
    # dist folder moved -- that's intentional here.
    $procs = Get-Process -Name "vry" -ErrorAction SilentlyContinue
    if (-not $procs) {
        return
    }

    Write-Step "Stopping running vry.exe process(es) (pid: $($procs.Id -join ', '))"
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue

    # Give Windows a moment to actually release the file handle after the
    # process dies -- killing it doesn't guarantee the lock is gone the
    # very next instruction, and the file build step is next.
    for ($i = 0; $i -lt 10; $i++) {
        Start-Sleep -Milliseconds 300
        if (-not (Get-Process -Name "vry" -ErrorAction SilentlyContinue)) {
            return
        }
    }

    if (Get-Process -Name "vry" -ErrorAction SilentlyContinue) {
        Fail "Couldn't stop vry.exe (it may be unresponsive) -- close it manually via Task Manager and re-run."
    }
}

# Always run relative to this script's own folder, regardless of the caller's
# working directory (so double-clicking it in Explorer works too).
Set-Location -Path $PSScriptRoot

if (-not (Test-Path ".\vry.spec")) {
    Fail "vry.spec not found in $PSScriptRoot. Run this script from the project root."
}

Stop-VryProcesses

Write-Step "Checking for Python"
$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    Fail "python was not found on PATH. Install Python 3.10-3.11 and try again (see INSTALL.bat / README.md)."
}
python --version

Write-Step "Installing requirements (pip install -r requirements.txt)"
python -m pip install -r requirements.txt
if ($LASTEXITCODE -ne 0) {
    Fail "pip install failed -- see output above."
}

if ($Clean) {
    Write-Step "Removing old build\ and dist\ folders (-Clean)"
    Remove-Item -Recurse -Force ".\build" -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force ".\dist" -ErrorAction SilentlyContinue
}

Write-Step "Building vry.exe (pyinstaller vry.spec)"
python -m PyInstaller vry.spec --noconfirm
if ($LASTEXITCODE -ne 0) {
    Fail "PyInstaller build failed -- see output above."
}

$exePath = Join-Path $PSScriptRoot "dist\vry\vry.exe"
if (-not (Test-Path $exePath)) {
    Fail "Build finished but $exePath wasn't produced. Check the PyInstaller output above."
}

Write-Step "Done"
Write-Host "Built: $exePath" -ForegroundColor Green
Write-Host "Ship the whole dist\vry\ folder -- vry.exe depends on the _internal\ folder next to it."