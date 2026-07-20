$repoRoot = git rev-parse --show-toplevel
$scriptsDir = Join-Path $repoRoot "scripts"
$hookSrc    = Join-Path $scriptsDir "pre-commit"
$gitHooksDir = Join-Path $repoRoot ".git" | Join-Path -ChildPath "hooks"
$hookDst     = Join-Path $gitHooksDir "pre-commit"
Copy-Item -Path $hookSrc -Destination $hookDst -Force
Write-Host "Installed pre-commit hook to $hookDst"