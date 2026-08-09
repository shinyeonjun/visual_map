param(
    [Parameter(Mandatory = $true)]
    [string]$ProjectRoot,
    [string]$Root,
    [string]$Bridge,
    [string]$ProvidersRoot,
    [string]$OutputRoot,
    [switch]$IncludeLegacy
)

$ErrorActionPreference = 'Stop'
$scriptRoot = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Root)) { $Root = Join-Path $scriptRoot '..\..' }
if ([string]::IsNullOrWhiteSpace($Bridge)) { $Bridge = Join-Path $scriptRoot '..\..\rust\target\release\code-memory-language.exe' }
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    # Keep staged source outside the bridge's conventional build-directory
    # exclusions; callers can still provide an explicit output path.
    $OutputRoot = Join-Path ([IO.Path]::GetTempPath()) 'codebase-workspace-external-gate'
}
$ProjectRoot = (Resolve-Path $ProjectRoot).Path
$Root = (Resolve-Path $Root).Path
$Bridge = (Resolve-Path $Bridge).Path
$OutputRoot = if ([IO.Path]::IsPathRooted($OutputRoot)) {
    [IO.Path]::GetFullPath($OutputRoot)
} else {
    [IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputRoot))
}

function Copy-Tree([string]$Source, [string]$Destination) {
    $excluded = '\\(\.git|node_modules|build|dist|target|vendor|obj|bin|\.dart_tool|\.gradle|__pycache__|\.venv|venv)(\\|$)'
    Get-ChildItem -LiteralPath $Source -Recurse -File |
        Where-Object { $_.FullName -notmatch $excluded } |
        ForEach-Object {
            $relative = $_.FullName.Substring($Source.Length).TrimStart([char]92, [char]47)
            $target = Join-Path $Destination $relative
            New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
            Copy-Item -LiteralPath $_.FullName -Destination $target -Force
        }
}

function Add-JsConfig([string]$Stage) {
    $config = Join-Path $Stage 'tsconfig.json'
    if (-not (Test-Path $config)) {
        @'
{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": false,
    "jsx": "react-jsx",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "target": "ES2020",
    "skipLibCheck": true
  },
    "include": ["src/**/*", "*.js"]
}
'@ | Set-Content -LiteralPath $config -Encoding UTF8
    }
}

