# MCPHub 使用与架构手册

## 这个项目是什么

`MCPHub` 是一个通用的、基于 Rust 的 MCP Hub，用来管理上游 MCP Server。

它的目标是先抽出一个可复用的 MCP 核心层，用于：

- 用稳定 id 注册上游 MCP endpoint
- 发现并缓存上游工具
- 检查缓存下来的输入 schema
- 通过本地 CLI 直接调用工具
- 通过 stdio 把 Hub 自己暴露成一个 MCP Server
- 在长生命周期运行态里复用上游连接

这个仓库当前重点是先把通用能力做扎实，形成一个可复用的 MCP 核心。

## 当前项目状态

当前成熟度：可运行原型 / 早期基础设施

已经比较稳的部分：

- Rust CLI 工程结构
- 持久化 registry，路径为 `~/.mcphub/state.json`
- HTTP 上游 MCP 支持
- stdio 上游 MCP 支持
- 工具发现与本地缓存
- schema 检查与输入模板生成
- 通过 `qualified_name` 直接调用工具
- 本地 schema-aware 的 `invoke` 调用入口
- 通过 stdio 暴露 hub-native MCP 工具
- 长生命周期运行时中的连接复用
- 可观察连接复用状态的健康检查

仍然属于早期实现的部分：

- daemon 目前是本地型、轻量级的第一版
- 没有自动后台刷新 endpoint / tool 元数据
- 认证信息目前仍是明文配置存储
- 还没有完整支持复杂 schema 类型，如数组深层 coercion、深层 union 判定与索引路径
- 还没有流式输出的更好体验
- 还没有权限、策略、审计日志这类治理能力

## 项目结构

- `src/cli.rs`
  本地 CLI 命令入口，如 `register-http`、`discover`、`tool-info`、`call`、`invoke`、`health`
- `src/facade.rs`
  基于 `rmcp` 宏实现的 MCP facade server
- `src/runtime.rs`
  进程内 session pool，负责复用上游连接
- `src/service.rs`
  registry、tool inspect、tool target resolve 等较高层逻辑
- `src/mcp_client.rs`
  面向 HTTP / stdio transport 的上游 MCP client 操作
- `src/schema_utils.rs`
  schema-aware coercion 和输入模板生成
- `src/registry.rs`
  endpoint 与缓存工具的持久化状态管理
- `src/models.rs`
  endpoint、tool、inspection、health、call output 等共享数据模型
- `docs/PLAN.md`
  开发规划与阶段状态

## 核心概念

### Endpoint

Endpoint 就是一个注册进来的上游 MCP Server。

当前支持两类 transport：

- `http`
- `stdio`

每个 endpoint 保存的信息包括：

- `id`
- `name`
- `transport`
- HTTP 模式下的 `url` 与 `headers`
- stdio 模式下的 `command`、`args`、`env`、`cwd`

### Tool Catalog

当你对某个 endpoint 执行 `discover` 后，MCPHub 会把工具元数据缓存到本地，包括：

- `endpoint_id`
- `name`
- `qualified_name`
- `description`
- `input_schema`

这份缓存支撑了下面这些能力：

- `list-tools`
- `tool-info`
- schema-aware 的 CLI coercion
- `qualified_name` 解析

### Qualified Name

工具的标准选择器格式是：

`<endpoint_id>/<tool_name>`

例如：

`context7/resolve-library-id`

当前这些入口都支持这种写法：

- `tool-info`
- `call`
- `invoke`
- facade 的 `get-tool-info`
- facade 的 `call-tool`

## 运行时模型

当前有两种主要使用方式。

### 短生命周期 CLI 模式

大多数 CLI 命令都是一次性进程：

1. 加载 registry 状态
2. 解析 endpoint 与 tool 元数据
3. 在当前进程里创建或复用一个 runtime
4. 调用上游 MCP
5. 输出结果

因为普通 CLI 进程执行完就退出，所以连接复用只在同一个进程内部有意义。

### 长生命周期 Facade 模式

`mcphub serve-stdio` 会把 MCPHub 本身启动成一个 stdio MCP Server。

