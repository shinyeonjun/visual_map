param(
    [switch]$VerifyOnly,
    [string]$OutputDirectory = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function ConvertFrom-JsonDictionary {
    param([string]$Json)

    if ($PSVersionTable.PSVersion.Major -ge 6) {
        return $Json | ConvertFrom-Json -AsHashtable
    }

    Add-Type -AssemblyName System.Web.Extensions
    $serializer = New-Object System.Web.Script.Serialization.JavaScriptSerializer
    $serializer.MaxJsonLength = [int]::MaxValue
    $serializer.RecursionLimit = 256
    return $serializer.DeserializeObject($Json)
}

function Get-MapValue {
    param(
        [object]$Map,
        [string]$Key
    )

    if ($null -eq $Map) {
        return $null
    }
    if ($Map -is [System.Collections.IDictionary]) {
        return $Map[$Key]
    }
    $property = $Map.PSObject.Properties[$Key]
    if ($null -ne $property) {
        return $property.Value
    }
    return $null
}

function Get-LicenseValue {
    param([object]$Entry)

    $license = Get-MapValue $Entry "license"
    if ($license -is [string] -and -not [string]::IsNullOrWhiteSpace($license)) {
        return $license.Trim()
    }
    $licenseFile = Get-MapValue $Entry "license_file"
    if ($null -eq $licenseFile) {
        $licenseFile = Get-MapValue $Entry "licenseFile"
    }
    if ($licenseFile -is [string] -and -not [string]::IsNullOrWhiteSpace($licenseFile)) {
        return "LicenseRef-File"
    }
    return $null
}

function Assert-SafeField {
    param(
        [string]$Value,
        [string]$Label,
        [int]$MaxLength
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "$Label is missing"
    }
    if ($Value.Length -gt $MaxLength -or $Value -match "[\x00-\x1f]") {
        throw "$Label is not a safe inventory field"
    }
}

function New-InventoryEntry {
    param(
        [string]$Ecosystem,
        [string]$Name,
        [string]$Version,
        [string]$License,
        [string]$Source
    )

    Assert-SafeField $Ecosystem "ecosystem" 16
    Assert-SafeField $Name "package name" 256
    Assert-SafeField $Version "package version" 128
    Assert-SafeField $License "package license" 512
    Assert-SafeField $Source "package source" 32
    return [pscustomobject][ordered]@{
        ecosystem = $Ecosystem
        name = $Name
        version = $Version
        license = $License
        source = $Source
    }
}

function Get-NpmPackageName {
    param(
        [string]$LockPath,
        [object]$Entry
    )

    $declaredName = Get-MapValue $Entry "name"
    if ($declaredName -is [string] -and -not [string]::IsNullOrWhiteSpace($declaredName)) {
        return $declaredName.Trim()
    }
    $normalized = $LockPath.Replace("\", "/")
    $marker = "node_modules/"
    $index = $normalized.LastIndexOf($marker, [System.StringComparison]::Ordinal)
    if ($index -lt 0) {
        throw "Cannot derive npm package name from lock entry"
    }
    $tail = $normalized.Substring($index + $marker.Length)
    $parts = @($tail.Split("/", [System.StringSplitOptions]::RemoveEmptyEntries))
    if ($parts.Count -eq 0) {
        throw "Cannot derive npm package name from lock entry"
    }
    if ($parts[0].StartsWith("@", [System.StringComparison]::Ordinal)) {
        if ($parts.Count -lt 2) {
            throw "Scoped npm lock entry is incomplete"
        }
        return "$($parts[0])/$($parts[1])"
    }
    return $parts[0]
}

function Get-NpmSource {
    param([object]$Entry)

    if ((Get-MapValue $Entry "link") -eq $true) {
        return "local"
    }
    $resolved = Get-MapValue $Entry "resolved"
    if ($resolved -is [string] -and $resolved.StartsWith("git", [System.StringComparison]::OrdinalIgnoreCase)) {
        return "git"
    }
    return "npm-registry"
}

function Get-CargoSource {
    param([object]$Entry)

    $source = Get-MapValue $Entry "source"
    if (-not ($source -is [string]) -or [string]::IsNullOrWhiteSpace($source)) {
        return "local"
    }
    if ($source.StartsWith("git+", [System.StringComparison]::OrdinalIgnoreCase)) {
        return "git"
    }
    if ($source -match "crates\.io") {
        return "crates.io"
    }
    return "cargo-registry"
}

function ConvertTo-MarkdownCell {
    param([string]$Value)
    return $Value.Replace("|", "\|")
}

$packageLockPath = Join-Path $repoRoot "package-lock.json"
$cargoManifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
if (-not (Test-Path -LiteralPath $packageLockPath -PathType Leaf)) {
    throw "package-lock.json is missing"
}
if (-not (Test-Path -LiteralPath $cargoManifestPath -PathType Leaf)) {
    throw "src-tauri/Cargo.toml is missing"
}

$packageLock = ConvertFrom-JsonDictionary ([System.IO.File]::ReadAllText($packageLockPath))
if ((Get-MapValue $packageLock "lockfileVersion") -ne 3) {
    throw "package-lock.json must use lockfileVersion 3"
}
$npmPackages = Get-MapValue $packageLock "packages"
if (-not ($npmPackages -is [System.Collections.IDictionary])) {
    throw "package-lock.json packages must be an object"
}
$npmRoot = Get-MapValue $npmPackages ""
if ($null -eq $npmRoot) {
    throw "package-lock.json root package entry is missing"
}

$cargoRaw = (& cargo metadata --locked --format-version 1 --manifest-path $cargoManifestPath | Out-String)
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($cargoRaw)) {
    throw "cargo metadata --locked failed"
}
$cargoMetadata = ConvertFrom-JsonDictionary $cargoRaw
$cargoPackages = @(Get-MapValue $cargoMetadata "packages")
$cargoResolve = Get-MapValue $cargoMetadata "resolve"
$cargoRootId = Get-MapValue $cargoResolve "root"
if ($cargoPackages.Count -eq 0 -or [string]::IsNullOrWhiteSpace([string]$cargoRootId)) {
    throw "cargo metadata did not return a root package"
}

$dependencies = New-Object System.Collections.Generic.List[object]
$missingLicenses = New-Object System.Collections.Generic.List[string]
$seen = New-Object System.Collections.Generic.HashSet[string]([System.StringComparer]::Ordinal)

foreach ($property in @($npmPackages.GetEnumerator() | Sort-Object { [string]$_.Key })) {
    $lockPath = [string]$property.Key
    if ($lockPath -eq "") {
        continue
    }
    $entry = $property.Value
    $name = Get-NpmPackageName $lockPath $entry
    $version = [string](Get-MapValue $entry "version")
    $license = Get-LicenseValue $entry
    $source = Get-NpmSource $entry
    Assert-SafeField $name "npm package name" 256
    Assert-SafeField $version "npm package version" 128
    if ([string]::IsNullOrWhiteSpace($license)) {
        $missingLicenses.Add("npm:$name@$version")
        continue
    }
    $key = "npm`0$name`0$version`0$license`0$source"
    if ($seen.Add($key)) {
        $dependencies.Add((New-InventoryEntry "npm" $name $version $license $source))
    }
}

$cargoRoot = $null
foreach ($entry in $cargoPackages) {
    $id = [string](Get-MapValue $entry "id")
    if ($id -eq [string]$cargoRootId) {
        $cargoRoot = $entry
        continue
    }
    $name = [string](Get-MapValue $entry "name")
    $version = [string](Get-MapValue $entry "version")
    $license = Get-LicenseValue $entry
    $source = Get-CargoSource $entry
    Assert-SafeField $name "Cargo package name" 256
    Assert-SafeField $version "Cargo package version" 128
    if ([string]::IsNullOrWhiteSpace($license)) {
        $missingLicenses.Add("cargo:$name@$version")
        continue
    }
    $key = "cargo`0$name`0$version`0$license`0$source"
    if ($seen.Add($key)) {
        $dependencies.Add((New-InventoryEntry "cargo" $name $version $license $source))
    }
}
if ($null -eq $cargoRoot) {
    throw "cargo metadata root package is missing from packages"
}
if ($missingLicenses.Count -gt 0) {
    throw "Dependency license metadata is missing: $($missingLicenses -join ', ')"
}

$npmRootLicense = Get-LicenseValue $npmRoot
if ([string]::IsNullOrWhiteSpace($npmRootLicense)) {
    $npmRootLicense = "UNDECLARED"
}
$cargoRootLicense = Get-LicenseValue $cargoRoot
if ([string]::IsNullOrWhiteSpace($cargoRootLicense)) {
    $cargoRootLicense = "UNDECLARED"
}
$ownerPackages = @(
    New-InventoryEntry "npm" ([string](Get-MapValue $npmRoot "name")) ([string](Get-MapValue $npmRoot "version")) $npmRootLicense "workspace-root"
    New-InventoryEntry "cargo" ([string](Get-MapValue $cargoRoot "name")) ([string](Get-MapValue $cargoRoot "version")) $cargoRootLicense "workspace-root"
)
$ownerGatePending = @($ownerPackages | Where-Object { $_.license -eq "UNDECLARED" }).Count -gt 0
$ownerGateStatus = "resolved"
if ($ownerGatePending) {
    $ownerGateStatus = "pending"
}

$sortedDependencies = @($dependencies | Sort-Object ecosystem, name, version, license, source)
$npmCount = @($sortedDependencies | Where-Object { $_.ecosystem -eq "npm" }).Count
$cargoCount = @($sortedDependencies | Where-Object { $_.ecosystem -eq "cargo" }).Count
$inventory = [pscustomobject][ordered]@{
    schemaVersion = 1
    ownerGates = @(
        [pscustomobject][ordered]@{
            id = "application-license"
            status = $ownerGateStatus
            reason = "The Backend Visual Map application license is an owner decision and does not block dependency inventory generation."
            packages = $ownerPackages
        }
    )
    dependencies = $sortedDependencies
    summary = [pscustomobject][ordered]@{
        npm = $npmCount
        cargo = $cargoCount
        total = $sortedDependencies.Count
    }
}

$json = $inventory | ConvertTo-Json -Depth 8 -Compress
if ([string]::IsNullOrWhiteSpace($json)) {
    throw "Dependency inventory JSON generation failed"
}

$markdown = New-Object System.Collections.Generic.List[string]
$markdown.Add("# Application Dependency Inventory")
$markdown.Add("")
$markdown.Add('Generated from `package-lock.json` v3 and `cargo metadata --locked --format-version 1`.')
$markdown.Add("Only normalized package identity, version, license, ecosystem, and source class are included.")
$markdown.Add("")
$markdown.Add("## Release owner gate")
$markdown.Add("")
$gateMark = "x"
if ($ownerGatePending) {
    $gateMark = " "
}
$markdown.Add(('- [{0}] Application license declared (`{1}`). This owner decision does not hide or waive dependency license failures.' -f $gateMark, $ownerGateStatus))
$markdown.Add("")
$markdown.Add("| Ecosystem | Package | Version | License | Source |")
$markdown.Add("| --- | --- | --- | --- | --- |")
foreach ($entry in $ownerPackages) {
    $markdown.Add("| $(ConvertTo-MarkdownCell $entry.ecosystem) | $(ConvertTo-MarkdownCell $entry.name) | $(ConvertTo-MarkdownCell $entry.version) | $(ConvertTo-MarkdownCell $entry.license) | $(ConvertTo-MarkdownCell $entry.source) |")
}
$markdown.Add("")
$markdown.Add("## Locked dependencies")
$markdown.Add("")
$markdown.Add("$($sortedDependencies.Count) unique locked package versions: $npmCount npm, $cargoCount Cargo.")
$markdown.Add("")
$markdown.Add("| Ecosystem | Package | Version | License | Source |")
$markdown.Add("| --- | --- | --- | --- | --- |")
foreach ($entry in $sortedDependencies) {
    $markdown.Add("| $(ConvertTo-MarkdownCell $entry.ecosystem) | $(ConvertTo-MarkdownCell $entry.name) | $(ConvertTo-MarkdownCell $entry.version) | $(ConvertTo-MarkdownCell $entry.license) | $(ConvertTo-MarkdownCell $entry.source) |")
}
$markdownText = $markdown -join [Environment]::NewLine

if ($VerifyOnly) {
    Write-Output "Dependency inventory verified in memory: $($sortedDependencies.Count) dependencies; owner gate $ownerGateStatus."
    exit 0
}

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot "release-artifacts"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot $OutputDirectory
}
[System.IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
$jsonPath = Join-Path $OutputDirectory "dependency-inventory.json"
$markdownPath = Join-Path $OutputDirectory "dependency-inventory.md"
[System.IO.File]::WriteAllText($jsonPath, $json + [Environment]::NewLine, $utf8NoBom)
[System.IO.File]::WriteAllText($markdownPath, $markdownText + [Environment]::NewLine, $utf8NoBom)
Write-Output "Dependency inventory written: $jsonPath"
Write-Output "Dependency inventory written: $markdownPath"
if ($ownerGatePending) {
    Write-Warning "Application license owner gate is pending; dependency licenses are complete."
}
