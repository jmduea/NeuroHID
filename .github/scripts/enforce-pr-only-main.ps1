#!/usr/bin/env pwsh

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

param(
    [string]$Ref = $env:GITHUB_REF,
    [string]$Sha = $env:GITHUB_SHA,
    [string]$Repository = $env:GITHUB_REPOSITORY,
    [string]$Token = $env:GITHUB_TOKEN
)

if ($Ref -ne 'refs/heads/main') {
    Write-Host "Branch policy check skipped for ref '$Ref'."
    exit 0
}

if (-not $Sha) {
    throw 'GITHUB_SHA is required for branch policy validation.'
}

if (-not $Repository) {
    throw 'GITHUB_REPOSITORY is required for branch policy validation.'
}

if (-not $Token) {
    throw 'GITHUB_TOKEN is required for branch policy validation.'
}

$url = "https://api.github.com/repos/$Repository/commits/$Sha/pulls"
$headers = @{
    Accept        = 'application/vnd.github+json'
    Authorization = "Bearer $Token"
}

$pulls = Invoke-RestMethod -Method Get -Uri $url -Headers $headers

if ($null -eq $pulls -or $pulls.Count -eq 0) {
    Write-Error "Direct push to 'main' detected at commit $Sha. Use a feature branch and open a pull request."
    exit 1
}

Write-Host "Main branch update is associated with PR #$($pulls[0].number). Policy check passed."