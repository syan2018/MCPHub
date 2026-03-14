use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EndpointTransport {
    Http,
    Stdio,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EndpointConfig {
    pub id: String,
    pub name: String,
    pub transport: EndpointTransport,
    pub url: Option<String>,
    pub headers: Vec<(String, String)>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
}

impl EndpointConfig {
    pub fn summary(&self) -> EndpointSummary {
        let (target, header_names) = match self.transport {
            EndpointTransport::Http => (
                self.url.clone().unwrap_or_default(),
                self.headers
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>(),
            ),
            EndpointTransport::Stdio => {
                let mut parts = Vec::new();
                if let Some(command) = &self.command {
                    parts.push(command.clone());
                }
                parts.extend(self.args.clone());
                (parts.join(" "), Vec::new())
            }
        };

        EndpointSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            transport: self.transport.clone(),
            target,
            header_count: self.headers.len(),
            header_names,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EndpointSummary {
    pub id: String,
    pub name: String,
    pub transport: EndpointTransport,
    pub target: String,
    pub header_count: usize,
    pub header_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCatalogEntry {
    pub endpoint_id: String,
    pub name: String,
    pub qualified_name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedToolTarget {
    pub endpoint_id: String,
    pub tool_name: String,
    pub qualified_name: String,
    pub cached_tool: Option<ToolCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolInspection {
    pub endpoint_id: String,
    pub tool_name: String,
    pub qualified_name: String,
    pub description: String,
    pub input_schema: Value,
    pub input_template: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct RegistryState {
    pub endpoints: Vec<EndpointConfig>,
    pub tools: Vec<ToolCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallOutput {
    pub endpoint_id: String,
    pub tool_name: String,
    pub success: bool,
    pub content: Vec<Value>,
    pub structured_content: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EndpointHealth {
    pub endpoint_id: String,
    pub healthy: bool,
    pub transport: EndpointTransport,
    pub target: String,
    pub reused_connection: bool,
    pub tool_count: Option<usize>,
    pub latency_ms: u128,
    pub error: Option<String>,
}
