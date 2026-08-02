# Architecture Index Contract

The architecture index is the compact output for Visual Map's large-scale
code view. It is intentionally separate from the raw language index: the raw
index keeps symbol and occurrence detail, while this file contains the stable
tree, boundaries, relations, and user-visible flows.

## Output

`code-memory.architecture-index.v3`

The Rust bridge writes it next to the normal index output:

```text
language-index.json
language-index.architecture.json
```

`index --architecture-out <path>` overrides the default sibling path.

## Nodes

| Kind | Meaning |
|---|---|
| `PROJECT` | The indexed repository. |
| `PACKAGE` | A dependency manifest boundary such as npm, PyPI, Cargo, or Maven. |
| `MODULE` | A source directory/module grouping. |
| `FILE` | A source file. |
| `ENDPOINT` | A verified HTTP or RPC entrypoint. |
| `COMPONENT` | A verified UI/component boundary. |
| `SERVICE` | A verified service boundary. |
| `JOB` | A verified scheduled job or server action. |
| `EVENT` | A verified event or async boundary. |
| `DYNAMIC_BOUNDARY` | A statically detected call whose target depends on runtime values. |
| `EXTERNAL_LIBRARY` | An imported package that is outside the repository. |
| `DATA_RESOURCE` | A database or file boundary inferred from source code. |

Every node has a stable `id`, display `label`, optional source `path`, and
properties. External packages have `external: true`.

`diagnostics[].code` is a stable kebab-case machine category. Consumers must
branch on this field, not on the human-readable `message`. The same diagnostic
also carries its evidence path when the gap is file-scoped.
The default external-library label is `<package> 라이브러리` (for example,
`pandas 라이브러리`). The stable node ID and the `name` property retain the
machine package name.

`FILE` nodes have a `semantic` property: `indexed` means the language provider
returned symbols or occurrences for the file, while `empty` means the file is
kept for source coverage but has no semantic facts (for example a package
`__init__.py`). Visual Map may hide `empty` files in the overview without
deleting them from the raw language index.

`MODULE` nodes use the same `semantic` property. A module is a structural
boundary, not automatically every directory: directories with multiple source
files, multiple child directories, or a package manifest become boundaries;
single-file directory chains are folded into the nearest boundary. The file
tree remains complete through `FILE` nodes, and `source_files` gives the number
of files grouped into the compact module.

Provider stderr is operational noise, not a language diagnostic. Known
provider info lines are suppressed; actionable provider messages are kept in
the process log with a `[provider:<name>]` prefix. They do not change a
language's success status unless the provider itself fails or returns invalid
output.

## Edges

`level: tree` edges describe scope:

- `CONTAINS`: project/package/module/file ownership.

`level: summary` edges describe the map:

- `ENTRYPOINT_TO`: verified framework entrypoint to its source module.
- `CALLS`: provider-resolved module call relation.
- `IMPLEMENTS`: provider-resolved implementation relation.
- `IMPORTS`: provider-resolved module import relation.
- `USES_LIBRARY`: source module uses an external package.
- `DYNAMIC_CALL`: source module contains an explicit runtime-dependent dispatch pattern.
- `READS`, `WRITES`: source-level database/file boundary candidates.

Each edge carries source evidence. An edge is not emitted when the target
cannot be resolved to a repository module or a recognized boundary. This keeps
the visual map from presenting guessed routes or guessed call targets as facts.

For imports, `resolution: external` means the imported target was not found in
the indexed project sources. The edge is a library boundary, not a guessed
internal `CALLS` edge. Local relative imports and local package/module sources
are excluded from this boundary. C/C++ angle-bracket includes are represented
as system-library boundaries unless a matching project header is indexed.

TypeScript and JavaScript local module imports are emitted from the compiler
project model as `resolution: internal` file-level edges. This covers aliases,
project references, package exports, and Vue SFC script imports without
requiring the imported file to be a direct SCIP config root.

External libraries are represented at package level only. The core engine does
not maintain a package-name/API-operation table; framework or database packs
may add a verified, project-specific boundary when they have evidence.

## Flows

`flows` are bounded summaries starting at verified framework entrypoints. Each
flow contains its entrypoint node, reachable node IDs, and edge IDs. They are
for the first screen of Visual Map; the raw index remains available when a
developer needs exact symbols and source ranges.

## Deliberate boundary

Database names and operations found in code are represented as
`DATA_RESOURCE` placeholders with `resolution: db_memory`. `db_memory` can
later replace or enrich these nodes when a database schema is connected. The
code engine does not pretend that a database schema was discovered from a code
string alone.

The architecture layer is static. It does not execute the project and does
not claim runtime-only dispatch, reflection, dependency injection, or dynamic
plugin behavior without a provider/framework fact that resolves it. Explicit
static markers such as `getattr`, `Class.forName`, `eval`, `dlsym`, and
equivalent patterns are represented as `DYNAMIC_BOUNDARY` nodes instead of
guessed call targets.

The raw language index also contains `coverage`. Each discovered source file is
listed as `indexed`, `excluded`, or `missing`, with a machine-readable reason.
This keeps the overview compact while allowing the UI to explain why a source
file is absent from semantic relations.
