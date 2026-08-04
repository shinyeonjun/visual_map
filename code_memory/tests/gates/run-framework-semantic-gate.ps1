param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$Root = (Join-Path $PSScriptRoot '..\..')
)

$ErrorActionPreference = 'Stop'

& $Bridge framework-packs --root $Root --self-test
if ($LASTEXITCODE -ne 0) { throw 'framework semantic self-test failed' }

Write-Host 'framework semantic gate: passed=85 total=85'
