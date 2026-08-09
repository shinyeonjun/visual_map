# Framework pack contract

`packs/framework` is the fixed framework boundary for the 10-language
code-memory contract. It currently contains 72 declared packs. PHP and Ruby
packs are not packaged or tested because those languages are outside the
active product contract.

Each language manifest contains:

- `signals`: source/dependency evidence that can select the pack;
- `outputs`: normalized facts the pack may emit after AST/LSP evidence is
  found;
- `kind`: the flow category (`web`, `rpc`, `ui`, `async`, `desktop`, or
  `game`).

Each pack directory also contains `fixture.json` with schema
`code-memory.framework-fixture.v1`. It stores the source/metadata files used by
the pack gate and the expected `facts`/`relations`. The fixture facts must
match `rule_sets`; a pack without this contract is invalid.

Each framework `pack.json` also contains `rule_sets`, which selects the shared
analyzers used to turn those signals into normalized facts.

The selected adapter family is executable, not descriptive metadata: the
loader rejects a pack when its `rule_sets` contains a fact family that the
adapter does not implement.

Signals are only selectors. A signal such as `app.get` is not itself a route
edge. The analyzer must still resolve the handler and source range from the
language AST/provider. If it cannot, it must emit a diagnostic rather than a
fabricated `HANDLES` relation.

The shared Rust adapter converts provider facts into normalized route, service,
middleware, component, event, RPC, async, schema, and job shapes. It emits
`HTTP_ROUTE` plus `HANDLES` only when the path, method, and handler argument
are present. Framework-specific DSLs that do not fit a shared family remain a
separate certification task; a detected pack is not by itself proof that every
framework feature is understood.

The Rust language index now carries two additional arrays:

- `frameworks`: detected packs, matched source signals, files, and normalized
  framework facts;
- `framework_relations`: source-located `HANDLES` edges from resolved handlers
  to route fact IDs.

Each detected framework also reports the shared adapter family that executed it
(for example `registration-routing`, `annotation-routing`,
`filesystem-routing`, `component-events`, `rpc-service`, or
`async-events`). Facts preserve their `source_range` and the evidence marker
that created them, plus fact-specific properties such as event name, schedule,
service, or middleware target. RPC endpoints can also produce `HANDLES`
relations when their handler symbol resolves uniquely.

Framework facts without a uniquely resolved symbol are retained as facts, but
  no `HANDLES` edge is created for them.

## Canonical migration boundary

The arrays above remain transitional donor output. Backend static HTTP
registrations now also pass through a separate typed Framework IR and the
canonical Fact Bundle:

```text
pack signal + provider fact
  -> typed route candidate
  -> current source digest/range validation
  -> HttpRoute + Exposes
  -> exact provider handler identity -> Handles
```

The boundaries are intentionally separate:

- framework detection says which analyzer may run;
- a route fact requires a static method, static path, and exact source
  evidence;
- a handler binding additionally requires an exact existing provider symbol.

Failure at a later boundary never upgrades an earlier signal. A detected pack
with a dynamic route creates a typed gap, not a route. A valid route with an
unresolved handler stays visible, but has no `Handles` edge. Name similarity,
suffix matching, and same-file guessing are forbidden.

Canonical route identity is typed. `HttpRoute` stores normalized uppercase
`method` and absolute `path` details, and its qualified identity is exactly
`{METHOD} {path}`. Consumers must not parse a display label to recover them.

Receipts distinguish raw donor candidates from planned records after exact
duplicate removal, and require `planned = emitted + rejected`. This prevents a
repeated donor fact from lowering apparent coverage. `FrameworkBindings` is
owned once by the canonical framework adapter rather than being independently
reported by both language and framework layers.

Pack JSON bytes and the typed adapter version participate in the analyzer-set
digest and therefore the snapshot identity. A semantic rule change must either
bump the adapter version or change the pack bytes; unchanged source cannot keep
a stale route snapshot.

The catalog gate checks the exact supported list:

```powershell
.\tests\gates\run-framework-pack-gate.ps1
```

The semantic gate runs every declared pack through its fixture and shared
adapter and checks that each declared rule emits a fact:

```powershell
.\tests\gates\run-framework-semantic-gate.ps1
```

The Rust loader checks the same files at runtime:

```powershell
cargo run --manifest-path rust\Cargo.toml -- framework-packs --root .
```

The pack boundary and shared adapter families are implemented. The catalog,
semantic, and provider-backed gates all pass for all 72 packs. The provider
gate validates each pack through its configured language provider, source range,
fact, and relation contract. This does not claim that every version-specific or
framework-specific DSL in every real project is understood; those remain
compatibility coverage work, not a license to fabricate relations.

The canonical flow gate executes one reviewed flow for each supported language
and validates final bundle output for every HTTP case:

```powershell
.\tests\gates\run-framework-flow-gate.ps1
```

It currently passes 10/10 flows: JavaScript/TypeScript Express, Go Gin, Rust
Axum, C++ Crow, C# ASP.NET Core, Dart Shelf, Python FastAPI, Java Spring MVC,
and C GTK/GLib. The nine HTTP cases require Framework IR plus canonical
`HttpRoute`/`Exposes`/`Handles`; C validates the event donor flow only until a
typed canonical event adapter exists.

The supported stack boundary is defined by the language and database support
contracts in
`../../../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md`.
