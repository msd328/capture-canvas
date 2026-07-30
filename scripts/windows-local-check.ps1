$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Write-Host "[RecorderCheck] Installing locked frontend dependencies"
bun install --frozen-lockfile

Write-Host "[RecorderCheck] Linting frontend"
bun run lint

Write-Host "[RecorderCheck] Building frontend"
bun run build

Push-Location (Join-Path $repoRoot "src-tauri")
try {
    Write-Host "[RecorderCheck] Checking Rust formatting"
    cargo fmt --all -- --check

    Write-Host "[RecorderCheck] Running Rust tests"
    cargo test --no-default-features --locked

    Write-Host "[RecorderCheck] Checking Windows recorder"
    cargo check --no-default-features --locked
}
finally {
    Pop-Location
}

Write-Host "[RecorderCheck] All local validation steps passed"
