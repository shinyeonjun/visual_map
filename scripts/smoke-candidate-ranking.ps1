[CmdletBinding()]
param(
    [string]$EnginePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoArgs = @(
    "test",
    "--locked",
    "--manifest-path",
    (Join-Path $repoRoot "src-tauri\Cargo.toml"),
    "candidate"
)

& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "Candidate ranking tests failed."
}

$smokeArgs = @{}
if (-not [string]::IsNullOrWhiteSpace($EnginePath)) {
    $smokeArgs.EnginePath = $EnginePath
}
& (Join-Path $PSScriptRoot "smoke-code-engine-contract.ps1") @smokeArgs
if ($LASTEXITCODE -ne 0) {
    throw "Focused code evidence smoke failed."
}

Write-Host "Candidate ranking smoke passed."
