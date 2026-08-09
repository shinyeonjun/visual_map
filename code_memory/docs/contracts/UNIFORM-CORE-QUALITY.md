# Uniform ten-language quality contract

Status: active release gate, 2026-08-09.

“Ten supported languages” means ten release-blocking peers, not five strong
languages plus five best-effort parsers. The common product contract is about
evidence and honest failure, not forcing every language to expose syntax it
does not have.

## Common minimum

For every supported language the engine must:

1. include complete source bytes in Source Census or record a typed exclusion;
2. assign every included file to a validated Analysis Unit;
3. execute the planned semantic provider or record why it did not execute;
4. emit a valid Language IR stream with exact source coordinates;
5. publish definitions and at least one evidence-backed relation for the
   closed positive fixture;
6. publish file coverage and capability receipts;
7. never create an edge from a name-only or path-only match;
8. publish one valid canonical SQLite bundle and manifest;
9. preserve semantic and bundle identity across identical runs.

Language-native differences remain explicit. A capability that cannot be
proven in a context is partial/unsupported with a typed reason; it is not
reported as complete with zero results.

## Executable gate

```powershell
.\tests\gates\run-uniform-core-quality-gate.ps1 `
  -Bridge .\rust\target\release\code-memory-language.exe `
  -ProvidersRoot .\providers
```

The gate compares the product, bridge, and framework-pack language catalogs,
then runs real fixtures for Rust, TypeScript, JavaScript, Python, Java, C#,
C/C++, Go, and Dart. C and C++ share one clang project run but are independent
catalog obligations. Every project must publish non-empty nodes, edges,
evidence, file coverage, capability receipts, and identity digests through the
canonical path.

`-AllowMissingProvider` is diagnostic-only. Signed release packaging invokes
the gate without it and then runs independent canonical bundle-byte
determinism. Legacy JSON-output gates are not part of the release path.

## Interpretation

Passing this gate proves contract liveness on a closed corpus. It does not
prove arbitrary-repository accuracy. Syntax-specific precision/recall,
negative cases, ambiguity, large files, multiple targets, and real-repository
holdouts belong to [semantic quality](SEMANTIC-QUALITY.md) and Rust
characterization tests.
