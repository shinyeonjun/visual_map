# Backend Visual Map Phase 47

Date: 2026-07-06

## Changed Files

- `src-tauri/src/atlas/model.rs`
- `src-tauri/src/atlas/snapshot.rs`
- `src-tauri/src/atlas/mod.rs`
- `src-tauri/src/atlas/tests.rs`
- `src-tauri/src/atlas/visual_map.rs`
- `src-tauri/src/lib.rs`
- `src/App.tsx`
- `src/inventorySnapshot.ts`
- `src/operationStatus.ts`
- `src/types/visual-map.ts`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Inventory snapshots now carry source metadata and stale reasons.
- Snapshot metadata records saved time, engine expected version, code source path/type, and DB profile source metadata without DB secrets.
- Loading a snapshot marks it stale when the repo path changes, DB profile changes, DB source type changes, or source metadata is missing.
- `get_visual_map` refuses to render stale snapshots instead of showing old data as current.
- App restart restore skips stale snapshots.

## Checks

- `cargo fmt` passed.
- `cargo test` initially failed because a test helper was not re-exported; fixed in `src-tauri/src/atlas/mod.rs`.
- `cargo test` passed after the fix: 50 tests.
- `npm run typecheck` passed.
- `npm run build` passed.
- `npm run tauri dev` restart/startup smoke passed; stopped with Ctrl-C.

## Skipped Work

- Did not implement Phase 48 DB profile form redesign.
- Did not add DB row-data access.
- Did not persist DB passwords or connection strings.
- Did not render raw full graphs directly.

## Risks

- Engine version metadata stores the registry expected version, not a fresh `--version` run, to avoid extra sidecar calls during snapshot save.
