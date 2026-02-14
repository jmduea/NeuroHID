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
    $changedFiles = @(git diff --name-only "$BaseRef...HEAD" | ForEach-Object { $_.Replace('\\', '/') })
} else {
    $changedFiles = @(git diff-tree --no-commit-id --name-only -r HEAD | ForEach-Object { $_.Replace('\\', '/') })
}

$rustFiles = @($changedFiles | Where-Object { $_ -match '^crates/.+\.rs$|^third_party/.+\.rs$' })
if ($rustFiles.Count -eq 0) {
    Write-Host "No changed Rust files in unsafe-scope paths."
    exit 0
}

$violations = New-Object System.Collections.Generic.List[string]

foreach ($file in $rustFiles) {
    if (-not (Test-Path $file)) {
        continue
    }

    $lines = Get-Content -Path $file
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index] -match '\bunsafe\s*\{') {
            $lineNum = $index + 1
            $contextStart = [Math]::Max(0, $index - 3)
            $hasSafetyComment = $false
            for ($scan = $contextStart; $scan -lt $index; $scan++) {
                if ($lines[$scan] -match 'SAFETY:') {
                    $hasSafetyComment = $true
                    break
                }
            }

            if (-not $hasSafetyComment) {
                $violations.Add(("{0}:{1} missing SAFETY comment for unsafe block" -f $file, $lineNum))
            }
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Host "Unsafe compliance violations:" 
    $violations | ForEach-Object { Write-Host "  - $_" }
    throw "Unsafe compliance failed"
}

Write-Host "Unsafe compliance passed."
