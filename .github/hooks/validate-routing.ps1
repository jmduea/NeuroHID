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
$workflowContractPath = Join-Path $repoRoot ".github\agents\_shared\multi-agent-phase-workflow.md"

if (-not (Test-Path $resolvedHooksPath)) {
    throw "Missing hooks manifest: $resolvedHooksPath"
}

$hooks = Get-Content -Raw -Path $resolvedHooksPath | ConvertFrom-Json
$routes = @($hooks.hooks.UserPromptSubmit)

if ($routes.Count -eq 0) {
    throw "hooks.json has no UserPromptSubmit routes"
}

$missingAgentTargets = New-Object System.Collections.Generic.List[string]
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

        $normalized = $relativeAgent -replace '^\.github/', '.github\\' -replace '/', '\\'
        $target = Join-Path $repoRoot $normalized
        if (-not (Test-Path $target)) {
            $missingAgentTargets.Add($relativeAgent)
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
$referencedAgents = New-Object System.Collections.Generic.HashSet[string]
$agentRefPattern = '\.github/agents/[A-Za-z0-9._\-]+\.md'

if (-not $SkipContractReferenceChecks) {
    foreach ($file in $contentFiles) {
        if (-not (Test-Path $file)) {
            throw "Missing contract file: $file"
        }

        $content = Get-Content -Raw -Path $file
        $matches = [regex]::Matches($content, $agentRefPattern)
        foreach ($match in $matches) {
            [void]$referencedAgents.Add($match.Value)
        }
    }
}

$missingInContracts = New-Object System.Collections.Generic.List[string]
if (-not $SkipContractReferenceChecks) {
    foreach ($agentRef in $referencedAgents) {
        $normalized = $agentRef -replace '^\.github/', '.github\\' -replace '/', '\\'
        $target = Join-Path $repoRoot $normalized
        if (-not (Test-Path $target)) {
            $missingInContracts.Add($agentRef)
        }
    }
}

$undocumentedHookAgents = New-Object System.Collections.Generic.List[string]
if (-not $SkipContractReferenceChecks) {
    foreach ($agentRef in $hookAgentRefs) {
        if (-not $referencedAgents.Contains($agentRef)) {
            $undocumentedHookAgents.Add($agentRef)
        }
    }
}

$failed = $false

if ($missingAgentTargets.Count -gt 0) {
    $failed = $true
    Write-Host "Missing hook route targets:"
    $missingAgentTargets | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
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

if ($missingInContracts.Count -gt 0) {
    $failed = $true
    Write-Host "Missing agent files referenced by AGENTS.md/playbook:"
    $missingInContracts | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($undocumentedHookAgents.Count -gt 0) {
    $failed = $true
    Write-Host "Hook-routed agents missing from documentation contracts:"
    $undocumentedHookAgents | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($failed) {
    throw "Routing integrity validation failed"
}

Write-Host "Routing integrity checks passed."