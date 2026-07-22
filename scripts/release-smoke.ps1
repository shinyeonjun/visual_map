[CmdletBinding()]
param(
  [string]$InstallerPath,
  [switch]$Internal,
  [switch]$ExerciseInstall,
  [switch]$AcknowledgeSystemChanges
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

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

function Get-ProductUninstallEntries {
  $roots = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
  )
  foreach ($registryRoot in $roots) {
    if (-not (Test-Path -LiteralPath $registryRoot)) { continue }
    foreach ($key in Get-ChildItem -LiteralPath $registryRoot -ErrorAction SilentlyContinue) {
      $entry = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction SilentlyContinue
      $displayName = $entry.PSObject.Properties["DisplayName"]
      if ($null -eq $displayName -or [string]$displayName.Value -ne "Backend Visual Map") { continue }
      $installLocation = $entry.PSObject.Properties["InstallLocation"]
      $uninstallString = $entry.PSObject.Properties["UninstallString"]
      [pscustomobject]@{
        Key = $key.PSPath
        InstallLocation = if ($installLocation) { [string]$installLocation.Value } else { "" }
        UninstallString = if ($uninstallString) { [string]$uninstallString.Value } else { "" }
      }
    }
  }
}

if ($ExerciseInstall -and -not $AcknowledgeSystemChanges) {
  throw "-ExerciseInstall requires -AcknowledgeSystemChanges because it writes installer state and then removes it."
}

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
  $candidates = @(Get-ChildItem -LiteralPath (Join-Path $root "src-tauri\target\release\bundle\nsis") -Filter "*.exe" -File -ErrorAction SilentlyContinue)
  if ($candidates.Count -ne 1) {
    throw "Expected exactly one NSIS installer, found $($candidates.Count)."
  }
  $InstallerPath = $candidates[0].FullName
}
$InstallerPath = [IO.Path]::GetFullPath($InstallerPath)
if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
  throw "Installer not found: $InstallerPath"
}

Push-Location $root
try {
  & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-product-version.ps1
  if ($LASTEXITCODE -ne 0) { throw "Product version verification failed." }
  if ($Internal) {
    & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\prepare-engines.ps1 -VerifyOnly -AllowDevelopmentArtifact
    if ($LASTEXITCODE -ne 0) { throw "Internal engine verification failed." }
    & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-third-party-notices.ps1
  } else {
    & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\prepare-engines.ps1 -VerifyOnly -Release
    if ($LASTEXITCODE -ne 0) { throw "Release engine verification failed." }
    & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-third-party-notices.ps1 -Release
  }
  if ($LASTEXITCODE -ne 0) { throw "Notice/license verification failed." }
} finally {
  Pop-Location
}

$stream = [IO.File]::OpenRead($InstallerPath)
try {
  if ($stream.ReadByte() -ne 0x4D -or $stream.ReadByte() -ne 0x5A) {
    throw "Installer is not a Windows PE executable."
  }
} finally {
  $stream.Dispose()
}

$installer = Get-Item -LiteralPath $InstallerPath
if ($installer.Length -lt 1MB) {
  throw "Installer is unexpectedly small: $($installer.Length) bytes."
}
$hash = Get-Sha256 $InstallerPath

