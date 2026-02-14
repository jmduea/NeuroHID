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

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$protocolDoc = "docs/runtime-ml-protocol-v2.md"
$specDoc = "docs/SPECIFICATION.md"

if (-not (Test-Path $protocolDoc)) {
    throw "Missing protocol doc: $protocolDoc"
}
if (-not (Test-Path $specDoc)) {
    throw "Missing specification doc: $specDoc"
}

$doc = Get-Content -Raw -Path $protocolDoc
$requiredTokens = @(
    '"v": 2',
    "hello",
    "decision_event",
    "errp_window",
    "runtime_telemetry",
    "trainer_status",
    "candidate_model_ready"
)

$missing = @()
foreach ($token in $requiredTokens) {
    if ($doc -notmatch [regex]::Escape($token)) {
        $missing += $token
    }
}

if ($missing.Count -gt 0) {
    Write-Host "Protocol doc is missing required contract tokens:" 
    $missing | ForEach-Object { Write-Host "  - $_" }
    throw "Protocol contract doc check failed"
}

Invoke-CheckedCommand -Command "cargo" -Arguments @("test", "-p", "neurohid-types", "ipc_v2::tests")
Invoke-CheckedCommand -Command "cargo" -Arguments @("test", "-p", "neurohid-ipc")

Write-Host "Protocol contract checks passed."
