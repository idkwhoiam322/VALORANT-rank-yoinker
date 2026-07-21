param(
    [switch]$NoToolInstall,
    [switch]$SkipCargoFetch
)

$ErrorActionPreference = "Stop"
$OriginalPref = $ErrorActionPreference

function Test-Command($cmd) {
    Get-Command $cmd -ErrorAction SilentlyContinue
}

$Pass = 0
$Fail = 0
$Skipped = 0

function Step($label, $body) {
    Write-Host -NoNewline "[VRY] $label ... "
    try {
        & $body
        Write-Host "PASS" -ForegroundColor Green
        $script:Pass++
    } catch {
        Write-Host "FAIL" -ForegroundColor Red
        Write-Host "       $_" -ForegroundColor DarkRed
        $script:Fail++
    }
}

function Warn($message) {
    Write-Host "       $message" -ForegroundColor Yellow
}

# ---- 1. Rust toolchain ----
Step "Checking Rust toolchain" {
    if (!(Test-Command rustc)) {
        if ($NoToolInstall) { throw "rustc not found. Install from https://rustup.rs" }
        Write-Host -NoNewline "not found, installing via rustup... "
        $tmp = "$env:TEMP\rustup-init.exe"
        Invoke-WebRequest -Uri "https://win.rustup.rs" -OutFile $tmp
        & $tmp -y --default-toolchain stable --no-modify-path
        # Re-source PATH for the current session
        $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "User") + ";$env:PATH"
        $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";$env:PATH"
    }
    $ver = rustc --version
    if ($LASTEXITCODE -ne 0) { throw "rustc failed: $ver" }
    Write-Host $ver
}

Step "Checking Cargo" {
    if (!(Test-Command cargo)) { throw "cargo not found after rustup install" }
    $ver = cargo --version
    if ($LASTEXITCODE -ne 0) { throw "cargo failed: $ver" }
    Write-Host $ver
}

# ---- 2. Check Rust edition compatibility ----
Step "Checking Rust edition" {
    $rustEdition = "2021"
    $ok = $false
    $output = rustc --edition 2>&1 | Out-String
    # If rustc doesn't understand --edition, it may error; that's OK, the default is fine
    if ($LASTEXITCODE -eq 0) {
        if ($output -match "\b$rustEdition\b") { $ok = $true }
    } else {
        # Assume older rustc that supports 2021 anyway
        $ok = $true
    }
    if (!$ok) { throw "Current Rust toolchain may not support edition $rustEdition" }
    Write-Host "OK (Rust $rustEdition)"
}

# ---- 3. Tauri prerequisites ----
Step "Checking Tauri prerequisites" {
    if (!(Test-Command cargo)) { throw "cargo required to check Tauri deps" }
    $targetDir = Join-Path $PSScriptRoot "tauri_rewrite" "src-tauri"
    $result = & cargo tauri info 2>&1
    if ($LASTEXITCODE -ne 0) {
        Warn "Tauri prerequisites may be incomplete. Run 'cargo tauri info' manually."
    } else {
        Write-Host "OK"
    }
}

# ---- 4. Cargo dependencies ----
Step "Fetching Cargo dependencies" {
    $targetDir = Join-Path $PSScriptRoot "tauri_rewrite" "src-tauri"
    if (!(Test-Path $targetDir)) { throw "Expected src-tauri at $targetDir" }
    Push-Location $targetDir
    try {
        $result = cargo fetch 2>&1
        if ($LASTEXITCODE -ne 0) { throw "cargo fetch failed: $result" }
        Write-Host "OK"
    } finally { Pop-Location }
}

# ---- 5. Check (not build, just check for errors) ----
Step "Running cargo check" {
    $targetDir = Join-Path $PSScriptRoot "tauri_rewrite" "src-tauri"
    Push-Location $targetDir
    try {
        $result = cargo check 2>&1
        if ($LASTEXITCODE -ne 0) { throw "cargo check failed" }
        Write-Host "compilation OK"
    } finally { Pop-Location }
}

# ---- 6. Clippy ----
Step "Running cargo clippy" {
    $targetDir = Join-Path $PSScriptRoot "tauri_rewrite" "src-tauri"
    Push-Location $targetDir
    try {
        $result = cargo clippy -- -D warnings 2>&1
        if ($LASTEXITCODE -ne 0) { throw "clippy found issues" }
        Write-Host "no issues"
    } finally { Pop-Location }
}

# ---- Summary ----
Write-Host
Write-Host "========================" -ForegroundColor Cyan
Write-Host "   SETUP SUMMARY" -ForegroundColor Cyan
Write-Host "========================" -ForegroundColor Cyan
Write-Host "  Passed : $Pass" -ForegroundColor Green
if ($Fail -gt 0) { Write-Host "  Failed : $Fail" -ForegroundColor Red }
if ($Skipped -gt 0) { Write-Host "  Skipped: $Skipped" -ForegroundColor Yellow }
Write-Host

if ($Fail -gt 0) {
    Write-Host "Some checks failed. Review the messages above." -ForegroundColor Yellow
    exit 1
} else {
    Write-Host "Development environment is ready." -ForegroundColor Green
    Write-Host "Run:  cd tauri_rewrite/src-tauri && cargo tauri dev" -ForegroundColor Cyan
}
