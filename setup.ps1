param(
    [switch]$NoToolInstall,
    [switch]$SkipCargoFetch
)

# Auto-elevate: restart as Administrator if not already running elevated.
# Tool install (rustup, winget) requires admin rights on Windows.
if (-NOT ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "Not running as Administrator. Relaunching with elevated privileges..." -ForegroundColor Yellow
    $argList = @("-NoProfile", "-File", "`"$PSCommandPath`"")
    if ($NoToolInstall) { $argList += "-NoToolInstall" }
    if ($SkipCargoFetch) { $argList += "-SkipCargoFetch" }
    Start-Process -FilePath powershell -ArgumentList $argList -Verb RunAs
    exit 0
}

$ErrorActionPreference = "Stop"
$OriginalPref = $ErrorActionPreference

function Test-Command($cmd) {
    Get-Command $cmd -ErrorAction SilentlyContinue
}

$RepoRoot = $PSScriptRoot
$TauriDir = Join-Path $RepoRoot "tauri_rewrite"
$SrcTauriDir = Join-Path $TauriDir "src-tauri"
$FrontendDir = Join-Path $TauriDir "frontend"

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
    $result = & cargo tauri info 2>&1
    if ($LASTEXITCODE -ne 0) {
        Warn "Tauri prerequisites may be incomplete. Run 'cargo tauri info' manually."
    } else {
        Write-Host "OK"
    }
}

# ---- 4. Cargo dependencies ----
Step "Fetching Cargo dependencies" {
    if (!(Test-Path $SrcTauriDir)) { throw "Expected src-tauri at $SrcTauriDir" }
    Push-Location $SrcTauriDir
    try {
        $result = cargo fetch 2>&1
        if ($LASTEXITCODE -ne 0) { throw "cargo fetch failed: $result" }
        Write-Host "OK"
    } finally { Pop-Location }
}

# ---- 5. Check (not build, just check for errors) ----
Step "Running cargo check" {
    Push-Location $SrcTauriDir
    try {
        $result = cargo check 2>&1
        if ($LASTEXITCODE -ne 0) { throw "cargo check failed" }
        Write-Host "compilation OK"
    } finally { Pop-Location }
}

# ---- 6. Clippy ----
Step "Running cargo clippy" {
    Push-Location $SrcTauriDir
    try {
        $result = cargo clippy -- -D warnings 2>&1
        if ($LASTEXITCODE -ne 0) { throw "clippy found issues" }
        Write-Host "no issues"
    } finally { Pop-Location }
}

# ---- 7. Node.js / npm ----
Step "Checking Node.js" {
    if (!(Test-Command node)) {
        if ($NoToolInstall) { throw "node not found. Install from https://nodejs.org" }
        Write-Host -NoNewline "not found, installing via winget... "
        winget install OpenJS.NodeJS.LTS 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "winget install failed. Install Node.js manually from https://nodejs.org" }
        # Re-source PATH for the current session
        $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "User") + ";$env:PATH"
        $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";$env:PATH"
    }
    $ver = node --version
    if ($LASTEXITCODE -ne 0) { throw "node failed: $ver" }
    Write-Host $ver
}

Step "Checking npm" {
    if (!(Test-Command npm)) { throw "npm not found after Node.js install" }
    $ver = npm --version
    if ($LASTEXITCODE -ne 0) { throw "npm failed: $ver" }
    Write-Host $ver
}

# ---- 8. Frontend dev dependencies ----
Step "Setting up frontend tooling" {
    Push-Location $TauriDir
    try {
        # Create package.json if absent
        if (!(Test-Path (Join-Path $TauriDir "package.json"))) {
            $result = npm init -y 2>&1
            if ($LASTEXITCODE -ne 0) { throw "npm init failed: $result" }
        }
        # Install TypeScript if absent
        $hasTsc = Test-Command tsc
        if ($hasTsc) {
            # Check if it's the local one or a global one
            $localTsc = Join-Path $TauriDir "node_modules" ".bin" "tsc"
            if (!(Test-Path $localTsc)) { $hasTsc = $false }
        }
        if (!$hasTsc) {
            $result = npm install -D typescript 2>&1
            if ($LASTEXITCODE -ne 0) { throw "npm install typescript failed: $result" }
        }
        Write-Host "OK"
    } finally { Pop-Location }
}

# ---- 9. Frontend type-check ----
Step "Running TypeScript check (--checkJs)" {
    Push-Location $TauriDir
    try {
        $result = npx tsc --noEmit 2>&1
        $exit = $LASTEXITCODE
        if ($exit -ne 0) {
            # Show the actual errors but still throw
            Write-Host
            Write-Host $result -ForegroundColor Yellow
            throw "TypeScript found type errors"
        }
        Write-Host "no type errors"
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
    Write-Host "Commands:" -ForegroundColor Cyan
    Write-Host "  cd tauri_rewrite/src-tauri && cargo tauri dev   # Run in dev mode" -ForegroundColor Cyan
    Write-Host "  cd tauri_rewrite && npx tsc --noEmit           # Type-check frontend JS" -ForegroundColor Cyan
    Write-Host "  cd tauri_rewrite/src-tauri && cargo clippy     # Rust lint check" -ForegroundColor Cyan
}
