# Unreal Workflow

## Goal

`MCPHub` now includes an Unreal-oriented CLI layer for projects that host a
local MCP server through the `UnrealCopilot` plugin.

The migration direction is:

- Unreal Editor hosts the real MCP server inside the project
- `MCPHub` discovers the project, finds the engine install, and checks the
  effective UnrealCopilot MCP settings
- `MCPHub` can launch the editor, wait for the MCP endpoint, register it, and
  cache tools like any other upstream server

This replaces the older Unreal-specific hub idea with a thinner and more
reusable control layer on top of the generic Rust core.

## Commands

From the `MCPHub` directory:

```powershell
cargo run -- unreal status
cargo run -- unreal launch --wait-seconds 180
cargo run -- unreal connect --launch --wait-seconds 180
```

Useful flags:

- `--project <path>`
  Point at a `.uproject` file or a directory inside the project.
- `--endpoint-id <id>`
  Override the registered endpoint id. If omitted, MCPHub derives one from the
  project name.
- `--engine-dir <path>`
  Override automatic engine detection.
- `--json`
  Emit machine-readable output.

## What `unreal status` Does

- walks upward from the current directory until it finds a `.uproject`
- reads `EngineAssociation` from the project file
- detects the engine directory
- reads UnrealCopilot MCP defaults from:
  - `Config/DefaultEditorPerProjectUserSettings.ini`
  - `Saved/Config/WindowsEditor/EditorPerProjectUserSettings.ini`
- computes the expected MCP endpoint URL
- probes endpoint health through the real MCP protocol

## What `unreal launch` Does

- resolves the project and engine executable
- launches `UnrealEditor.exe <project>.uproject`
- redirects editor stdout and stderr into timestamped files under `Saved/Logs/`
- optionally waits for the embedded MCP endpoint to become healthy

Default log file names look like:

- `Saved/Logs/mcphub-unreal-stdout-<timestamp>.log`
- `Saved/Logs/mcphub-unreal-stderr-<timestamp>.log`

## What `unreal connect` Does

- probes the UnrealCopilot MCP endpoint
- optionally launches the editor if the endpoint is not yet available
- registers the endpoint into `~/.mcphub/state.json`
- runs MCP discovery and caches tool metadata locally

This means the project becomes a normal MCPHub-managed endpoint after the first
successful connect.

## Verified Lyra Example

The current Lyra workspace in this repository was validated with:

- project:
  `D:\Projects\Games\Unreal Projects\LyraStarterGame\LyraStarterGame.uproject`
- engine:
  `D:\Epic Games\UE_5.7`
- endpoint:
  `http://127.0.0.1:19840/mcp`
- auto-start:
  `bAutoStartMcpServer=True`

Validated commands:

```powershell
target\debug\mcphub.exe unreal status --json
target\debug\mcphub.exe unreal connect --endpoint-id lyra-local --json
```

The live project reported a healthy endpoint and 12 discovered tools.

## Current Limits

- the Unreal helper currently supports UnrealCopilot projects configured for
  `http` transport
- engine discovery is Windows-oriented right now
- settings extraction is intentionally focused on the fields needed for launch
  and connect
- launch automation assumes the plugin auto-starts its MCP server or otherwise
  brings it up during editor startup

## Next Useful Extensions

- support `sse` transport when the plugin uses it
- persist per-project Unreal profiles under `~/.mcphub`
- support custom MCP headers for secured Unreal-side deployments
- expose Unreal lifecycle actions through the stdio facade
- add richer project-state reporting such as editor process detection and log
  tail shortcuts
