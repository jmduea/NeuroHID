param(
    [string]$OutputPath = "docs/architecture/index.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

$workspaceManifest = "Cargo.toml"
if (-not (Test-Path $workspaceManifest)) {
    throw "Missing workspace manifest: $workspaceManifest"
}

$metadataJson = cargo metadata --format-version 1 --no-deps
$metadata = $metadataJson | ConvertFrom-Json
$packages = @($metadata.packages | Sort-Object name)

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# Architecture Index")
$lines.Add("")
$lines.Add("Generated: $(Get-Date -Format o)")
$lines.Add("")
$lines.Add("## Workspace Packages")
$lines.Add("")
$lines.Add("| Package | Manifest | Edition |")
$lines.Add("| --- | --- | --- |")
foreach ($package in $packages) {
    $manifestPath = $package.manifest_path -replace '\\', '/'
    if ($manifestPath -match '.+/neurohid/(.+)$') {
        $manifestPath = $matches[1]
    }
    $lines.Add("| $($package.name) | $manifestPath | $($package.edition) |")
}

$lines.Add("")
$lines.Add("## Governance Links")
$lines.Add("")
$lines.Add('- Default workflow: `.github/agents/_shared/multi-agent-phase-workflow.md`')
$lines.Add('- Scope map: `.github/automation/scope-map.json`')
$lines.Add('- Architecture gate: `.github/workflows/architecture-gate.yml`')
$lines.Add('- Protocol spec: `docs/runtime-ml-protocol-v2.md`')

Set-Content -Path $OutputPath -Value $lines -Encoding UTF8
Write-Host "Architecture index generated at: $OutputPath"
