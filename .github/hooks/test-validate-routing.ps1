Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$validator = Join-Path $repoRoot ".github\hooks\validate-routing.ps1"
$fixturesDir = Join-Path $repoRoot ".github\hooks\fixtures"

if (-not (Test-Path $validator)) {
    throw "Missing validator script: $validator"
}

$cases = @(
    @{ Name = "valid"; Path = "hooks.valid.json"; ShouldPass = $true },
    @{ Name = "invalid no catch-all"; Path = "hooks.invalid.no-catchall.json"; ShouldPass = $false },
    @{ Name = "invalid catch-all not last"; Path = "hooks.invalid.catchall-not-last.json"; ShouldPass = $false },
    @{ Name = "invalid duplicate matcher"; Path = "hooks.invalid.duplicate-matcher.json"; ShouldPass = $false },
    @{ Name = "invalid duplicate agent"; Path = "hooks.invalid.duplicate-agent-in-route.json"; ShouldPass = $false }
)

$failures = New-Object System.Collections.Generic.List[string]

foreach ($case in $cases) {
    $hooksPath = Join-Path $fixturesDir $case.Path
    if (-not (Test-Path $hooksPath)) {
        $failures.Add("$($case.Name): missing fixture $hooksPath")
        continue
    }

    $passed = $false
    try {
        & $validator -HooksPath $hooksPath -SkipContractReferenceChecks
        $passed = $true
    } catch {
        $passed = $false
    }

    if ($case.ShouldPass -and -not $passed) {
        $failures.Add("$($case.Name): expected pass, got failure")
    }

    if ((-not $case.ShouldPass) -and $passed) {
        $failures.Add("$($case.Name): expected failure, got pass")
    }
}

if ($failures.Count -gt 0) {
    Write-Host "Routing validator fixture tests failed:"
    $failures | ForEach-Object { Write-Host "  - $_" }
    throw "Fixture test suite failed"
}

Write-Host "Routing validator fixture tests passed."
