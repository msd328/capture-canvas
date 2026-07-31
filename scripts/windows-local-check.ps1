$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Invoke-RecorderCheckStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,

        [Parameter(Mandatory = $true)]
        [scriptblock]$Command,

        [string]$FailureHint = ""
    )

    Write-Host "[RecorderCheck] $Label"
    & $Command
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) {
        $exitCode = 0
    }
    if ($exitCode -ne 0) {
        Write-Host "[RecorderCheck] FAILED: $Label (exit code $exitCode)" -ForegroundColor Red
        if (-not [string]::IsNullOrWhiteSpace($FailureHint)) {
            Write-Host "[RecorderCheck] $FailureHint" -ForegroundColor Yellow
        }
        exit $exitCode
    }
}

Invoke-RecorderCheckStep `
    -Label "Installing locked frontend dependencies" `
    -Command { bun install --frozen-lockfile }

Invoke-RecorderCheckStep `
    -Label "Linting frontend" `
    -Command { bun run lint } `
    -FailureHint "For Prettier or 'Delete CR' errors, run 'bunx prettier --write eslint.config.js src vite.config.ts', review 'git diff', then rerun this script."

Invoke-RecorderCheckStep `
    -Label "Building frontend" `
    -Command { bun run build }

Push-Location (Join-Path $repoRoot "src-tauri")
try {
    Invoke-RecorderCheckStep `
        -Label "Checking Rust formatting" `
        -Command { cargo fmt --all -- --check } `
        -FailureHint "Run 'cargo fmt --all' from src-tauri, review the diff, then rerun this script."

    Invoke-RecorderCheckStep `
        -Label "Running Rust tests" `
        -Command { cargo test --no-default-features --locked }

    Invoke-RecorderCheckStep `
        -Label "Checking Windows recorder" `
        -Command { cargo check --no-default-features --locked }
}
finally {
    Pop-Location
}

Write-Host "[RecorderCheck] All local validation steps passed" -ForegroundColor Green