function Read-Index([string]$Path) {
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($null -ne $pwsh) {
        $literalPath = $Path.Replace("'", "''")
        $code = "`$index = Get-Content -LiteralPath '$literalPath' -Raw | ConvertFrom-Json -Depth 1000; `$index | Select-Object languages,frameworks,framework_relations | ConvertTo-Json -Depth 100"
        return ((& $pwsh.Source -NoProfile -Command $code) -join "`n") | ConvertFrom-Json
    }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Read-Architecture([string]$Path) {
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($null -ne $pwsh) {
        $literalPath = $Path.Replace("'", "''")
        $code = '$architecture = Get-Content -LiteralPath ' + "'" + $literalPath + "'" + ' -Raw | ConvertFrom-Json -Depth 1000; $architecture | Select-Object nodes,edges,diagnostics | ConvertTo-Json -Depth 100'
        return ((& $pwsh.Source -NoProfile -Command $code) -join [Environment]::NewLine) | ConvertFrom-Json
    }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Invoke-Index([string]$Name, [string]$Stage) {
    $out = Join-Path $OutputRoot "$Name.json"
    $args = @('index', '--root', $Stage, '--out', $out, '--packs-root', $Root)
    if ($ProvidersRoot) { $args += @('--providers-root', $ProvidersRoot) }
    & $Bridge @args
    if ($LASTEXITCODE -ne 0) { throw "${Name}: bridge failed with exit code $LASTEXITCODE" }
    if (-not (Test-Path $out)) { throw "${Name}: output was not written" }
    return Read-Index $out
}

function Require([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "external gate failed: $Message" }
}

if (Test-Path $OutputRoot) { Remove-Item -LiteralPath $OutputRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

$serverStage = Join-Path $OutputRoot 'server-routes'
Copy-Tree (Join-Path $ProjectRoot 'server\app\api\http\routes') (Join-Path $serverStage 'app\api\http\routes')
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'pyproject.toml') -Destination $serverStage -Force
$server = Invoke-Index 'server-routes' $serverStage
$serverArchitecture = Read-Architecture (Join-Path $OutputRoot 'server-routes.architecture.json')
$fastapi = @($server.frameworks | Where-Object { $_.language -eq 'python' -and $_.id -eq 'fastapi' }) | Select-Object -First 1
$routes = @($fastapi.facts | Where-Object kind -eq 'HTTP_ROUTE')
$handles = @($server.framework_relations | Where-Object { $_.framework -eq 'fastapi' -and $_.kind -eq 'HANDLES' })
Require (@($server.languages | Where-Object { $_.id -eq 'python' -and $_.status -eq 'indexed' }).Count -eq 1) 'server Python was not indexed'
Require ($null -ne $fastapi) 'FastAPI was not detected in server routes'
Require ($routes.Count -ge 50) "expected at least 50 FastAPI routes, found $($routes.Count)"
Require (@($routes | Where-Object { [string]::IsNullOrWhiteSpace($_.symbol) }).Count -eq 0) 'a FastAPI route has no resolved handler symbol'
Require ($handles.Count -eq $routes.Count) "FastAPI HANDLES count $($handles.Count) does not equal route count $($routes.Count)"
Require (@($serverArchitecture.nodes | Where-Object kind -eq 'MODULE').Count -gt 0) 'server architecture has no modules'
Require (@($serverArchitecture.nodes | Where-Object { $_.kind -eq 'ENDPOINT' -and $_.properties.execution_root -eq 'true' }).Count -gt 0) 'server architecture has no verified execution roots'
Require (@($serverArchitecture.edges | Where-Object kind -eq 'ENTRYPOINT_TO').Count -gt 0) 'server architecture has no entrypoint bindings'
Write-Host "PASS server-routes: routes=$($routes.Count) handles=$($handles.Count)"

$webStage = Join-Path $OutputRoot 'web'
Copy-Tree (Join-Path $ProjectRoot 'client\web') $webStage
Add-JsConfig $webStage
$web = Invoke-Index 'web' $webStage
$webArchitecture = Read-Architecture (Join-Path $OutputRoot 'web.architecture.json')
$react = @($web.frameworks | Where-Object { $_.language -eq 'javascript' -and $_.id -eq 'react' }) | Select-Object -First 1
$components = @($react.facts | Where-Object kind -eq 'COMPONENT')
Require (@($web.languages | Where-Object { $_.id -eq 'javascript' -and $_.status -eq 'indexed' }).Count -eq 1) 'web JavaScript was not indexed'
Require ($null -ne $react) 'React was not detected in web'
Require ($components.Count -gt 0) 'web React component facts are empty'
Require (@($webArchitecture.nodes | Where-Object { $_.id -eq 'external:npm:react' }).Count -eq 1) 'web architecture has no React boundary'
Write-Host "PASS web: components=$($components.Count)"

$overlayJsStage = Join-Path $OutputRoot 'overlay-js'
New-Item -ItemType Directory -Force -Path (Join-Path $overlayJsStage 'src') | Out-Null
Copy-Tree (Join-Path $ProjectRoot 'client\overlay\src') (Join-Path $overlayJsStage 'src')
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'client\overlay\package.json') -Destination $overlayJsStage -Force
Add-JsConfig $overlayJsStage
$overlayJs = Invoke-Index 'overlay-js' $overlayJsStage
$overlayJsArchitecture = Read-Architecture (Join-Path $OutputRoot 'overlay-js.architecture.json')
$tauriJs = @($overlayJs.frameworks | Where-Object { $_.language -eq 'javascript' -and $_.id -eq 'tauri' }) | Select-Object -First 1
$asyncCalls = @($tauriJs.facts | Where-Object kind -eq 'ASYNC_CALLS')
Require (@($overlayJs.languages | Where-Object { $_.id -eq 'javascript' -and $_.status -eq 'indexed' }).Count -eq 1) 'overlay JavaScript was not indexed'
Require ($null -ne $tauriJs) 'Tauri JavaScript pack was not detected'
Require ($asyncCalls.Count -ge 4) "expected at least 4 Tauri invoke facts, found $($asyncCalls.Count)"
Require (@($asyncCalls | Where-Object {
            [string]::IsNullOrWhiteSpace($_.properties.target) -and $_.properties.resolution -ne 'dynamic'
        }).Count -eq 0) 'a Tauri invoke fact has neither a literal target nor a dynamic resolution marker'
Require (@($asyncCalls | Where-Object { $_.properties.target -in @('start_live_audio_stream', 'prewarm_live_audio_stream', 'stop_live_audio_stream') }).Count -eq 3) 'literal Tauri invoke targets do not match source'
Require (@($overlayJsArchitecture.nodes | Where-Object { $_.id -eq 'external:npm:@tauri-apps/api' }).Count -eq 1) 'overlay architecture has no Tauri JavaScript boundary'
Write-Host "PASS overlay-js: invoke_facts=$($asyncCalls.Count)"

$overlayRustStage = Join-Path $OutputRoot 'overlay-rust'
Copy-Tree (Join-Path $ProjectRoot 'client\overlay\src-tauri\src') (Join-Path $overlayRustStage 'src')
Copy-Item -LiteralPath (Join-Path $ProjectRoot 'client\overlay\src-tauri\Cargo.toml') -Destination $overlayRustStage -Force
$cargoLock = Join-Path $ProjectRoot 'client\overlay\src-tauri\Cargo.lock'
if (Test-Path -LiteralPath $cargoLock) {
    Copy-Item -LiteralPath $cargoLock -Destination $overlayRustStage -Force
}
$overlayRust = Invoke-Index 'overlay-rust' $overlayRustStage
$overlayRustArchitecture = Read-Architecture (Join-Path $OutputRoot 'overlay-rust.architecture.json')
$tauriRust = @($overlayRust.frameworks | Where-Object { $_.language -eq 'rust' -and $_.id -eq 'tauri' }) | Select-Object -First 1
$rpc = @($tauriRust.facts | Where-Object kind -eq 'RPC_ENDPOINT')
$rpcHandles = @($overlayRust.framework_relations | Where-Object { $_.framework -eq 'tauri' -and $_.kind -eq 'HANDLES' })
Require (@($overlayRust.languages | Where-Object { $_.id -eq 'rust' -and $_.status -eq 'indexed' }).Count -eq 1) 'overlay Rust was not indexed'
Require ($null -ne $tauriRust) 'Tauri Rust pack was not detected'
Require ($rpc.Count -gt 0) 'Tauri Rust command facts are empty'
Require ($rpcHandles.Count -gt 0) 'Tauri Rust HANDLES relations are empty'
Require (@($overlayRustArchitecture.nodes | Where-Object { $_.id -eq 'external:cargo:tauri' }).Count -eq 1) 'overlay architecture has no Tauri Rust boundary'
Write-Host "PASS overlay-rust: rpc=$($rpc.Count) handles=$($rpcHandles.Count)"

if ($IncludeLegacy) {
    $legacyStage = Join-Path $OutputRoot 'legacy'
    Copy-Tree (Join-Path $ProjectRoot 'legacy') $legacyStage
    Add-JsConfig $legacyStage
    $legacy = Invoke-Index 'legacy' $legacyStage
    Write-Host "PASS legacy: documents=$(@($legacy.documents).Count)"
}

$comparison = Join-Path $scriptRoot 'compare-index-to-source.ps1'
$comparisonShell = if ($PSVersionTable.PSEdition -eq 'Core') {
    Join-Path $PSHOME 'pwsh.exe'
} else {
    (Get-Command pwsh.exe -ErrorAction SilentlyContinue).Source
}
if ([string]::IsNullOrWhiteSpace($comparisonShell)) {
    $comparisonShell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
}
if (-not (Test-Path -LiteralPath $comparisonShell)) {
    throw "Windows PowerShell was not found for source comparison: $comparisonShell"
}
& $comparisonShell -NoProfile -ExecutionPolicy Bypass -File $comparison -ProjectRoot $ProjectRoot -OutputRoot $OutputRoot
if ($LASTEXITCODE -ne 0) { throw 'source comparison failed' }

Write-Host "external project gate: passed output=$OutputRoot"
