# MCPHub Handbook

## What This Project Is

`MCPHub` is a generic Rust-based hub for working with upstream MCP servers.

It is designed to sit between a user-facing client and one or more upstream
MCP servers, and provide a stable control surface for:

- registering endpoints under stable ids
- discovering and caching upstream tools
- inspecting cached input schemas
- calling tools directly from a local CLI
- exposing the hub itself as an MCP server over stdio
- reusing upstream connections inside a long-lived runtime

This repository focuses on a reusable MCP core that can work across many
different upstream servers.

## Current Status

Current maturity: working prototype / early foundation

What is already solid:

- Rust CLI project structure
- persistent registry at `~/.mcphub/state.json`
- HTTP upstream MCP support
- stdio upstream MCP support
- tool discovery and local caching
- schema inspection with generated starter templates
- direct invocation by `qualified_name`
- local schema-aware invoke flow
- stdio facade server that exposes hub-native MCP tools
- in-process connection reuse for long-lived runtime scenarios
- health checks with connection reuse visibility
- local daemon mode for cross-command shared runtime reuse
- schema helpers for defaults, enums, and simple composed schemas

What is still early-stage:

- daemon is local-only and intentionally simple
- no automatic background refresh of endpoint/tool metadata
- no authentication secret storage beyond plain persisted config
- no advanced schema coercion for arrays, deep unions, or indexed item paths
- no streaming UX for progressive tool output
- no policy layer, permissions model, or audit log

## Project Layout

- `src/cli.rs`
  Local CLI commands such as `register-http`, `discover`, `tool-info`, `call`,
  `invoke`, and `health`.
- `src/facade.rs`
  MCP facade server implemented with `rmcp` server-side macros.
- `src/runtime.rs`
  In-process session pool for reusable upstream client connections.
- `src/service.rs`
  Higher-level coordination over registry, inspection, and target resolution.
- `src/mcp_client.rs`
  Upstream MCP client operations over HTTP and stdio transports.
- `src/schema_utils.rs`
  Schema-aware coercion and input template generation.
- `src/registry.rs`
  Persistent state management for endpoints and cached tools.
- `src/models.rs`
  Shared data structures for endpoints, tools, inspection, health, and call
  output.
- `docs/PLAN.md`
  Development plan and phase tracking.

## Core Concepts

### Endpoint

An endpoint is a registered upstream MCP server.

Supported transports:

- `http`
- `stdio`

Each endpoint stores:

- `id`
- `name`
- `transport`
- `url` and `headers` for HTTP
- `command`, `args`, `env`, and `cwd` for stdio

### Tool Catalog

When an endpoint is discovered, MCPHub caches tool metadata locally:

- `endpoint_id`
- `name`
- `qualified_name`
- `description`
- `input_schema`

This cache is what enables:

- `list-tools`
- `tool-info`
- schema-aware CLI coercion
- `qualified_name` resolution

### Qualified Name

The canonical tool selector format is:

`<endpoint_id>/<tool_name>`

Example:

`context7/resolve-library-id`

This is supported by:

- `tool-info`
- `call`
- `invoke`
- facade `get-tool-info`
- facade `call-tool`

## Runtime Model

There are currently two operational modes.

### Short-lived CLI Mode

Most CLI commands are one-shot process executions:

1. load registry state
2. resolve endpoint and tool metadata
3. create or reuse an in-process runtime only for that process
4. call upstream MCP
5. print normalized output

Because a normal CLI process exits after the command finishes, connection reuse
only matters within that single process execution.

### Long-lived Facade Mode

`mcphub serve-stdio` starts MCPHub itself as a stdio MCP server.

Inside that process, `HubRuntime` maintains a session pool keyed by endpoint id.
When the same endpoint is used repeatedly:

- the existing connection is reused when possible
- endpoint configuration changes invalidate the old pooled session
- a failed reused session is retried once after invalidation

This is the first version of the "real hub" runtime behavior.

## State and Persistence

Current state file:

- `~/.mcphub/state.json`

Stored data includes:

- endpoint configurations
- HTTP headers
- cached tool catalog entries

Important note:

- secrets in HTTP headers are currently stored in plaintext in the state file

That is acceptable for prototyping, but not a final security posture.

## Command Reference

### Register Endpoints

Register an HTTP endpoint:

```powershell
cargo run -- register-http context7 https://mcp.context7.com/mcp --header CONTEXT7_API_KEY=YOUR_KEY
```

Register a stdio endpoint:

```powershell
cargo run -- register-stdio my-server python --arg -m --arg my_mcp_server
```

List endpoints:

```powershell
cargo run -- list-endpoints
```

Remove an endpoint:

```powershell
cargo run -- remove-endpoint my-server
```

### Discover and Inspect Tools

Discover tools for one endpoint:

```powershell
cargo run -- discover context7
```

List cached tools:

```powershell
cargo run -- list-tools context7
```

List cached tools as JSON:

```powershell
cargo run -- list-tools context7 --json
```

Inspect one tool:

```powershell
cargo run -- tool-info context7/resolve-library-id --json
```

`tool-info` returns:

- description
- raw input schema
- generated input template based on required fields

### Call Tools

Call with raw JSON:

```powershell
cargo run -- call context7/resolve-library-id --arguments-json "{\"libraryName\":\"react\",\"query\":\"react\"}"
```

Call with dotted `--set` paths:

