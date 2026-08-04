[CmdletBinding()]
param(
    [string]$SourceRoot,
    [ValidateSet("Full", "Compact")]
    [string]$BundleMode = "Full",
    [string]$ProviderBaseUrl = $env:VISUAL_MAP_PROVIDER_BASE_URL,
    [switch]$Release,
    [switch]$VerifyOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression

$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = $env:VISUAL_MAP_PROVIDERS_ROOT
}
if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Join-Path $repoRoot "code_memory\providers"
}
$SourceRoot = [IO.Path]::GetFullPath($SourceRoot)
$BundleRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "src-tauri\engines\provider-bundles"))
$CatalogPath = Join-Path $BundleRoot "providers-manifest.json"
$SignaturePath = Join-Path $BundleRoot "providers-manifest.sig"
$DevelopmentPublicKey = "IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="
$SignerManifest = Join-Path $repoRoot "src-tauri\Cargo.toml"

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace("-", "")
    } finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

function Get-StringSha256([string]$Value) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
        return ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace("-", "")
    } finally {
        $algorithm.Dispose()
    }
}

function Compress-Directory([string]$SourceDirectory, [string]$ArchivePath) {
    $parent = Split-Path -Parent $SourceDirectory
    $zip = [IO.Compression.ZipFile]::Open($ArchivePath, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($file in @(Get-ChildItem -LiteralPath $SourceDirectory -File -Recurse -Force | Sort-Object FullName)) {
            $relativePath = $file.FullName.Substring($parent.Length + 1).Replace("\", "/")
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $zip,
                $file.FullName,
                $relativePath,
                [IO.Compression.CompressionLevel]::SmallestSize
            ) | Out-Null
        }
    } finally {
        $zip.Dispose()
    }
}

function Compress-RootFiles([string]$SourceDirectory, [string]$ArchivePath, [string[]]$FileNames) {
    $zip = [IO.Compression.ZipFile]::Open($ArchivePath, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($fileName in $FileNames) {
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $zip,
                (Join-Path $SourceDirectory $fileName),
                $fileName.Replace("\", "/"),
                [IO.Compression.CompressionLevel]::SmallestSize
            ) | Out-Null
        }
    } finally {
        $zip.Dispose()
    }
}

function Get-EntryPoint([string]$RelativePath) {
    $path = Join-Path $SourceRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required provider entrypoint was not found: $path"
    }
    $file = Get-Item -LiteralPath $path
    return [pscustomobject]@{
        path = $RelativePath.Replace("\", "/")
        sha256 = Get-Sha256 $path
        bytes = [uint64]$file.Length
    }
}

function Get-PublicKey {
    if ($Release) {
        if ([string]::IsNullOrWhiteSpace($env:VISUAL_MAP_PROVIDER_CATALOG_PUBLIC_KEY)) {
            throw "Release provider catalogs require VISUAL_MAP_PROVIDER_CATALOG_PUBLIC_KEY."
        }
        return $env:VISUAL_MAP_PROVIDER_CATALOG_PUBLIC_KEY.Trim()
    }
    return $DevelopmentPublicKey
}

function Invoke-Signer([string[]]$Arguments) {
    & cargo run --quiet --locked --manifest-path $SignerManifest --bin provider-catalog-sign -- @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Provider catalog signer failed with exit code $LASTEXITCODE."
    }
}

