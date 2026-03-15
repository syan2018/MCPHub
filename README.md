# MCPHub

Additional documentation:

- [English Handbook](docs/HANDBOOK.md)
- [中文手册](docs/HANDBOOK.zh-CN.md)

`MCPHub` is a generic Rust-based hub for managing upstream MCP servers.

The core idea is simple:

- register upstream MCP endpoints under stable ids
- discover their tools through the MCP protocol
- cache those tool descriptors locally
- call upstream tools by `endpoint_id + tool_name`
- expose hub-native MCP tools through a stdio facade
- grow toward a CLI-style facade over schema-driven invocation

## Implemented In This First Rust Slice

- standalone Cargo project
- persistent endpoint registry
- HTTP endpoint registration
- stdio endpoint registration
- MCP tool discovery through `rmcp`
- direct upstream tool invocation
- CLI for register/list/discover/call
- stdio MCP facade server (`serve-stdio`)
- structured tool catalog output with `list-tools --json`
- schema inspection with `tool-info`, including `tool-info --all --json <endpoint_id>`
- qualified-name invocation such as `context7/resolve-library-id`
- nested CLI argument assignment via dotted `--set`
- schema-driven local invoke flow with positional `KEY=VALUE`
- runtime health checks with observable connection reuse
- local daemon mode for cross-command runtime reuse
- richer schema handling for defaults, enums, and simple composed schemas

## Planned Next

- namespaced / qualified-name invocation options
- richer output formatting for content types
- health monitoring and refresh policies
- better CLI argument coercion from cached JSON schema

## Quick Start

```powershell
cd MCPHub
cargo run -- register-http docs-main http://127.0.0.1:19840/mcp
cargo run -- discover docs-main
cargo run -- list-tools docs-main
cargo run -- tool-info docs-main/search
cargo run -- tool-info --all --json docs-main
cargo run -- call docs-main/search --arguments-json "{\"query\":\"rust\",\"domain\":\"docs\"}"
cargo run -- invoke docs-main/search query=rust domain=docs
cargo run -- daemon start
cargo run -- invoke docs-main/search query=rust domain=docs --daemon
```

To inspect cached schemas as JSON:

```powershell
cargo run -- list-tools docs-main --json
```

To register a stdio server:

```powershell
cargo run -- register-stdio my-server python --arg -m --arg my_mcp_server
```

To serve MCPHub itself as a stdio MCP facade:

```powershell
cargo run -- serve-stdio
```

To inspect every cached tool for one endpoint, including starter input
templates:

```powershell
cargo run -- tool-info --all --json context7
```

You can also inspect one cached tool directly by qualified name:

```powershell
cargo run -- tool-info context7/resolve-library-id --json
```

To use the schema-driven local facade form:

```powershell
cargo run -- invoke context7/resolve-library-id libraryName=react query=react
```

To observe connection reuse inside a single long-lived runtime:

```powershell
cargo run -- health context7 --repeat 2 --json
```

To reuse one runtime across separate shell invocations, start the local daemon:

```powershell
cargo run -- daemon start
cargo run -- daemon status
cargo run -- health context7 --daemon --json
cargo run -- invoke context7/resolve-library-id libraryName=react query=react --daemon
cargo run -- daemon stop
```
