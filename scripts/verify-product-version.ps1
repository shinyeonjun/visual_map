[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$packageVersion = (Get-Content -LiteralPath (Join-Path $repoRoot "package.json") -Raw | ConvertFrom-Json).version
$tauriVersion = (Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
$cargoText = Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri\Cargo.toml") -Raw
$packageSection = [regex]::Match($cargoText, '(?ms)^\[package\]\s*(?<body>.*?)(?=^\[|\z)')
$cargoVersionMatch = [regex]::Match($packageSection.Groups['body'].Value, '(?m)^version\s*=\s*"(?<version>[^"]+)"\s*$')

if (-not $packageSection.Success -or -not $cargoVersionMatch.Success) {
    throw "Could not read [package].version from src-tauri/Cargo.toml."
}

$cargoVersion = $cargoVersionMatch.Groups['version'].Value
$versions = @(@($packageVersion, $cargoVersion, $tauriVersion) | Select-Object -Unique)
if ($versions.Count -ne 1) {
    throw "Product version mismatch: package.json=$packageVersion, Cargo.toml=$cargoVersion, tauri.conf.json=$tauriVersion"
}

Write-Host "Product version is consistent: $packageVersion"
