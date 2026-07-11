# Backend Visual Map Production Completion - Phase 0

Status: Implemented locally; review and owner gates pending  
Date: 2026-07-10

## Result

- Engine executables are covered by `.gitignore` and described by a versionable checksum manifest.
- `scripts/prepare-engines.ps1` downloads only the pinned ZIP assets, verifies both archive and extracted executable SHA-256 values, and refuses to replace unknown files without `-Force`.
- CI accepts only published release artifacts. The existing local DB executable is recorded as a development-only exception because it does not match the published `v0.1.0` asset.
- Node `24.18.0` and Rust `1.96.1` are pinned.
- Windows CI is configured to run engine preparation, version consistency, frontend checks, Rust formatting, Clippy, and Rust tests without publishing.

## Provenance Audit

| Engine | Published release | Published executable SHA-256 | Existing local SHA-256 | Result |
| --- | --- | --- | --- | --- |
| `codebase-memory-mcp.exe` | `DeusData/codebase-memory-mcp` `v0.8.1`, commit `f0c9be19c5d74b84f418d807bfdce7b5d6a261ff` | `12375E6A39A31F003D776E54EF487640F9F59DB7DBEEE96973A9EE9EB18BC7BB` | same | Release artifact verified |
| `database-memory.exe` | `shinyeonjun/rdb-memory-mcp` `v0.1.0`, commit `6102ad7da5e00c8506dc9c88a6678928f4aab692` | `0EF227676A705FC4C9F461F9F24349E52794EF4A80CC92A0E23C70D2BBD9453E` | `F346C4E2B6BBDAEED7851D8ACBEA18E0278724B3D61E6C256E6C6C09318A3C17` | Local development artifact; strict release verification rejects it |

The two existing files in `src-tauri/engines` were not modified.

## Verification

Passed:

```powershell
.\scripts\verify-product-version.ps1
.\scripts\prepare-engines.ps1 -VerifyOnly -AllowDevelopmentArtifact
npm ci
npm run typecheck
npm run build
```

A clean temporary engine directory also passed:

```powershell
.\scripts\prepare-engines.ps1 -DestinationPath <temp>
.\scripts\prepare-engines.ps1 -DestinationPath <temp> -VerifyOnly
```

The strict command correctly rejects the unchanged local DB development artifact:

```powershell
.\scripts\prepare-engines.ps1 -VerifyOnly
```

Rust formatting and lint checks were attempted while Phase 1 and Phase 2 source edits were still in progress in the shared workspace. They currently report those unrelated incomplete source changes and must be rerun after the implementation agents finish.

## Remaining Gates

- `backend_map` is intentionally still not a Git repository; no remote was added and nothing was pushed.
- The owner must decide whether the first source push may be public because `shinyeonjun/visual_map` is public.
- `LICENSE` remains a non-redistributable placeholder.
- A new DB engine release must replace the development-only artifact before product distribution; CI already rejects that development checksum.
- Complete bundled license texts, signing, and release publication remain later release gates.
