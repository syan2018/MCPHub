# MCPHub Plan

## Vision

Build a transport-aware MCP hub in Rust that can manage any upstream MCP server,
not just Unreal-specific ones.

## Why Rust

- good fit for a long-running broker / proxy process
- strong CLI ecosystem
- official MCP Rust SDK exists as `rmcp`
- easier to evolve into a standalone binary than the original Python sketch

## Verified Foundation

The current implementation strategy is based on the official Rust MCP SDK:

- crate: `rmcp`
- features in use now:
  - `client`
  - `transport-streamable-http-client-reqwest`
  - `transport-child-process`

This gives us the two transports that matter most for phase 1:

- streamable HTTP MCP servers
- stdio child-process MCP servers

## Phase 1 Scope

- persistent endpoint registry
- endpoint types:
  - `http`
  - `stdio`
- discover upstream tools
- cache tool descriptors
- call upstream tools by `endpoint_id + tool_name`
- CLI commands for the flows above

## Phase 2 Scope

- expose MCPHub itself as an MCP server
- add hub-native tools:
  - `register_endpoint`
  - `list_endpoints`
  - `discover_tools`
  - `list_tools`
  - `call_tool`
- decide whether to keep explicit `endpoint_id + tool_name` or also support
  `qualified_name = endpoint_id/tool_name`

### Phase 2 Status

Implemented first facade slice over stdio with hub-native tools for:

- `list-endpoints`
- `check-endpoint-health`
- `discover-tools`
- `get-tool-info`
- `list-tools`
- `call-tool`
- `register-http-endpoint`
- `register-stdio-endpoint`
- `remove-endpoint`

This validates the core "hub as MCP server over other MCP servers" shape.

## Phase 3 Scope

- CLI facade over cached tool schemas
- richer type coercion
- direct invocation by `qualified_name`
- health checks and reconnect policies
- optional presets for domain-specific hubs such as Unreal

### Phase 3 Status

Implemented the first CLI ergonomics slice:

- `tool-info` shows cached schema plus an input template
- `call` accepts either `endpoint_id + tool_name` or `qualified_name`
- dotted `--set` paths can build nested arguments
- cached schema is used for basic value coercion so string-typed fields stay
  strings even when the raw token looks numeric
- `invoke` provides a simpler local facade form: `mcphub invoke <qualified_name> key=value`
- `health --repeat N` exposes whether a connection was reused within one runtime
- local daemon mode now allows reuse across separate CLI invocations
- schema helpers now cover defaults, enums, and simple `allOf`/`oneOf`/`anyOf`
  traversal for templates and coercion

## Current Design

### Endpoint Registry

Each upstream MCP is tracked by:

- `id`
- `name`
- `transport`
- `url` for HTTP
- `command`, `args`, `env`, `cwd` for stdio

### Tool Catalog

Each discovered tool is stored with:

- `endpoint_id`
- `name`
- `qualified_name`
- `description`
- `input_schema`

### Invocation Model

The generic call path is:

1. load endpoint by id
2. create a short-lived `rmcp` client connection
3. initialize the upstream MCP
4. call `list_all_tools()` or `call_tool(...)`
5. normalize and print the result

That keeps the first slice simple and stateless enough to validate the design
before we add long-lived sessions.
