# Codebase Workspace installation layout

The application must not modify a user's system language installations or the
connected project. Managed analyzers, caches, snapshots, logs, and settings are
owned by the app and remain removable as one local boundary.

## Installed application

```text
%LOCALAPPDATA%\Programs\Codebase Workspace\
  codebase-workspace.exe
  resources\
    engines\
      manifest.json
      code-memory-language.exe
      database-memory.exe
      packs\
      provider-bundles\
```

Release packaging verifies engine hashes and the signed provider catalog before
including these resources.

## Local application data

```text
%LOCALAPPDATA%\CodebaseWorkspace\
  app-state.sqlite
  engines\
  workspaces\
    <workspace-id>\
      workspace.json
      fact-graph.sqlite
      snapshots\
      semantic\
      conversation\
  cache\
    code-memory\
    provider-roots\
  managed-providers\
    v3\
      <catalog-version>-<catalog-digest>\
        manifest.json
        .provider-catalog-receipt.json
        .provider-pack-receipt.json
        node\
          .provider-pack-receipt.json
        java\
          .provider-pack-receipt.json
        ...
  logs\
```

Workspace IDs derive from canonical local repository paths and are not display
names. Snapshot, semantic, and conversation generations are versioned; a failed
refresh never mutates the last published generation.

## Provider isolation

Managed providers resolve from the verified provider bundle first. PATH is a
development fallback, not a release dependency. Provider caches and temporary
project models live under app-owned cache directories and never under the
connected repository. Provider execution is timeout/cancellation bounded and
must terminate descendants.

The installer carries a signed catalog plus compressed provider packs. Before
the first analysis of a repository, the code sidecar performs Source Census
without any provider and emits the exact supported-language set plus manifest
digest. The desktop verifies the catalog and extracts only `core` and packs
whose declared language set intersects that receipt. The index process reuses
the validated preflight Source Manifest and a final fresh census still rejects
any repository change before publication.

The app-data store is addressed by the signed catalog digest, not by a selected
language combination. `core` creates the catalog root atomically. Every other
pack is extracted into a private sibling staging directory, checked against the
signed archive size/digest, safe-path/unpacked-size limits, entrypoint bytes and
digest, then atomically renamed into its one top-level directory. Catalog and
per-pack receipts are stored separately. An installed pack is never merged or
overwritten, and another repository reuses it even when its language selection
differs. Thus `TypeScript`, `Java`, and `TypeScript + Java` do not create three
copies of `core`, `node`, or `java`.

The store is append-only within one immutable catalog identity: a later
analysis may add a previously unused signed pack, but cannot mutate any pack
already published. Only packs required by the current Source Census have their
entrypoints reverified and scheduled. The compressed archives still ship with
the offline installer; splitting them into separately downloadable artifacts
requires a signed update transport and is not implied by selective local
activation.

## Project boundary

The engine reads source and supported project metadata. It must not write IDE
settings, dependency locks, build files, generated sources, package caches, or
language-server state into the repository. Temporary inferred TypeScript,
JavaScript, and other provider workspaces live in app cache.

## Uninstall boundary

Uninstall removes the executable and resources. User-approved data removal may
also delete `CodebaseWorkspace` app data. The product must not remove user
repositories, external DBs, system SDKs, global provider installations, or
files outside its canonical app-data root.
