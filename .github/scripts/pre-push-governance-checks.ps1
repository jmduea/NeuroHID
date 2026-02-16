param(
    [ValidateSet('focused', 'full')]
    [string]$RustScope = 'focused'
)

$ErrorActionPreference = 'Stop'

$impactPath = '.github/automation/local-impact-output.json'

& pwsh -File './.github/scripts/classify-impact.ps1' -OutputJsonPath $impactPath
if ($LASTEXITCODE -ne 0) {
    throw 'Impact classification failed.'
}

$impact = Get-Content -Path $impactPath -Raw | ConvertFrom-Json -Depth 10

$args = @('-File', './.github/scripts/run-agent-ready-tasks.ps1', '-RustScope', $RustScope)

if (-not $impact.rust) {
    $args += '-SkipRust'
}

if ($impact.python) {
    $args += '-WithPython'
}

if ($impact.docs) {
    $args += '-WithDocs'
}

if ($impact.protocol) {
    $args += '-WithProtocol'
}

if ($impact.unsafe) {
    $args += '-WithUnsafe'
}

& pwsh @args
if ($LASTEXITCODE -ne 0) {
    throw 'Canonical agent-ready checks failed.'
}

& pwsh -File './.github/scripts/validate-policy-integrity.ps1'
if ($LASTEXITCODE -ne 0) {
    throw 'Policy integrity validation failed.'
}

Write-Host 'Pre-push governance checks passed.'