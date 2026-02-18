param()

$ErrorActionPreference = 'Stop'

function New-TempTestRoot {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("policy-integrity-test-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    return $root
}

function Write-File {
    param(
        [string]$Path,
        [string]$Content
    )

    $directory = Split-Path -Parent $Path
    if (-not (Test-Path $directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    Set-Content -Path $Path -Value $Content -Encoding UTF8
}

function Initialize-Fixture {
    param(
        [string]$Root,
        [string]$ValidatorSourcePath
    )

    $validatorDestination = Join-Path $Root '.github/scripts/validate-policy-integrity.ps1'
    $validatorDirectory = Split-Path -Parent $validatorDestination
    if (-not (Test-Path $validatorDirectory)) {
        New-Item -ItemType Directory -Path $validatorDirectory -Force | Out-Null
    }

    Copy-Item -Path $ValidatorSourcePath -Destination $validatorDestination -Force

    Write-File -Path (Join-Path $Root '.github/workflows/ci.yml') -Content @"
name: CI
jobs:
  test-job:
    name: Test Check
    runs-on: ubuntu-latest
    steps:
      - run: echo ok
"@

    Write-File -Path (Join-Path $Root 'docs/development-guide.md') -Content @"
# Development Guide

- Test Check
"@

    Write-File -Path (Join-Path $Root 'docs/automation/checklist.md') -Content "# Checklist"

    Write-File -Path (Join-Path $Root 'CONTRIBUTING.md') -Content @"
# Contributing

See docs/automation/checklist.md.
"@

    Write-File -Path (Join-Path $Root 'CHANGELOG.md') -Content @'
# Changelog

## [Unreleased]

- Added validator reference `.github/workflows/ci.yml`.

## [0.1.0]
'@

    Write-File -Path (Join-Path $Root '.github/automation/policy-manifest.json') -Content @"
{
  "version": 1,
  "required_workflows": [
    ".github/workflows/ci.yml"
  ],
  "branch_policy": {
    "required_status_checks": {
      "baseline": [
        {
          "name": "Test Check",
          "workflow": ".github/workflows/ci.yml",
          "job": "test-job"
        }
      ],
      "conditional": []
    }
  },
  "doc_path_assertions": [
    {
      "source": "CONTRIBUTING.md",
      "reference": "docs/automation/checklist.md"
    }
  ],
  "forbidden_doc_claims": []
}
"@
}

function Invoke-Validator {
    param(
        [string]$Root
    )

    Push-Location $Root
    try {
        $output = & pwsh -File './.github/scripts/validate-policy-integrity.ps1' -ManifestPath './.github/automation/policy-manifest.json' 2>&1
        return @{
            ExitCode = $LASTEXITCODE
            Output   = ($output | Out-String)
        }
    }
    finally {
        Pop-Location
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$validatorSource = Join-Path $repoRoot '.github/scripts/validate-policy-integrity.ps1'

if (-not (Test-Path $validatorSource)) {
    throw "Validator source not found: $validatorSource"
}

$passRoot = New-TempTestRoot
$failRoot = New-TempTestRoot
$forbiddenClaimRoot = New-TempTestRoot

try {
    Initialize-Fixture -Root $passRoot -ValidatorSourcePath $validatorSource
    $passResult = Invoke-Validator -Root $passRoot
    if ($passResult.ExitCode -ne 0) {
        throw "Expected passing fixture to succeed, got exit code $($passResult.ExitCode)`n$($passResult.Output)"
    }

    Initialize-Fixture -Root $failRoot -ValidatorSourcePath $validatorSource
    Write-File -Path (Join-Path $failRoot 'CHANGELOG.md') -Content @'
# Changelog

## [Unreleased]

- Added validator reference `.github/workflows/ci.yml`.
- Added stale pointer `missing/reference.md`.

## [0.1.0]
'@

    $failResult = Invoke-Validator -Root $failRoot
    if ($failResult.ExitCode -eq 0) {
        throw 'Expected failing fixture to fail, but it passed.'
    }

    if ($failResult.Output -notmatch 'Unreleased references missing paths') {
        throw "Failing fixture did not produce expected missing-reference error.`n$($failResult.Output)"
    }

    Initialize-Fixture -Root $forbiddenClaimRoot -ValidatorSourcePath $validatorSource
    Write-File -Path (Join-Path $forbiddenClaimRoot '.github/automation/policy-manifest.json') -Content @"
{
    "version": 1,
    "required_workflows": [
        ".github/workflows/ci.yml"
    ],
    "branch_policy": {
        "required_status_checks": {
            "baseline": [
                {
                    "name": "Test Check",
                    "workflow": ".github/workflows/ci.yml",
                    "job": "test-job"
                }
            ],
            "conditional": []
        }
    },
    "doc_path_assertions": [
        {
            "source": "CONTRIBUTING.md",
            "reference": "docs/automation/checklist.md"
        }
    ],
    "forbidden_doc_claims": [
        {
            "source": "CHANGELOG.md",
            "pattern": "UV Command Policy",
            "description": "Fixture guard against stale governance workflow claims."
        }
    ]
}
"@

    Write-File -Path (Join-Path $forbiddenClaimRoot 'CHANGELOG.md') -Content @'
# Changelog

## [Unreleased]

- Added validator reference `.github/workflows/ci.yml`.
- Legacy mention: UV Command Policy.

## [0.1.0]
'@

    $forbiddenResult = Invoke-Validator -Root $forbiddenClaimRoot
    if ($forbiddenResult.ExitCode -eq 0) {
        throw 'Expected forbidden-claim fixture to fail, but it passed.'
    }

    if ($forbiddenResult.Output -notmatch 'Forbidden stale claim detected') {
        throw "Forbidden-claim fixture did not produce expected stale-claim error.`n$($forbiddenResult.Output)"
    }

    Write-Host 'validate-policy-integrity fixture tests passed.'
    $global:LASTEXITCODE = 0
    exit 0
}
finally {
    if (Test-Path $passRoot) {
        Remove-Item -Path $passRoot -Recurse -Force
    }
    if (Test-Path $failRoot) {
        Remove-Item -Path $failRoot -Recurse -Force
    }
    if (Test-Path $forbiddenClaimRoot) {
        Remove-Item -Path $forbiddenClaimRoot -Recurse -Force
    }
}