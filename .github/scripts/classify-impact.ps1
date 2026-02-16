param(
    [string]$OutputJsonPath = ".github/automation/impact-output.json"
)

$ErrorActionPreference = 'Stop'

function Get-ChangedFiles {
    $eventName = $env:GITHUB_EVENT_NAME

    if ($eventName -like 'pull_request*' -and -not [string]::IsNullOrWhiteSpace($env:GITHUB_BASE_REF)) {
        $baseRef = $env:GITHUB_BASE_REF
        git fetch --no-tags --depth=1 origin $baseRef 2>$null | Out-Null
        $prDiff = git diff --name-only "origin/$baseRef...HEAD" 2>$null
        if ($LASTEXITCODE -eq 0 -and $prDiff) {
            return @($prDiff | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
        }
    }

    if ($eventName -eq 'push' -and -not [string]::IsNullOrWhiteSpace($env:GITHUB_EVENT_BEFORE) -and $env:GITHUB_EVENT_BEFORE -notmatch '^0+$') {
        $pushDiff = git diff --name-only $env:GITHUB_EVENT_BEFORE HEAD 2>$null
        if ($LASTEXITCODE -eq 0 -and $pushDiff) {
            return @($pushDiff | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
        }
    }

    git rev-parse --verify HEAD~1 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) {
        $fallback = git diff --name-only HEAD~1 HEAD 2>$null
        if ($LASTEXITCODE -eq 0 -and $fallback) {
            return @($fallback | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
        }
    }

    $singleCommit = git show --pretty='' --name-only HEAD 2>$null
    if ($LASTEXITCODE -eq 0 -and $singleCommit) {
        return @($singleCommit | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
    }

    return @()
}

function Has-PathPrefix {
    param(
        [string[]]$Files,
        [string[]]$Prefixes
    )

    foreach ($file in $Files) {
        foreach ($prefix in $Prefixes) {
            if ($file.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
                return $true
            }
        }
    }
    return $false
}

function Has-RegexMatch {
    param(
        [string[]]$Files,
        [string]$Pattern
    )

    return [bool]($Files | Where-Object { $_ -match $Pattern } | Select-Object -First 1)
}

$changedFiles = Get-ChangedFiles
$impactUnknown = $changedFiles.Count -eq 0

if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') {
    Write-Warning 'Could not determine changed files in CI; defaulting impact categories to true.'
}

$rust = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') { $true } else { Has-PathPrefix -Files $changedFiles -Prefixes @('crates/', 'Cargo.toml') }
if (-not $rust) {
    $rust = Has-RegexMatch -Files $changedFiles -Pattern '\.rs$'
}

$python = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') { $true } else { Has-PathPrefix -Files $changedFiles -Prefixes @('python/') }

$docs = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') { $true } else { Has-PathPrefix -Files $changedFiles -Prefixes @('docs/') }
if (-not $docs) {
    $docs = [bool]($changedFiles | Where-Object {
            $_ -in @('README.md', 'CHANGELOG.md', 'CONTRIBUTING.md')
        } | Select-Object -First 1)
}

$protocol = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') {
    $true
}
else {
    [bool]($changedFiles | Where-Object {
            $_ -like 'docs/runtime-ml-protocol-v2.md' -or
            $_ -like 'crates/neurohid-types/*' -or
            $_ -like 'crates/neurohid-ipc/*'
        } | Select-Object -First 1)
}

$architecture = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') {
    $true
}
else {
    [bool]($changedFiles | Where-Object {
            $_ -like 'docs/architecture/*' -or
            $_ -like 'crates/neurohid-core/*' -or
            $_ -like 'crates/neurohid-ipc/*' -or
            $_ -like 'crates/neurohid-storage/*' -or
            $_ -like '.github/workflows/architecture-gate.yml'
        } | Select-Object -First 1)
}

$automation = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') { $true } else { Has-PathPrefix -Files $changedFiles -Prefixes @('.github/') }

$unsafe = if ($impactUnknown -and $env:GITHUB_ACTIONS -eq 'true') { $true } else { $false }
$changedRustFiles = $changedFiles | Where-Object {
    $_ -like 'crates/*' -and $_ -match '\.rs$' -and (Test-Path $_)
}
foreach ($path in $changedRustFiles) {
    if (Select-String -Path $path -Pattern '\bunsafe\b' -Quiet) {
        $unsafe = $true
        break
    }
}

$requiredChecks = @(
    'Enforce PR-only main updates',
    'Determine Impact',
    'Focused Gates',
    'Test',
    'Clippy',
    'Format',
    'Documentation',
    'Python Tests',
    'Rust Coverage',
    'Governance Integrity',
    'TDD Evidence'
)

$result = [ordered]@{
    rust            = $rust
    python          = $python
    docs            = $docs
    protocol        = $protocol
    architecture    = $architecture
    unsafe          = $unsafe
    automation      = $automation
    impact_unknown  = $impactUnknown
    required_checks = ($requiredChecks -join ',')
    changed_files   = $changedFiles
}

$outputDir = Split-Path -Parent $OutputJsonPath
if (-not [string]::IsNullOrWhiteSpace($outputDir) -and -not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

$result | ConvertTo-Json -Depth 5 | Set-Content -Path $OutputJsonPath -Encoding UTF8

if ($env:GITHUB_OUTPUT) {
    @(
        "rust=$($rust.ToString().ToLowerInvariant())"
        "python=$($python.ToString().ToLowerInvariant())"
        "docs=$($docs.ToString().ToLowerInvariant())"
        "protocol=$($protocol.ToString().ToLowerInvariant())"
        "architecture=$($architecture.ToString().ToLowerInvariant())"
        "unsafe=$($unsafe.ToString().ToLowerInvariant())"
        "automation=$($automation.ToString().ToLowerInvariant())"
        "impact_unknown=$($impactUnknown.ToString().ToLowerInvariant())"
        "required_checks=$($requiredChecks -join ',')"
    ) | Add-Content -Path $env:GITHUB_OUTPUT
}

Write-Host "Impact classification written to $OutputJsonPath"