在这个进程内部，`HubRuntime` 会维护一个按 endpoint id 建立的 session pool。

当同一个 endpoint 被反复调用时：

- 如果旧连接还能用，就复用
- 如果 endpoint 配置发生变化，就使旧 session 失效
- 如果复用连接失败，会先失效旧连接，再重连重试一次

这是目前最接近“真正 Hub 运行态”的实现。

## 状态与持久化

当前状态文件路径：

- `~/.mcphub/state.json`

里面会保存：

- endpoint 配置
- HTTP headers
- 缓存下来的工具目录

一个重要说明：

- HTTP header 里的 secret 目前是明文保存在状态文件里的

对于原型阶段这是可接受的，但这不是最终安全形态。

## 命令说明

### 注册 Endpoint

注册 HTTP endpoint：

```powershell
cargo run -- register-http context7 https://mcp.context7.com/mcp --header CONTEXT7_API_KEY=YOUR_KEY
```

注册 stdio endpoint：

```powershell
cargo run -- register-stdio my-server python --arg -m --arg my_mcp_server
```

列出 endpoint：

```powershell
cargo run -- list-endpoints
```

删除 endpoint：

```powershell
cargo run -- remove-endpoint my-server
```

### 发现与检查工具

发现某个 endpoint 的工具：

```powershell
cargo run -- discover context7
```

列出缓存工具：

```powershell
cargo run -- list-tools context7
```

以 JSON 形式列出缓存工具：

```powershell
cargo run -- list-tools context7 --json
```

检查单个工具：

```powershell
cargo run -- tool-info context7/resolve-library-id --json
```

`tool-info` 会返回：

- description
- 原始输入 schema
- 基于 required 字段生成的输入模板

### 调用工具

用原始 JSON 调用：

```powershell
cargo run -- call context7/resolve-library-id --arguments-json "{\"libraryName\":\"react\",\"query\":\"react\"}"
```

用 dotted `--set` 传嵌套参数：

```powershell
cargo run -- call hub-facade call-tool `
  --set qualified_name=context7/resolve-library-id `
  --set arguments.libraryName=react `
  --set arguments.query=react
