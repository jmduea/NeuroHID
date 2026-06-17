$ErrorActionPreference = 'Stop'

Write-Host 'Running unsafe compliance checks...'

cargo clippy --workspace -- -A missing_docs -A unsafe_code -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc -D clippy::undocumented_unsafe_blocks -D clippy::missing_safety_doc
if ($LASTEXITCODE -ne 0) {
    throw 'Unsafe compliance checks failed.'
}

Write-Host 'Unsafe compliance checks passed.'
