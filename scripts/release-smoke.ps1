[CmdletBinding()]
param(
  [string]$InstallerPath,
  [switch]$Internal,
  [switch]$RequireSignature,
  [switch]$ExerciseInstall,
  [switch]$AcknowledgeSystemChanges
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Get-SignToolPath {
  $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($command) { return $command.Source }
  $kits = "C:\Program Files (x86)\Windows Kits\10\bin"
  if (-not (Test-Path -LiteralPath $kits -PathType Container)) { return $null }
  return Get-ChildItem -LiteralPath $kits -Filter signtool.exe -File -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.Directory.Name -eq "x64" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
}

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

function Test-AuthenticodeSignature([string]$Path, [switch]$Required) {
  $signTool = Get-SignToolPath
  if (-not $signTool) {
    if ($Required) { throw "signtool.exe is required to verify Authenticode signatures." }
    return "Unknown (signtool unavailable)"
  }
  & $signTool verify /pa /q $Path *> $null
  if ($LASTEXITCODE -eq 0) { return "Valid" }
  if ($Required) { throw "Authenticode verification failed: $Path" }
  return "NotSignedOrInvalid"
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
$signatureStatus = Test-AuthenticodeSignature -Path $InstallerPath -Required:$RequireSignature
$hash = Get-Sha256 $InstallerPath

if ($ExerciseInstall) {
  $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
  $installRoot = Join-Path $tempBase ("bvm-release-smoke-" + [guid]::NewGuid().ToString("N"))
  $resolvedInstallRoot = [IO.Path]::GetFullPath($installRoot)
  if (-not $resolvedInstallRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use install path outside the temp directory: $resolvedInstallRoot"
  }

  $installed = $false
  try {
    $install = Start-Process -FilePath $InstallerPath -ArgumentList @("/S", "/D=$resolvedInstallRoot") -Wait -PassThru -WindowStyle Hidden
    if ($install.ExitCode -ne 0) { throw "Silent install failed with exit code $($install.ExitCode)." }
    $installed = $true
    $app = Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter "backend-visual-map.exe" -File -Recurse | Select-Object -First 1
    if (-not $app) { throw "Installed application executable was not found." }
    foreach ($engine in "codebase-memory-mcp.exe", "database-memory.exe") {
      if (-not (Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter $engine -File -Recurse | Select-Object -First 1)) {
        throw "Installed resource is missing: $engine"
      }
    }
    [void](Test-AuthenticodeSignature -Path $app.FullName -Required:$RequireSignature)

    $process = Start-Process -FilePath $app.FullName -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 5
    if ($process.HasExited -and $process.ExitCode -ne 0) {
      throw "Installed application exited with code $($process.ExitCode)."
    }
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }

    $uninstaller = Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter "*uninstall*.exe" -File -Recurse | Select-Object -First 1
    if (-not $uninstaller) { throw "Uninstaller was not found." }
    $uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru -WindowStyle Hidden
    if ($uninstall.ExitCode -ne 0) { throw "Silent uninstall failed with exit code $($uninstall.ExitCode)." }
    $installed = $false
  } finally {
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
  }
}

Write-Output "PASS: installer smoke completed."
Write-Output "Installer: $InstallerPath"
Write-Output "SHA256: $hash"
Write-Output "Signature: $signatureStatus"
