param(
    [ValidateSet('focused', 'full')]
    [string]$RustScope = 'focused',
    [switch]$SkipRust,
    [switch]$WithPython,
    [switch]$WithDocs,
    [switch]$WithProtocol,
    [switch]$WithUnsafe
)

$ErrorActionPreference = 'Stop'

function Invoke-Step {
    param(
        [string]$Name,
        [string[]]$Command
    )

    Write-Host "==> $Name"
    & $Command[0] $Command[1..($Command.Length - 1)]
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed: $Name"
    }
}

if (-not $SkipRust) {
    Invoke-Step -Name 'Rust fmt check' -Command @('cargo', 'fmt', '--check')
    Invoke-Step -Name 'Rust clippy' -Command @('cargo', 'clippy', '--workspace', '--', '-D', 'warnings', '-A', 'missing_docs')

    if ($RustScope -eq 'full') {
        Invoke-Step -Name 'Rust tests (full)' -Command @('cargo', 'test', '--workspace')
    }
    else {
        Invoke-Step -Name 'Rust tests (focused)' -Command @('cargo', 'test', '--workspace', '--lib')
    }
}

if ($WithPython) {
    Invoke-Step -Name 'Python sync' -Command @('uv', 'sync', '--directory', 'python', '--extra', 'dev')
    Invoke-Step -Name 'Python ruff' -Command @('uv', 'run', '--project', 'python', 'ruff', 'check', 'python/src')
    Invoke-Step -Name 'Python black' -Command @('uv', 'run', '--project', 'python', 'black', '--check', 'python/src')
    Invoke-Step -Name 'Python mypy' -Command @('uv', 'run', '--project', 'python', 'mypy', 'python/src')
    Invoke-Step -Name 'Python pytest' -Command @('uv', 'run', '--project', 'python', 'pytest', 'python/tests', '-q')
}

if ($WithDocs) {
    Invoke-Step -Name 'Rust docs' -Command @('cargo', 'doc', '--workspace', '--no-deps')
}

if ($WithProtocol) {
    Invoke-Step -Name 'Protocol contracts' -Command @('pwsh', '-File', './.github/scripts/verify-protocol-contracts.ps1')
}

if ($WithUnsafe) {
    Invoke-Step -Name 'Unsafe compliance' -Command @('pwsh', '-File', './.github/scripts/check-unsafe-compliance.ps1')
}

Write-Host 'Canonical agent-ready tasks completed.'
