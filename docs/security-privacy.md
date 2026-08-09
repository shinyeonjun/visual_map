# Security and privacy boundary

## Product default

The desktop app is local-first and single-user. Project paths, graph snapshots,
map state, and conversations are stored only in the application's local data
directory. There is no account, sync, sharing, collaboration, telemetry, or
automatic GitHub upload in the current product boundary.

## AI boundary

The user selects one installed Codex or Claude CLI adapter for both semantic-map
analysis and chat. No provider is selected automatically. Before the first
semantic analysis for a workspace/provider pair, the UI requires explicit
consent that selected source-evidence excerpts are handed to that CLI and may
reach its external AI service.

The implementation enforces:

- explicit provider/model selection;
- bounded, purpose-specific evidence retrieval instead of whole-repository
  prompt dumps;
- source-excerpt secret redaction before prompt compilation, plus a second
  fail-closed check immediately before the provider process is started;
- masking for key/value credentials, authenticated connection strings, bearer
  and JWT values, common provider-token shapes, and private-key blocks;
- no raw DB row access;
- no credential persistence in workspace or conversation records;
- visible analysis version and stale-state handling after source changes.

Automatic secret recognition cannot prove that every arbitrary high-entropy
literal is a credential. The consent UI says this explicitly; repositories
must not treat committed source as a credential store. The provider receives
only the redacted bounded excerpts selected from verified evidence, never a
whole-repository prompt dump.

Static Fact publication is independent from AI success. If semantic analysis
fails, the newly verified static snapshot remains current and usable while no
semantic revision is published for it. An older semantic map is never shown as
if it belonged to the new static snapshot.

## Filesystem and process boundary

- Workspace paths are canonicalized and constrained to local directories.
- Source opening/reveal commands reject escape through relative paths or
  symlinks/reparse points.
- Sidecars are selected through a verified manifest and run with bounded output,
  timeout, cancellation, and descendant-process cleanup.
- Fact Graph publication is transactional and fail closed.

## Database boundary

Database Memory reads catalogs, schema objects, SQLite metadata, or isolated DDL
metadata. `row_data_access: false` is part of its executable contract. Connection
strings are runtime secrets and are not written to Fact Graph snapshots.

Any future cloud sync, team sharing, browser service, direct provider API, or
agent-facing unified retrieval server requires an explicit threat-model update
before implementation.
