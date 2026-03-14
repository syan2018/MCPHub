use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::models::{EndpointConfig, RegistryState, ToolCatalogEntry};

pub struct Registry {
    state_path: PathBuf,
    state: RegistryState,
}

impl Registry {
    pub fn new(state_path: PathBuf) -> Result<Self> {
        let state = if state_path.exists() {
            let raw = fs::read_to_string(&state_path)
                .with_context(|| format!("failed to read state file {}", state_path.display()))?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            RegistryState::default()
        };

        Ok(Self { state_path, state })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_path, raw)
            .with_context(|| format!("failed to write state file {}", self.state_path.display()))
    }

    pub fn upsert_endpoint(&mut self, endpoint: EndpointConfig) -> Result<()> {
        if let Some(existing) = self
            .state
            .endpoints
            .iter_mut()
            .find(|item| item.id == endpoint.id)
        {
            *existing = endpoint;
        } else {
            self.state.endpoints.push(endpoint);
            self.state.endpoints.sort_by(|a, b| a.id.cmp(&b.id));
        }
        self.save()
    }

    pub fn remove_endpoint(&mut self, endpoint_id: &str) -> Result<bool> {
        let before = self.state.endpoints.len();
        self.state.endpoints.retain(|item| item.id != endpoint_id);
        self.state
            .tools
            .retain(|item| item.endpoint_id != endpoint_id);
        let changed = self.state.endpoints.len() != before;
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    pub fn get_endpoint(&self, endpoint_id: &str) -> Option<&EndpointConfig> {
        self.state
            .endpoints
            .iter()
            .find(|item| item.id == endpoint_id)
    }

    pub fn list_endpoints(&self) -> &[EndpointConfig] {
        &self.state.endpoints
    }

    pub fn replace_tools(&mut self, endpoint_id: &str, tools: Vec<ToolCatalogEntry>) -> Result<()> {
        self.state
            .tools
            .retain(|item| item.endpoint_id != endpoint_id);
        self.state.tools.extend(tools);
        self.state
            .tools
            .sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
        self.save()
    }

    pub fn list_tools(&self, endpoint_id: Option<&str>) -> Vec<&ToolCatalogEntry> {
        self.state
            .tools
            .iter()
            .filter(|item| endpoint_id.is_none_or(|id| item.endpoint_id == id))
            .collect()
    }

    pub fn get_tool(&self, endpoint_id: &str, tool_name: &str) -> Option<&ToolCatalogEntry> {
        self.state
            .tools
            .iter()
            .find(|item| item.endpoint_id == endpoint_id && item.name == tool_name)
    }

    pub fn get_tool_by_qualified_name(&self, qualified_name: &str) -> Option<&ToolCatalogEntry> {
        self.state
            .tools
            .iter()
            .find(|item| item.qualified_name == qualified_name)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::models::{EndpointConfig, EndpointTransport, ToolCatalogEntry};

    use super::Registry;

    #[test]
    fn upsert_and_get_endpoint() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut registry = Registry::new(path).unwrap();

        registry
            .upsert_endpoint(EndpointConfig {
                id: "ue-main".into(),
                name: "UE Main".into(),
                transport: EndpointTransport::Http,
                url: Some("http://127.0.0.1:19840/mcp".into()),
                headers: vec![],
                command: None,
                args: vec![],
                env: vec![],
                cwd: None,
            })
            .unwrap();

        let endpoint = registry.get_endpoint("ue-main").unwrap();
        assert_eq!(endpoint.name, "UE Main");
    }

    #[test]
    fn replace_tools_for_endpoint() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut registry = Registry::new(path).unwrap();

        registry
            .replace_tools(
                "ue-main",
                vec![ToolCatalogEntry {
                    endpoint_id: "ue-main".into(),
                    name: "search".into(),
                    qualified_name: "ue-main/search".into(),
                    description: "Search".into(),
                    input_schema: serde_json::json!({}),
                }],
            )
            .unwrap();

        let tools = registry.list_tools(Some("ue-main"));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].qualified_name, "ue-main/search");
    }
}