function Test-Catalog {
    if (-not (Test-Path -LiteralPath $CatalogPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $SignaturePath -PathType Leaf)) {
        throw "Signed provider catalog was not found in $BundleRoot"
    }
    $publicKey = Get-PublicKey
    Invoke-Signer @(
        "verify", "--catalog", $CatalogPath, "--signature", $SignaturePath,
        "--public-key", $publicKey
    )
    $catalog = Get-Content -LiteralPath $CatalogPath -Raw | ConvertFrom-Json
    if ([int]$catalog.schemaVersion -ne 2) {
        throw "Unsupported provider catalog schema: $($catalog.schemaVersion)"
    }
    $declaredEntryPoints = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($pack in @($catalog.packs)) {
        if ([string]$pack.id -ne "core" -and @($pack.languages).Count -eq 0) {
            throw "Provider catalog contains an unreachable pack: $($pack.id)"
        }
        $archivePath = Join-Path $BundleRoot ([string]$pack.fileName)
        if (Test-Path -LiteralPath $archivePath -PathType Leaf) {
            if ((Get-Item -LiteralPath $archivePath).Length -ne [uint64]$pack.compressedBytes -or
                (Get-Sha256 $archivePath) -ne [string]$pack.sha256) {
                throw "Provider bundle verification failed: $archivePath"
            }
        } elseif ([string]::IsNullOrWhiteSpace([string]$pack.downloadUrl) -or
            -not ([string]$pack.downloadUrl).StartsWith("https://", [StringComparison]::OrdinalIgnoreCase)) {
            throw "Provider pack has neither a local archive nor an HTTPS download URL: $($pack.id)"
        }
        foreach ($entryPoint in @($pack.entrypoints)) {
            [void]$declaredEntryPoints.Add(([string]$entryPoint.path).Replace("\", "/"))
        }
    }
    $providerManifest = Get-Content -LiteralPath (Join-Path $SourceRoot "manifest.json") -Raw | ConvertFrom-Json
    foreach ($provider in @($providerManifest.providers)) {
        $providerPath = ([string]$provider.path).Replace("\", "/")
        if (-not $declaredEntryPoints.Contains($providerPath)) {
            throw "Provider catalog does not verify declared provider entrypoint: $providerPath"
        }
    }
    Write-Host "Verified signed provider catalog: $CatalogPath"
}

if (-not (Test-Path -LiteralPath $SourceRoot -PathType Container)) {
    throw "Provider source directory was not found: $SourceRoot"
}
$providerManifestPath = Join-Path $SourceRoot "manifest.json"
if (-not (Test-Path -LiteralPath $providerManifestPath -PathType Leaf)) {
    throw "Provider manifest was not found: $providerManifestPath"
}
if ($BundleMode -eq "Compact") {
    if ([string]::IsNullOrWhiteSpace($ProviderBaseUrl)) {
        throw "Compact provider bundles require VISUAL_MAP_PROVIDER_BASE_URL or -ProviderBaseUrl."
    }
    $baseUri = [Uri]$ProviderBaseUrl
    if ($baseUri.Scheme -ne "https") {
        throw "Provider download base URL must use HTTPS."
    }
    $ProviderBaseUrl = $ProviderBaseUrl.TrimEnd("/")
}
if ($VerifyOnly) {
    Test-Catalog
    exit 0
}

$packLanguages = @{
    core = [string[]]@()
    node = [string[]]@("typescript", "javascript", "python")
    java = [string[]]@("java")
    dotnet = [string[]]@("csharp")
    clang = [string[]]@("c", "cpp")
    go = [string[]]@("go")
    rust = [string[]]@("rust")
    php = [string[]]@("php")
    ruby = [string[]]@("ruby")
    dart = [string[]]@("dart")
}
$packEntryPoints = @{
    core = [string[]]@("manifest.json")
    node = [string[]]@("node/project-model.cjs", "node/runtime/node.exe", "node/scip-typescript.cmd", "node/pyright-langserver.cmd")
    java = [string[]]@("java/jdtls.cmd", "java/runtime/bin/java.exe")
    dotnet = [string[]]@("dotnet/scip-dotnet.exe", "dotnet/runtime/dotnet.exe")
    clang = [string[]]@("clang/bin/clangd.exe")
    go = [string[]]@("go/gopls.exe", "go/runtime/bin/go.exe")
    rust = [string[]]@("rust/toolchain/bin/rust-analyzer.exe", "rust/toolchain/bin/cargo.exe", "rust/toolchain/bin/rustc.exe")
    php = [string[]]@("php/scip-php.cmd", "php/runtime/php.exe")
    ruby = [string[]]@("ruby/runtime/bin/ruby-lsp.bat", "ruby/runtime/bin/ruby.exe")
    dart = [string[]]@("dart/sdk/bin/dart.exe")
}

# Python semantics are provided by Pyright in the node pack. The standalone
# Python runtime is not referenced by the provider manifest or any language.
$unreferencedSourceDirectories = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
[void]$unreferencedSourceDirectories.Add("python")

if (Test-Path -LiteralPath $BundleRoot) {
    Remove-Item -LiteralPath $BundleRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $BundleRoot -Force | Out-Null
$packs = @()
$rootMetadata = [Collections.Generic.List[string]]::new()
$rootMetadata.Add("manifest.json")
if (Test-Path -LiteralPath (Join-Path $SourceRoot "README.md") -PathType Leaf) {
    $rootMetadata.Add("README.md")
}
$sources = @([pscustomobject]@{
    id = "core"
    path = $SourceRoot
    files = [string[]]$rootMetadata.ToArray()
})
foreach ($directory in @(Get-ChildItem -LiteralPath $SourceRoot -Directory -Force | Sort-Object Name)) {
    if ($unreferencedSourceDirectories.Contains($directory.Name)) {
        continue
    }
    if (-not $packLanguages.ContainsKey($directory.Name)) {
        throw "Provider directory has no catalog mapping: $($directory.Name)"
    }
    $sources += [pscustomobject]@{ id = $directory.Name; path = $directory.FullName; files = $null }
}

foreach ($source in $sources) {
    $archiveName = "providers-$($source.id).zip"
    $archivePath = Join-Path $BundleRoot $archiveName
    Write-Host "Compressing provider pack: $($source.id)"
    if ($source.id -eq "core") {
        Compress-RootFiles $SourceRoot $archivePath $source.files
        $unpackedBytes = [uint64](($source.files | ForEach-Object { (Get-Item -LiteralPath (Join-Path $SourceRoot $_)).Length } | Measure-Object -Sum).Sum)
    } else {
        Compress-Directory $source.path $archivePath
        $unpackedBytes = [uint64]((Get-ChildItem -LiteralPath $source.path -File -Recurse -Force | Measure-Object Length -Sum).Sum)
    }
    $sha256 = Get-Sha256 $archivePath
    $entryPoints = @($packEntryPoints[$source.id] | ForEach-Object { Get-EntryPoint $_ })
    $downloadUrl = if ([string]::IsNullOrWhiteSpace($ProviderBaseUrl)) { $null } else { "$ProviderBaseUrl/$archiveName" }
    $packs += [pscustomobject]@{
        id = [string]$source.id
        version = $sha256.Substring(0, 16).ToLowerInvariant()
        fileName = $archiveName
        sha256 = $sha256.ToLowerInvariant()
        compressedBytes = [uint64](Get-Item -LiteralPath $archivePath).Length
        unpackedBytes = $unpackedBytes
        languages = [string[]]$packLanguages[$source.id]
        dependencies = if ($source.id -eq "core") { [string[]]@() } else { [string[]]@("core") }
        entrypoints = $entryPoints
        downloadUrl = $downloadUrl
    }
}

$publicKey = Get-PublicKey
$manifestHash = Get-Sha256 $providerManifestPath
$catalog = [pscustomobject]@{
    schemaVersion = 2
    catalogVersion = $manifestHash.Substring(0, 16).ToLowerInvariant()
    keyId = (Get-StringSha256 $publicKey).Substring(0, 16).ToLowerInvariant()
    platform = "windows-x86_64"
    packs = $packs
    revocations = @()
}
[IO.File]::WriteAllText(
    $CatalogPath,
    ($catalog | ConvertTo-Json -Depth 8 -Compress),
    [Text.UTF8Encoding]::new($false)
)
if ($Release) {
    Invoke-Signer @("sign", "--catalog", $CatalogPath, "--signature", $SignaturePath)
} else {
    Invoke-Signer @("sign", "--catalog", $CatalogPath, "--signature", $SignaturePath, "--development")
}
Test-Catalog
if ($Release) {
    & (Join-Path $PSScriptRoot "run-provider-bundle-gate.ps1") -Release
    if ($LASTEXITCODE -ne 0) {
        throw "Provider bundle reliability gate failed with exit code $LASTEXITCODE."
    }
}
if ($BundleMode -eq "Compact") {
    Get-ChildItem -LiteralPath $BundleRoot -Filter "providers-*.zip" -File | Remove-Item -Force
    Test-Catalog
}
Write-Host "Created $($packs.Count) signed provider pack records ($BundleMode): $BundleRoot"
