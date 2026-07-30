[CmdletBinding()]
param([string]$EnginePath)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($EnginePath)) {
    $EnginePath = Join-Path $repoRoot "src-tauri\engines\code-memory-language.exe"
}
$EnginePath = [IO.Path]::GetFullPath($EnginePath)
if (-not (Test-Path -LiteralPath $EnginePath -PathType Leaf)) { throw "Code engine not found: $EnginePath" }

$engineRoot = Split-Path -Parent $EnginePath
$packsRoot = Join-Path $engineRoot "packs"
$providersRoot = Join-Path $engineRoot "providers"
if (-not (Test-Path -LiteralPath (Join-Path $packsRoot "framework") -PathType Container)) {
    $packsRoot = Join-Path $repoRoot "src-tauri\engines\packs"
}
if (Test-Path -LiteralPath (Join-Path $packsRoot "framework") -PathType Container) { $env:CODE_MEMORY_PACKS_ROOT = $packsRoot }
if (Test-Path -LiteralPath $providersRoot -PathType Container) { $env:CODE_MEMORY_PROVIDERS_ROOT = $providersRoot }

$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$runRoot = Join-Path $tempRoot ("backend-visual-map-evidence-" + [guid]::NewGuid().ToString("N"))
$cacheRoot = Join-Path $runRoot "cache"
$sidecarRunner = Join-Path $PSScriptRoot "run-sidecar-json.mjs"
New-Item -ItemType Directory -Path $cacheRoot -Force | Out-Null

function Invoke-CodeTool([string]$Tool, [hashtable]$Payload) {
    $argsPath = Join-Path $runRoot (([guid]::NewGuid().ToString("N")) + ".json")
    [IO.File]::WriteAllText($argsPath, ($Payload | ConvertTo-Json -Compress -Depth 10), [Text.UTF8Encoding]::new($false))
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & node $sidecarRunner $EnginePath $Tool $argsPath 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $previousErrorAction }
    if ($exitCode -ne 0) { throw "$Tool failed: $($output -join [Environment]::NewLine)" }
    foreach ($line in @($output)) {
        $text = [string]$line
        if ($text.TrimStart().StartsWith("{") -or $text.TrimStart().StartsWith("[")) {
            try { return $text | ConvertFrom-Json } catch { }
        }
    }
    throw "$Tool did not return JSON."
}

function Index-Fixture([string]$Name, [string]$Path) {
    $project = "evidence-$Name"
    $env:CBM_CACHE_DIR = Join-Path $cacheRoot $Name
    $env:CBM_ALLOWED_ROOT = [IO.Path]::GetFullPath($Path)
    $index = Invoke-CodeTool "index_repository" @{ repo_path = $Path; mode = "full"; name = $project; persistence = $false }
    return [string]$index.project
}

function Get-Architecture([string]$Project) {
    return Invoke-CodeTool "get_architecture" @{ project = $Project }
}

function Has-Node($Architecture, [string]$Kind, [string]$PathPart) {
    return @($Architecture.nodes | Where-Object { [string]$_.kind -eq $Kind -and ([string]$_.path -like "*$PathPart*" -or [string]$_.name -like "*$PathPart*") }).Count -gt 0
}

function Has-Edge($Architecture, [string]$Kind, [string]$EvidencePart) {
    return @($Architecture.edges | Where-Object {
        [string]$_.kind -eq $Kind -and @($_.evidence | Where-Object { [string]$_.path -like "*$EvidencePart*" }).Count -gt 0
    }).Count -gt 0
}

try {
    $goldenPath = Join-Path $runRoot "golden-fastapi"
    Copy-Item -LiteralPath (Join-Path $repoRoot "code_memory\tests\fixtures\evidence\golden-fastapi") -Destination $goldenPath -Recurse
    $goldenProject = Index-Fixture "golden" $goldenPath
    $golden = Get-Architecture $goldenProject
    if (-not (Has-Node $golden "ENDPOINT" "server.py")) { throw "Golden fixture has no FastAPI endpoint." }
    if (-not (Has-Edge $golden "IMPORTS" "server.py")) { throw "Golden fixture has no import boundary." }
    if (-not (Has-Edge $golden "READS" "services.py")) { throw "Golden fixture has no static SQL READS edge." }

    $negativePath = Join-Path $runRoot "negative-sql"
    Copy-Item -LiteralPath (Join-Path $repoRoot "code_memory\tests\fixtures\evidence\negative-sql") -Destination $negativePath -Recurse
    $negativeProject = Index-Fixture "negative" $negativePath
    $negative = Get-Architecture $negativeProject
    $falseReads = @($negative.edges | Where-Object {
        [string]$_.kind -eq "READS" -and @($_.evidence | Where-Object { [string]$_.path -like "*server.py*" }).Count -gt 0
    })
    if ($falseReads.Count -gt 0) {
        Write-Host "False READS edges:" -ForegroundColor Yellow
        $falseReads | ConvertTo-Json -Depth 10 | Write-Host
        throw "Negative fixture produced $($falseReads.Count) SQL READS false positive(s)."
    }
    Write-Host "Code evidence golden/negative gate passed."
}
finally {
    Remove-Item Env:CBM_CACHE_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:CBM_ALLOWED_ROOT -ErrorAction SilentlyContinue
    Remove-Item Env:CODE_MEMORY_PACKS_ROOT -ErrorAction SilentlyContinue
    Remove-Item Env:CODE_MEMORY_PROVIDERS_ROOT -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $runRoot) { Remove-Item -LiteralPath $runRoot -Recurse -Force -ErrorAction SilentlyContinue }
}
