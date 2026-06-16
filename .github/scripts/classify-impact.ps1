param(
    [string]$OutputJsonPath = ".github/automation/impact-output.json"
)

$ErrorActionPreference = 'Stop'

function Get-ChangedFiles {
    $fallback = git diff --name-only HEAD~1 HEAD 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $fallback) {
        return @()
    }
    return @($fallback | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
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

$rust = Has-PathPrefix -Files $changedFiles -Prefixes @('crates/', 'apps/', 'Cargo.toml')
if (-not $rust) {
    $rust = Has-RegexMatch -Files $changedFiles -Pattern '\.rs$'
}

$python = Has-PathPrefix -Files $changedFiles -Prefixes @('python/')

$docs = Has-PathPrefix -Files $changedFiles -Prefixes @('docs/')
if (-not $docs) {
    $docs = [bool]($changedFiles | Where-Object {
            $_ -in @('README.md', 'CHANGELOG.md', 'CONTRIBUTING.md')
        } | Select-Object -First 1)
}

$protocol = [bool]($changedFiles | Where-Object {
        $_ -like 'docs/protocol-and-api.md' -or
        $_ -like 'crates/neurohid-types/*' -or
        $_ -like 'crates/neurohid-ipc/*'
    } | Select-Object -First 1)

$architecture = [bool]($changedFiles | Where-Object {
        $_ -like 'docs/architecture/*' -or
        $_ -like 'crates/neurohid-core/*' -or
        $_ -like 'crates/neurohid-ipc/*' -or
        $_ -like 'crates/neurohid-storage/*' -or
        $_ -like '.github/workflows/architecture-gate.yml'
    } | Select-Object -First 1)

$automation = Has-PathPrefix -Files $changedFiles -Prefixes @('.github/')

$unsafe = $false
$changedRustFiles = $changedFiles | Where-Object {
    ($_ -like 'crates/*' -or $_ -like 'apps/*') -and $_ -match '\.rs$' -and (Test-Path $_)
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
    'Rust Coverage'
)

$result = [ordered]@{
    rust            = $rust
    python          = $python
    docs            = $docs
    protocol        = $protocol
    architecture    = $architecture
    unsafe          = $unsafe
    automation      = $automation
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
        "required_checks=$($requiredChecks -join ',')"
    ) | Add-Content -Path $env:GITHUB_OUTPUT
}

Write-Host "Impact classification written to $OutputJsonPath"

exit 0
