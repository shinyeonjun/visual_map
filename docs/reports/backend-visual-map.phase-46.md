# Backend Visual Map Phase 46

Date: 2026-07-06

## Changed Files

- `src/App.tsx`
- `src/hooks/useWorkspaces.ts`
- `src/components/workbench/WorkspaceCard.tsx`
- `src/operationStatus.ts`
- `src/styles/forms.css`
- `src/types/controls.ts`
- `src/types/workspace.ts`
- `src-tauri/src/workspace/store.rs`
- `src-tauri/src/workspace/mod.rs`
- `src-tauri/src/workspace/tests.rs`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Added Local Folder / GitHub URL source mode in the workspace card.
- GitHub URL workspace creation now clones with `git clone --depth 1` into the app-managed workspace repo directory before saving the workspace.
- The saved workspace `repoPath` is the cloned local path, so existing code indexing continues to use a local folder.
- Clone operations use a dedicated running status label.
- GitHub URL parsing is constrained to `github.com/owner/repo` and `git@github.com:owner/repo.git`.
- Existing workspace repo directories are not overwritten.

## Checks

- `cargo fmt` passed.
- `cargo test` passed: 48 tests.
- `npm run typecheck` initially failed because `lucide-react` did not export `Github`; fixed by using `GitBranch`.
- `npm run typecheck` passed after the fix.
- `npm run build` passed.
- `npm run tauri dev` smoke passed: Vite and the Tauri app started; stopped with Ctrl-C.
- Manual public clone smoke passed with `https://github.com/octocat/Hello-World.git`.

## Skipped Work

- Did not implement private GitHub authentication.
- Did not implement Phase 47 snapshot staleness metadata.
- Did not add DB row-data access.
- Did not persist DB passwords.
- Did not render raw full graphs directly.

## Risks

- HTTPS private repositories fail fast because `GIT_TERMINAL_PROMPT=0` disables interactive credential prompts.
- SSH URLs depend on the user's existing local SSH/git setup.
