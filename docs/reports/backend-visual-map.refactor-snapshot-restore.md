# Backend Visual Map Refactor: Snapshot Restore

Date: 2026-07-06

## Scope

- Implemented workspace inventory snapshot restore after workspace open/list selection.
- Preserved existing UI design and manual inventory load/index flows.
- Did not add new engine scans, DB row access, or map projection features.

## Changed Files

- `src-tauri/src/lib.rs`
- `src/App.tsx`
- `src/snapshotRestore.ts`
- `src/hooks/useCodeInventory.ts`
- `src/hooks/useDbProfiles.ts`

## Results

- Added Tauri command `load_inventory_snapshot`, reusing existing `atlas::load_inventory_snapshot`.
- When a workspace becomes current, the app now tries to load the persisted inventory snapshot.
- Snapshot code items restore into `CodeInventory` enough for code/API/file panels.
- Snapshot DB items restore into `DbInventory` enough for table/column panels.
- Existing `useVisualMap` still auto-loads the visual map from the same persisted snapshot.
- Missing snapshots are treated as normal for new workspaces.

## Checks

- `npm run typecheck`: passed
- `npm run build`: passed
- `cargo test`: passed, 40 tests

## Skipped

- Full-fidelity raw engine inventory reconstruction from snapshot; the snapshot format is normalized and intentionally lossy.
- Workbench component split; next priority.
- Atlas component split, CSS split, Rust `workspace.rs` split; later priorities.
