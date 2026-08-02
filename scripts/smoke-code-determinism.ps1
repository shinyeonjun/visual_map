[CmdletBinding()]
param(
    [string]$EnginePath,
    [string]$ProvidersRoot,
    [string]$PacksRoot,
    [switch]$KeepFixture
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if ([string]::IsNullOrWhiteSpace($EnginePath)) {
    $EnginePath = Join-Path $repoRoot "src-tauri\engines\code-memory-language.exe"
}
$EnginePath = [IO.Path]::GetFullPath($EnginePath)
if (-not (Test-Path -LiteralPath $EnginePath -PathType Leaf)) {
    throw "Code engine not found: $EnginePath"
}

$engineRoot = Split-Path -Parent $EnginePath
if ([string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $candidate = Join-Path $engineRoot "providers"
    if (Test-Path -LiteralPath $candidate -PathType Container) {
        $ProvidersRoot = $candidate
    }
}
if ([string]::IsNullOrWhiteSpace($PacksRoot)) {
    $candidate = Join-Path $engineRoot "packs"
    if (Test-Path -LiteralPath (Join-Path $candidate "framework") -PathType Container) {
        # The bridge receives the directory that contains packs/, not packs/
        # itself. This keeps --packs-root consistent with the other gates.
        $PacksRoot = $engineRoot
    } else {
        $PacksRoot = Join-Path $repoRoot "src-tauri\engines"
    }
}

$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("backend-visual-map-determinism-" + [guid]::NewGuid().ToString("N"))
$cacheOne = Join-Path $fixtureRoot "cache-one"
$cacheTwo = Join-Path $fixtureRoot "cache-two"
$outOne = Join-Path $fixtureRoot "index-one.json"
$outTwo = Join-Path $fixtureRoot "index-two.json"
$architectureOne = Join-Path $fixtureRoot "architecture-one.json"
$architectureTwo = Join-Path $fixtureRoot "architecture-two.json"
$sourceRoot = Join-Path $fixtureRoot "repo"
New-Item -ItemType Directory -Path $sourceRoot -Force | Out-Null

try {
    New-Item -ItemType Directory -Path (Join-Path $sourceRoot "src") -Force | Out-Null
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "src\callee.ts"),
        "export function add(left: number, right: number): number { return left + right; }`n",
        [Text.UTF8Encoding]::new($false)
    )
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "src\caller.ts"),
        "import { add } from './callee';`nexport function total(): number { return add(1, 2); }`n",
        [Text.UTF8Encoding]::new($false)
    )
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "tsconfig.json"),
        '{"compilerOptions":{"module":"ESNext","moduleResolution":"Bundler","strict":true},"include":["src/**/*.ts"]}',
        [Text.UTF8Encoding]::new($false)
    )

    function Invoke-Index([string]$CacheRoot, [string]$OutputPath, [string]$ArchitecturePath) {
        $env:CODE_MEMORY_CACHE_ROOT = $CacheRoot
        $arguments = @(
            "index",
            "--root", $sourceRoot,
            "--out", $OutputPath,
            "--architecture-out", $ArchitecturePath,
            "--packs-root", ([IO.Path]::GetFullPath($PacksRoot))
        )
        if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
            $arguments += @("--providers-root", ([IO.Path]::GetFullPath($ProvidersRoot)))
        }
        & $EnginePath @arguments 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) {
            throw "Determinism index run failed with exit code $LASTEXITCODE"
        }
        if (-not (Test-Path -LiteralPath $OutputPath -PathType Leaf)) {
            throw "Determinism index run did not write $OutputPath"
        }
    }

    Invoke-Index $cacheOne $outOne $architectureOne
    Invoke-Index $cacheTwo $outTwo $architectureTwo

    $first = Get-Content -LiteralPath $outOne -Raw | ConvertFrom-Json
    $second = Get-Content -LiteralPath $outTwo -Raw | ConvertFrom-Json
    $firstLanguage = @($first.languages | Where-Object id -eq "typescript") | Select-Object -First 1
    $secondLanguage = @($second.languages | Where-Object id -eq "typescript") | Select-Object -First 1
    if ($null -eq $firstLanguage -or $firstLanguage.status -ne "indexed" -or
        $null -eq $secondLanguage -or $secondLanguage.status -ne "indexed") {
        throw "Determinism gate requires an indexed TypeScript provider in both runs."
    }

    $first.timings = @()
    $second.timings = @()
    $firstCanonical = $first | ConvertTo-Json -Compress -Depth 100
    $secondCanonical = $second | ConvertTo-Json -Compress -Depth 100
    if ($firstCanonical -ne $secondCanonical) {
        throw "Language index changed between identical independent runs."
    }

    $firstArchitecture = Get-Content -LiteralPath $architectureOne -Raw | ConvertFrom-Json
    $secondArchitecture = Get-Content -LiteralPath $architectureTwo -Raw | ConvertFrom-Json
    $firstArchitectureCanonical = $firstArchitecture | ConvertTo-Json -Compress -Depth 100
    $secondArchitectureCanonical = $secondArchitecture | ConvertTo-Json -Compress -Depth 100
    if ($firstArchitectureCanonical -ne $secondArchitectureCanonical) {
        throw "Architecture index changed between identical independent runs."
    }

    Write-Host "PASS deterministic code and architecture indexes"
}
finally {
    if (-not $KeepFixture -and (Test-Path -LiteralPath $fixtureRoot)) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    } elseif ($KeepFixture) {
        Write-Host "Kept determinism fixture: $fixtureRoot"
    }
}
