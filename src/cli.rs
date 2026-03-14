use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::daemon::{self, DaemonRequest, DaemonResponse};
use crate::facade;
use crate::mcp_client::parse_arguments_json;
use crate::runtime::HubRuntime;
use crate::schema_utils::{build_default_input, coerce_value_for_path};
use crate::service::HubService;

#[derive(Debug, Parser)]
#[command(name = "mcphub")]
#[command(about = "Generic Rust MCP hub")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    RegisterHttp(RegisterHttpArgs),
    RegisterStdio(RegisterStdioArgs),
    RemoveEndpoint(RemoveEndpointArgs),
    ListEndpoints,
    Discover(DiscoverArgs),
    ListTools(ListToolsArgs),
    ToolInfo(ToolInfoArgs),
    Call(CallArgs),
    Invoke(InvokeArgs),
    Health(HealthArgs),
    Daemon(DaemonArgs),
    ServeStdio,
}

#[derive(Debug, Args)]
struct RegisterHttpArgs {
    endpoint_id: String,
    url: String,
    #[arg(long = "header", value_parser = parse_env_pair)]
    headers: Vec<(String, String)>,
    #[arg(long, default_value = "")]
    name: String,
}

#[derive(Debug, Args)]
struct RegisterStdioArgs {
    endpoint_id: String,
    command: String,
    #[arg(long = "arg")]
    args: Vec<String>,
    #[arg(long = "env", value_parser = parse_env_pair)]
    env: Vec<(String, String)>,
    #[arg(long)]
    cwd: Option<String>,
    #[arg(long, default_value = "")]
    name: String,
}

#[derive(Debug, Args)]
struct RemoveEndpointArgs {
    endpoint_id: String,
}

#[derive(Debug, Args)]
struct DiscoverArgs {
    endpoint_id: String,
    #[arg(long)]
    daemon: bool,
}

#[derive(Debug, Args)]
struct ListToolsArgs {
    endpoint_id: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct CallArgs {
    endpoint_or_qualified: String,
    tool_name: Option<String>,
    #[arg(long = "arguments-json", default_value = "{}")]
    arguments_json: String,
    #[arg(long = "set", value_parser = parse_env_pair)]
    set: Vec<(String, String)>,
    #[arg(long)]
    daemon: bool,
}

#[derive(Debug, Args)]
struct ToolInfoArgs {
    endpoint_or_qualified: String,
    tool_name: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct InvokeArgs {
    qualified_name: String,
    #[arg(value_name = "KEY=VALUE")]
    arguments: Vec<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    daemon: bool,
}

#[derive(Debug, Args)]
struct HealthArgs {
    endpoint_id: Option<String>,
    #[arg(long, default_value_t = 1)]
    repeat: usize,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    daemon: bool,
}

#[derive(Debug, Args)]
struct DaemonArgs {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    Start,
    Status,
    Stop,
    #[command(hide = true)]
    Run,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::RegisterHttp(args) => register_http(args).await,
        Command::RegisterStdio(args) => register_stdio(args).await,
        Command::RemoveEndpoint(args) => remove_endpoint(args).await,
        Command::ListEndpoints => list_endpoints().await,
        Command::Discover(args) => discover(args).await,
        Command::ListTools(args) => list_tools(args).await,
        Command::ToolInfo(args) => tool_info(args).await,
        Command::Call(args) => call(args).await,
        Command::Invoke(args) => invoke(args).await,
        Command::Health(args) => health(args).await,
        Command::Daemon(args) => daemon_command(args).await,
        Command::ServeStdio => facade::serve_stdio().await,
    }
}

async fn register_http(args: RegisterHttpArgs) -> Result<()> {
    let mut service = HubService::load()?;
    let header_count = args.headers.len();
    service.register_http_endpoint(&args.endpoint_id, &args.url, args.headers, &args.name)?;
    println!(
        "registered {} -> {} ({} header(s))",
        args.endpoint_id, args.url, header_count
    );
    Ok(())
}

async fn register_stdio(args: RegisterStdioArgs) -> Result<()> {
    let mut service = HubService::load()?;
    service.register_stdio_endpoint(
        &args.endpoint_id,
        &args.command,
        args.args,
        args.env,
        args.cwd,
        &args.name,
    )?;
    println!("registered {} -> stdio:{}", args.endpoint_id, args.command);
    Ok(())
}

async fn remove_endpoint(args: RemoveEndpointArgs) -> Result<()> {
    let mut service = HubService::load()?;
    if service.remove_endpoint(&args.endpoint_id)? {
        println!("removed {}", args.endpoint_id);
        return Ok(());
    }
    bail!("endpoint '{}' not found", args.endpoint_id)
}

async fn list_endpoints() -> Result<()> {
    let service = HubService::load()?;
    if service.list_endpoints().is_empty() {
        println!("no endpoints registered");
        return Ok(());
    }

    for endpoint in service.list_endpoints() {
        match endpoint.transport {
            crate::models::EndpointTransport::Http => {
                println!(
                    "{} [{}] {} headers={}",
                    endpoint.id,
                    endpoint.name,
                    endpoint.url.as_deref().unwrap_or("-"),
                    endpoint.headers.len()
                );
            }
            crate::models::EndpointTransport::Stdio => {
                println!(
                    "{} [{}] stdio:{} {}",
                    endpoint.id,
                    endpoint.name,
                    endpoint.command.as_deref().unwrap_or("-"),
                    endpoint.args.join(" ")
                );
            }
        }
    }
    Ok(())
}

async fn discover(args: DiscoverArgs) -> Result<()> {
    let tools = if args.daemon {
        match daemon::request(DaemonRequest::Discover {
            endpoint_id: args.endpoint_id.clone(),
        })
        .await?
        {
            DaemonResponse::DiscoverResult { tools } => tools,
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {:?}", other),
        }
    } else {
        let mut service = HubService::load()?;
        service.discover_tools(&args.endpoint_id).await?
    };
    println!(
        "discovered {} tool(s) for {}",
        tools.len(),
        args.endpoint_id
    );
    for tool in tools {
        println!("  {}", tool.qualified_name);
    }
    Ok(())
}

async fn list_tools(args: ListToolsArgs) -> Result<()> {
    let service = HubService::load()?;
    let tools = service.list_tools(args.endpoint_id.as_deref());
    if tools.is_empty() {
        println!("no cached tools");
        return Ok(());
    }

    if args.json {
        let serialized = tools.into_iter().cloned().collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&serialized)?);
        return Ok(());
    }

    for tool in tools {
        println!("{}", tool.qualified_name);
    }
    Ok(())
}

