$ErrorActionPreference = 'Stop'

Write-Host 'Running unsafe compliance checks...'

cargo clippy --workspace -- -D warnings -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc
if ($LASTEXITCODE -ne 0) {
    throw 'Unsafe compliance checks failed.'
}

Write-Host 'Unsafe compliance checks passed.'
