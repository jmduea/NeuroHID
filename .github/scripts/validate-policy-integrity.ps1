param(
    [string]$ManifestPath = ".github/automation/policy-manifest.json"
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $ManifestPath)) {
    throw "Policy manifest not found: $ManifestPath"
}

$manifest = Get-Content -Path $ManifestPath -Raw | ConvertFrom-Json -Depth 10

function Assert-FileExists {
    param(
        [string]$Path,
        [string]$Message
    )

    if (-not (Test-Path $Path)) {
        throw "$Message Missing file: $Path"
    }
}

function Assert-WorkflowHasJob {
    param(
        [string]$WorkflowPath,
        [string]$JobId,
        [string]$JobName
    )

    $workflowContent = Get-Content -Path $WorkflowPath -Raw
    if ($workflowContent -notmatch "(?m)^\s*$([regex]::Escape($JobId)):\s*$") {
        throw "Workflow '$WorkflowPath' is missing job id '$JobId'."
    }

    if ($workflowContent -notmatch "(?m)^\s*name:\s*$([regex]::Escape($JobName))\s*$") {
        throw "Workflow '$WorkflowPath' is missing expected job name '$JobName'."
    }
}

function Get-UnreleasedSection {
    param(
        [string]$ChangelogContent
    )

    $match = [regex]::Match($ChangelogContent, '(?ms)^##\s+\[Unreleased\]\s*(.*?)(?=^##\s+\[[^\]]+\]|\z)')
    if (-not $match.Success) {
        throw "CHANGELOG.md is missing '## [Unreleased]' section."
    }

    return $match.Groups[1].Value
}

function Test-LocalReferenceCandidate {
    param(
        [string]$Reference
    )

    if ([string]::IsNullOrWhiteSpace($Reference)) {
        return $false
    }

    $ref = $Reference.Trim()
    if ($ref.StartsWith('http://') -or $ref.StartsWith('https://') -or $ref.StartsWith('mailto:') -or $ref.StartsWith('#')) {
        return $false
    }

    if ($ref.Contains('*') -or $ref.Contains('{') -or $ref.Contains('}') -or $ref.Contains('$(')) {
        return $false
    }

    if ($ref -match '\s') {
        return $false
    }

    if ($ref -match '^[A-Za-z]:\\') {
        return $false
    }

    if ($ref -match '[\\/]' -or $ref -match '\.(md|ya?ml|json|ps1|sh|toml|rs|py)$') {
        return $true
    }

    return $false
}

function Normalize-LocalReferencePath {
    param(
        [string]$Reference
    )

    $normalized = $Reference.Trim()
    $normalized = $normalized -replace '\\', '/'
    $normalized = $normalized -replace '^\./', ''
    $normalized = $normalized.TrimStart('/')

    $anchorIndex = $normalized.IndexOf('#')
    if ($anchorIndex -ge 0) {
        $normalized = $normalized.Substring(0, $anchorIndex)
    }

    $queryIndex = $normalized.IndexOf('?')
    if ($queryIndex -ge 0) {
        $normalized = $normalized.Substring(0, $queryIndex)
    }

    return $normalized
}

function Get-ChangelogAddedLines {
    param(
        [string]$Path
    )

    $diffOutput = $null
    $eventName = $env:GITHUB_EVENT_NAME

    if ($eventName -like 'pull_request*' -and -not [string]::IsNullOrWhiteSpace($env:GITHUB_BASE_REF)) {
        $baseRef = $env:GITHUB_BASE_REF
        git fetch --no-tags --depth=1 origin $baseRef 2>$null | Out-Null
        $diffOutput = git diff --unified=0 "origin/$baseRef...HEAD" -- $Path 2>$null
    }
    elseif ($eventName -eq 'push' -and -not [string]::IsNullOrWhiteSpace($env:GITHUB_EVENT_BEFORE) -and $env:GITHUB_EVENT_BEFORE -notmatch '^0+$') {
        $diffOutput = git diff --unified=0 $env:GITHUB_EVENT_BEFORE HEAD -- $Path 2>$null
    }
    else {
        git rev-parse --verify HEAD~1 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
            $diffOutput = git diff --unified=0 HEAD~1 HEAD -- $Path 2>$null
        }
    }

    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($diffOutput)) {
        return @()
    }

    $added = @()
    foreach ($line in ($diffOutput -split "`r?`n")) {
        if ($line.StartsWith('+++') -or $line.StartsWith('@@')) {
            continue
        }

        if ($line.StartsWith('+')) {
            $added += $line.Substring(1)
        }
    }

    return $added
}