async fn call(args: CallArgs) -> Result<()> {
    let service = HubService::load()?;
    let target =
        service.resolve_tool_target(&args.endpoint_or_qualified, args.tool_name.as_deref())?;
    let schema = target.cached_tool.as_ref().map(|tool| &tool.input_schema);
    let mut arguments = parse_arguments_json(&args.arguments_json)?;
    for (key, value) in args.set {
        let key_parts = split_path(&key)?;
        let coerced = coerce_value_for_path(schema, &key_parts, &value);
        insert_argument_path_segments(&mut arguments, &key_parts, coerced);
    }
    let result = if args.daemon {
        match daemon::request(DaemonRequest::Call {
            endpoint_id: target.endpoint_id.clone(),
            tool_name: target.tool_name.clone(),
            arguments,
        })
        .await?
        {
            DaemonResponse::CallResult { output } => output,
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {:?}", other),
        }
    } else {
        service
            .call_tool(&target.endpoint_id, &target.tool_name, arguments)
            .await?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn tool_info(args: ToolInfoArgs) -> Result<()> {
    let service = HubService::load()?;
    let target =
        service.resolve_tool_target(&args.endpoint_or_qualified, args.tool_name.as_deref())?;
    let tool = service.inspect_tool(&target.endpoint_id, &target.tool_name)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&tool)?);
        return Ok(());
    }

    println!("qualified_name: {}", tool.qualified_name);
    println!("description: {}", tool.description);
    println!();
    println!("input_schema:");
    println!("{}", serde_json::to_string_pretty(&tool.input_schema)?);
    println!();
    println!("input_template:");
    println!("{}", serde_json::to_string_pretty(&tool.input_template)?);
    Ok(())
}

async fn invoke(args: InvokeArgs) -> Result<()> {
    let service = HubService::load()?;
    let target = service.resolve_tool_target(&args.qualified_name, None)?;
    let tool = service.inspect_tool(&target.endpoint_id, &target.tool_name)?;
    let mut arguments = match build_default_input(&tool.input_schema) {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };

    for raw in args.arguments {
        let (key, value) = parse_env_pair(&raw).map_err(anyhow::Error::msg)?;
        let key_parts = split_path(&key)?;
        let coerced = coerce_value_for_path(Some(&tool.input_schema), &key_parts, &value);
        insert_argument_path_segments(&mut arguments, &key_parts, coerced);
    }

    if args.dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(arguments))?
        );
        return Ok(());
    }

    let result = if args.daemon {
        match daemon::request(DaemonRequest::Call {
            endpoint_id: target.endpoint_id.clone(),
            tool_name: target.tool_name.clone(),
            arguments,
        })
        .await?
        {
            DaemonResponse::CallResult { output } => output,
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {:?}", other),
        }
    } else {
        let runtime = HubRuntime::new();
        runtime
            .call_tool(&target.endpoint_id, &target.tool_name, arguments)
            .await?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn health(args: HealthArgs) -> Result<()> {
    let service = HubService::load()?;
    let mut statuses = Vec::new();
    let repeat = args.repeat.max(1);

    match args.endpoint_id.as_deref() {
        Some(endpoint_id) => {
            for _ in 0..repeat {
                statuses.push(fetch_health(endpoint_id, args.daemon).await?);
            }
        }
        None => {
            let endpoint_ids = service
                .list_endpoints()
                .iter()
                .map(|endpoint| endpoint.id.clone())
                .collect::<Vec<_>>();
            for _ in 0..repeat {
                for endpoint_id in &endpoint_ids {
                    statuses.push(fetch_health(endpoint_id, args.daemon).await?);
                }
            }
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&statuses)?);
        return Ok(());
    }

    for status in statuses {
        println!(
            "{} healthy={} reused={} latency_ms={} tools={} target={}",
            status.endpoint_id,
            status.healthy,
            status.reused_connection,
            status.latency_ms,
            status
                .tool_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "-".to_string()),
            status.target
        );
        if let Some(error) = status.error {
            println!("  error: {}", error);
        }
    }
    Ok(())
}