if ($ExerciseInstall) {
  $existingEntries = @(Get-ProductUninstallEntries)
  if ($existingEntries.Count -gt 0) {
    throw "Refusing installer smoke because Backend Visual Map is already installed."
  }
  $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
  $installRoot = Join-Path $tempBase ("bvm-release-smoke-" + [guid]::NewGuid().ToString("N"))
  $resolvedInstallRoot = [IO.Path]::GetFullPath($installRoot)
  if (-not $resolvedInstallRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use install path outside the temp directory: $resolvedInstallRoot"
  }
  $smokeAppData = Join-Path $resolvedInstallRoot "smoke-app-data"
  $smokeWebViewData = Join-Path $resolvedInstallRoot "smoke-webview-data"

  $installed = $false
  $process = $null
  try {
    $install = Start-Process -FilePath $InstallerPath -ArgumentList @("/S", "/D=$resolvedInstallRoot") -Wait -PassThru -WindowStyle Hidden
    if ($install.ExitCode -ne 0) { throw "Silent install failed with exit code $($install.ExitCode)." }
    $installed = $true
    $installedEntries = @(Get-ProductUninstallEntries)
    if ($installedEntries.Count -eq 0) { throw "Installer did not register an uninstall entry." }
    foreach ($entry in $installedEntries) {
      $entryPaths = "$($entry.InstallLocation) $($entry.UninstallString)"
      if ($entryPaths.IndexOf($resolvedInstallRoot, [StringComparison]::OrdinalIgnoreCase) -lt 0) {
        throw "Installer registered an uninstall entry outside the isolated install root: $($entry.Key)"
      }
    }
    $app = Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter "backend-visual-map.exe" -File -Recurse | Select-Object -First 1
    if (-not $app) { throw "Installed application executable was not found." }
    foreach ($engine in "codebase-memory-mcp.exe", "database-memory.exe") {
      if (-not (Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter $engine -File -Recurse | Select-Object -First 1)) {
        throw "Installed resource is missing: $engine"
      }
    }
    $previousAppData = $env:BACKEND_VISUAL_MAP_APP_DATA_DIR
    $previousWebViewData = $env:WEBVIEW2_USER_DATA_FOLDER
    try {
      $env:BACKEND_VISUAL_MAP_APP_DATA_DIR = $smokeAppData
      $env:WEBVIEW2_USER_DATA_FOLDER = $smokeWebViewData
      $process = Start-Process -FilePath $app.FullName -PassThru -WindowStyle Hidden
    } finally {
      $env:BACKEND_VISUAL_MAP_APP_DATA_DIR = $previousAppData
      $env:WEBVIEW2_USER_DATA_FOLDER = $previousWebViewData
    }
    Start-Sleep -Seconds 5
    if ($process.HasExited) {
      throw "Installed application exited before the smoke window completed with code $($process.ExitCode)."
    }
    if ($Internal -and -not (Test-Path -LiteralPath $smokeAppData -PathType Container)) {
      throw "Installed internal application did not use the isolated app-data directory."
    }
    if (-not (Test-Path -LiteralPath $smokeWebViewData -PathType Container)) {
      throw "Installed application did not use the isolated WebView2 data directory."
    }
    Stop-Process -Id $process.Id -Force
    Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue

    $uninstaller = Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter "*uninstall*.exe" -File -Recurse | Select-Object -First 1
    if (-not $uninstaller) { throw "Uninstaller was not found." }
    $uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru -WindowStyle Hidden
    if ($uninstall.ExitCode -ne 0) { throw "Silent uninstall failed with exit code $($uninstall.ExitCode)." }
    $installed = $false
  } finally {
    if ($process -and -not $process.HasExited) {
      Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
      Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue
    }
    if ($installed -and (Test-Path -LiteralPath $resolvedInstallRoot)) {
      $cleanupUninstaller = Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter "*uninstall*.exe" -File -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
      if ($cleanupUninstaller) {
        Start-Process -FilePath $cleanupUninstaller.FullName -ArgumentList "/S" -Wait -WindowStyle Hidden -ErrorAction SilentlyContinue
      }
    }
    Start-Sleep -Milliseconds 500
    if (Test-Path -LiteralPath $resolvedInstallRoot) {
      Remove-Item -LiteralPath $resolvedInstallRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $resolvedInstallRoot) {
      throw "Installer smoke cleanup left files behind: $resolvedInstallRoot"
    }
    $remainingEntries = @(Get-ProductUninstallEntries)
    if ($remainingEntries.Count -gt 0) {
      throw "Installer smoke cleanup left uninstall registry entries behind."
    }
  }
}

Write-Output "PASS: installer smoke completed."
Write-Output "Installer: $InstallerPath"
Write-Output "SHA256: $hash"
Write-Output "Distribution: local validation only"
