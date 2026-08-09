# Evidence provider contract

The evidence pipeline supplements SCIP/LSP; it does not replace semantic code
indexing. Its current donor boundary is `code-memory.collection-report.v1`.
The final product does not keep a separate collection truth surface: selected
collectors are typed adapters in the same canonical analysis job.

## Safety classes

| Class | Behavior | Default |
|---|---|---|
| Passive | Reads bounded project descriptors or existing artifacts | Enabled by `collect` |
| Tool-assisted | Runs a read-only metadata tool through the bounded provider runtime | Git metadata only |

No collector automatically runs tests, builds, Docker, Kubernetes, Helm,
Terraform, application code, or a shell. Managed provider manifests are
excluded from project discovery so bundled runtimes cannot become project
facts.

## Collectors

| ID | Accepted evidence | Output intent |
|---|---|---|
| `build-graph` | package/build descriptors | Project and build-unit ownership plus declared dependencies |
| `contracts` | OpenAPI, AsyncAPI, Protobuf, GraphQL | Declared endpoints, channels, operations, services, and messages |
| `git-revision` | Git repository, commit, and branch metadata | Revision identity only; per-file changes come from SourceManifest comparison |
| `database-assets` | Conventional ORM schema and migration paths | Schema inventory and ordered migration sets |
| `messaging` | Explicit broker listener/publisher APIs | Static or dynamic producer/consumer boundaries |
| `deployment` | Dockerfile, Compose, Kubernetes, Helm Chart, Terraform plan JSON | Declared deployment and infrastructure topology |

Every fact has a stable key. Every relation names existing fact keys, carries a
truth class and evidence type, and retains source/artifact location where the
input provides one. Unsupported or malformed input produces a diagnostic; it
must not become a guessed fact.

## Large-project rules

- Provider stdout/stderr and descriptor reads are bounded.
- Managed provider roots, dependency directories, build output, and caches are
  not recursively re-indexed.
- Test/fixture build units and migrations remain visible with
  `source_scope=test` so consumers can hide them without deleting evidence.
- Framework routes and handlers come from the primary `index` pipeline; the
  supplemental report does not duplicate them.
- CI reports and runtime traces are outside the static source/DB product scope.

This contract intentionally stops before the final Codebase Workspace UI/data-merging
contract. Migration rules are fixed in
[product requirements section 48](../../../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md#48-정적-원재료-파이프라인-구현-설계).
After typed adapter parity is proven, the separate `collect` command and
`collection-report.v1` product path are deleted. Prose documents, CI results,
and runtime telemetry remain outside this static product path.
