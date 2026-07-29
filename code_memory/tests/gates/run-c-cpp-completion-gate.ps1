param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [Parameter(Mandatory = $true)][string]$ProvidersRoot
)

$ErrorActionPreference = 'Stop'
$fixture = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures\native-lsp-c')).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$outputRoot = Join-Path $PSScriptRoot '..\..\build\c-cpp-completion-gate'
$out = Join-Path $outputRoot 'index.json'
$architecture = Join-Path $outputRoot 'architecture.json'
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
Remove-Item -LiteralPath $out, $architecture -Force -ErrorAction SilentlyContinue

& $Bridge index --root $fixture --providers-root (Resolve-Path $ProvidersRoot).Path --packs-root $packsRoot --out $out --architecture-out $architecture
if ($LASTEXITCODE -ne 0) { throw 'C/C++ bridge execution failed' }

$result = Get-Content $out -Raw | ConvertFrom-Json
foreach ($id in @('c', 'cpp')) {
    $language = @($result.languages | Where-Object id -eq $id) | Select-Object -First 1
    if ($null -eq $language -or $language.status -ne 'indexed') {
        throw "${id}: status is not indexed"
    }
}

$duplicateDocuments = @($result.documents | Group-Object path | Where-Object Count -gt 1)
if ($duplicateDocuments.Count -gt 0) { throw 'duplicate semantic document emitted' }
$duplicateCoverage = @($result.coverage | Group-Object path | Where-Object Count -gt 1)
if ($duplicateCoverage.Count -gt 0) { throw 'duplicate coverage record emitted' }
$relationKeys = @($result.relations | ForEach-Object {
        "$($_.from)|$($_.to)|$($_.kind)|$($_.path)|$([string]::Join(',', $_.range))"
    })
if (($relationKeys | Group-Object | Where-Object Count -gt 1).Count -gt 0) {
    throw 'duplicate semantic relation emitted'
}
if (@($result.relations | Where-Object kind -eq 'CALLS' | Where-Object to -match 'declared_value').Count -eq 0) {
    throw 'declaration/implementation call target was not resolved'
}
if (@($result.relations | Where-Object kind -eq 'DEFINITION').Count -eq 0) {
    throw 'declaration/implementation relation was not emitted'
}
if (@($result.relations | Where-Object kind -eq 'IMPLEMENTATION' | Where-Object {
            $_.from -match 'Implementation' -and $_.to -match 'Contract'
        }).Count -eq 0) {
    throw 'C++ inheritance relation was not emitted'
}
if (@($result.relations | Where-Object kind -eq 'USES_TYPE').Count -eq 0) {
    throw 'C/C++ type relation was not emitted'
}
if (@($result.relations | Where-Object kind -match 'ESTIMATED|GUESS|INFERRED').Count -gt 0) {
    throw 'estimated relation emitted'
}

Write-Host 'PASS c/cpp completion gate'
