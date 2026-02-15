$ErrorActionPreference = 'Stop'

if (-not (Test-Path 'docs/runtime-ml-protocol-v2.md')) {
    throw 'Missing protocol document: docs/runtime-ml-protocol-v2.md'
}

Write-Host 'Verifying protocol contracts (neurohid-types tests)...'

cargo test -p neurohid-types
if ($LASTEXITCODE -ne 0) {
    throw 'Protocol contract verification failed.'
}

Write-Host 'Protocol contract verification passed.'
