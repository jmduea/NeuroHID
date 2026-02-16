# RTK auto-rewrite hook for PowerShell PreToolUse hooks.
# Reads hook JSON from stdin and emits updatedInput when a rewrite applies.

$ErrorActionPreference = 'Stop'

if (-not (Get-Command rtk -ErrorAction SilentlyContinue)) {
    exit 0
}

$inputJson = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($inputJson)) {
    exit 0
}

try {
    $payload = $inputJson | ConvertFrom-Json -Depth 10
}
catch {
    exit 0
}

$command = [string]$payload.tool_input.command
if ([string]::IsNullOrWhiteSpace($command)) {
    exit 0
}

if ($command -match '^(rtk\s+|.*/rtk\s+)') {
    exit 0
}

$rewritten = $null

$rewriteMap = @(
    @{ Pattern = '^git\s+status(\s|$)'; Replacement = 'rtk git status' },
    @{ Pattern = '^git\s+diff(\s|$)'; Replacement = 'rtk git diff' },
    @{ Pattern = '^git\s+log(\s|$)'; Replacement = 'rtk git log' },
    @{ Pattern = '^git\s+add(\s|$)'; Replacement = 'rtk git add' },
    @{ Pattern = '^git\s+commit(\s|$)'; Replacement = 'rtk git commit' },
    @{ Pattern = '^git\s+push(\s|$)'; Replacement = 'rtk git push' },
    @{ Pattern = '^git\s+pull(\s|$)'; Replacement = 'rtk git pull' },
    @{ Pattern = '^gh\s+'; Replacement = 'rtk gh ' },
    @{ Pattern = '^cargo\s+test(\s|$)'; Replacement = 'rtk cargo test' },
    @{ Pattern = '^cargo\s+build(\s|$)'; Replacement = 'rtk cargo build' },
    @{ Pattern = '^cargo\s+clippy(\s|$)'; Replacement = 'rtk cargo clippy' },
    @{ Pattern = '^cargo\s+check(\s|$)'; Replacement = 'rtk cargo check' },
    @{ Pattern = '^cargo\s+fmt(\s|$)'; Replacement = 'rtk cargo fmt' },
    @{ Pattern = '^ls(\s|$)'; Replacement = 'rtk ls' },
    @{ Pattern = '^(rg|grep)\s+'; Replacement = 'rtk grep ' },
    @{ Pattern = '^find\s+'; Replacement = 'rtk find ' }
)

foreach ($rule in $rewriteMap) {
    if ($command -match $rule.Pattern) {
        $rewritten = [regex]::Replace($command, $rule.Pattern, $rule.Replacement, 1)
        break
    }
}

if ([string]::IsNullOrWhiteSpace($rewritten) -or $rewritten -eq $command) {
    exit 0
}

$updatedInput = @{}
if ($payload.tool_input) {
    foreach ($property in $payload.tool_input.PSObject.Properties) {
        $updatedInput[$property.Name] = $property.Value
    }
}
$updatedInput.command = $rewritten

$result = @{
    hookSpecificOutput = @{
        hookEventName = 'PreToolUse'
        permissionDecision = 'allow'
        permissionDecisionReason = 'RTK auto-rewrite'
        updatedInput = $updatedInput
    }
}

$result | ConvertTo-Json -Depth 10 -Compress | Write-Output
