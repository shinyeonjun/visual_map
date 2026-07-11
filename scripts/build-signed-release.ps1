[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$CertificateThumbprint,
  [string]$TimestampUrl = "http://timestamp.digicert.com"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$thumbprint = ($CertificateThumbprint -replace "\s", "").ToUpperInvariant()
if ($thumbprint -notmatch "^[0-9A-F]{40}$") {
  throw "Certificate thumbprint must be a 40-character SHA-1 value."
}
$certificate = Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My -ErrorAction SilentlyContinue |
  Where-Object { $_.Thumbprint -eq $thumbprint -and $_.HasPrivateKey } |
  Select-Object -First 1
if (-not $certificate) {
  throw "A code-signing certificate with a private key was not found for thumbprint $thumbprint."
}

$configPath = Join-Path ([IO.Path]::GetTempPath()) ("backend-visual-map-signing-" + [guid]::NewGuid().ToString("N") + ".json")
$config = @{
  bundle = @{
    windows = @{
      certificateThumbprint = $thumbprint
      digestAlgorithm = "sha256"
      timestampUrl = $TimestampUrl
      tsp = $false
    }
  }
} | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText($configPath, $config, [Text.UTF8Encoding]::new($false))

Push-Location $root
try {
  & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\security-audit.ps1
  if ($LASTEXITCODE -ne 0) { throw "Security audit failed." }
  & npx tauri build --ci --config $configPath
  if ($LASTEXITCODE -ne 0) { throw "Signed Tauri build failed." }
  & powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\release-smoke.ps1 -RequireSignature
  if ($LASTEXITCODE -ne 0) { throw "Signed installer smoke failed." }
} finally {
  Pop-Location
  $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
  $resolvedConfig = [IO.Path]::GetFullPath($configPath)
  if ($resolvedConfig.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -LiteralPath $resolvedConfig -Force -ErrorAction SilentlyContinue
  }
}

