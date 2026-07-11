# Backend Visual Map Production Completion - Phase 11

Status: Release candidate complete except for trusted Windows code signing  
Date: 2026-07-11

## Completed Locally

- Added a release-only GitHub Actions workflow for pinned engines, security checks, multi-language code validation, required network DB validation, certificate import, signed Tauri build, clean install/start/uninstall smoke and installer upload.
- Added `build-signed-release.ps1`; it requires a real private-key certificate and merges the SHA-256/timestamp signing configuration into Tauri without storing certificate material.
- Added `release-smoke.ps1` with PE, size, checksum, engine/notices/license, optional Authenticode and opt-in clean install/start/uninstall checks.
- Built a fresh internal NSIS installer containing the current product changes.
- Installed that NSIS package into an isolated temporary directory, verified both bundled engines, started the installed application for five seconds, silently uninstalled it and confirmed zero remaining registry entries, temp directories and processes.
- Added a pinned real-repository matrix covering Java/Spring, C#/.NET and a Python/FastAPI + TypeScript monorepo.
- Added a release-only RDB matrix gate requiring PostgreSQL and at least one of MySQL, SQL Server or Oracle in addition to the SQLite DDL contract smoke.
- Ran that required matrix against isolated PostgreSQL 16 and MySQL 8.4 containers; SQLite DDL, PostgreSQL and MySQL metadata indexing all passed, then the containers, images and Docker Desktop service were removed.

## Evidence

Current release-candidate installer:

- path: `src-tauri/target/release/bundle/nsis/Backend Visual Map_0.1.0_x64-setup.exe`
- SHA-256: `80E0402330EA1B140D4075E6EEE46C07B24EAA66F2A4448B3E9E4A9844E7F2A7`
- size: 26,970,984 bytes
- signature: `NotSignedOrInvalid` (release candidate only; distributable workflow requires `Valid`)
- clean install/start/uninstall: passed with zero remaining product processes and uninstall registry entries

Pinned code field matrix:

| Repository | Commit | Routes | Calls | Located nodes |
| --- | --- | ---: | ---: | ---: |
| spring-projects/spring-petclinic | `51045d1648dad955df586150c1a1a6e22ef400c2` | 19 | 268 | 645 |
| ardalis/CleanArchitecture | `a064d0b369b719ba03da71da1560d208d7e02e03` | 4 | 706 | 2,623 |
| fastapi/full-stack-fastapi-template | `4cd0d9e51aebd1af6f82d91ad0df4c9e41f4dea2` | 29 | 613 | 1,244 |

Published database engine:

- release: `https://github.com/shinyeonjun/rdb-memory-mcp/releases/tag/v0.1.1`
- source commit: `acedac7bcc92f6f2c25b5890f121b64da2a90779`
- archive SHA-256: `44754EE82BD873D9802F3FABCAEE3AFB510469E7586CDC542BC72A065716A0E6`
- CLI SHA-256: `D2633542BC12BC14EDA79C05AB61B9DD3DE2B3FD92DAB94F2BEC49A349F42BF4`
- format, clippy, 74 tests, release build, SQLite DDL, PostgreSQL 16 and MySQL 8.4: passed

## Verification

```powershell
npm run build:internal
npm run smoke:release-internal
powershell -File scripts/release-smoke.ps1 -Internal -ExerciseInstall -AcknowledgeSystemChanges
npm run smoke:code-matrix
powershell -File scripts/smoke-rdb-productization.ps1 -DatabaseMemory .\src-tauri\engines\database-memory.exe
# Also passed with -RequireReleaseMatrix using PostgreSQL 16 and MySQL 8.4.
```

The release engine gate and redistribution-license gate now pass with the public `v0.1.1` engine and MIT product license. The required network DB matrix passed against isolated PostgreSQL 16 and MySQL 8.4 containers.

## Remaining External Gate

1. Provide a trusted Windows Authenticode PFX/password through the protected `release` environment secrets. The workflow fails closed when the certificate is absent.

The product uses the MIT license, the database engine is publicly pinned, and public source publication is approved. No unsigned installer is represented as a signed distributable.
