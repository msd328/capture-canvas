$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

if (-not (Get-Command supabase -ErrorAction SilentlyContinue)) {
    Write-Host "[SupabaseCheck] FAILED: Supabase CLI is not installed or not on PATH." -ForegroundColor Red
    Write-Host "[SupabaseCheck] Install the Supabase CLI, ensure Docker Desktop is running, then rerun this script." -ForegroundColor Yellow
    exit 1
}

function Invoke-SupabaseCheckStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,

        [Parameter(Mandatory = $true)]
        [scriptblock]$Command
    )

    Write-Host "[SupabaseCheck] $Label"
    & $Command
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) {
        $exitCode = 0
    }
    if ($exitCode -ne 0) {
        Write-Host "[SupabaseCheck] FAILED: $Label (exit code $exitCode)" -ForegroundColor Red
        exit $exitCode
    }
}

Invoke-SupabaseCheckStep `
    -Label "Starting local Supabase services" `
    -Command { supabase start }

Invoke-SupabaseCheckStep `
    -Label "Resetting database from committed migrations" `
    -Command { supabase db reset }

Invoke-SupabaseCheckStep `
    -Label "Running pgTAP database tests" `
    -Command { supabase test db }

Write-Host "[SupabaseCheck] Local database validation passed" -ForegroundColor Green
