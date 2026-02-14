param(
    [string]$HooksPath,
    [switch]$SkipContractReferenceChecks
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$resolvedHooksPath = if ([string]::IsNullOrWhiteSpace($HooksPath)) {
    Join-Path $repoRoot ".github\hooks\hooks.json"
} else {
    if ([System.IO.Path]::IsPathRooted($HooksPath)) {
        $HooksPath
    } else {
        Join-Path $repoRoot $HooksPath
    }
}
$playbookPath = Join-Path $repoRoot "docs\automation\agent-skill-invocation-playbook.md"
$topAgentsPath = Join-Path $repoRoot "AGENTS.md"
$triggersPath = Join-Path $repoRoot ".github\hooks\TRIGGERS.md"
$workflowContractPath = Join-Path $repoRoot "_bmad\neurohid\workflows\neurohid-phase-workflow\workflow.md"

$knownAgentIds = @(
    "writer",
    "completion-finisher",
    "rust-skill-router",
    "architect",
    "api-reviewer",
    "product-manager",
    "planner",
    "test-engineer",
    "verifier",
    "ux-researcher",
    "designer",
    "scientist",
    "deep-executor"
)

if (-not (Test-Path $resolvedHooksPath)) {
    throw "Missing hooks manifest: $resolvedHooksPath"
}

$hooks = Get-Content -Raw -Path $resolvedHooksPath | ConvertFrom-Json
$routes = @($hooks.hooks.UserPromptSubmit)

if ($routes.Count -eq 0) {
    throw "hooks.json has no UserPromptSubmit routes"
}

$unknownAgentIds = New-Object System.Collections.Generic.List[string]
$duplicateMatchers = New-Object System.Collections.Generic.List[string]
$duplicateAgentsInRoute = New-Object System.Collections.Generic.List[string]
$seenMatchers = @{}
$hookAgentRefs = New-Object System.Collections.Generic.HashSet[string]
$catchAllMatcher = "(?s).+"
$catchAllIndexes = New-Object System.Collections.Generic.List[int]

for ($index = 0; $index -lt $routes.Count; $index++) {
    $route = $routes[$index]
    $matcher = [string]$route.matcher
    if ($matcher -eq $catchAllMatcher) {
        $catchAllIndexes.Add($index)
    }

    if ($seenMatchers.ContainsKey($matcher)) {
        $duplicateMatchers.Add($matcher)
    } else {
        $seenMatchers[$matcher] = $true
    }

    $agentsInRoute = New-Object System.Collections.Generic.HashSet[string]
    foreach ($agentRef in $route.agents) {
        $relativeAgent = [string]$agentRef
        if ($agentsInRoute.Contains($relativeAgent)) {
            $duplicateAgentsInRoute.Add("$matcher -> $relativeAgent")
        } else {
            [void]$agentsInRoute.Add($relativeAgent)
        }

        [void]$hookAgentRefs.Add($relativeAgent)

        if (-not ($knownAgentIds -contains $relativeAgent)) {
            $unknownAgentIds.Add($relativeAgent)
        }
    }
}

$catchAllPolicyErrors = New-Object System.Collections.Generic.List[string]
if ($catchAllIndexes.Count -eq 0) {
    $catchAllPolicyErrors.Add("Missing required catch-all matcher: $catchAllMatcher")
} elseif ($catchAllIndexes.Count -gt 1) {
    $catchAllPolicyErrors.Add("Catch-all matcher must appear once, found $($catchAllIndexes.Count)")
} else {
    $catchAllIndex = $catchAllIndexes[0]
    if ($catchAllIndex -ne ($routes.Count - 1)) {
        $catchAllPolicyErrors.Add("Catch-all matcher must be last route")
    }
}

$contentFiles = @($playbookPath, $topAgentsPath, $triggersPath, $workflowContractPath)
$requiredContractTokens = @(
    "writer",
    "completion-finisher",
    "rust-skill-router",
    "architect",
    "api-reviewer",
    "product-manager",
    "planner",
    "test-engineer",
    "verifier",
    "ux-researcher",
    "designer",
    "scientist"
)

if (-not $SkipContractReferenceChecks) {
    $combinedContent = ""
    foreach ($file in $contentFiles) {
        if (-not (Test-Path $file)) {
            throw "Missing contract file: $file"
        }

        $combinedContent += [string](Get-Content -Raw -Path $file)
        $combinedContent += "`n"
    }

    $missingContractTokens = New-Object System.Collections.Generic.List[string]
    foreach ($token in $requiredContractTokens) {
        if (-not $combinedContent.Contains($token)) {
            $missingContractTokens.Add($token)
        }
    }

    $undocumentedHookAgents = New-Object System.Collections.Generic.List[string]
    foreach ($agentRef in $hookAgentRefs) {
        if (-not $combinedContent.Contains($agentRef)) {
            $undocumentedHookAgents.Add($agentRef)
        }
    }
}

$failed = $false

if ($unknownAgentIds.Count -gt 0) {
    $failed = $true
    Write-Host "Unknown BMAD agent IDs in hook routes:"
    $unknownAgentIds | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($duplicateMatchers.Count -gt 0) {
    $failed = $true
    Write-Host "Duplicate matchers in hooks.json:"
    $duplicateMatchers | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($duplicateAgentsInRoute.Count -gt 0) {
    $failed = $true
    Write-Host "Duplicate agents within the same hook route:"
    $duplicateAgentsInRoute | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($catchAllPolicyErrors.Count -gt 0) {
    $failed = $true
    Write-Host "Catch-all route policy violations:"
    $catchAllPolicyErrors | ForEach-Object { Write-Host "  - $_" }
}

if ((-not $SkipContractReferenceChecks) -and ($missingContractTokens.Count -gt 0)) {
    $failed = $true
    Write-Host "Routing vocabulary drift detected in contracts:"
    $missingContractTokens | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ((-not $SkipContractReferenceChecks) -and ($undocumentedHookAgents.Count -gt 0)) {
    $failed = $true
    Write-Host "Hook-routed BMAD agents missing from documentation contracts:"
    $undocumentedHookAgents | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($failed) {
    throw "Routing integrity validation failed"
}

Write-Host "Routing integrity checks passed."