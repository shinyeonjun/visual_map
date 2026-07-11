param(
  [switch]$SkipBuildChecks,
  [switch]$SkipDependencyAudit
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure([string]$Message) {
  $script:failures.Add($Message)
}

function Invoke-Checked([string]$Label, [scriptblock]$Command) {
  Write-Output "CHECK: $Label"
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed with exit code $LASTEXITCODE"
  }
}

$tauriConfigPath = Join-Path $root "src-tauri\tauri.conf.json"
$capabilityPath = Join-Path $root "src-tauri\capabilities\default.json"
$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw | ConvertFrom-Json
$capability = Get-Content -LiteralPath $capabilityPath -Raw | ConvertFrom-Json
$productionCsp = [string]$tauriConfig.app.security.csp

if ([string]::IsNullOrWhiteSpace($productionCsp) -or $productionCsp -notmatch "default-src 'self'") {
  Add-Failure "production CSP must define default-src 'self'"
}
if ($productionCsp -match "unsafe-eval|http://localhost|ws://") {
  Add-Failure "production CSP contains a development-only script/network allowance"
}

$forbiddenCapabilityPrefixes = @(
  "shell:",
  "process:",
  "fs:",
  "opener:",
  "http:"
)
foreach ($permission in @($capability.permissions)) {
  foreach ($prefix in $forbiddenCapabilityPrefixes) {
    if ([string]$permission -like "$prefix*") {
      Add-Failure "unexpected broad Tauri capability: $permission"
    }
  }
}

$sourceRoots = @(
  (Join-Path $root "src"),
  (Join-Path $root "src-tauri\src")
)
$secretPattern = '-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[0-9A-Z]{16}'
$secretMatches = & rg -n --pcre2 -- $secretPattern @sourceRoots 2>$null
if ($LASTEXITCODE -eq 0) {
  Add-Failure "a private-key or credential-shaped literal was found: $($secretMatches -join '; ')"
} elseif ($LASTEXITCODE -ne 1) {
  Add-Failure "credential scan could not run"
}

$unsafeDbMatches = & rg -n --pcre2 -- 'unsafe-row-sampling|row_data_access\s*[:=]\s*true|(?i)arbitrary\s+sql\s+console' @sourceRoots 2>$null
if ($LASTEXITCODE -eq 0) {
  Add-Failure "row-data or arbitrary-SQL product path detected: $($unsafeDbMatches -join '; ')"
} elseif ($LASTEXITCODE -ne 1) {
  Add-Failure "row-data boundary scan could not run"
}

$dynamicJsMatches = & rg -n --pcre2 -- '(^|[^A-Za-z])(eval\s*\(|new\s+Function\s*\()' (Join-Path $root "src") 2>$null
if ($LASTEXITCODE -eq 0) {
  Add-Failure "dynamic JavaScript execution detected: $($dynamicJsMatches -join '; ')"
} elseif ($LASTEXITCODE -ne 1) {
  Add-Failure "dynamic JavaScript scan could not run"
}

if ($failures.Count -gt 0) {
  $failures | ForEach-Object { Write-Error $_ }
  exit 1
}

Push-Location $root
try {
  Invoke-Checked "product version consistency" { & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-product-version.ps1 }
  Invoke-Checked "engine notice coverage" { & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-third-party-notices.ps1 }
  Invoke-Checked "declared local engine integrity" { & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\prepare-engines.ps1 -VerifyOnly -AllowDevelopmentArtifact }

  if (-not $SkipDependencyAudit) {
    Invoke-Checked "production npm dependency audit" { & npm audit --omit=dev --audit-level=high }
  }
  if (-not $SkipBuildChecks) {
    Invoke-Checked "Rust security and persistence regression suite" { & cargo test --locked --manifest-path .\src-tauri\Cargo.toml }
    Invoke-Checked "frontend type safety" { & npm run typecheck }
  }
} finally {
  Pop-Location
}

Write-Output "PASS: local security audit completed without widening the metadata-only boundary."
