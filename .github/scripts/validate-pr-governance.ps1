param(
    [string]$ManifestPath = ".github/automation/policy-manifest.json",
    [string]$Repo = $env:GITHUB_REPOSITORY,
    [int]$PrNumber = 0,
    [string]$PrBody = $env:PR_BODY,
    [string]$Token = $env:GITHUB_TOKEN,
    [string]$PrLabels = $env:PR_LABELS
)

$ErrorActionPreference = 'Stop'

if ($PrNumber -le 0) {
    $parsedPrNumber = 0
    if ([int]::TryParse($env:PR_NUMBER, [ref]$parsedPrNumber)) {
        $PrNumber = $parsedPrNumber
    }
}

if ([string]::IsNullOrWhiteSpace($ManifestPath) -or -not (Test-Path $ManifestPath)) {
    throw "Policy manifest not found: $ManifestPath"
}

if ([string]::IsNullOrWhiteSpace($Repo)) {
    throw 'GITHUB_REPOSITORY is required.'
}

if ($PrNumber -le 0) {
    throw 'PR_NUMBER is required and must be > 0.'
}

if ([string]::IsNullOrWhiteSpace($Token)) {
    throw 'GITHUB_TOKEN is required for PR file inspection.'
}

if ([string]::IsNullOrWhiteSpace($PrBody)) {
    throw 'PR body is empty. Complete the PR template fields for governance checks.'
}

$manifest = Get-Content -Path $ManifestPath -Raw | ConvertFrom-Json -Depth 10
$minSectionChars = [int]$manifest.tdd_policy.minimum_section_characters
$applyNoTestImpactLabel = $false

function Get-SectionContent {
    param(
        [string]$Body,
        [string]$Heading
    )

    $pattern = "(?ms)^###\s+$([regex]::Escape($Heading))\s*\r?\n(.*?)(?=^###\s+|^##\s+|\z)"
    $match = [regex]::Match($Body, $pattern)
    if (-not $match.Success) {
        return $null
    }

    return $match.Groups[1].Value.Trim()
}

function Test-SectionHasSubstance {
    param(
        [string]$Content,
        [int]$MinChars
    )

    if ([string]::IsNullOrWhiteSpace($Content)) {
        return $false
    }

    $normalized = ($Content -replace '\r|\n', ' ' -replace '\s+', ' ').Trim()
    if ($normalized.Length -lt $MinChars) {
        return $false
    }

    if ($normalized -match '^(?:-|_|`)+$') {
        return $false
    }

    if ($normalized -match '(?i)<required>|todo|tbd|n/a') {
        return $false
    }

    return $true
}

function Get-PullRequestChangedFiles {
    param(
        [string]$Repository,
        [int]$Number,
        [string]$AccessToken
    )

    $headers = @{
        Authorization          = "Bearer $AccessToken"
        Accept                 = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
    }

    $files = @()
    $page = 1
    do {
        $url = "https://api.github.com/repos/$Repository/pulls/$Number/files?per_page=100&page=$page"
        $response = Invoke-RestMethod -Uri $url -Headers $headers -Method Get
        $batch = @($response)
        foreach ($item in $batch) {
            if ($item.filename) {
                $files += [string]$item.filename
            }
        }
        $page += 1
    } while ($batch.Count -eq 100)

    return $files | Select-Object -Unique
}

foreach ($heading in $manifest.tdd_policy.required_pr_sections) {
    $content = Get-SectionContent -Body $PrBody -Heading $heading
    if (-not (Test-SectionHasSubstance -Content $content -MinChars $minSectionChars)) {
        throw "PR section '### $heading' is missing or lacks actionable evidence."
    }
}

$changedFiles = Get-PullRequestChangedFiles -Repository $Repo -Number $PrNumber -AccessToken $Token

$productionPatterns = @($manifest.tdd_policy.production_path_patterns)
$testPatterns = @($manifest.tdd_policy.test_path_patterns)

$hasProductionChanges = $false
foreach ($pattern in $productionPatterns) {
    if ($changedFiles | Where-Object { $_ -match $pattern } | Select-Object -First 1) {
        $hasProductionChanges = $true
        break
    }
}

$hasTestChanges = $false
foreach ($pattern in $testPatterns) {
    if ($changedFiles | Where-Object { $_ -match $pattern } | Select-Object -First 1) {
        $hasTestChanges = $true
        break
    }
}

if ($hasProductionChanges -and -not $hasTestChanges) {
    $requiredLabel = [string]$manifest.tdd_policy.no_test_impact.label
    $labelList = @()
    if (-not [string]::IsNullOrWhiteSpace($PrLabels)) {
        $labelList = $PrLabels.Split(',').ForEach({ $_.Trim() }) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    }
    $hasLabelOverride = [bool]($labelList | Where-Object { $_ -eq $requiredLabel } | Select-Object -First 1)

    $rationaleHeading = [string]$manifest.tdd_policy.no_test_impact.section
    $rationaleMin = [int]$manifest.tdd_policy.no_test_impact.minimum_characters
    $rationaleContent = Get-SectionContent -Body $PrBody -Heading $rationaleHeading
    $hasRationaleOverride = Test-SectionHasSubstance -Content $rationaleContent -MinChars $rationaleMin

    if ($hasRationaleOverride -and -not $hasLabelOverride) {
        $applyNoTestImpactLabel = $true
        Write-Warning "Auto-label hint enabled: apply '$requiredLabel' to align PR metadata with rationale."
    }

    if (-not $hasLabelOverride -and -not $hasRationaleOverride) {
        throw "Production code changed without test-file updates. Add tests or provide a substantive '### $rationaleHeading' section and apply label '$requiredLabel'."
    }
}

if ($env:GITHUB_OUTPUT) {
    "apply_no_test_impact_label=$($applyNoTestImpactLabel.ToString().ToLowerInvariant())" | Add-Content -Path $env:GITHUB_OUTPUT
}

Write-Host 'PR governance checks passed (TDD evidence and code/test consistency).'