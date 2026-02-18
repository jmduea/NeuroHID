param()

$ErrorActionPreference = 'Stop'

function Assert-Command {
    param(
        [string]$Name,
        [string]$InstallHint
    )

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' is not available. $InstallHint"
    }
}

function Assert-PathExists {
    param(
        [string]$Path,
        [string]$Message
    )

    if (-not (Test-Path $Path)) {
        throw "$Message Missing path: $Path"
    }
}

Assert-Command -Name 'git' -InstallHint 'Install Git and ensure it is on PATH.'
Assert-Command -Name 'pwsh' -InstallHint 'Install PowerShell 7+ and ensure it is on PATH.'
Assert-Command -Name 'rtk' -InstallHint 'Install RTK to match command governance policy.'

$bashCommand = Get-Command bash -ErrorAction SilentlyContinue
if ($bashCommand) {
    if (-not (Get-Command jq -ErrorAction SilentlyContinue)) {
        Write-Warning 'jq is not available; bash hook fallback rewrite is disabled. PowerShell hook rewrite remains active.'
    }
}

Assert-PathExists -Path '.github/hooks/hooks.json' -Message 'Hook configuration not found.'
Assert-PathExists -Path '.github/hooks/rtk-rewrite.ps1' -Message 'PowerShell hook rewriter not found.'
Assert-PathExists -Path '.github/automation/policy-manifest.json' -Message 'Policy manifest not found.'
Assert-PathExists -Path '.github/scripts/pre-push-governance-checks.ps1' -Message 'Pre-push governance helper not found.'

$branch = (git rev-parse --abbrev-ref HEAD 2>$null).Trim()
if ([string]::IsNullOrWhiteSpace($branch)) {
    throw 'Unable to determine current branch.'
}

if ($branch -eq 'main') {
    throw 'Current branch is main. Create a feature branch before implementation work.'
}

Write-Host 'Governance setup verification passed.'
Write-Host "Current branch: $branch"