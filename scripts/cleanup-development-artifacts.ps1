param(
    [switch]$Execute,
    [ValidateRange(1, 1048576)]
    [int]$MinimumArtifactSizeMiB = 300,
    [switch]$IncludeAllArtifacts,
    [switch]$IncludeAnalysisCache,
    [switch]$IncludeRepositoryOutputs
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$artifactPrefixes = @(
    "codebase-workspace-",
    "code-memory-",
    "visual-map-",
    "backend-visual-map-"
)

function Get-NormalizedPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    return [System.IO.Path]::GetFullPath($Path).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
}

function Assert-DirectChildPath {
    param(
        [Parameter(Mandatory = $true)][string]$Parent,
        [Parameter(Mandatory = $true)][string]$Child
    )

    $normalizedParent = Get-NormalizedPath -Path $Parent
    $normalizedChild = Get-NormalizedPath -Path $Child
    $actualParent = [System.IO.Path]::GetDirectoryName($normalizedChild)
    if (-not [string]::Equals(
            $actualParent,
            $normalizedParent,
            [System.StringComparison]::OrdinalIgnoreCase
        )) {
        throw "Refusing to clean a path outside the approved parent: $normalizedChild"
    }

    return $normalizedChild
}

function Assert-DescendantPath {
    param(
        [Parameter(Mandatory = $true)][string]$Parent,
        [Parameter(Mandatory = $true)][string]$Child
    )

    $normalizedParent = Get-NormalizedPath -Path $Parent
    $normalizedChild = Get-NormalizedPath -Path $Child
    $requiredPrefix = $normalizedParent + [System.IO.Path]::DirectorySeparatorChar
    if (-not $normalizedChild.StartsWith(
            $requiredPrefix,
            [System.StringComparison]::OrdinalIgnoreCase
        )) {
        throw "Refusing to clean a path outside the approved repository: $normalizedChild"
    }
    return $normalizedChild
}

function Get-DirectorySizeBytes {
    param([Parameter(Mandatory = $true)][string]$Path)

    [int64]$totalBytes = 0
    foreach ($file in Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction SilentlyContinue) {
        $totalBytes += [int64]$file.Length
    }
    return $totalBytes
}

$tempRoot = Get-NormalizedPath -Path ([System.IO.Path]::GetTempPath())
$tempArtifacts = @(
    Get-ChildItem -LiteralPath $tempRoot -Directory -Force -ErrorAction SilentlyContinue |
        Where-Object {
            $candidateName = $_.Name
            $artifactPrefixes.Where({ $candidateName.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase) }).Count -gt 0
        }
)

$localAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
$cacheParent = Get-NormalizedPath -Path (Join-Path $localAppData "CodebaseWorkspace\cache")
$analysisCache = Assert-DirectChildPath -Parent $cacheParent -Child (Join-Path $cacheParent "code-memory")

$approvedTempArtifacts = @(
    foreach ($artifact in $tempArtifacts) {
    $approved = Assert-DirectChildPath -Parent $tempRoot -Child $artifact.FullName
    $approvedName = [System.IO.Path]::GetFileName($approved)
    $matchesPrefix = $artifactPrefixes.Where({
            $approvedName.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase)
        }).Count -gt 0
    if (-not $matchesPrefix) {
        throw "Refusing to clean an unexpected temp artifact: $approved"
    }
        [pscustomobject]@{
            Path = $approved
            SizeBytes = Get-DirectorySizeBytes -Path $approved
        }
    }
)

$selectedTempArtifacts = @(
    $approvedTempArtifacts |
        Where-Object {
            $IncludeAllArtifacts -or $_.SizeBytes -ge ([int64]$MinimumArtifactSizeMiB * 1MB)
        }
)

$repositoryRoot = Get-NormalizedPath -Path (Join-Path $PSScriptRoot "..")
$repositoryOutputRelatives = @(
    ".build",
    ".qa",
    ".tmp",
    "build",
    "coverage",
    "dist",
    "release-artifacts",
    "target",
    "code_memory\build",
    "db_memory\dist",
    "db_memory\.database-memory"
)
$selectedRepositoryOutputs = @()
if ($IncludeRepositoryOutputs) {
    $selectedRepositoryOutputs = @(
        foreach ($relativePath in $repositoryOutputRelatives) {
            $candidate = Assert-DescendantPath `
                -Parent $repositoryRoot `
                -Child (Join-Path $repositoryRoot $relativePath)
            if (Test-Path -LiteralPath $candidate) {
                [pscustomobject]@{
                    Path = $candidate
                    SizeBytes = Get-DirectorySizeBytes -Path $candidate
                }
            }
        }
    )
}

Write-Host "Discovered development artifacts: $($approvedTempArtifacts.Count)"
Write-Host "Selected temp artifacts: $($selectedTempArtifacts.Count) (minimum ${MinimumArtifactSizeMiB}MiB)"
$selectedTempArtifacts |
    Sort-Object SizeBytes -Descending |
    ForEach-Object {
        Write-Host ("  {0:N2} MiB  {1}" -f ($_.SizeBytes / 1MB), $_.Path)
    }
Write-Host "Analysis cache selected: $IncludeAnalysisCache ($analysisCache)"
Write-Host "Repository-generated outputs selected: $($selectedRepositoryOutputs.Count)"
$selectedRepositoryOutputs |
    Sort-Object SizeBytes -Descending |
    ForEach-Object {
        Write-Host ("  {0:N2} MiB  {1}" -f ($_.SizeBytes / 1MB), $_.Path)
    }
Write-Host "Preserved: workspace records, engine installations, WebView data, and source repositories"

if (-not $Execute) {
    Write-Host "Preview only. Re-run with -Execute to remove the approved targets."
    exit 0
}

$failedTargets = [System.Collections.Generic.List[string]]::new()
$tempFailureCount = 0
foreach ($artifact in $selectedTempArtifacts) {
    try {
        Remove-Item -LiteralPath ("\\?\" + $artifact.Path) -Recurse -Force -ErrorAction Stop
    }
    catch {
        $tempFailureCount++
        $failedTargets.Add("$($artifact.Path) :: $($_.Exception.Message)")
    }
}

$repositoryFailureCount = 0
foreach ($artifact in $selectedRepositoryOutputs) {
    try {
        Remove-Item -LiteralPath ("\\?\" + $artifact.Path) -Recurse -Force -ErrorAction Stop
    }
    catch {
        $repositoryFailureCount++
        $failedTargets.Add("$($artifact.Path) :: $($_.Exception.Message)")
    }
}

$analysisCacheRemoved = $false
if ($IncludeAnalysisCache -and (Test-Path -LiteralPath $analysisCache)) {
    Remove-Item -LiteralPath $analysisCache -Recurse -Force -ErrorAction Stop
    $analysisCacheRemoved = $true
}

Write-Host "Removed temp artifacts: $($selectedTempArtifacts.Count - $tempFailureCount)/$($selectedTempArtifacts.Count)"
Write-Host "Removed repository outputs: $($selectedRepositoryOutputs.Count - $repositoryFailureCount)/$($selectedRepositoryOutputs.Count)"
Write-Host "Removed analysis cache: $analysisCacheRemoved"

if ($failedTargets.Count -gt 0) {
    $failedTargets | ForEach-Object { Write-Error $_ }
    exit 1
}
