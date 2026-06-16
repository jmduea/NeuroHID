# PyO3 on Windows expects python3.lib; CPython 3.14 ships python314.lib.
$ErrorActionPreference = 'Stop'

$pythonLocation = $env:pythonLocation
if ([string]::IsNullOrWhiteSpace($pythonLocation)) {
    Write-Error 'pythonLocation is not set (actions/setup-python required first).'
}

$libsDir = Join-Path $pythonLocation 'libs'
if (-not (Test-Path $libsDir)) {
    Write-Error "Python libs directory not found: $libsDir"
}

$target = Join-Path $libsDir 'python3.lib'
if (Test-Path $target) {
    Write-Host "python3.lib already present at $target"
    exit 0
}

$source = Get-ChildItem -Path $libsDir -Filter 'python*.lib' |
    Where-Object { $_.Name -ne 'python3.lib' } |
    Sort-Object Name -Descending |
    Select-Object -First 1

if (-not $source) {
    Write-Error "No python*.lib found under $libsDir"
}

Copy-Item -Path $source.FullName -Destination $target
Write-Host "Linked $($source.Name) -> python3.lib for PyO3 on Windows"
