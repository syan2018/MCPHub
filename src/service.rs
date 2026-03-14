use anyhow::{Context, Result, anyhow};

use crate::config::state_path;
use crate::mcp_client::McpClient;
use crate::models::{
    EndpointConfig, EndpointTransport, ResolvedToolTarget, ToolCallOutput, ToolCatalogEntry,
    ToolInspection,
};
use crate::registry::Registry;
use crate::schema_utils::build_input_template;

pub struct HubService {
    registry: Registry,
}

impl HubService {
    pub fn load() -> Result<Self> {
        Ok(Self {
            registry: Registry::new(state_path())?,
        })
    }

    pub fn register_http_endpoint(
        &mut self,
        endpoint_id: &str,
        url: &str,
        headers: Vec<(String, String)>,
        name: &str,
    ) -> Result<()> {
        self.registry.upsert_endpoint(EndpointConfig {
            id: endpoint_id.to_string(),
            name: if name.is_empty() {
                endpoint_id.to_string()
            } else {
                name.to_string()
            },
            transport: EndpointTransport::Http,
            url: Some(url.to_string()),
            headers,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        })
    }

    pub fn register_stdio_endpoint(
        &mut self,
        endpoint_id: &str,
        command: &str,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        name: &str,
    ) -> Result<()> {
        self.registry.upsert_endpoint(EndpointConfig {
            id: endpoint_id.to_string(),
            name: if name.is_empty() {
                endpoint_id.to_string()
            } else {
                name.to_string()
            },
            transport: EndpointTransport::Stdio,
            url: None,
            headers: Vec::new(),
            command: Some(command.to_string()),
            args,
            env,
            cwd,
        })
    }

    pub fn remove_endpoint(&mut self, endpoint_id: &str) -> Result<bool> {
        self.registry.remove_endpoint(endpoint_id)
    }

    pub fn list_endpoints(&self) -> &[EndpointConfig] {
        self.registry.list_endpoints()
    }

    pub fn get_endpoint(&self, endpoint_id: &str) -> Option<&EndpointConfig> {
        self.registry.get_endpoint(endpoint_id)
    }

    pub fn list_tools(&self, endpoint_id: Option<&str>) -> Vec<&ToolCatalogEntry> {
        self.registry.list_tools(endpoint_id)
    }

    pub fn store_discovered_tools(
        &mut self,
        endpoint_id: &str,
        tools: Vec<ToolCatalogEntry>,
    ) -> Result<()> {
        self.registry.replace_tools(endpoint_id, tools)
    }

    pub fn inspect_tool(&self, endpoint_id: &str, tool_name: &str) -> Result<ToolInspection> {
        let tool = self
            .registry
            .get_tool(endpoint_id, tool_name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown cached tool '{}/{}'", endpoint_id, tool_name))?;

        Ok(ToolInspection {
            endpoint_id: tool.endpoint_id.clone(),
            tool_name: tool.name.clone(),
            qualified_name: tool.qualified_name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            input_template: build_input_template(&tool.input_schema),
        })
    }

    pub fn resolve_tool_target(
        &self,
        endpoint_or_qualified: &str,
        explicit_tool_name: Option<&str>,
    ) -> Result<ResolvedToolTarget> {
        let (endpoint_id, tool_name) = match explicit_tool_name {
            Some(tool_name) => (endpoint_or_qualified.to_string(), tool_name.to_string()),
            None => endpoint_or_qualified
                .split_once('/')
                .map(|(endpoint_id, tool_name)| (endpoint_id.to_string(), tool_name.to_string()))
                .ok_or_else(|| {
                    anyhow!(
                        "expected either <endpoint_id> <tool_name> or a qualified name like <endpoint_id>/<tool_name>"
                    )
                })?,
        };

        let qualified_name = format!("{}/{}", endpoint_id, tool_name);
        let cached_tool = self
            .registry
            .get_tool(&endpoint_id, &tool_name)
            .cloned()
            .or_else(|| {
                self.registry
                    .get_tool_by_qualified_name(&qualified_name)
                    .cloned()
            });

        Ok(ResolvedToolTarget {
            endpoint_id,
            tool_name,
            qualified_name,
            cached_tool,
        })
    }

    pub async fn discover_tools(&mut self, endpoint_id: &str) -> Result<Vec<ToolCatalogEntry>> {
        let endpoint = self
            .registry
            .get_endpoint(endpoint_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown endpoint '{}'", endpoint_id))?;
        let tools = McpClient::discover_tools(&endpoint).await?;
        self.store_discovered_tools(endpoint_id, tools.clone())?;
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        endpoint_id: &str,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ToolCallOutput> {
        let endpoint = self
            .registry
            .get_endpoint(endpoint_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown endpoint '{}'", endpoint_id))?;
        McpClient::call_tool(&endpoint, tool_name, arguments)
            .await
            .with_context(|| format!("tool call failed for {}/{}", endpoint_id, tool_name))
    }
}
