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
