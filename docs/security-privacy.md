# Security and privacy boundary

## Product default

The desktop app is local-first and single-user. Project paths, graph snapshots,
map state, and conversations are stored only in the application's local data
directory. There is no account, sync, sharing, collaboration, telemetry, or
automatic GitHub upload in the current product boundary.

## AI boundary

The user selects one installed Codex or Claude CLI adapter for both semantic-map
analysis and chat. No provider is selected automatically. Static and database
analysis remains available from the last published snapshot when the provider
is unavailable; AI-derived refresh and chat are unavailable.

Before AI integration is enabled, the implementation must enforce:

- explicit provider/model selection;
- bounded, purpose-specific evidence retrieval instead of whole-repository
  prompt dumps;
- secret/path redaction at the process boundary;
- no raw DB row access;
- no credential persistence in workspace or conversation records;
- visible analysis version and stale-state handling after source changes.

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
