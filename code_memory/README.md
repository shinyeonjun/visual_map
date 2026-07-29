# Visual Map Code Memory

This directory is the extraction boundary for Visual Map's code-graph engine.

It deliberately does not copy the upstream MCP product as a whole. The first
boundary contains only reusable source-discovery and syntax-extraction pieces:

- repository file discovery and language detection;
- Tree-sitter runtime and vendored grammars;
- definitions, imports, and unified per-file AST facts;
- the upstream extraction data structures and arena-backed result lifetime.

The upstream SQLite graph store, Cypher implementation, MCP server, CLI, UI,
watcher, embeddings, and product-specific pipeline are excluded. Visual Map
will define its own canonical graph contract and persistence layer around the
extracted facts.

Upstream source was taken from `codebase-memory-mcp` commit `affa2231`.
The upstream project is MIT licensed; see `THIRD_PARTY_NOTICES.md`.

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

The output is `code-memory.language-index.v1`: documents, symbols,
definition/reference occurrences, source ranges, detected framework packs,
framework facts, provider status, and diagnostics. It is an interchange file
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
`code-memory.architecture-index.v1`. This is the Visual Map-oriented view:
project/package/module/file tree, verified framework entrypoints, external
library boundaries, database/file boundaries, module-level relations, and
bounded entrypoint flows. Use `--architecture-out` to choose another path.
The contract is documented in `docs/contracts/ARCHITECTURE-INDEX.md`.

## Framework packs

The supported framework catalog is under `packs/framework`. Validate all 84
declared packs with:

```powershell
.\tests\gates\run-framework-pack-gate.ps1
.\tests\gates\run-framework-semantic-gate.ps1
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
