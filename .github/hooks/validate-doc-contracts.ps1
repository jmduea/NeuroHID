Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")

$contractFiles = @(
    "AGENTS.md",
    ".github/hooks/TRIGGERS.md",
    "docs/automation/agent-skill-invocation-playbook.md",
    "_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md",
    ".github/automation/scope-map.json",
    "docs/architecture/index.md"
)

$canonicalSources = @(
    "https://doc.rust-lang.org/book/",
    "https://doc.rust-lang.org/stable/reference/",
    "https://doc.rust-lang.org/stable/cargo/",
    "https://effective-rust.com/"
)

$requiredAgentTokens = @(
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

$missingFiles = New-Object System.Collections.Generic.List[string]
$bareUrlViolations = New-Object System.Collections.Generic.List[string]
$missingTokens = New-Object System.Collections.Generic.List[string]

$loaded = @{}
foreach ($relativePath in $contractFiles) {
    $fullPath = Join-Path $repoRoot ($relativePath -replace '/', '\\')
    if (-not (Test-Path $fullPath)) {
        $missingFiles.Add($relativePath)
        continue
    }

    $content = Get-Content -Raw -Path $fullPath
    $loaded[$relativePath] = $content

    foreach ($source in $canonicalSources) {
        $escaped = [regex]::Escape($source)
        $barePattern = "(?<!<)$escaped(?!>)"
        if ([regex]::IsMatch($content, $barePattern)) {
            $bareUrlViolations.Add("$relativePath -> $source")
        }
    }
}

if ($missingFiles.Count -eq 0) {
    $coverageFiles = @(
        "AGENTS.md",
        ".github/hooks/TRIGGERS.md",
        "docs/automation/agent-skill-invocation-playbook.md",
        "_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md"
    )

    $combinedContent = ""
    foreach ($file in $coverageFiles) {
        $combinedContent += [string]$loaded[$file]
        $combinedContent += "`n"
    }

    foreach ($token in $requiredAgentTokens) {
        if (-not $combinedContent.Contains($token)) {
            $missingTokens.Add($token)
        }
    }
}

$failed = $false

if ($missingFiles.Count -gt 0) {
    $failed = $true
    Write-Host "Missing documentation contract files:"
    $missingFiles | ForEach-Object { Write-Host "  - $_" }
}

if ($bareUrlViolations.Count -gt 0) {
    $failed = $true
    Write-Host "Bare canonical URLs found (must use angle bracket links):"
    $bareUrlViolations | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($missingTokens.Count -gt 0) {
    $failed = $true
    Write-Host "Routing vocabulary drift detected (missing agent references in combined contracts):"
    $missingTokens | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
}

if ($failed) {
    throw "Documentation contract validation failed"
}

Write-Host "Documentation contract checks passed."
