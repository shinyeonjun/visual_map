# Evidence provider contract

The evidence pipeline supplements SCIP/LSP; it does not replace semantic code
indexing. Its boundary is `code-memory.collection-report.v1`.

## Safety classes

| Class | Behavior | Default |
|---|---|---|
| Passive | Reads bounded project descriptors or existing artifacts | Enabled by `collect` |
| Tool-assisted | Runs a read-only metadata tool through the bounded provider runtime | Git metadata only |
| Active explicit | Runs a caller-selected executable and argument array | Only the `verify` command |

No collector automatically runs tests, builds, Docker, Kubernetes, Helm,
Terraform, application code, or a shell. Managed provider manifests are
excluded from project discovery so bundled runtimes cannot become project
facts.

## Collectors

| ID | Accepted evidence | Output intent |
|---|---|---|
| `build-graph` | package/build descriptors | Project and build-unit ownership plus declared dependencies |
| `frameworks` | Existing framework packs and source snapshot | Source-located routes, handlers, DI, events, and other framework facts |
| `contracts` | OpenAPI, AsyncAPI, Protobuf, GraphQL | Declared endpoints, channels, operations, services, and messages |
| `git-revision` | Git revision and porcelain status | Revision identity and changed-file set |
| `ci-evidence` | SARIF, LCOV, JUnit, `verification-run.v1` | Aggregated static-analysis, coverage, test-suite, and verification evidence |
| `database-assets` | Conventional ORM schema and migration paths | Schema inventory and ordered migration sets |
| `messaging` | Explicit broker listener/publisher APIs | Static or dynamic producer/consumer boundaries |
| `deployment` | Dockerfile, Compose, Kubernetes, Helm Chart, Terraform plan JSON | Declared deployment and infrastructure topology |
| `telemetry` | OTLP trace JSON | Aggregated runtime services, operations, and observed calls |

Every fact has a stable key. Every relation names existing fact keys, carries a
truth class and evidence type, and retains source/artifact location where the
input provides one. Unsupported or malformed input produces a diagnostic; it
must not become a guessed fact.

## Large-project rules

- CI test cases are aggregated by suite/class, SARIF results by rule, level,
  and path, and OTLP spans by service/operation edge.
- Provider stdout/stderr and descriptor reads are bounded.
- Managed provider roots, dependency directories, build output, and caches are
  not recursively re-indexed.
- Test/fixture build units and migrations remain visible with
  `source_scope=test` so consumers can hide them without deleting evidence.
- A test file or test relation is not proof of verification. Only an imported
  test artifact or explicit verification run can supply that claim.

## Explicit verification

```powershell
cargo run --manifest-path rust\Cargo.toml -- verify `
  --root D:\repo `
  --tool cargo `
  --arg test `
  --label "Rust tests" `
  --timeout-seconds 600
```

The executable name is restricted to a simple tool name and arguments are
passed directly without shell parsing. The latest report defaults to
`.code_memory\evidence\verification-run.json`; `collect` imports that exact
hidden artifact. Raw stdout/stderr is not persisted because it can contain
secrets. The offline environment is a package-manager policy, not an OS-level
network sandbox; callers that require hard isolation must run the engine in an
isolated process/container.

This contract intentionally stops before the final VisualMap UI/data-merging
contract. That contract should be fixed only after these provider outputs have
been measured on representative small, medium, and large repositories.
