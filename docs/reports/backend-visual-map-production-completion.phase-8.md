# Backend Visual Map Production Completion - Phase 8

Status: Complete locally  
Date: 2026-07-11

## Result

- Code evidence opens in allowlisted VS Code or Cursor executables at a validated line and column.
- Explorer reveal remains available when an editor is missing.
- Source paths are canonicalized and must remain inside the registered repository; traversal and reparse-point escapes are rejected.
- The answer panel now has a local investigation tray with add, checked, remove and compact Markdown-copy actions.
- Investigation persistence is capped at 50 entries and stores only `path`, `line`, `column`, `evidenceId` and `checked`.
- Source contents, evidence text, credentials and connection strings are not persisted in the tray or included in the Markdown summary.

## Verification

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml source -- --nocapture
npm run typecheck
npm run build
powershell -File scripts/smoke-ui.ps1 -Scenario source-jump
```

Result: 12 focused Rust tests passed, including repository escape, reparse-point escape and position bounds. The live Tauri smoke selected a code item, found all three source actions, added and checked an investigation item, verified the exact persisted field set and exercised Markdown export state handling.

The CDP automation context denies system clipboard access, so it verifies completed success/failure handling rather than the Windows clipboard contents. Clipboard contents remain an installer smoke/manual release check.