```powershell
cargo run -- call hub-facade call-tool `
  --set qualified_name=context7/resolve-library-id `
  --set arguments.libraryName=react `
  --set arguments.query=react
```

### Schema-driven Local Invoke

`invoke` is a simpler CLI entrypoint intended to feel more like a local command
front-end over cached MCP schemas.

Example:

```powershell
cargo run -- invoke context7/resolve-library-id libraryName=react query=react
```

Useful behaviors:

- accepts `KEY=VALUE` pairs directly
- resolves the cached schema for the selected tool
- applies basic type coercion for that schema path
- supports dotted keys for nested objects

Dry-run example:

```powershell
cargo run -- invoke context7/query-docs libraryId=/facebook/react query="hooks" --dry-run
```

### Health Checks

Check one endpoint:

```powershell
cargo run -- health context7 --json
```

Check the same endpoint twice in one runtime to observe reuse:

```powershell
cargo run -- health context7 --repeat 2 --json
```

When connection reuse is working, later checks in the same process should show:

- `reused_connection: true`

### Daemon Mode

Start the local daemon:

```powershell
cargo run -- daemon start
```

Check status:

```powershell
cargo run -- daemon status
```

Use daemon-backed discovery, invoke, and health:

```powershell
cargo run -- discover context7 --daemon
cargo run -- invoke context7/resolve-library-id libraryName=react query=react --daemon
cargo run -- health context7 --daemon --json
```

Stop the daemon:

```powershell
cargo run -- daemon stop
```

## Facade MCP Server

Start MCPHub itself as a stdio MCP server:

```powershell
cargo run -- serve-stdio
```

Currently exposed hub-native tools:

- `list-endpoints`
- `check-endpoint-health`
- `discover-tools`
- `get-tool-info`
- `list-tools`
- `call-tool`
- `register-http-endpoint`
- `register-stdio-endpoint`
- `remove-endpoint`

Example facade calls from the local CLI:

```powershell
cargo run -- call hub-facade get-tool-info --set qualified_name=context7/resolve-library-id
```

```powershell
cargo run -- call hub-facade check-endpoint-health --set endpoint_id=context7
```

## Schema Handling Behavior

Current schema support is intentionally conservative.

Implemented:

- required-field template generation
- primitive type coercion for:
  - string
  - boolean
  - integer
  - number
  - null
- object/array parsing when valid JSON is supplied
- nested path lookup through `properties`

Not yet implemented:

- `const`
- array item coercion by indexed path
- deep merge with schema-generated defaults
- output schema guided rendering

## Validation Completed

The following flows have been verified during development:

- register and discover `context7` over HTTP
- call `context7/resolve-library-id`
- register `hub-facade` as a stdio endpoint
- discover hub-native tools through the facade
- call `get-tool-info` through the facade
- call `call-tool` through the facade
- run `health --repeat 2` and observe `reused_connection=true` on the second
  check

Unit test status at the time of writing:

- `cargo test` passes
- current test count: 11

## Known Limitations

- connection reuse currently exists only inside one running process
- plain CLI commands still do not share runtime across shell invocations unless
  `--daemon` is used
- no SSE/SSE-auth variant handling beyond what `rmcp` streamable HTTP client
  already provides
- no UI or TUI for browsing endpoints/tools
- output rendering is still mostly raw JSON / normalized JSON
- no retry/backoff policy beyond single retry after reused-session failure
- no endpoint tagging, grouping, or workspace-scoped registry
- no per-tool aliases or generated wrappers

## Recommended Next Features

### High Priority

- stronger daemon lifecycle
  The daemon exists now, but it still needs service discovery, better startup
  metadata, stale-process recovery, and more operational polish.
- secure secret storage
  Replace plaintext header persistence with OS credential store or encrypted
  local secret storage.
- richer schema-driven invoke
  Extend support to arrays, unions, indexed paths, and better validation messages.
- qualified-name native facade routing
  Optionally expose each discovered tool as a hub-generated façade namespace,
  not only through generic `call-tool`.

### Medium Priority

- auto-refresh and health policy
  Refresh tool cache and endpoint health on configurable schedules.
- structured output rendering
  Render text/json/image/resource outputs more ergonomically.
- endpoint profiles
  Presets for docs servers, coding assistants, and internal MCP fleets.
- import/export config
  Move endpoint registry in and out of JSON/TOML bundles cleanly.

### Longer-term

- access control and policy engine
- audit logging and invocation history
- multi-user or shared daemon mode
- HTTP facade server in addition to stdio facade
- workspace-aware registries
- generated shell completions from cached schemas

## Recommended Evaluation Questions

If you are deciding where to take the project next, these are the most useful
questions to answer:

1. Should MCPHub primarily be a local power-user CLI, a reusable MCP proxy
   server, or both equally?
2. Do you want one long-lived daemon that all shell commands talk to?
3. Is secure credential storage required before broader adoption?
4. Do you want dynamic generated per-tool commands, or is generic
   `invoke <qualified_name>` sufficient?
5. Which opinionated workflows, if any, should live on top of this generic
   hub?

## Bottom Line

Today, MCPHub is already good enough to validate the architecture:

- generic endpoint registry
- generic tool discovery
- generic tool invocation
- reusable runtime for long-lived hub processes
- both local CLI and MCP facade surfaces

It is not yet a production-hardened platform, but it is a strong base to
evolve into one.
