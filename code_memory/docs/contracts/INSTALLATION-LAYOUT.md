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