function Assert-ChangelogUnreleasedReferencesExist {
    param(
        [string]$ChangelogPath
    )

    Assert-FileExists -Path $ChangelogPath -Message 'Changelog is missing.'

    $changelogContent = Get-Content -Path $ChangelogPath -Raw
    $unreleased = Get-UnreleasedSection -ChangelogContent $changelogContent
    $addedLines = Get-ChangelogAddedLines -Path $ChangelogPath

    $scopeText = $unreleased
    if ($addedLines.Count -gt 0) {
        $scopeText = ($addedLines -join "`n")
    }

    $candidates = New-Object System.Collections.Generic.HashSet[string]

    $codeMatches = [regex]::Matches($scopeText, '`([^`]+)`')
    foreach ($match in $codeMatches) {
        $raw = [string]$match.Groups[1].Value
        if (Test-LocalReferenceCandidate -Reference $raw) {
            [void]$candidates.Add((Normalize-LocalReferencePath -Reference $raw))
        }
    }

    $linkMatches = [regex]::Matches($scopeText, '\]\(([^)]+)\)')
    foreach ($match in $linkMatches) {
        $raw = [string]$match.Groups[1].Value.Trim()
        $target = ($raw -split '\s+', 2)[0]
        if (Test-LocalReferenceCandidate -Reference $target) {
            [void]$candidates.Add((Normalize-LocalReferencePath -Reference $target))
        }
    }

    function Test-ReferencePathExists {
        param(
            [string]$RefPath
        )

        if (Test-Path $RefPath) {
            return $true
        }

        if ($RefPath -notmatch '[/\\]') {
            $aliases = @(
                ".github/workflows/$RefPath",
                ".github/scripts/$RefPath",
                ".github/hooks/$RefPath",
                ".github/automation/$RefPath",
                "docs/$RefPath"
            )

            foreach ($alias in $aliases) {
                if (Test-Path $alias) {
                    return $true
                }
            }
        }

        return $false
    }

    $missing = @()
    foreach ($path in $candidates) {
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }

        if (-not (Test-ReferencePathExists -RefPath $path)) {
            $missing += $path
        }
    }

    if ($missing.Count -gt 0) {
        $list = ($missing | Sort-Object -Unique) -join ', '
        if ($addedLines.Count -gt 0) {
            throw "CHANGELOG.md added Unreleased references missing paths: $list"
        }

        throw "CHANGELOG.md Unreleased references missing paths: $list"
    }
}

foreach ($workflow in $manifest.required_workflows) {
    Assert-FileExists -Path $workflow -Message 'Required workflow file is missing.'
}

foreach ($check in $manifest.branch_policy.required_status_checks.baseline) {
    Assert-FileExists -Path $check.workflow -Message 'Required status check workflow is missing.'
    Assert-WorkflowHasJob -WorkflowPath $check.workflow -JobId $check.job -JobName $check.name
}

foreach ($check in $manifest.branch_policy.required_status_checks.conditional) {
    Assert-FileExists -Path $check.workflow -Message 'Conditional status check workflow is missing.'
    Assert-WorkflowHasJob -WorkflowPath $check.workflow -JobId $check.job -JobName $check.name
}

$developmentGuidePath = 'docs/development-guide.md'
Assert-FileExists -Path $developmentGuidePath -Message 'Development guide is missing.'
$developmentGuide = Get-Content -Path $developmentGuidePath -Raw
foreach ($check in $manifest.branch_policy.required_status_checks.baseline) {
    if ($developmentGuide -notmatch [regex]::Escape($check.name)) {
        throw "docs/development-guide.md is missing required status check '$($check.name)'."
    }
}

foreach ($assertion in $manifest.doc_path_assertions) {
    Assert-FileExists -Path $assertion.source -Message 'Doc assertion source file is missing.'
    $sourceContent = Get-Content -Path $assertion.source -Raw
    if ($sourceContent -match [regex]::Escape($assertion.reference)) {
        Assert-FileExists -Path $assertion.reference -Message "Doc reference in '$($assertion.source)' points to missing path."
    }
}

foreach ($claim in $manifest.forbidden_doc_claims) {
    Assert-FileExists -Path $claim.source -Message 'Forbidden-claim source file is missing.'
    $sourceContent = Get-Content -Path $claim.source -Raw
    if ($sourceContent -match $claim.pattern) {
        throw "Forbidden stale claim detected in '$($claim.source)': pattern '$($claim.pattern)'. $($claim.description)"
    }
}

Assert-ChangelogUnreleasedReferencesExist -ChangelogPath 'CHANGELOG.md'

Write-Host "Policy integrity validation passed for $ManifestPath"
$global:LASTEXITCODE = 0
exit 0