param()

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$internal = $env:BACKEND_VISUAL_MAP_BUILD_SCOPE -eq "internal"

function Invoke-Checked([string]$Label, [scriptblock]$Command) {
  Write-Output "CHECK: $Label"
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed with exit code $LASTEXITCODE"
  }
}

Push-Location $root
try {
  Invoke-Checked "frontend build" { & npm run build }
  if ($internal) {
    Invoke-Checked "declared internal engines" {
      & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\prepare-engines.ps1 -VerifyOnly -AllowDevelopmentArtifact
    }
    Invoke-Checked "third-party notices" {
      & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-third-party-notices.ps1
    }
    Invoke-Checked "locked dependency inventory" {
      & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\generate-dependency-inventory.ps1 -VerifyOnly
    }
    Write-Warning "INTERNAL BUILD ONLY: use the release build path before redistributing this installer."
  } else {
    Invoke-Checked "release engine gate" { & npm run verify:release-engines }
    Invoke-Checked "release notices" {
      & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-third-party-notices.ps1 -Release
    }
    Invoke-Checked "release dependency inventory" { & npm run release:inventory }
  }
} finally {
  Pop-Location
}
