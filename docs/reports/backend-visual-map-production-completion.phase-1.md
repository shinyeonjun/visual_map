# Backend Visual Map Production Completion - Phase 1

Status: Complete locally  
Date: 2026-07-11

## Result

- Code inventory uses exact engine labels with bounded pagination; bucket names no longer redefine Route or File semantics.
- CALLS uses `source`/`target` columns and keeps only relationships whose endpoints exist in the canonical inventory.
- HANDLES is ingested in the engine direction (`handler -> route`) and projected as a confirmed product relationship without fabricating a fallback.
- Source path and start/end line/column metadata are preserved when the engine supplies them.
- Duplicate qualified names with conflicting labels or source locations fail loudly.
- Code cache paths are isolated by engine id, version and contract version.
- A sanitized real-sidecar fixture from `D-meeting-overlay-assistant` locks Route, Function, File, CALLS, HANDLES and source-location response shapes for engine `0.8.1`.

## Verification

Passed:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml workspace::
powershell -NoProfile -File scripts/smoke-code-engine-contract.ps1
npm run typecheck
```

Observed real-binary contract:

- synthetic repository: exact-label checks passed; CALLS and source locations returned typed columns; unsupported Express Route/HANDLES remained empty rather than fabricated;
- meeting-overlay repository: Route search returned only `Route`, HANDLES linked `bootstrap_admin` to the decorator route, and CALLS linked the handler to `_to_session_response`;
- the static fixture contains no source body, secret, connection string or user-home path.

## Remaining Gates

- Phase 0 owner decisions remain open: Git initialization/remote connection, public source visibility and product license.
- Public release still requires a published DB engine artifact, complete third-party notices and signing.
- Snapshot V2 and single Rust ownership continue in Phase 2; this report does not claim later phases complete.
