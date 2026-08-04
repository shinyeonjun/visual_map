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

function Get-Sha256([string]$Path) {
  $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  $algorithm = [Security.Cryptography.SHA256]::Create()
  try {
    ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace("-", "")
  } finally {
    $algorithm.Dispose()
    $stream.Dispose()
  }
}

Push-Location $root
try {
  Invoke-Checked "integrated code engine source build" {
    & cargo +1.96.1 build --locked --release --manifest-path .\code_memory\rust\Cargo.toml
  }
  $sourceCodeEngine = Join-Path $root "code_memory\rust\target\release\code-memory-language.exe"
  $bundledCodeEngine = Join-Path $root "src-tauri\engines\code-memory-language.exe"
  if (-not (Test-Path -LiteralPath $bundledCodeEngine -PathType Leaf)) {
    throw "Bundled code engine is missing: $bundledCodeEngine"
  }
  $sourceCodeEngineHash = Get-Sha256 $sourceCodeEngine
  $bundledCodeEngineHash = Get-Sha256 $bundledCodeEngine
  if ($sourceCodeEngineHash -ne $bundledCodeEngineHash) {
    throw "Bundled code engine is stale. Expected source build $sourceCodeEngineHash but found $bundledCodeEngineHash."
  }
  if ($internal -and $env:BACKEND_VISUAL_MAP_SKIP_PROVIDER_RESOURCES -eq "1") {
    Write-Output "SKIP: managed language providers (internal CI build)"
  } elseif ($internal) {
    Invoke-Checked "existing managed language provider bundle" {
      & .\scripts\prepare-provider-assets.ps1 -VerifyOnly
    }
  } else {
    $providerBundleMode = $env:VISUAL_MAP_PROVIDER_BUNDLE_MODE
    if ([string]::IsNullOrWhiteSpace($providerBundleMode)) {
      $providerBundleMode = if ([string]::IsNullOrWhiteSpace($env:VISUAL_MAP_PROVIDER_BASE_URL)) { "Full" } else { "Compact" }
    }
    Invoke-Checked "managed language providers" {
      & .\scripts\prepare-provider-assets.ps1 -Release -BundleMode $providerBundleMode
    }
  }
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
