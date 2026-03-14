# MCPHub

Additional documentation:

- [English Handbook](docs/HANDBOOK.md)
- [中文手册](docs/HANDBOOK.zh-CN.md)
- [Unreal Workflow](docs/UNREAL_WORKFLOW.md)
- [Unreal 工作流](docs/UNREAL_WORKFLOW.zh-CN.md)

`MCPHub` is a generic Rust-based hub for managing upstream MCP servers.

The core idea is simple:

- register upstream MCP endpoints under stable ids
- discover their tools through the MCP protocol
- cache those tool descriptors locally
- call upstream tools by `endpoint_id + tool_name`
- expose hub-native MCP tools through a stdio facade
- grow toward a CLI-style facade over schema-driven invocation

This repository is the clean-room start of that direction, separate from the
older Unreal-specific Python hub.

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
- single-tool schema inspection with `tool-info`
- qualified-name invocation such as `context7/resolve-library-id`
- nested CLI argument assignment via dotted `--set`
- schema-driven local invoke flow with positional `KEY=VALUE`
- runtime health checks with observable connection reuse
- local daemon mode for cross-command runtime reuse
- richer schema handling for defaults, enums, and simple composed schemas
- Unreal-aware project discovery, engine detection, launch, and connect flow

## Planned Next

- namespaced / qualified-name invocation options
- richer output formatting for content types
- health monitoring and refresh policies
- better CLI argument coercion from cached JSON schema
- persistable Unreal project profiles on top of the generic core

## Quick Start

```powershell
cd MCPHub
cargo run -- register-http ue-main http://127.0.0.1:19840/mcp
cargo run -- discover ue-main
cargo run -- list-tools ue-main
cargo run -- tool-info ue-main/search
cargo run -- call ue-main/search --arguments-json "{\"query\":\"Lyra\",\"domain\":\"cpp\"}"
cargo run -- invoke ue-main/search query=Lyra domain=cpp
cargo run -- daemon start
cargo run -- invoke ue-main/search query=Lyra domain=cpp --daemon
```

To inspect cached schemas as JSON:

```powershell
cargo run -- list-tools ue-main --json
```

To register a stdio server:

```powershell
cargo run -- register-stdio my-server python --arg -m --arg my_mcp_server
```

To serve MCPHub itself as a stdio MCP facade:

```powershell
cargo run -- serve-stdio
```

To proxy a nested upstream tool call without raw JSON escaping, use dotted
`--set` paths:

```powershell
cargo run -- call hub-facade call-tool `
  --set endpoint_id=context7 `
  --set tool_name=resolve-library-id `
  --set arguments.libraryName=react `
  --set arguments.query=react
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

## Unreal Quick Start

For projects that use the `UnrealCopilot` plugin and expose MCP from inside the
editor process:

```powershell
cargo run -- unreal status
cargo run -- unreal connect --launch --wait-seconds 180
```

To pin a stable endpoint id:

```powershell
cargo run -- unreal connect --endpoint-id lyra-local --launch --wait-seconds 180
```

And the facade can resolve tools by `qualified_name` too:

```powershell
cargo run -- call hub-facade get-tool-info `
  --set qualified_name=context7/resolve-library-id
```

The facade also exposes endpoint health checks:

```powershell
cargo run -- call hub-facade check-endpoint-health `
  --set endpoint_id=context7
```
