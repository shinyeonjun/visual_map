# Third-Party Notices

Backend Visual Map uses the following local sidecar programs. They run only as
internal metadata-analysis tools. The application does not automatically
register either program with an MCP client.

## codebase-memory-mcp

- Project: `DeusData/codebase-memory-mcp`
- Source: https://github.com/DeusData/codebase-memory-mcp
- Bundled program: `codebase-memory-mcp.exe`
- License: MIT
- Copyright (c) 2025 DeusData

```text
MIT License

Copyright (c) 2025 DeusData

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## database-memory

- Project: `shinyeonjun/rdb-memory-mcp`
- Source: https://github.com/shinyeonjun/rdb-memory-mcp
- Bundled program: `database-memory.exe`
- License: MIT
- Copyright (c) 2026

```text
MIT License

Copyright (c) 2026

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Application dependencies

JavaScript and Rust dependencies are resolved by `package-lock.json` and
`src-tauri/Cargo.lock`. Their license inventory must be regenerated and
reviewed for every release candidate with `npm run release:inventory`; CI runs
the same locked-metadata contract in verification mode. Generated inventory
files live under the ignored `release-artifacts/` directory. The application
package license remains an explicit owner gate; a missing dependency license
fails generation.
