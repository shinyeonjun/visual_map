# Language provider contract

Status: active canonical-only contract, 2026-08-09.

The engine supports exactly ten product languages. Each language must produce
the same provider-neutral Language IR contract even though its semantic tool
and project boundary differ.

| Language | Semantic provider | Required project/context authority |
| --- | --- | --- |
| TypeScript | `scip-typescript` | tsconfig/jsconfig and referenced projects |
| JavaScript | `scip-typescript` | jsconfig/tsconfig or explicit source-only context |
| Python | `pyright-langserver` | Python environment and pyright/package roots |
| Java | `jdtls` | Maven/Gradle/source-set/module context |
| C# | `scip-dotnet` | solution/project/TFM context |
| C | `scip-clang`/clangd | compile command, target, flags, header context |
| C++ | `scip-clang`/clangd | compile command, target, standard, header context |
| Go | `gopls` | go.mod/go.work/module context |
| Rust | `rust-analyzer` | Cargo target/features/cfg/edition context |
| Dart | Dart analysis server | package config and analysis options |

## Provider boundary

Providers resolve semantic identities and targets. They do not own product
node IDs, truth classes, final relation kinds, coverage claims, or persistence.
Their DTO batches are immediately checked against Source Census, Analysis Plan,
the executed provider context, and provider-independent syntax inventories.

Every scheduled file is accounted for exactly once as executed or with a typed
omission. Provider-specific shards retain their Analysis Unit identity. Java
and C# tools that require writable IDE/build state run in a manifest-sealed
cache copy, never by mutating the selected repository.

## Managed runtime

Release builds use the signed provider catalog and managed provider paths.
Checksums, entrypoints, language ownership, and catalog signatures are verified
before execution. `CODE_MEMORY_REQUIRE_MANAGED_PROVIDERS=1` rejects a provider
resolved from an unmanaged system path.

Useful diagnostics:

```powershell
code-memory-language list
code-memory-language doctor --providers-root D:\provider-root
```

The `index` command accepts `--root`, `--providers-root`, and `--packs-root`.
It publishes one canonical Fact artifact and deliberately rejects the removed
`--out` and `--architecture-out` compatibility options.

## Fail-closed rules

- Missing tool, timeout, crash, malformed protocol, missing configuration, and
  incomplete project coverage are distinct typed failures.
- A provider may not analyze files outside its planned scope.
- A resolved target must map to one registered project-local definition or an
  explicitly known external target. Ambiguity creates a gap, not an edge.
- UTF-16 and UTF-8 coordinates are normalized with source bytes; invalid ranges
  are discarded.
- Source/config/provider checksums and actual execution context participate in
  cache and snapshot identity.
- A repository change during execution prevents publication.

Development may run the canonical language gate with
`-AllowMissingProvider` to diagnose a partially installed machine. Release
packaging may not skip any supported language.
