param(
    [ValidateSet('focused', 'cross', 'workspace')]
    [string]$RustScope = 'focused',
    [switch]$SkipRust,
    [switch]$WithPython,
    [switch]$WithDocs,
    [switch]$WithProtocol,
    [switch]$WithUnsafe
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-CheckedCommand {
    param(
        [string]$Command,
        [string[]]$Arguments
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed: $Command $($Arguments -join ' ')"
    }
}

Write-Host "Running agent-ready task sequence..."
Write-Host "Rust scope: $RustScope"

if (-not $SkipRust) {
    switch ($RustScope) {
        'focused' {
            Invoke-CheckedCommand -Command "cargo" -Arguments @("check", "-p", "neurohid-hub")
        }
        'cross' {
            Invoke-CheckedCommand -Command "cargo" -Arguments @("check", "-p", "neurohid-hub", "-p", "neurohid-calibration")
        }
        'workspace' {
            Invoke-CheckedCommand -Command "cargo" -Arguments @("check")
        }
    }

    Invoke-CheckedCommand -Command "cargo" -Arguments @("test", "-p", "neurohid-ipc")
}

if ($WithUnsafe) {
    Write-Host "Running unsafe compliance checks..."
    Invoke-CheckedCommand -Command "pwsh" -Arguments @("-File", "./.github/scripts/check-unsafe-compliance.ps1")
}

if ($WithProtocol) {
    Write-Host "Running protocol contract checks..."
    Invoke-CheckedCommand -Command "pwsh" -Arguments @("-File", "./.github/scripts/verify-protocol-contracts.ps1")
}

if ($WithDocs) {
    Write-Host "Running docs freshness checks..."
    Invoke-CheckedCommand -Command "pwsh" -Arguments @("-File", "./.github/scripts/check-docs-freshness.ps1")
}

if ($WithPython) {
    Write-Host "Running Python quality checks..."
    Invoke-CheckedCommand -Command "uv" -Arguments @("sync", "--directory", "python", "--extra", "dev")
    Invoke-CheckedCommand -Command "uv" -Arguments @("run", "--project", "python", "ruff", "check", "python/src", "python/tests")
    Invoke-CheckedCommand -Command "uv" -Arguments @("run", "--project", "python", "black", "--check", "python/src", "python/tests")
    Invoke-CheckedCommand -Command "uv" -Arguments @("run", "--project", "python", "mypy", "python/src")
    Invoke-CheckedCommand -Command "uv" -Arguments @("run", "--project", "python", "pytest", "python/tests", "-q")
}

Write-Host "Agent-ready sequence completed successfully."
