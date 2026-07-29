param(
    [Parameter(Mandatory = $true)]
    [string]$ProjectRoot,
    [string]$OutputRoot
)

$ErrorActionPreference = 'Stop'
$scriptRoot = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputRoot)) { $OutputRoot = Join-Path $scriptRoot '..\..\build\external-gate' }
$ProjectRoot = (Resolve-Path $ProjectRoot).Path
$OutputRoot = (Resolve-Path $OutputRoot).Path

function Read-Json([string]$Path, [string]$Property) {
    $pwsh = if ($PSVersionTable.PSEdition -eq 'Core') {
        [pscustomobject]@{ Source = (Join-Path $PSHOME 'pwsh.exe') }
    } else {
        Get-Command pwsh -ErrorAction SilentlyContinue
    }
    if ($null -ne $pwsh) {
        $literalPath = $Path.Replace("'", "''")
        $code = "`$value = Get-Content -LiteralPath '$literalPath' -Raw | ConvertFrom-Json -Depth 1000; `$value | Select-Object $Property | ConvertTo-Json -Depth 100"
        return ((& $pwsh.Source -NoProfile -Command $code) -join "`n") | ConvertFrom-Json
    }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Require([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "source comparison failed: $Message" }
}

function Get-Facts([string]$Path, [string]$Framework, [string]$Kind) {
    $index = Read-Json $Path 'frameworks'
    $pack = @($index.frameworks | Where-Object id -eq $Framework) | Select-Object -First 1
    Require ($null -ne $pack) "$Framework was not detected in $Path"
    return @($pack.facts | Where-Object kind -eq $Kind)
}

function Get-ActualRoutes([string]$RoutesRoot) {
    $pattern = @'
(?m)^\s*@(?:router|app)\.(get|post|put|patch|delete|options|head)\s*\(\s*["'']([^"'']+)["'']
'@
    $regex = [regex]$pattern
    $result = @()
    Get-ChildItem -LiteralPath $RoutesRoot -Recurse -File -Filter '*.py' | ForEach-Object {
        $relative = $_.FullName.Substring($RoutesRoot.Length + 1).Replace('\', '/')
        $source = Get-Content -LiteralPath $_.FullName -Raw
        foreach ($match in $regex.Matches($source)) {
            $result += [pscustomobject]@{
                source_file = "app/api/http/routes/$relative"
                method = $match.Groups[1].Value.ToUpperInvariant()
                local_path = $match.Groups[2].Value
                matched = $false
            }
        }
    }
    return $result
}

function RouteMatches([object]$Actual, [object]$Fact) {
    if ($Actual.source_file -ne $Fact.source_file -or $Actual.method -ne $Fact.method) {
        return $false
    }
    $actualPath = $Actual.local_path
    $indexedPath = [string]$Fact.path
    return $actualPath -eq '/' -or $indexedPath -eq $actualPath -or $indexedPath.EndsWith($actualPath)
}

$actualRoutes = @(Get-ActualRoutes (Join-Path $ProjectRoot 'server\app\api\http\routes'))
$indexedRoutes = @(Get-Facts (Join-Path $OutputRoot 'server-routes.json') 'fastapi' 'HTTP_ROUTE')
Require ($actualRoutes.Count -eq $indexedRoutes.Count) "FastAPI route count source=$($actualRoutes.Count) index=$($indexedRoutes.Count)"
foreach ($fact in $indexedRoutes) {
    $match = $actualRoutes | Where-Object { -not $_.matched -and (RouteMatches $_ $fact) } | Select-Object -First 1
    Require ($null -ne $match) "indexed route has no matching source: $($fact.method) $($fact.path) $($fact.source_file)"
    $match.matched = $true
}
Require (@($actualRoutes | Where-Object { -not $_.matched }).Count -eq 0) 'source has an unindexed FastAPI route'
Write-Host "PASS source/server: routes=$($actualRoutes.Count) indexed=$($indexedRoutes.Count)"

$componentPattern = @'
(?m)\b(?:function\s+([A-Z][A-Za-z0-9_]*)\s*\(|(?:const|let)\s+([A-Z][A-Za-z0-9_]*)\s*=\s*(?:\([^)]*\)|[A-Za-z0-9_]*)\s*=>|class\s+([A-Z][A-Za-z0-9_]*)\s+extends)
'@
$componentRegex = [regex]$componentPattern
$actualComponents = @()
$webRoot = Join-Path $ProjectRoot 'client\web'
Get-ChildItem -LiteralPath (Join-Path $webRoot 'src') -Recurse -File | Where-Object { $_.Extension -in '.js', '.jsx', '.ts', '.tsx' } | ForEach-Object {
    $relative = $_.FullName.Substring($webRoot.Length + 1).Replace('\', '/')
    $source = Get-Content -LiteralPath $_.FullName -Raw
    foreach ($match in $componentRegex.Matches($source)) {
        $name = @($match.Groups | Select-Object -Skip 1 | Where-Object Success | Select-Object -First 1).Value
        $actualComponents += [pscustomobject]@{ path = $relative; name = $name; matched = $false }
    }
}
$architecture = Read-Json (Join-Path $OutputRoot 'web.architecture.json') 'nodes'
$indexedComponents = @($architecture.nodes | Where-Object kind -eq 'COMPONENT')
Require ($actualComponents.Count -eq $indexedComponents.Count) "React component count source=$($actualComponents.Count) index=$($indexedComponents.Count)"
foreach ($node in $indexedComponents) {
    $name = ([string]$node.label) -replace '^react: ', ''
    $match = $actualComponents | Where-Object { -not $_.matched -and $_.path -eq $node.path -and $_.name -eq $name } | Select-Object -First 1
    Require ($null -ne $match) "indexed component has no matching source: $($node.path) $name"
    $match.matched = $true
}
Write-Host "PASS source/web: components=$($actualComponents.Count) indexed=$($indexedComponents.Count)"

$invokeRegex = [regex]'invoke\(\s*["'']([^"'']+)["'']'
$actualInvokes = @()
$overlayRoot = Join-Path $ProjectRoot 'client\overlay'
Get-ChildItem -LiteralPath (Join-Path $overlayRoot 'src') -Recurse -File | Where-Object { $_.Extension -in '.js', '.jsx', '.ts', '.tsx' } | ForEach-Object {
    $source = Get-Content -LiteralPath $_.FullName -Raw
    foreach ($match in $invokeRegex.Matches($source)) { $actualInvokes += $match.Groups[1].Value }
}
$jsFacts = @(Get-Facts (Join-Path $OutputRoot 'overlay-js.json') 'tauri' 'ASYNC_CALLS')
$indexedInvokes = @($jsFacts | Where-Object { $_.properties.target } | ForEach-Object { [string]$_.properties.target })
Require ((@($actualInvokes | Sort-Object -Unique) -join '|') -eq (@($indexedInvokes | Sort-Object -Unique) -join '|')) 'literal Tauri invoke targets differ from source'
Require (@($jsFacts | Where-Object { $_.properties.resolution -eq 'dynamic' }).Count -eq 1) 'dynamic Tauri invoke was not marked dynamic'
Write-Host "PASS source/overlay-js: literal_invokes=$(@($actualInvokes | Sort-Object -Unique).Count) dynamic=1"

$commandRegex = [regex]'(?ms)#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)'
$actualCommands = @()
Get-ChildItem -LiteralPath (Join-Path $overlayRoot 'src-tauri\src') -Recurse -File -Filter '*.rs' | ForEach-Object {
    $source = Get-Content -LiteralPath $_.FullName -Raw
    foreach ($match in $commandRegex.Matches($source)) { $actualCommands += $match.Groups[1].Value }
}
$rpcFacts = @(Get-Facts (Join-Path $OutputRoot 'overlay-rust.json') 'tauri' 'RPC_ENDPOINT')
$indexedCommands = @($rpcFacts | ForEach-Object { if ([string]$_.symbol -match '#([^@]+)@') { $Matches[1] } })
Require ((@($actualCommands | Sort-Object -Unique) -join '|') -eq (@($indexedCommands | Sort-Object -Unique) -join '|')) 'Tauri Rust commands differ from source'
Write-Host "PASS source/overlay-rust: commands=$(@($actualCommands | Sort-Object -Unique).Count) indexed=$(@($indexedCommands | Sort-Object -Unique).Count)"

Write-Host 'source comparison: passed'

