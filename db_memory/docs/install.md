# Database Memory installation

Database Memory is shipped as the desktop application's local database-analysis
engine. A standalone CLI build is retained for development, certification, and
release verification; there is no separate MCP server binary.

## Build from source

Prerequisites:

- Rust and Cargo.
- A working native C/C++ toolchain for adapter dependencies.
- Oracle only: a matching Oracle Instant Client 11.2 or later at runtime.

```powershell
cargo build --release
```

The output is `target/release/database-memory.exe` on Windows and
`target/release/database-memory` on macOS/Linux.

The optional ODBC entrypoint is explicit:

```powershell
cargo build --release --features database-memory-core/odbc
```

ODBC driver availability is not a completeness claim. Only the certified SQL
Server bridge is accepted; other products fail closed.

## Platform notes

- Windows: use the MSVC Rust toolchain and a matching MSVC build environment.
- macOS: install Xcode Command Line Tools or another clang toolchain.
- Linux: install gcc or clang and normal native build tooling.
- Oracle: make the Instant Client shared libraries discoverable before starting
  the desktop app or CLI.

The desktop packaging scripts copy the verified CLI into
`src-tauri/engines/database-memory(.exe)` and validate its manifest hash before
release.
