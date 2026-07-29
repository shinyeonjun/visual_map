# Language semantic contract

This layer is responsible for language semantics only. Framework and ORM
relations are separate adapters and are not created here.

## Relations produced

The normalized `code-memory.language-index.v1` output may contain:

| Relation | Meaning |
|---|---|
| `CALLS` | A function or method body calls the target symbol. |
| `REFERENCES` | A source occurrence refers to the target symbol. |
| `IMPORTS` | A source file imports or requires the target. |
| `IMPLEMENTATION` | The provider reports a type implementation/supertype relation. |
| `TYPE_DEFINITION` | The provider resolves a value to its type definition. |
| `USES_TYPE` | A C/C++ provider resolves a type occurrence to a typedef, struct, class, enum, or template symbol. |
| `DEFINITION` | A C/C++ provider resolves a declaration to its implementation. |
| `DEFINITION_OVERRIDE` | The provider reports an override/definition relation. |
| `SYMBOL_REFERENCE` | SCIP reports a symbol-level reference relationship. |

The bridge does not invent an edge from a name match. A relation is emitted
only when the SCIP indexer or language server resolves it. C/C++ calls come
from clangd call hierarchy, type and inheritance edges come from clangd type
queries, and declaration/implementation edges come from clangd definition
queries. Lexical parsing is used only for file-level include/import boundaries.

For TypeScript and JavaScript, call classification uses the bundled TypeScript
compiler `Program`/`TypeChecker` call ranges before the SCIP occurrence target
is labeled `CALLS`. If the project model is unavailable, the bridge retains a
source-range fallback; it does not invent an unresolved target.

For TypeScript and JavaScript, the bridge also runs the bundled TypeScript
compiler API as a project model. Its file-level `file_relations` contain
`IMPORTS` edges resolved with the project's `tsconfig`/`jsconfig`, including
`extends`, project references, `baseUrl`, `paths`, and package exports. These
edges are separate from symbol-level SCIP relations because a top-level import
does not always have an enclosing function symbol. `project_model_files` lists
local files reached by that model, including Vue SFC files whose script blocks
were parsed. No edge is emitted for an unresolved import.

The model also partitions TypeScript and JavaScript into `units`. An explicit
config is used when it owns the file; files not covered by any config are put
into a generated, read-only synthetic unit. The bridge writes synthetic
configs only below its provider scratch directory and passes the real project
root to `scip-typescript` for package and local-file resolution. It never
writes a config, lockfile, or index file into the user's project.

## Required provider inputs

Exact results depend on the project metadata used by the language tool:

| Language family | Project metadata |
|---|---|
| TypeScript / JavaScript | `tsconfig.json`, including `allowJs`/`checkJs` for JavaScript |
| Python | Pyright configuration, type hints, stubs, and selected environment |
| Java | Maven/Gradle project and resolved JDK/classpath |
| C# | restored `.csproj`/`.sln` |
| C / C++ | `compile_commands.json` or equivalent compile flags |
| Go | `go.mod`/workspace and Go toolchain |
| Rust | Cargo workspace, features, build scripts, and rust-analyzer |
| PHP | Composer autoload metadata |
| Ruby | project gems; RBS/Sorbet metadata when available |
| Dart | `pubspec.yaml` and Dart Analysis Server |

## Acceptance fixture

Every language fixture must exercise:

1. a cross-file function or method call;
2. a type or interface implementation;
3. a generic/container type where the language supports it;
4. a source range that can be mapped back to the enclosing symbol.

The acceptance gate must check the exact target symbol and source range. An
empty provider result or unresolved required relation fails the gate instead
of becoming a guessed graph edge.

Dynamic runtime dispatch remains outside this static contract for now.

CMake and Meson projects are supported through their generated
`compile_commands.json` (including common `build/`, `out/`, and preset build
directories). VCXPROJ files are detected and included in cache/module
invalidation, but they are not treated as a compiler context by themselves:
an exact C/C++ result still requires a compile database, `compile_flags.txt`,
or `.clangd`. The bridge never fabricates MSVC flags from XML.

Vue single-file components are covered at the script boundary. Imports in
`<script>` and `<script setup>` are resolved, and statically imported component
tags in `<template>` produce internal file relations. Dynamic components,
global registration, and framework auto-imports require Vue/Nuxt tooling and
are not guessed by the bridge.

The gate is run with:

```powershell
.\tests\gates\run-language-semantic-gate.ps1 -AllowMissingProvider
```

`-AllowMissingProvider` is only for a developer machine missing a provider;
release validation must run without it so every configured language is tested.
