param(
    [string]$BaseRef = "origin/main"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$hasOriginMain = $false
try {
    git rev-parse --verify $BaseRef *> $null
    if ($LASTEXITCODE -eq 0) {
        $hasOriginMain = $true
    }
} catch {
    $hasOriginMain = $false
}

if ($hasOriginMain) {
    $changed = @(git diff --name-only "$BaseRef...HEAD" | ForEach-Object { $_.Replace('\\', '/') })
} else {
    $changed = @(git diff-tree --no-commit-id --name-only -r HEAD | ForEach-Object { $_.Replace('\\', '/') })
}

if ($changed.Count -eq 0) {
    Write-Host "No changed files detected."
    exit 0
}

$codeChanged = $false
$protocolChanged = $false
$docsTouched = $false
$changelogTouched = $false
$readmeTouched = $false
$specTouched = $false
$notebookChanged = $false

foreach ($file in $changed) {
    if ($file -match '^(crates/|python/src/)') {
        $codeChanged = $true
    }
    if ($file -match '^docs/runtime-ml-protocol-v2\.md$|^docs/lab-kernel-protocol\.md$|^crates/neurohid-(ipc|types|core)/') {
        $protocolChanged = $true
    }
    if ($file -match '^docs/') {
        $docsTouched = $true
    }
    if ($file -eq 'CHANGELOG.md') {
        $changelogTouched = $true
    }
    if ($file -eq 'README.md' -or $file -eq 'python/README.md' -or $file -eq 'python/notebooks/README.md') {
        $readmeTouched = $true
    }
    if ($file -eq 'docs/SPECIFICATION.md') {
        $specTouched = $true
    }
    if ($file -match '^python/notebooks/') {
        $notebookChanged = $true
    }
}

$errors = New-Object System.Collections.Generic.List[string]

if ($codeChanged -and -not ($docsTouched -or $readmeTouched -or $changelogTouched)) {
    $errors.Add("Code changes detected without docs/README/CHANGELOG updates.")
}

if ($protocolChanged -and -not $changelogTouched) {
    $errors.Add("Protocol-related changes detected without CHANGELOG update.")
}

if ($protocolChanged -and -not ($specTouched -or ($changed -contains 'docs/runtime-ml-protocol-v2.md'))) {
    $errors.Add("Protocol-related changes detected without SPECIFICATION or runtime protocol doc updates.")
}

if ($notebookChanged -and -not ($changed -contains 'python/notebooks/README.md')) {
    $errors.Add("Notebook path/content changed without python/notebooks/README.md update.")
}

if ($errors.Count -gt 0) {
    Write-Host "Docs freshness checks failed:"
    foreach ($item in $errors) {
        Write-Host "  - $item"
    }
    throw "Docs freshness validation failed"
}

Write-Host "Docs freshness checks passed."
