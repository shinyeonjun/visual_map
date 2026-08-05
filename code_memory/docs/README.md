# Code Memory documentation

- `contracts/`: current provider and semantic output contracts
- Installation behavior is defined in `contracts/INSTALLATION-LAYOUT.md`.
- `../rust/src/`: Rust bridge code, split into `providers/`, `architecture/`,
  `frameworks/`, and small pipeline modules. `main.rs` is only the CLI and
  orchestration entry point.
- `../tests/gates/`: executable semantic, framework, and external-project gates
- Build output, provider workspaces, and test caches belong under ignored
  `../artifacts/` or the local VisualMap cache, never in fixtures.
- Framework support is defined in `contracts/FRAMEWORK-PACKS.md` and the
  manifests under `../packs/framework/`.
- Current field POC: `../../docs/reports/poc-validation-2026-08-05.md`
- Local generation and snapshot storage contract:
  `../../docs/contracts/engine-index-data-contract.md`
- Engine incidents and open blockers:
  `../../docs/troubleshooting/code-memory-engine.md`
