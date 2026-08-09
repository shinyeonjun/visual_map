# Language provider contract

code-memory-language supports exactly these ten Windows language IDs:

| Language | Primary provider | Windows requirement |
|---|---|---|
| TypeScript / JavaScript | scip-typescript | Node.js 18 or 20 is the officially supported range; the installed package also runs in the current Node environment |
| Python | pyright-langserver (native LSP) | Node.js plus a usable Python project environment |
| Java | Eclipse JDT Language Server (native LSP) | JDK 21+ and optional Maven/Gradle project metadata |
| C# | scip-dotnet | .NET solution/project and restored dependencies |
| C / C++ | scip-clang; clangd fallback on Windows | compile_commands.json for SCIP, or clangd for the fallback |
| Go | gopls (native LSP) | Go module/workspace and Go toolchain |
| Rust | native rust-analyzer | Rust toolchain with the rust-analyzer component |
| Dart | native Dart Analysis Server LSP | Dart project metadata |

Python's Pyright executable and Node runtime share the `node` delivery pack;
there is no separate Python runtime pack. This keeps the capability intact
without installing an unused interpreter.

The Rust bridge starts native LSP servers directly over JSON-RPC. It does not
depend on the separately installed lsp-to-scip executable. Both SCIP and LSP
results are normalized into code-memory.language-index.v2.

The current normalized index is a transitional provider contract. It will feed
the versioned Language IR and canonical static normalizer defined in
[product requirements section 47](../../../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md#47-10언어-정적-분석-데이터-계약).
Frontend and AI consumers must not depend directly on language-specific
provider payloads.

Every index also records `provider_provenance`: the selected tool, whether it
came from the managed provider pack or the machine PATH, and the provider
version when the managed manifest declares one. A PATH result is valid but
unmanaged and must not be presented as byte-for-byte reproducible.

## Commands

\`\`\`powershell
# Open a new PowerShell after installing tools so PATH is refreshed.
cargo run --release --manifest-path rust\Cargo.toml -- doctor
cargo run --release --manifest-path rust\Cargo.toml -- index --root D:\repo --out D:\repo\.code_memory\language-index.json
# App-managed provider pack (PATH remains a fallback)
cargo run --release --manifest-path rust\Cargo.toml -- doctor --providers-root D:\CodebaseWorkspace\providers
cargo run --release --manifest-path rust\Cargo.toml -- index --root D:\repo --providers-root D:\CodebaseWorkspace\providers
\`\`\`

doctor checks executable presence. index performs the stronger check:
the provider must start and produce a readable semantic index. A provider
diagnostic is retained in the output instead of being silently replaced with
an inferred relation.

## Windows findings in this environment

- scip-typescript indexes the TypeScript fixture successfully.
- Eclipse JDT Language Server indexes the Java fixture successfully on
  Windows with JDK 21; the fixture produced one document, four symbols, and
  five occurrences.
- rust-analyzer indexes the Rust fixture successfully after installing the
  Rust component.
- gopls indexes the Go fixture through native LSP when the Go toolchain and
  gopls are present on PATH.
- clangd indexes both the C and C++ fixtures successfully as the Windows
  fallback for the unavailable `scip-clang` binary.
- The official scip-clang release does not provide a Windows binary, so the
  bridge uses clangd when scip-clang is absent.
- Swift is intentionally outside the initial Windows language set.
- Java now uses JDTLS; the old scip-java path is no longer used for the Java
  registry entry.
- Kotlin, PHP, and Ruby are outside the ten-language contract. They can be added
  later only after a provider produces reliable call/reference occurrences,
  not just file and symbol indexing.

## Current completion boundary

The bundled-provider semantic gate currently passes all ten configured
language cases. That is the minimum static Codebase Workspace contract, not a claim of
compiler-complete support for every dependency or project configuration.

The cold/warm large-source semantic gate also passes all ten languages with one
provider-visible source per language above 1.1 MB. scip-typescript's upstream
1 MB default is overridden with the maximum byte size of the exact scheduled
source set; it is not replaced by another fixed product ceiling.

Hierarchical LSP definition ownership is preserved through the LSP-to-SCIP
bridge. Flat Go receiver methods and Rust impl methods are attached only when
their provider-native owner resolves to one unique definition. The strict
normal and large-source gates currently report 43/43 emitted visual members
with a non-dangling definition owner.

The separate exhaustive definition gate measures the source denominator rather
than only emitted members. It passes 117/117 reviewed definitions and 55/55
reviewed owners across all ten languages, with no extra definition and identical
cold/warm definition-set digests. The large-source form repeats the same check.

- All ten languages pass the same closed core relation and definition gates;
  none is treated as a lower-quality product tier.
- Language-specific project context, dependency metadata, dynamic dispatch, and
  provider diagnostics still need their own reviewed fixtures. A missing context
  remains a typed gap rather than lowering the common evidence contract.
- Rust large workspaces keep provider-backed declarations/imports and bounded
  map-boundary call enrichment. rust-analyzer's bundled version does not
  support the type-hierarchy request, so trait implementation edges are not
  invented.
- Dynamic dispatch, runtime registration, framework auto-imports, and ORM/DB
  semantics are outside this language-only contract.

## Provider shadow decision — 2026-08-08

열 언어의 current provider와 batch/compiler 대안을 같은 evidence-level comparator로 직접 비교했다. 전체
측정, fixture, Windows packaging 판정은
[10-language provider shadow evaluation](./PROVIDER-SHADOW-EVALUATION-2026-08-08.md)에 기록한다.

- TypeScript/JavaScript: 현재 `scip-typescript` 유지. same-provider normalized shadow 100%.
- Python: Pyright LSP 유지. `scip-python`은 실제 884-file 저장소에서 current confirmed fact를 보존하지 못함.
- Java: JDTLS 유지. `scip-java`의 compiler-plugin 설계는 유망하지만 official launcher의 Windows Maven
  실행 경계가 막힘.
- C#: `scip-dotnet` 유지. `--skip-dotnet-restore`는 실제 호출을 누락하므로 절대 사용하지 않는다. restore는
  정확도 경계다.
- C/C++: clangd 유지. official `scip-clang` Windows binary가 없음.
- Go: gopls 유지. `scip-go`는 빠르고 의미 결정적이지만 raw current-fact 보존 gate를 아직 통과하지 못함.
- Rust: rust-analyzer LSP 유지. `rust-analyzer scip`이 implementation/type relation을 현재 계약대로 보존하지
  못함.
- Dart: Analysis Server LSP 유지. `scip_dart`가 constructor relation을 downgrade하고 full Dart SDK
  packaging이 필요함.

빠른 candidate가 있어도 current confirmed definition/occurrence/relation 중 하나라도 잃으면 production
provider를 바꾸지 않는다. candidate-only fact도 human/canonical ground truth 검증 전에는 confirmed로
승격하지 않는다.
