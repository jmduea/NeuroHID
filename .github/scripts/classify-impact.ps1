param(
    [string[]]$ChangedFiles,
    [string]$BaseRef = "origin/main",
    [string]$OutputJsonPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-ChangedFilesFromGit {
    param([string]$Base)

    $hasOriginMain = $false
    try {
        git rev-parse --verify $Base *> $null
        if ($LASTEXITCODE -eq 0) {
            $hasOriginMain = $true
        }
    } catch {
        $hasOriginMain = $false
    }

    if ($hasOriginMain) {
        $files = git diff --name-only "$Base...HEAD"
    } else {
        $files = git diff-tree --no-commit-id --name-only -r HEAD
    }

    if (-not $files) {
        return @()
    }
    return @($files | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
}

function MatchesAny {
    param(
        [string]$Path,
        [string[]]$Regexes
    )

    foreach ($regex in $Regexes) {
        if ($Path -match $regex) {
            return $true
        }
    }

    return $false
}

if (-not $ChangedFiles -or $ChangedFiles.Count -eq 0) {
    $ChangedFiles = Get-ChangedFilesFromGit -Base $BaseRef
}

$normalized = @($ChangedFiles | ForEach-Object { $_.Replace('\\', '/') } | Sort-Object -Unique)

$impactFlags = [ordered]@{
    any = ($normalized.Count -gt 0)
    rust = $false
    python = $false
    docs = $false
    protocol = $false
    architecture = $false
    unsafe = $false
    automation = $false
    notebooks = $false
}

$matchers = @{
    rust = @('^crates/.+\.rs$', '^Cargo\.toml$', '^crates/.+/Cargo\.toml$')
    python = @('^python/.+\.py$', '^python/pyproject\.toml$')
    docs = @('^docs/', '^README\.md$', '^CHANGELOG\.md$', '^python/README\.md$', '^python/notebooks/README\.md$')
    protocol = @('^docs/runtime-ml-protocol-v2\.md$', '^docs/lab-kernel-protocol\.md$', '^crates/neurohid-(ipc|types|core)/')
    architecture = @('^crates/neurohid-(ipc|storage|core)/', '^docs/SPECIFICATION\.md$', '^docs/adr/')
    unsafe = @('^crates/.+\.rs$', '^third_party/.+\.rs$')
    automation = @('^\.github/', '^AGENTS\.md$', '^docs/automation/')
    notebooks = @('^python/notebooks/')
}

$unsafeFileCandidates = New-Object System.Collections.Generic.List[string]

foreach ($path in $normalized) {
    foreach ($flag in @('rust', 'python', 'docs', 'protocol', 'architecture', 'automation', 'notebooks')) {
        if (-not $impactFlags[$flag] -and (MatchesAny -Path $path -Regexes $matchers[$flag])) {
            $impactFlags[$flag] = $true
        }
    }

    if (MatchesAny -Path $path -Regexes $matchers.unsafe) {
        $unsafeFileCandidates.Add($path)
    }
}

if ($unsafeFileCandidates.Count -gt 0) {
    foreach ($path in $unsafeFileCandidates) {
        if (Test-Path $path) {
            $content = Get-Content -Raw -Path $path
            if ($content -match '(?m)\bunsafe\s*\{') {
                $impactFlags.unsafe = $true
                break
            }
        }
    }
}

$requiredChecks = New-Object System.Collections.Generic.HashSet[string]
if ($impactFlags.rust) {
    [void]$requiredChecks.Add('rust-ci')
}
if ($impactFlags.python -or $impactFlags.notebooks) {
    [void]$requiredChecks.Add('python-quality')
}
if ($impactFlags.docs -or $impactFlags.protocol -or $impactFlags.rust -or $impactFlags.python) {
    [void]$requiredChecks.Add('docs-freshness')
}
if ($impactFlags.protocol) {
    [void]$requiredChecks.Add('protocol-contracts')
}
if ($impactFlags.architecture) {
    [void]$requiredChecks.Add('architecture-gate')
}
if ($impactFlags.unsafe) {
    [void]$requiredChecks.Add('unsafe-compliance')
}
if ($impactFlags.automation) {
    [void]$requiredChecks.Add('agent-routing-integrity')
}

$result = [ordered]@{
    changed_files = $normalized
    flags = $impactFlags
    required_checks = @($requiredChecks | Sort-Object)
}

$json = $result | ConvertTo-Json -Depth 6
if (-not [string]::IsNullOrWhiteSpace($OutputJsonPath)) {
    Set-Content -Path $OutputJsonPath -Value $json -Encoding UTF8
}

if ($env:GITHUB_OUTPUT) {
    "impact_json=$($json -replace "`n", '')" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    foreach ($flag in $impactFlags.Keys) {
        "$flag=$($impactFlags[$flag])" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    }
    "required_checks=$((@($requiredChecks | Sort-Object) -join ','))" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}

$json