async fn daemon_command(args: DaemonArgs) -> Result<()> {
    match args.command {
        DaemonCommand::Start => daemon::start().await,
        DaemonCommand::Status => daemon::status().await,
        DaemonCommand::Stop => daemon::stop().await,
        DaemonCommand::Run => daemon::run().await,
    }
}

async fn fetch_health(
    endpoint_id: &str,
    use_daemon: bool,
) -> Result<crate::models::EndpointHealth> {
    if use_daemon {
        match daemon::request(DaemonRequest::Health {
            endpoint_id: endpoint_id.to_string(),
        })
        .await?
        {
            DaemonResponse::HealthResult { status } => Ok(status),
            DaemonResponse::Error { message } => Err(anyhow::Error::msg(message)),
            other => Err(anyhow::Error::msg(format!(
                "unexpected daemon response: {:?}",
                other
            ))),
        }
    } else {
        let runtime = HubRuntime::new();
        runtime.health_check(endpoint_id).await
    }
}

fn parse_env_pair(raw: &str) -> Result<(String, String), String> {
    let (key, value) = raw
        .split_once('=')
        .ok_or_else(|| "expected KEY=VALUE".to_string())?;
    if key.trim().is_empty() {
        return Err("environment variable key cannot be empty".to_string());
    }
    Ok((key.to_string(), value.to_string()))
}

#[cfg(test)]
fn parse_inline_value(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

#[cfg(test)]
fn insert_argument_path(
    arguments: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) -> Result<()> {
    let parts = split_path(key)?;
    insert_argument_path_segments(arguments, &parts, value);
    Ok(())
}

fn insert_argument_path_segments(
    arguments: &mut serde_json::Map<String, serde_json::Value>,
    parts: &[&str],
    value: serde_json::Value,
) {
    if let Some((first, rest)) = parts.split_first() {
        if rest.is_empty() {
            arguments.insert((*first).to_string(), value);
            return;
        }

        let entry = arguments
            .entry((*first).to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !entry.is_object() {
            *entry = serde_json::Value::Object(serde_json::Map::new());
        }
        insert_argument_path_segments(entry.as_object_mut().expect("object ensured"), rest, value);
    }
}

fn split_path(key: &str) -> Result<Vec<&str>> {
    let parts = key.split('.').map(str::trim).collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        bail!(
            "invalid --set path '{}': expected segments separated by '.'",
            key
        );
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{insert_argument_path, parse_inline_value, split_path};
    use crate::schema_utils::{build_input_template, coerce_value_for_path};

    #[test]
    fn parse_inline_value_accepts_json_literals() {
        assert_eq!(parse_inline_value("3"), json!(3));
        assert_eq!(parse_inline_value("true"), json!(true));
        assert_eq!(parse_inline_value("{\"x\":1}"), json!({"x": 1}));
    }

    #[test]
    fn parse_inline_value_falls_back_to_string() {
        assert_eq!(parse_inline_value("react"), json!("react"));
    }

    #[test]
    fn insert_argument_path_builds_nested_objects() {
        let mut arguments = serde_json::Map::new();
        insert_argument_path(
            &mut arguments,
            "arguments.libraryName",
            parse_inline_value("react"),
        )
        .unwrap();
        insert_argument_path(
            &mut arguments,
            "arguments.query",
            parse_inline_value("react"),
        )
        .unwrap();

        assert_eq!(
            serde_json::Value::Object(arguments),
            json!({
                "arguments": {
                    "libraryName": "react",
                    "query": "react"
                }
            })
        );
    }

    #[test]
    fn split_path_rejects_empty_segments() {
        assert!(split_path("arguments..query").is_err());
    }

    #[test]
    fn coerce_value_prefers_cached_string_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        });

        assert_eq!(
            coerce_value_for_path(Some(&schema), &["query"], "123"),
            json!("123")
        );
    }

    #[test]
    fn build_template_for_required_fields() {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["query"]
        });

        assert_eq!(build_input_template(&schema), json!({"query": "<string>"}));
    }
}
