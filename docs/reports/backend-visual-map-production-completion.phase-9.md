# Backend Visual Map Production Completion - Phase 9

Status: Complete locally  
Date: 2026-07-11

## Result

- Canonical snapshot search remains local and returns bounded grouped results before any focused enrichment.
- Visual-map requests use a monotonically increasing request generation; stale workspace, mode and enrichment responses cannot overwrite the current selection.
- Delayed code-evidence enrichment is independently cancellable and leaves the last complete projection visible on failure.
- The synthetic fixture now contains 10,000 code nodes, 40,000 confirmed call edges and 200 DB tables.
- Architecture overview remains capped at 40 cards and 80 edges; focused projections remain below 36 nodes with no dangling edges.
- Snapshot restore, overview projection and focus projection emit repeatable local timing metrics during the large-snapshot test.
- The snapshot cache remains capped at two workspaces; no telemetry or source content is uploaded.
- Live UI smoke covers 1440x900 and 1180x760, the 40-card ceiling, root overflow and the 300ms local-search target.

## Verification

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml large_snapshot_projection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --release large_snapshot_projection -- --nocapture
powershell -File scripts/smoke-ui.ps1 -Scenario large-repo -Width 1440 -Height 900
powershell -File scripts/smoke-ui.ps1 -Scenario large-repo -Width 1180 -Height 760
npm run typecheck
npm run build
```

Release metrics on the current Windows machine:

- snapshot items: 10,200
- snapshot edges: 40,000
- restore: 284ms
- overview projection: 63ms
- focus projection: 46ms
- live 9k-item UI search: 22-37ms

No process-memory dependency was added. Memory growth is bounded structurally by the two-entry snapshot cache and the visible projection caps; add native working-set sampling only if field measurements show those bounds are insufficient.
