use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use http::{HeaderName, HeaderValue};
use rmcp::model::{CallToolRequestParams, JsonObject};
use rmcp::service::RunningService;
use rmcp::service::ServiceExt;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::TokioChildProcess;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::Value;
use tokio::process::Command;

use crate::models::{
    EndpointConfig, EndpointHealth, EndpointTransport, ToolCallOutput, ToolCatalogEntry,
};

pub struct McpClient;
pub type ClientSession = RunningService<rmcp::RoleClient, ()>;

impl McpClient {
    pub async fn discover_tools(endpoint: &EndpointConfig) -> Result<Vec<ToolCatalogEntry>> {
        let client = Self::connect(endpoint).await?;
        Self::discover_tools_on_client(&client, endpoint).await
    }

    pub async fn call_tool(
        endpoint: &EndpointConfig,
        tool_name: &str,
        arguments: JsonObject,
    ) -> Result<ToolCallOutput> {
        let client = Self::connect(endpoint).await?;
        Self::call_tool_on_client(&client, endpoint, tool_name, arguments).await
    }

    pub async fn health_check(endpoint: &EndpointConfig) -> Result<EndpointHealth> {
        let client = Self::connect(endpoint).await?;
        Self::health_check_on_client(&client, endpoint, false).await
    }

    pub async fn connect(endpoint: &EndpointConfig) -> Result<ClientSession> {
        match endpoint.transport {
            EndpointTransport::Http => Self::connect_http(endpoint).await,
            EndpointTransport::Stdio => Self::connect_stdio(endpoint).await,
        }
    }

    pub async fn discover_tools_on_client(
        client: &ClientSession,
        endpoint: &EndpointConfig,
    ) -> Result<Vec<ToolCatalogEntry>> {
        let tools = client
            .peer()
            .list_all_tools()
            .await
            .with_context(|| format!("failed to list tools for {}", endpoint.id))?;

        Ok(tools
            .into_iter()
            .map(|tool| ToolCatalogEntry {
                endpoint_id: endpoint.id.clone(),
                qualified_name: format!("{}/{}", endpoint.id, tool.name),
                name: tool.name.to_string(),
                description: tool.description.as_deref().unwrap_or_default().to_string(),
                input_schema: Value::Object((*tool.input_schema).clone()),
            })
            .collect())
    }

    pub async fn call_tool_on_client(
        client: &ClientSession,
        endpoint: &EndpointConfig,
        tool_name: &str,
        arguments: JsonObject,
    ) -> Result<ToolCallOutput> {
        let result = client
            .peer()
            .call_tool(CallToolRequestParams::new(tool_name.to_owned()).with_arguments(arguments))
            .await
            .with_context(|| format!("failed to call {}/{}", endpoint.id, tool_name))?;

        Ok(Self::convert_call_result(endpoint, tool_name, result))
    }

    pub async fn health_check_on_client(
        client: &ClientSession,
        endpoint: &EndpointConfig,
        reused_connection: bool,
    ) -> Result<EndpointHealth> {
        let started = Instant::now();
        let tools = client
            .peer()
            .list_all_tools()
            .await
            .with_context(|| format!("failed to list tools for {}", endpoint.id))?;
        Ok(EndpointHealth {
            endpoint_id: endpoint.id.clone(),
            healthy: true,
            transport: endpoint.transport.clone(),
            target: endpoint.summary().target,
            reused_connection,
            tool_count: Some(tools.len()),
            latency_ms: started.elapsed().as_millis(),
            error: None,
        })
    }

    async fn connect_http(endpoint: &EndpointConfig) -> Result<ClientSession> {
        let url = endpoint
            .url
            .as_ref()
            .context("http endpoint is missing url")?;
        let transport = Self::build_http_transport(endpoint, url)?;
        let client: ClientSession = ()
            .serve(transport)
            .await
            .with_context(|| format!("failed to connect to http endpoint {}", endpoint.id))?;
        Ok(client)
    }

    async fn connect_stdio(endpoint: &EndpointConfig) -> Result<ClientSession> {
        let command = Self::build_command(endpoint)?;
        let transport = TokioChildProcess::new(command)
            .with_context(|| format!("failed to spawn stdio endpoint {}", endpoint.id))?;
        let client: ClientSession = ()
            .serve(transport)
            .await
            .with_context(|| format!("failed to connect to stdio endpoint {}", endpoint.id))?;
        Ok(client)
    }

    fn convert_call_result(
        endpoint: &EndpointConfig,
        tool_name: &str,
        result: rmcp::model::CallToolResult,
    ) -> ToolCallOutput {
        let content = result
            .content
            .into_iter()
            .map(|item| {
                serde_json::to_value(item)
                    .unwrap_or_else(|_| Value::String("unserializable-content".into()))
            })
            .collect();

        ToolCallOutput {
            endpoint_id: endpoint.id.clone(),
            tool_name: tool_name.to_string(),
            success: !result.is_error.unwrap_or(false),
            content,
            structured_content: result.structured_content,
            error: None,
        }
    }

    fn build_command(endpoint: &EndpointConfig) -> Result<Command> {
        let program = endpoint
            .command
            .as_ref()
            .context("stdio endpoint is missing command")?;
        if program.trim().is_empty() {
            bail!("stdio endpoint command is empty");
        }

        let mut command = Command::new(program);
        command.args(&endpoint.args);
        if let Some(cwd) = &endpoint.cwd {
            if !cwd.trim().is_empty() {
                command.current_dir(cwd);
            }
        }
        for (key, value) in &endpoint.env {
            command.env(key, value);
        }
        Ok(command)
    }

    fn build_http_transport(
        endpoint: &EndpointConfig,
        url: &str,
    ) -> Result<StreamableHttpClientTransport<reqwest::Client>> {
        let mut headers = HashMap::new();
        for (key, value) in &endpoint.headers {
            let name = HeaderName::try_from(key.as_str())
                .with_context(|| format!("invalid header name '{}' for {}", key, endpoint.id))?;
            let header_value = HeaderValue::try_from(value.as_str()).with_context(|| {
                format!("invalid header value for '{}' on {}", key, endpoint.id)
            })?;
            headers.insert(name, header_value);
        }

        let config =
            StreamableHttpClientTransportConfig::with_uri(url.to_string()).custom_headers(headers);
        Ok(StreamableHttpClientTransport::from_config(config))
    }
}

pub fn parse_arguments_json(raw: &str) -> Result<JsonObject> {
    if raw.trim().is_empty() {
        return Ok(JsonObject::new());
    }
    let value: Value = serde_json::from_str(raw).context("arguments JSON is invalid")?;
    match value {
        Value::Object(map) => Ok(map),
        _ => bail!("arguments JSON must decode to an object"),
    }
}
