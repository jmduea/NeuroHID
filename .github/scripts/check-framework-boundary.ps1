# Enforces the framework boundary: neuroide-hub, neuroide, and neurohid-service may only have
# path dependencies that appear in the canonical allowlist. No permanent exceptions; fix by
# re-export from core or by updating the allowlist and code together.
#
# Allowlist source: .github/framework-allowlist.toml (single source of truth).
# Must match docs/framework-surface.md. See 07-CONTEXT.md for policy.

$ErrorActionPreference = 'Stop'

$RepoRoot = if ($PSScriptRoot) {
    $scriptDir = $PSScriptRoot
    while ($scriptDir -and -not (Test-Path (Join-Path $scriptDir 'Cargo.toml'))) {
        $scriptDir = Split-Path -Parent $scriptDir
    }
    if ($scriptDir) { $scriptDir } else { Get-Location }
} else {
    Get-Location
}

$AllowlistPath = Join-Path $RepoRoot '.github/framework-allowlist.toml'
if (-not (Test-Path $AllowlistPath)) {
    Write-Error "Allowlist not found: $AllowlistPath"
}

function Get-AllowlistSection {
    param([string]$Content, [string]$Section)
    # Allow comments/newlines between section header and allowed = [...]
    $pattern = "(?s)\[$Section\].*?allowed\s*=\s*\[([^\]]+)\]"
    if ($Content -match $pattern) {
        $match = $Matches[1]
        $matches = [regex]::Matches($match, '"([^"]+)"')
        return @($matches | ForEach-Object { $_.Groups[1].Value })
    }
    return @()
}

$allowlistContent = Get-Content -Path $AllowlistPath -Raw
$hubAllowed = @(Get-AllowlistSection -Content $allowlistContent -Section 'neuroide-hub')
$neuroideAllowed = @(Get-AllowlistSection -Content $allowlistContent -Section 'neuroide')
$serviceAllowed = @(Get-AllowlistSection -Content $allowlistContent -Section 'neurohid-service')

if ($hubAllowed.Count -eq 0 -or $neuroideAllowed.Count -eq 0 -or $serviceAllowed.Count -eq 0) {
    Write-Error "Could not parse [neuroide-hub], [neuroide], and [neurohid-service].allowed from $AllowlistPath"
}

$env:CARGO_TERM_COLOR = 'never'
$metadataJson = & cargo metadata --format-version=1 2>$null
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($metadataJson)) {
    Write-Error "cargo metadata failed"
}
$metadata = $metadataJson | ConvertFrom-Json

$workspaceMemberIds = $metadata.workspace_members
$workspacePackageNames = @(
    $metadata.packages |
        Where-Object { $workspaceMemberIds -contains $_.id } |
        ForEach-Object { $_.name }
)

function Get-PathDeps {
    param($Package)
    $pathDeps = @(
        $Package.dependencies |
            Where-Object { $workspacePackageNames -contains $_.name } |
            ForEach-Object { $_.name }
    )
    return $pathDeps
}

$packagesToCheck = @(
    @{ Name = 'neuroide-hub'; Allowed = $hubAllowed },
    @{ Name = 'neuroide'; Allowed = $neuroideAllowed },
    @{ Name = 'neurohid-service'; Allowed = $serviceAllowed }
)

$failed = $false
foreach ($entry in $packagesToCheck) {
    $pkg = $metadata.packages | Where-Object { $_.name -eq $entry.Name } | Select-Object -First 1
    if (-not $pkg) {
        Write-Warning "Package $($entry.Name) not found in workspace; skipping."
        continue
    }
    $pathDeps = Get-PathDeps -Package $pkg
    $allowedSet = [System.Collections.Generic.HashSet[string]]::new([string[]]$entry.Allowed)
    foreach ($dep in $pathDeps) {
        if (-not $allowedSet.Contains($dep)) {
            $failed = $true
            [Console]::Error.WriteLine("framework-boundary: $($entry.Name) has disallowed path dependency: $dep")
        }
    }
}

if ($failed) {
    [Console]::Error.WriteLine("framework-boundary: path dependencies must be in .github/framework-allowlist.toml; no permanent exceptions.")
    exit 1
}

exit 0
