[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$EnginePath,
    [string]$ExpectedVersion = "0.1.0",
    [string]$ExpectedContractVersion = "4"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$EnginePath = [IO.Path]::GetFullPath($EnginePath)
if (-not (Test-Path -LiteralPath $EnginePath -PathType Leaf)) {
    throw "Code engine was not found: $EnginePath"
}

$lines = @(& $EnginePath contract)
if ($LASTEXITCODE -ne 0) {
    throw "Code engine contract probe failed with exit code ${LASTEXITCODE}: $EnginePath"
}
if ($lines.Count -eq 0) {
    throw "Code engine contract probe returned no output: $EnginePath"
}

try {
    $contract = ($lines -join [Environment]::NewLine) | ConvertFrom-Json
} catch {
    throw "Code engine contract probe returned invalid JSON: $($_.Exception.Message)"
}

if ([string]$contract.schema -ne "codebase-workspace.code-engine-contract.v1") {
    throw "Unsupported code engine contract schema: $($contract.schema)"
}
if ([string]$contract.version -ne $ExpectedVersion) {
    throw "Code engine version mismatch: expected $ExpectedVersion, got $($contract.version)"
}
if ([string]$contract.contractVersion -ne $ExpectedContractVersion) {
    throw "Code engine contract mismatch: expected $ExpectedContractVersion, got $($contract.contractVersion)"
}

$commands = @($contract.commands | ForEach-Object { [string]$_ })
$uniqueCommands = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach ($command in $commands) {
    if ([string]::IsNullOrWhiteSpace($command) -or -not $uniqueCommands.Add($command)) {
        throw "Code engine contract contains an empty or duplicate command."
    }
}
$expectedCommands = @(
    "contract",
    "list",
    "doctor",
    "detect-languages",
    "index",
    "framework-packs",
    "compare-scip"
)
foreach ($required in $expectedCommands) {
    if (-not $uniqueCommands.Contains($required)) {
        throw "Code engine contract is missing required command '$required'."
    }
}
if ($uniqueCommands.Count -ne $expectedCommands.Count) {
    $unexpected = @($commands | Where-Object { $_ -notin $expectedCommands }) -join ", "
    throw "Code engine contract exposes unexpected commands: $unexpected"
}

Write-Output "Verified code engine contract v${ExpectedContractVersion}: $EnginePath"
