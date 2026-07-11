# Engine Sidecars

Release builds bundle the internal engine binaries as Tauri resources from
`src-tauri/engines`:

- `codebase-memory-mcp.exe`
- `database-memory.exe`

The app treats these as local internal sidecar engines. Do not run engine
installer, setup, MCP registration, or global config commands from packaging.
