# Framework pack contract

`packs/framework` is the fixed framework boundary for the 12-language
code-memory contract. It currently contains 84 declared packs (the original
82-pack baseline plus JavaScript and Rust Tauri desktop bridge packs).

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
semantic, and provider-backed gates all pass for all 84 packs. The provider
gate validates each pack through its configured language provider, source range,
fact, and relation contract. This does not claim that every version-specific or
framework-specific DSL in every real project is understood; those remain
compatibility coverage work, not a license to fabricate relations.

The supported stack boundary is defined in
`../../../docs/contracts/visual-map-supported-stack-contract.md`.
