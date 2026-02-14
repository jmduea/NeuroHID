param(
    [string]$ReportPath = ".github/hooks/harness-smoke-report.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$hooksPath = Join-Path $repoRoot ".github\hooks\hooks.json"

$resolvedReportPath = if ([System.IO.Path]::IsPathRooted($ReportPath)) {
    $ReportPath
} else {
    Join-Path $repoRoot ($ReportPath -replace '/', '\\')
}

$reportDir = Split-Path -Parent $resolvedReportPath
if (-not [string]::IsNullOrWhiteSpace($reportDir) -and -not (Test-Path $reportDir)) {
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
}

$checks = @(
    @{ Name = "routing integrity"; Command = ".github/hooks/validate-routing.ps1" },
    @{ Name = "routing fixtures"; Command = ".github/hooks/test-validate-routing.ps1" },
    @{ Name = "docs contracts"; Command = ".github/hooks/validate-doc-contracts.ps1" }
)

$checkResults = New-Object System.Collections.Generic.List[object]
foreach ($check in $checks) {
    $passed = $false
    $message = "passed"
    try {
        & (Join-Path $repoRoot ($check.Command -replace '/', '\\'))
        $passed = $true
    } catch {
        $passed = $false
        $message = $_.Exception.Message
    }

    $checkResults.Add([pscustomobject]@{
        Name = [string]$check.Name
        Passed = $passed
        Message = $message
    })
}

if (-not (Test-Path $hooksPath)) {
    throw "Missing hooks manifest: $hooksPath"
}

$hooks = Get-Content -Raw -Path $hooksPath | ConvertFrom-Json
$routes = @($hooks.hooks.UserPromptSubmit)

$scenarios = @(
    @{
        Name = "Docs request"
        Prompt = "Please update docs and changelog for this protocol change"
        RequiredAgents = @("writer", "completion-finisher")
    },
    @{
        Name = "Architecture review"
        Prompt = "Assess architecture and ADR impacts for this migration"
        RequiredAgents = @("architect", "api-reviewer", "writer")
    },
    @{
        Name = "Feature planning"
        Prompt = "Do feature planning and define scope for this epic"
        RequiredAgents = @("product-manager", "planner")
    },
    @{
        Name = "TDD workflow"
        Prompt = "Apply tdd approach and add a regression test"
        RequiredAgents = @("test-engineer", "verifier")
    },
    @{
        Name = "UX review"
        Prompt = "Run a UX and accessibility review for onboarding"
        RequiredAgents = @("ux-researcher", "designer", "writer")
    },
    @{
        Name = "Python ML"
        Prompt = "Review this ML training notebook and inference flow"
        RequiredAgents = @("scientist", "test-engineer", "writer")
    },
    @{
        Name = "Rust issue"
        Prompt = "Rust E0502 borrow checker error in Cargo workspace"
        RequiredAgents = @("rust-skill-router")
    },
    @{
        Name = "Generic coding task"
        Prompt = "Implement this refactor and get it ready to commit"
        RequiredAgents = @("deep-executor", "verifier", "writer", "completion-finisher")
    }
)

$scenarioResults = New-Object System.Collections.Generic.List[object]

foreach ($scenario in $scenarios) {
    $matchedAgents = New-Object System.Collections.Generic.HashSet[string]

    foreach ($route in $routes) {
        $matcher = [string]$route.matcher
        $prompt = [string]$scenario.Prompt

        if ([regex]::IsMatch($prompt, $matcher)) {
            foreach ($agent in $route.agents) {
                [void]$matchedAgents.Add([string]$agent)
            }
        }
    }

    $missingRequired = New-Object System.Collections.Generic.List[string]
    foreach ($required in $scenario.RequiredAgents) {
        if (-not $matchedAgents.Contains($required)) {
            $missingRequired.Add($required)
        }
    }

    $scenarioResults.Add([pscustomobject]@{
        Name = [string]$scenario.Name
        Prompt = [string]$scenario.Prompt
        MatchedAgents = @($matchedAgents | Sort-Object)
        MissingRequired = @($missingRequired | Sort-Object)
        Passed = ($missingRequired.Count -eq 0)
    })
}

$allChecksPass = @($checkResults | Where-Object { -not $_.Passed }).Count -eq 0
$allScenariosPass = @($scenarioResults | Where-Object { -not $_.Passed }).Count -eq 0
$overallPass = $allChecksPass -and $allScenariosPass

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# Harness Smoke Report")
$lines.Add("")
$lines.Add("Generated: $(Get-Date -Format o)")
$lines.Add("")
$lines.Add("Overall: " + $(if ($overallPass) { "PASS" } else { "FAIL" }))
$lines.Add("")
$lines.Add("## Validator Results")
$lines.Add("")
$lines.Add("| Check | Status | Details |")
$lines.Add("| --- | --- | --- |")
foreach ($result in $checkResults) {
    $status = if ($result.Passed) { "PASS" } else { "FAIL" }
    $details = ([string]$result.Message).Replace("|", "\\|")
    $lines.Add("| $($result.Name) | $status | $details |")
}

$lines.Add("")
$lines.Add("## Prompt-to-Route Matrix")
$lines.Add("")
$lines.Add("| Scenario | Status | Missing Required Agents | Matched Agents |")
$lines.Add("| --- | --- | --- | --- |")
foreach ($result in $scenarioResults) {
    $status = if ($result.Passed) { "PASS" } else { "FAIL" }
    $missing = if ($result.MissingRequired.Count -eq 0) { "-" } else { ($result.MissingRequired -join ", ") }
    $matched = if ($result.MatchedAgents.Count -eq 0) { "-" } else { ($result.MatchedAgents -join ", ") }
    $lines.Add("| $($result.Name) | $status | $missing | $matched |")
}

$lines.Add("")
$lines.Add("## Scenario Prompts")
$lines.Add("")
foreach ($result in $scenarioResults) {
    $lines.Add(("- **{0}**: ``{1}``" -f $result.Name, $result.Prompt))
}

Set-Content -Path $resolvedReportPath -Value $lines -Encoding UTF8
Write-Host "Smoke report written to: $resolvedReportPath"

if (-not $overallPass) {
    throw "Harness smoke checks failed"
}
