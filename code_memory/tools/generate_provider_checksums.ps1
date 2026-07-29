param(
  [string]$ProviderRoot = (Join-Path $PSScriptRoot '..\providers')
)

$ProviderRoot = [IO.Path]::GetFullPath($ProviderRoot)
$OutputPath = Join-Path $ProviderRoot 'checksums.json'
$Files = Get-ChildItem -LiteralPath $ProviderRoot -File -Recurse |
  Where-Object { $_.FullName -ne $OutputPath } |
  Sort-Object FullName

$Sha256 = [Security.Cryptography.SHA256]::Create()
try {
  $Entries = foreach ($File in $Files) {
    $Stream = [IO.File]::OpenRead($File.FullName)
    try {
      $Digest = $Sha256.ComputeHash($Stream)
    } finally {
      $Stream.Dispose()
    }

    [ordered]@{
      path   = $File.FullName.Substring($ProviderRoot.Length + 1).Replace('\', '/')
      bytes  = [int64]$File.Length
      sha256 = ([BitConverter]::ToString($Digest).Replace('-', '')).ToLowerInvariant()
    }
  }
} finally {
  $Sha256.Dispose()
}

[ordered]@{
  schema       = 'code-memory.provider-checksums.v1'
  platform     = 'windows-x64'
  generated_at = [DateTime]::UtcNow.ToString('o')
  file_count   = @($Entries).Count
  files        = @($Entries)
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $OutputPath -Encoding utf8

Write-Output ("WROTE {0} files to {1}" -f @($Entries).Count, $OutputPath)
