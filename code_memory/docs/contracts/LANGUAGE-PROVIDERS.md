# Language provider contract

code-memory-language supports exactly these twelve Windows language IDs:

| Language | Primary provider | Windows requirement |
|---|---|---|
| TypeScript / JavaScript | scip-typescript | Node.js 18 or 20 is the officially supported range; the installed package also runs in the current Node environment |
| Python | pyright-langserver (native LSP) | Node.js plus a usable Python project environment |
| Java | Eclipse JDT Language Server (native LSP) | JDK 21+ and optional Maven/Gradle project metadata |
| C# | scip-dotnet | .NET solution/project and restored dependencies |
| C / C++ | scip-clang; clangd fallback on Windows | compile_commands.json for SCIP, or clangd for the fallback |
| Go | gopls (native LSP) | Go module/workspace and Go toolchain |
| Rust | native rust-analyzer | Rust toolchain with the rust-analyzer component |
| PHP | scip-php | PHP project metadata |
| Ruby | native ruby-lsp | Ruby project and gem metadata |
| Dart | native Dart Analysis Server LSP | Dart project metadata |

The Rust bridge starts native LSP servers directly over JSON-RPC. It does not
depend on the separately installed lsp-to-scip executable. Both SCIP and LSP
results are normalized into code-memory.language-index.v2.

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
cargo run --release --manifest-path rust\Cargo.toml -- doctor --providers-root D:\VisualMap\providers
cargo run --release --manifest-path rust\Cargo.toml -- index --root D:\repo --providers-root D:\VisualMap\providers
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
- Kotlin is outside the initial twelve-language contract. It can be added
  later only after a provider produces reliable call/reference occurrences,
  not just file and symbol indexing.

## Current completion boundary

The bundled-provider semantic gate currently passes all twelve configured
language cases. That is the minimum static VisualMap contract, not a claim of
compiler-complete support for every dependency or project configuration.

- VisualMap core scope has been completed and regression-tested for
  TypeScript, JavaScript, C, C++, and C#.
- Python, Java, Go, Rust, PHP, Ruby, and Dart pass the same fixture gate and
  have real-project evidence, but remain under language-specific completion
  review for large workspaces, dependency metadata, and provider diagnostics.
- Rust large workspaces keep provider-backed declarations/imports and bounded
  map-boundary call enrichment. rust-analyzer's bundled version does not
  support the type-hierarchy request, so trait implementation edges are not
  invented.
- Dynamic dispatch, runtime registration, framework auto-imports, and ORM/DB
  semantics are outside this language-only contract.
