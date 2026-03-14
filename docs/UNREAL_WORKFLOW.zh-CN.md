# Unreal 工作流

## 目标

`MCPHub` 现在有了一层面向 Unreal 的 CLI，适用于这种架构：

- `UnrealCopilot` 插件在 Unreal Editor 进程内托管真实的 MCP Server
- `MCPHub` 负责发现工程、定位引擎、读取 UnrealCopilot 的 MCP 配置
- `MCPHub` 负责拉起 Editor、等待 MCP 端点可用、注册 endpoint，并缓存工具

这意味着我们不再需要复刻旧版 `UnrealMCPHub` 的 Python 管理模型，而是把
Unreal 迁移成 `MCPHub` 之上的一个项目适配层。

## 命令

在 `MCPHub` 目录下执行：

```powershell
cargo run -- unreal status
cargo run -- unreal launch --wait-seconds 180
cargo run -- unreal connect --launch --wait-seconds 180
```

常用参数：

- `--project <path>`
  指定 `.uproject` 文件，或工程内任意子目录。
- `--endpoint-id <id>`
  覆盖默认注册 id；如果不传，会由工程名自动生成。
- `--engine-dir <path>`
  覆盖自动引擎探测。
- `--json`
  输出结构化 JSON，方便脚本或其他系统消费。

## `unreal status` 做什么

- 从当前目录向上查找 `.uproject`
- 读取工程文件里的 `EngineAssociation`
- 自动定位引擎目录
- 从下面两个配置文件中读取 UnrealCopilot MCP 设置：
  - `Config/DefaultEditorPerProjectUserSettings.ini`
  - `Saved/Config/WindowsEditor/EditorPerProjectUserSettings.ini`
- 计算实际 MCP endpoint URL
- 通过真正的 MCP 协议对 endpoint 做健康检查

## `unreal launch` 做什么

- 解析工程与引擎路径
- 执行 `UnrealEditor.exe <project>.uproject`
- 将 Editor 的 stdout 和 stderr 重定向到 `Saved/Logs/` 下的时间戳日志
- 可选等待内嵌 MCP endpoint 变为 healthy

默认日志文件名类似：

- `Saved/Logs/mcphub-unreal-stdout-<timestamp>.log`
- `Saved/Logs/mcphub-unreal-stderr-<timestamp>.log`

## `unreal connect` 做什么

- 先探测 UnrealCopilot 的 MCP endpoint
- 如果 endpoint 还没起来，并且传了 `--launch`，就自动拉起 Editor
- 把 endpoint 注册进 `~/.mcphub/state.json`
- 执行 MCP discover，并把工具缓存下来

所以一旦 `connect` 成功，这个 Unreal 工程就和其他普通 MCP upstream 一样，
变成 `MCPHub` 统一管理的 endpoint。

## 当前 Lyra 验证结果

这个仓库对应的 Lyra 工程已经实测打通，关键参数如下：

- 工程：
  `D:\Projects\Games\Unreal Projects\LyraStarterGame\LyraStarterGame.uproject`
- 引擎：
  `D:\Epic Games\UE_5.7`
- endpoint：
  `http://127.0.0.1:19840/mcp`
- 自动启动：
  `bAutoStartMcpServer=True`

已验证命令：

```powershell
target\debug\mcphub.exe unreal status --json
target\debug\mcphub.exe unreal connect --endpoint-id lyra-local --json
```

实测结果是 endpoint healthy，discover 到 12 个工具。

## 当前限制

- Unreal helper 目前只支持 UnrealCopilot 的 `http` transport
- 引擎自动探测目前偏向 Windows
- 配置读取只覆盖当前 launch / connect 所需字段
- 启动流程默认假设插件会在 Editor 启动后自动拉起 MCP server，或在启动阶段完成拉起

## 下一步值得扩展的点

- 支持 `sse` transport
- 在 `~/.mcphub` 下持久化 per-project Unreal profile
- 支持带鉴权 header 的 Unreal MCP 端点
- 把 Unreal 生命周期控制暴露给 `serve-stdio` facade
- 增加更丰富的项目状态输出，比如 Editor 进程探测、日志 tail 快捷能力
