param(
    [Parameter(Mandatory = $true)][string]$Root,
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [Parameter(Mandatory = $true)][string]$ProvidersRoot
)

$ErrorActionPreference = 'Stop'
$rootPath = (Resolve-Path $Root).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$outputRoot = Join-Path $PSScriptRoot '..\..\build\c-cpp-scale-gate'
$stamp = [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff')
$out = Join-Path $outputRoot "$stamp.json"
$architecture = Join-Path $outputRoot "$stamp.architecture.json"
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

& $Bridge index --root $rootPath --providers-root (Resolve-Path $ProvidersRoot).Path --packs-root $packsRoot --out $out --architecture-out $architecture
if ($LASTEXITCODE -ne 0) { throw 'large C/C++ bridge execution failed' }

$result = Get-Content $out -Raw | ConvertFrom-Json
$languages = @($result.languages | Where-Object id -in @('c', 'cpp'))
if ($languages.Count -eq 0) { throw 'large fixture has no C/C++ language result' }
$duplicateDocuments = @($result.documents | Group-Object path | Where-Object Count -gt 1)
if ($duplicateDocuments.Count -gt 0) { throw 'large fixture emitted duplicate semantic documents' }
$duplicateCoverage = @($result.coverage | Group-Object path | Where-Object Count -gt 1)
if ($duplicateCoverage.Count -gt 0) { throw 'large fixture emitted duplicate coverage records' }
$badRelations = @($result.relations | Where-Object kind -match 'ESTIMATED|GUESS|INFERRED')
if ($badRelations.Count -gt 0) { throw 'large fixture emitted estimated relations' }

$sourceCount = ($languages | Measure-Object -Property files_found -Sum).Sum
if ($sourceCount -lt 250) { throw "fixture is not large enough for the large-map gate: files=$sourceCount" }
$failedWithoutContext = @($languages | Where-Object status -eq 'indexer-failed').Count -gt 0 -and
    @($result.diagnostics | Where-Object message -match 'no compile context').Count -gt 0
if ($failedWithoutContext) {
    Write-Host "PASS large C/C++ safety gate: $sourceCount files rejected without compiler context"
    exit 0
}
$errors = @($result.diagnostics | Where-Object level -eq 'error')
if ($errors.Count -gt 0) { throw 'large fixture emitted error diagnostics' }
if (@($languages | Where-Object files_missing -gt 0).Count -gt 0) {
    throw 'large fixture has missing C/C++ files'
}
$unexpectedCoverage = @($result.coverage | Where-Object {
    $_.status -ne 'indexed' -and $_.reason -notin @('not-in-active-build', 'header-not-reachable')
})
if ($unexpectedCoverage.Count -gt 0) {
    throw 'large fixture has a non-indexed file without an active-build exclusion reason'
}
Write-Host "PASS large C/C++ semantic gate: $sourceCount files accounted for ($(@($result.coverage | Where-Object status -eq 'indexed').Count) indexed, $(@($result.coverage | Where-Object status -eq 'excluded').Count) excluded)"
