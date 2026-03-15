use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::models::{
    EndpointHealth, EndpointSummary, ToolCallOutput, ToolCatalogEntry, ToolInspection,
};
use crate::runtime::HubRuntime;
use crate::service::HubService;

#[derive(Debug, Clone)]
pub struct SyncHttpEndpointResult {
    pub endpoint: EndpointSummary,
    pub tools: Vec<ToolCatalogEntry>,
    pub state_path: PathBuf,
}

pub fn state_path() -> PathBuf {
    crate::config::state_path()
}

pub fn register_http_endpoint(
    endpoint_id: &str,
    url: &str,
    headers: Vec<(String, String)>,
    name: &str,
) -> Result<EndpointSummary> {
    let mut service = HubService::load()?;
    service.register_http_endpoint(endpoint_id, url, headers, name)?;
    let endpoint = service
        .get_endpoint(endpoint_id)
        .map(|endpoint| endpoint.summary())
        .ok_or_else(|| {
            anyhow!(
                "registered endpoint '{}' could not be reloaded",
                endpoint_id
            )
        })?;
    Ok(endpoint)
}

pub fn register_stdio_endpoint(
    endpoint_id: &str,
    command: &str,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    name: &str,
) -> Result<EndpointSummary> {
    let mut service = HubService::load()?;
    service.register_stdio_endpoint(endpoint_id, command, args, env, cwd, name)?;
    let endpoint = service
        .get_endpoint(endpoint_id)
        .map(|endpoint| endpoint.summary())
        .ok_or_else(|| {
            anyhow!(
                "registered endpoint '{}' could not be reloaded",
                endpoint_id
            )
        })?;
    Ok(endpoint)
}

pub fn remove_endpoint(endpoint_id: &str) -> Result<bool> {
    let mut service = HubService::load()?;
    service.remove_endpoint(endpoint_id)
}

pub fn list_endpoints() -> Result<Vec<EndpointSummary>> {
    let service = HubService::load()?;
    Ok(service
        .list_endpoints()
        .iter()
        .map(|endpoint| endpoint.summary())
        .collect())
}

pub fn list_cached_tools(endpoint_id: Option<&str>) -> Result<Vec<ToolCatalogEntry>> {
    let service = HubService::load()?;
    Ok(service
        .list_tools(endpoint_id)
        .into_iter()
        .cloned()
        .collect())
}

pub fn inspect_tool(endpoint_id: &str, tool_name: &str) -> Result<ToolInspection> {
    let service = HubService::load()?;
    service.inspect_tool(endpoint_id, tool_name)
}

pub fn inspect_tools(endpoint_id: &str) -> Result<Vec<ToolInspection>> {
    let service = HubService::load()?;
    service.inspect_tools(endpoint_id)
}

pub async fn discover_tools(endpoint_id: &str) -> Result<Vec<ToolCatalogEntry>> {
    HubRuntime::new().discover_tools(endpoint_id).await
}

pub async fn call_tool(
    endpoint_id: &str,
    tool_name: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<ToolCallOutput> {
    HubRuntime::new()
        .call_tool(endpoint_id, tool_name, arguments)
        .await
}

pub async fn health_check(endpoint_id: &str) -> Result<EndpointHealth> {
    HubRuntime::new().health_check(endpoint_id).await
}

pub async fn sync_http_endpoint(
    endpoint_id: &str,
    url: &str,
    headers: Vec<(String, String)>,
    name: &str,
) -> Result<SyncHttpEndpointResult> {
    let endpoint = register_http_endpoint(endpoint_id, url, headers, name)?;
    let tools = discover_tools(endpoint_id).await?;
    Ok(SyncHttpEndpointResult {
        endpoint,
        tools,
        state_path: state_path(),
    })
}