```

### Schema 驱动的本地 Invoke

`invoke` 是一个更像“本地命令入口”的调用方式，目标是让 CLI 更像一个 schema 驱动的 façade。

例如：

```powershell
cargo run -- invoke context7/resolve-library-id libraryName=react query=react
```

当前 `invoke` 的行为：

- 直接接收 `KEY=VALUE`
- 根据缓存 schema 找到对应工具
- 按 schema 路径做基础类型转换
- 支持 dotted key 构造嵌套对象

Dry-run 示例：

```powershell
cargo run -- invoke context7/query-docs libraryId=/facebook/react query="hooks" --dry-run
```

### 健康检查

检查单个 endpoint：

```powershell
cargo run -- health context7 --json
```

在同一个 runtime 内连续检查两次，观察连接复用：

```powershell
cargo run -- health context7 --repeat 2 --json
```

如果连接复用生效，后面的结果里应该能看到：

- `reused_connection: true`

### Daemon 模式

启动本地 daemon：

```powershell
cargo run -- daemon start
```

查看状态：

```powershell
cargo run -- daemon status
```

通过 daemon 使用 discover、invoke 与 health：

```powershell
cargo run -- discover context7 --daemon
cargo run -- invoke context7/resolve-library-id libraryName=react query=react --daemon
cargo run -- health context7 --daemon --json
```

停止 daemon：

```powershell
cargo run -- daemon stop
```

## Facade MCP Server

把 MCPHub 自己作为 stdio MCP Server 启动：

```powershell
cargo run -- serve-stdio
```

当前暴露的 hub-native tools 有：

- `list-endpoints`
- `check-endpoint-health`
- `discover-tools`
- `get-tool-info`
- `list-tools`
- `call-tool`
- `register-http-endpoint`
- `register-stdio-endpoint`
- `remove-endpoint`

通过本地 CLI 调用 facade 的例子：

```powershell
cargo run -- call hub-facade get-tool-info --set qualified_name=context7/resolve-library-id
```

```powershell
cargo run -- call hub-facade check-endpoint-health --set endpoint_id=context7
```

## Schema 处理行为

当前 schema 支持是“保守而实用”的版本。

已经支持：

- required 字段的输入模板生成
- 以下 primitive type 的基础 coercion：
  - string
  - boolean
  - integer
  - number
  - null
- 输入 JSON 合法时，对 object / array 的解析
- 通过 `properties` 做嵌套路径查找

还没有支持：

- `const`
- 按数组索引路径做 item coercion
- 与 schema default 的深层 merge
- 基于 output schema 的结果渲染

## 已验证能力

开发过程中已经验证过这些链路：

- 通过 HTTP 注册并发现 `context7`
- 调用 `context7/resolve-library-id`
- 把 `hub-facade` 作为 stdio endpoint 注册回来
- 通过 facade 发现 hub-native tools
- 通过 facade 调用 `get-tool-info`
- 通过 facade 调用 `call-tool`
- 运行 `health --repeat 2` 并看到第二次返回 `reused_connection=true`

编写本文档时的测试状态：

- `cargo test` 通过
- 当前测试数：11

## 当前限制

- 连接复用目前只存在于单个运行进程内部
- 普通 CLI 若不启用 `--daemon`，仍然不会跨命令共享 runtime
- 除 `rmcp` 自带 HTTP streamable client 外，没有额外的 SSE 变体增强
- 还没有专门的 UI / TUI 浏览 endpoint 与 tool
- 输出渲染目前仍偏原始 JSON / 规范化 JSON
- 除“复用连接失败后重连重试一次”外，没有更完整的 retry / backoff 策略
- 还没有 endpoint tag、分组、workspace 级 registry
- 还没有 per-tool alias 或自动生成 wrapper

## 建议扩展方向

### 高优先级

- 更成熟的 daemon 生命周期
  现在已经有 daemon，但还缺少更完善的启动元数据、陈旧进程恢复、运维友好性等能力。
- 安全的 secret 存储
  把明文 header 持久化升级为系统凭据库或本地加密存储。
- 更强的 schema-driven invoke
  继续增强 array、union、索引路径，以及更好的校验报错。
- 基于 qualified-name 的原生 facade 路由
  不只通过泛型 `call-tool` 调用，还可以考虑把发现到的工具映射成更自然的 hub namespace。

### 中优先级

- 自动刷新与健康策略
  按可配置周期刷新工具缓存和 endpoint 健康状态。
- 结构化输出渲染
  更好地处理 text / json / image / resource 等类型。
- endpoint profile
  为文档服务、编码助手、内部 MCP 集群提供预置模板。
- 配置导入导出
  让 endpoint registry 可以更方便地以 JSON/TOML bundle 迁移。

### 更长期

- 权限控制与策略引擎
- 审计日志与调用历史
- 多用户 / 共享 daemon mode
- 除 stdio 外的 HTTP facade server
- workspace-aware registry
- 基于缓存 schema 的 shell completion / wrapper 生成

## 你在评估路线时最值得先回答的问题

如果你准备决定下一阶段投入方向，我建议优先想清楚这几个问题：

1. MCPHub 的主要定位，是本地 power-user CLI，还是通用 MCP proxy server，还是两者并重？
2. 你是否希望所有 shell 命令都通过一个后台常驻 daemon 来共享 runtime？
3. 在更广范围使用之前，安全 secret 存储是不是必须先做？
4. 你更想要动态生成的 per-tool command，还是泛型的 `invoke <qualified_name>` 已经足够？
5. 未来是否需要在这个通用 Hub 之上再叠加某些更强约束的工作流？

## 总结

到现在为止，MCPHub 已经足够验证整体架构：

- 通用 endpoint registry
- 通用 tool discovery
- 通用 tool invocation
- 面向长生命周期 Hub 的可复用 runtime
- 同时具备本地 CLI 和 MCP facade 两个表面

它还不是一个生产级、治理完备的平台，但已经是一个很不错的可演进基础。
