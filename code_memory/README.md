# Visual Map Code Memory

This directory is the source boundary for Visual Map's code-graph engine.
The production engine is the Rust executable under `rust/`; framework packs,
provider assets, contracts, and end-to-end fixtures live beside it.

## 12-language semantic bridge

The Rust bridge in `rust/` fixes the supported language list at:

`TypeScript, JavaScript, Python, Java, C#, C, C++, Go, Rust, PHP, Ruby, Dart`

It uses dedicated SCIP indexers where they exist and native LSP clients for
Go, Java, Python, Rust, Ruby, and Dart. All providers are static-analysis tools;
the application is never executed. The current contract and semantic checks
are documented in `docs/contracts/LANGUAGE-SEMANTICS.md`.

```powershell
cargo run --manifest-path rust\Cargo.toml -- list
cargo run --manifest-path rust\Cargo.toml -- doctor
cargo run --manifest-path rust\Cargo.toml -- index --root D:\path\to\repo --out D:\path\to\language-index.json
# Installed app resolves providers/manifest.json first; PATH is fallback.
cargo run --manifest-path rust\Cargo.toml -- doctor --providers-root D:\VisualMap\providers
cargo run --manifest-path rust\Cargo.toml -- index --root D:\path\to\repo --providers-root D:\VisualMap\providers
```

The output is `code-memory.language-index.v2`: documents, symbols,
definition/reference occurrences, source ranges, detected framework packs,
framework facts, provider status, provider provenance, and diagnostics. It is an interchange file
for the later Visual Map graph adapter, not the graph database itself.

Indexing is precision-first: there is no separate fast mode. Each language is
analyzed by its SCIP or native LSP provider, while generated files, dependency
folders, build output, tests, docs, caches, and `.git` metadata are excluded.
Providers run in parallel (default 3), write partial output after each language,
and stop safely on a per-provider timeout. Results are cached under
`%LOCALAPPDATA%\VisualMap\cache\code-memory` using source/config/provider
checksums. Set `CODE_MEMORY_MAX_PARALLEL`, `CODE_MEMORY_PROVIDER_TIMEOUT_SECONDS`,
or `CODE_MEMORY_LSP_MAX_SECONDS` only when a machine needs different limits.

JavaScript and TypeScript projects without `tsconfig.json` or `jsconfig.json`
are analyzed with a temporary inferred workspace in that cache; the project is
not modified. Nested Rust projects are resolved from their nearest
`Cargo.toml`, which allows monorepos to keep their existing layout.

Every `index` run also writes a sibling `*.architecture.json` file using
`code-memory.architecture-index.v3`. This is the Visual Map-oriented view:
project/package/module/file tree, verified framework entrypoints, external
library boundaries, database/file boundaries, module-level relations, and
bounded entrypoint flows. Use `--architecture-out` to choose another path.
The contract is documented in `docs/contracts/ARCHITECTURE-INDEX.md`.

## Evidence providers

`collect` adds non-language evidence without changing the SCIP/LSP provider
contract. It reads project descriptors and existing artifacts; it does not run
builds, tests, containers, Terraform, or application code.

```powershell
cargo run --manifest-path rust\Cargo.toml -- collect --root D:\path\to\repo --packs-root .
```

The output is `.code_memory\collection-report.json` using
`code-memory.collection-report.v1`. It contains build units, framework facts,
API contracts, Git revision state, CI evidence, migrations, explicit messaging
APIs, deployment descriptors, and aggregated OTLP traces. Missing inputs are
reported as `not-detected`, not fabricated as empty facts. The provider and
safety contract is documented in
`docs/contracts/EVIDENCE-PROVIDERS.md`.

Command execution is separate and explicit:

```powershell
cargo run --manifest-path rust\Cargo.toml -- verify --root D:\path\to\repo `
  --tool cargo --arg test --label "Rust tests"
```

`verify` never accepts a shell command string. It stores only status, duration,
exit code, provenance, and bounded output sizes; raw command output remains in
the terminal. Network-oriented package-manager environment is disabled unless
`CODE_MEMORY_ALLOW_NETWORK=1` is set by the caller.

## Framework packs

The supported framework catalog is under `packs/framework`. Validate all 85
declared packs with:

```powershell
.\tests\gates\run-framework-pack-gate.ps1
.\tests\gates\run-framework-semantic-gate.ps1
.\tests\gates\run-uniform-core-quality-gate.ps1
.\tests\gates\compare-index-to-source.ps1 -ProjectRoot D:\meeting-overlay-assistant
cargo run --manifest-path rust\Cargo.toml -- framework-packs --root .
cargo run --manifest-path rust\Cargo.toml -- framework-packs --root . --self-test
cargo run --manifest-path rust\Cargo.toml -- index --root D:\path\to\repo --packs-root .
```

The semantic gate runs every declared pack through its adapter family and
checks that each declared rule emits a source-located fact. The index output
also records the selected adapter, evidence marker, and fact properties.

Required provider commands are `scip-typescript`, `scip-dotnet`,
`scip-clang`, `gopls`, and `scip-php`.
Native LSP commands are `pyright-langserver`, `jdtls`, `rust-analyzer`,
`ruby-lsp`, and `dart`.
The C/Tree-sitter extractor remains available even when an external semantic
provider is missing.
