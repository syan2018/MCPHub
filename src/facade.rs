use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use rmcp::{
    Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::{
    EndpointHealth, EndpointSummary, ToolCallOutput, ToolCatalogEntry, ToolInspection,
};
use crate::runtime::HubRuntime;
use crate::service::HubService;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct DiscoverToolsRequest {
    endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct HealthCheckRequest {
    endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ListToolsRequest {
    endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CallToolRequest {
    endpoint_id: Option<String>,
    tool_name: Option<String>,
    qualified_name: Option<String>,
    #[serde(default = "empty_json_object")]
    arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct GetToolInfoRequest {
    endpoint_id: Option<String>,
    tool_name: Option<String>,
    qualified_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct RegisterHttpEndpointRequest {
    endpoint_id: String,
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct RegisterStdioEndpointRequest {
    endpoint_id: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    cwd: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct RemoveEndpointRequest {
    endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct RemoveEndpointResponse {
    removed: bool,
    endpoint_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EndpointListResponse {
    endpoints: Vec<EndpointSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ToolListResponse {
    tools: Vec<ToolCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ToolInspectionResponse {
    tool: ToolInspection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EndpointHealthResponse {
    status: EndpointHealth,
}

#[derive(Debug, Clone)]
pub struct HubFacade {
    runtime: Arc<HubRuntime>,
    tool_router: ToolRouter<Self>,
}

impl HubFacade {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(HubRuntime::new()),
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for HubFacade {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for HubFacade {}

#[tool_router(router = tool_router)]
impl HubFacade {
    #[tool(
        name = "list-endpoints",
        description = "List registered MCP endpoints."
    )]
    async fn list_endpoints(&self) -> Result<Json<EndpointListResponse>, String> {
        let service = HubService::load().map_err(to_tool_error)?;
        let endpoints = service
            .list_endpoints()
            .iter()
            .map(|endpoint| endpoint.summary())
            .collect::<Vec<_>>();
        Ok(Json(EndpointListResponse { endpoints }))
    }

    #[tool(
        name = "discover-tools",
        description = "Connect to an upstream MCP endpoint and refresh its cached tool catalog."
    )]
    async fn discover_tools(
        &self,
        Parameters(request): Parameters<DiscoverToolsRequest>,
    ) -> Result<Json<ToolListResponse>, String> {
        let tools = self
            .runtime
            .discover_tools(&request.endpoint_id)
            .await
            .map_err(to_tool_error)?;
        Ok(Json(ToolListResponse { tools }))
    }

    #[tool(
        name = "list-tools",
        description = "List cached tools for one endpoint or for all registered endpoints."
    )]
    async fn list_tools(
        &self,
        Parameters(request): Parameters<ListToolsRequest>,
    ) -> Result<Json<ToolListResponse>, String> {
        let service = HubService::load().map_err(to_tool_error)?;
        let tools = service
            .list_tools(request.endpoint_id.as_deref())
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        Ok(Json(ToolListResponse { tools }))
    }

    #[tool(
        name = "check-endpoint-health",
        description = "Check whether one registered endpoint is reachable and report latency plus reuse state."
    )]
    async fn check_endpoint_health(
        &self,
        Parameters(request): Parameters<HealthCheckRequest>,
    ) -> Result<Json<EndpointHealthResponse>, String> {
        let status = self
            .runtime
            .health_check(&request.endpoint_id)
            .await
            .map_err(to_tool_error)?;
        Ok(Json(EndpointHealthResponse { status }))
    }

    #[tool(
        name = "get-tool-info",
        description = "Get cached schema and a starter input template for one discovered tool."
    )]
    async fn get_tool_info(
        &self,
        Parameters(request): Parameters<GetToolInfoRequest>,
    ) -> Result<Json<ToolInspectionResponse>, String> {
        let service = HubService::load().map_err(to_tool_error)?;
        let target = resolve_request_target(
            request.endpoint_id.as_deref(),
            request.tool_name.as_deref(),
            request.qualified_name.as_deref(),
        )
        .map_err(to_tool_error)?;
        let tool = service
            .inspect_tool(&target.endpoint_id, &target.tool_name)
            .map_err(to_tool_error)?;
        Ok(Json(ToolInspectionResponse { tool }))
    }

    #[tool(
        name = "call-tool",
        description = "Call a tool on an upstream endpoint using endpoint_id + tool_name or qualified_name."
    )]
    async fn call_tool(
        &self,
        Parameters(request): Parameters<CallToolRequest>,
    ) -> Result<Json<ToolCallOutput>, String> {
        let arguments = as_json_object(request.arguments).map_err(to_tool_error)?;
        let target = resolve_request_target(
            request.endpoint_id.as_deref(),
            request.tool_name.as_deref(),
            request.qualified_name.as_deref(),
        )
        .map_err(to_tool_error)?;
        let result = self
            .runtime
            .call_tool(&target.endpoint_id, &target.tool_name, arguments)
            .await
            .map_err(to_tool_error)?;
        Ok(Json(result))
    }

    #[tool(
        name = "register-http-endpoint",
        description = "Register or update an HTTP MCP endpoint."
    )]
    async fn register_http_endpoint(
        &self,
        Parameters(request): Parameters<RegisterHttpEndpointRequest>,
    ) -> Result<Json<EndpointSummary>, String> {
        let mut service = HubService::load().map_err(to_tool_error)?;
        let headers = request.headers.into_iter().collect::<Vec<_>>();
        let name = request.name.unwrap_or_default();
        service
            .register_http_endpoint(&request.endpoint_id, &request.url, headers, &name)
            .map_err(to_tool_error)?;

        let summary = service
            .list_endpoints()
            .iter()
            .find(|endpoint| endpoint.id == request.endpoint_id)
            .map(|endpoint| endpoint.summary())
            .ok_or_else(|| "registered endpoint could not be reloaded".to_string())?;
        Ok(Json(summary))
    }

    #[tool(
        name = "register-stdio-endpoint",
        description = "Register or update a stdio MCP endpoint."
    )]
    async fn register_stdio_endpoint(
        &self,
        Parameters(request): Parameters<RegisterStdioEndpointRequest>,
    ) -> Result<Json<EndpointSummary>, String> {
        let mut service = HubService::load().map_err(to_tool_error)?;
        let env = request.env.into_iter().collect::<Vec<_>>();
        let name = request.name.unwrap_or_default();
        service
            .register_stdio_endpoint(
                &request.endpoint_id,
                &request.command,
                request.args,
                env,
                request.cwd,
                &name,
            )
            .map_err(to_tool_error)?;

        let summary = service
            .list_endpoints()
            .iter()
            .find(|endpoint| endpoint.id == request.endpoint_id)
            .map(|endpoint| endpoint.summary())
            .ok_or_else(|| "registered endpoint could not be reloaded".to_string())?;
        Ok(Json(summary))
    }

    #[tool(
        name = "remove-endpoint",
        description = "Remove an endpoint and its cached tools."
    )]
    async fn remove_endpoint(
        &self,
        Parameters(request): Parameters<RemoveEndpointRequest>,
    ) -> Result<Json<RemoveEndpointResponse>, String> {
        let mut service = HubService::load().map_err(to_tool_error)?;
        let removed = service
            .remove_endpoint(&request.endpoint_id)
            .map_err(to_tool_error)?;
        Ok(Json(RemoveEndpointResponse {
            removed,
            endpoint_id: request.endpoint_id,
        }))
    }
}

pub async fn serve_stdio() -> Result<()> {
    let server = HubFacade::new();
    let transport = rmcp::transport::stdio();
    let running = server
        .serve(transport)
        .await
        .map_err(|error| anyhow!("failed to start MCPHub facade: {error}"))?;
    running.waiting().await?;
    Ok(())
}

fn as_json_object(value: Value) -> Result<Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        other => bail!(
            "call-tool.arguments must be a JSON object, got {}",
            describe_value_kind(&other)
        ),
    }
}

fn empty_json_object() -> Value {
    Value::Object(Map::new())
}

fn describe_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn to_tool_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn resolve_request_target(
    endpoint_id: Option<&str>,
    tool_name: Option<&str>,
    qualified_name: Option<&str>,
) -> Result<crate::models::ResolvedToolTarget> {
    let service = HubService::load()?;
    match (endpoint_id, tool_name, qualified_name) {
        (Some(endpoint_id), Some(tool_name), None) => {
            service.resolve_tool_target(endpoint_id, Some(tool_name))
        }
        (None, None, Some(qualified_name)) => service.resolve_tool_target(qualified_name, None),
        _ => {
            bail!("provide either endpoint_id + tool_name or qualified_name when selecting a tool")
        }
    }
}
