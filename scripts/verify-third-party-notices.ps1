param(
  [switch]$Release
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $root "src-tauri\engines\manifest.json"
$noticesPath = Join-Path $root "THIRD_PARTY_NOTICES.md"
$licensePath = Join-Path $root "LICENSE"

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "Engine manifest is missing: $manifestPath"
}
if (-not (Test-Path -LiteralPath $noticesPath -PathType Leaf)) {
  throw "Third-party notices are missing: $noticesPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$notices = Get-Content -LiteralPath $noticesPath -Raw
$errors = [System.Collections.Generic.List[string]]::new()

foreach ($engine in @($manifest.engines)) {
  if ([string]::IsNullOrWhiteSpace($engine.id)) {
    $errors.Add("manifest contains an engine without an id")
    continue
  }
  if ([string]::IsNullOrWhiteSpace($engine.sourceRepository)) {
    $errors.Add("$($engine.id): sourceRepository is missing")
  } elseif (-not $notices.Contains([string]$engine.sourceRepository)) {
    $errors.Add("$($engine.id): source repository is absent from THIRD_PARTY_NOTICES.md")
  }
  if ([string]::IsNullOrWhiteSpace($engine.licenseId)) {
    $errors.Add("$($engine.id): licenseId is missing")
  } elseif (-not $notices.Contains([string]$engine.licenseId)) {
    $errors.Add("$($engine.id): license id '$($engine.licenseId)' is absent from THIRD_PARTY_NOTICES.md")
  }
  if ([string]::IsNullOrWhiteSpace($engine.executable.fileName) -or
      -not $notices.Contains([string]$engine.executable.fileName)) {
    $errors.Add("$($engine.id): executable notice is missing")
  }
  if ($null -eq $engine.releaseReady) {
    $errors.Add("$($engine.id): releaseReady decision is missing")
  }
  if ($Release -and $engine.releaseReady -ne $true) {
    $errors.Add("$($engine.id): engine is not releaseReady")
  }
}

foreach ($requiredText in @(
  "Permission is hereby granted, free of charge",
  'THE SOFTWARE IS PROVIDED "AS IS"',
  "Copyright (c) 2025 DeusData",
  "Copyright (c) 2026"
)) {
  if (-not $notices.Contains($requiredText)) {
    $errors.Add("required license text is missing: $requiredText")
  }
}

if ($Release) {
  if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
    $errors.Add("product LICENSE is missing")
  } else {
    $productLicense = Get-Content -LiteralPath $licensePath -Raw
    if ($productLicense -match "placeholder|not licensed for redistribution") {
      $errors.Add("product redistribution license is still an owner decision gate")
    }
  }
}

if ($errors.Count -gt 0) {
  $errors | ForEach-Object { Write-Error $_ }
  exit 1
}

Write-Output "PASS: engine notices and license texts match the manifest."
if (-not $Release) {
  Write-Output "NOTE: run with -Release to enforce the product redistribution-license gate."
}
