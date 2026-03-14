use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::Mutex;

use crate::mcp_client::{ClientSession, McpClient};
use crate::models::{EndpointConfig, EndpointHealth, ToolCallOutput, ToolCatalogEntry};
use crate::service::HubService;

#[derive(Debug)]
struct EndpointSession {
    signature: String,
    client: Mutex<ClientSession>,
}

#[derive(Debug, Default)]
struct SessionPool {
    sessions: Mutex<HashMap<String, Arc<EndpointSession>>>,
}

#[derive(Debug, Clone, Default)]
pub struct HubRuntime {
    pool: Arc<SessionPool>,
}

impl HubRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn discover_tools(&self, endpoint_id: &str) -> Result<Vec<ToolCatalogEntry>> {
        let mut service = HubService::load()?;
        let endpoint = service
            .get_endpoint(endpoint_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown endpoint '{}'", endpoint_id))?;

        let tools = self.pool.discover_tools(&endpoint).await?;
        service.store_discovered_tools(endpoint_id, tools.clone())?;
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        endpoint_id: &str,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ToolCallOutput> {
        let service = HubService::load()?;
        let endpoint = service
            .get_endpoint(endpoint_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown endpoint '{}'", endpoint_id))?;
        self.pool.call_tool(&endpoint, tool_name, arguments).await
    }

    pub async fn health_check(&self, endpoint_id: &str) -> Result<EndpointHealth> {
        let service = HubService::load()?;
        let endpoint = service
            .get_endpoint(endpoint_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown endpoint '{}'", endpoint_id))?;
        self.pool.health_check(&endpoint).await
    }
}

impl SessionPool {
    async fn discover_tools(&self, endpoint: &EndpointConfig) -> Result<Vec<ToolCatalogEntry>> {
        let (session, reused_connection) = self.get_or_connect(endpoint).await?;
        let result = {
            let client = session.client.lock().await;
            McpClient::discover_tools_on_client(&client, endpoint).await
        };

        match result {
            Ok(tools) => Ok(tools),
            Err(error) if reused_connection => {
                self.invalidate(&endpoint.id).await;
                let (session, _) = self.get_or_connect(endpoint).await?;
                let client = session.client.lock().await;
                McpClient::discover_tools_on_client(&client, endpoint)
                    .await
                    .map_err(|retry_error| retry_error.context(error.to_string()))
            }
            Err(error) => Err(error),
        }
    }

    async fn call_tool(
        &self,
        endpoint: &EndpointConfig,
        tool_name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ToolCallOutput> {
        let (session, reused_connection) = self.get_or_connect(endpoint).await?;
        let result = {
            let client = session.client.lock().await;
            McpClient::call_tool_on_client(&client, endpoint, tool_name, arguments.clone()).await
        };

        match result {
            Ok(output) => Ok(output),
            Err(error) if reused_connection => {
                self.invalidate(&endpoint.id).await;
                let (session, _) = self.get_or_connect(endpoint).await?;
                let client = session.client.lock().await;
                McpClient::call_tool_on_client(&client, endpoint, tool_name, arguments)
                    .await
                    .map_err(|retry_error| retry_error.context(error.to_string()))
            }
            Err(error) => Err(error),
        }
    }

    async fn health_check(&self, endpoint: &EndpointConfig) -> Result<EndpointHealth> {
        let (session, reused_connection) = self.get_or_connect(endpoint).await?;
        let result = {
            let client = session.client.lock().await;
            McpClient::health_check_on_client(&client, endpoint, reused_connection).await
        };

        match result {
            Ok(status) => Ok(status),
            Err(error) if reused_connection => {
                self.invalidate(&endpoint.id).await;
                let fallback = McpClient::health_check(endpoint).await;
                match fallback {
                    Ok(mut status) => {
                        status.reused_connection = false;
                        Ok(status)
                    }
                    Err(retry_error) => Ok(EndpointHealth {
                        endpoint_id: endpoint.id.clone(),
                        healthy: false,
                        transport: endpoint.transport.clone(),
                        target: endpoint.summary().target,
                        reused_connection: reused_connection,
                        tool_count: None,
                        latency_ms: 0,
                        error: Some(format!("{error}; retry failed: {retry_error}")),
                    }),
                }
            }
            Err(error) => Ok(EndpointHealth {
                endpoint_id: endpoint.id.clone(),
                healthy: false,
                transport: endpoint.transport.clone(),
                target: endpoint.summary().target,
                reused_connection,
                tool_count: None,
                latency_ms: 0,
                error: Some(error.to_string()),
            }),
        }
    }

    async fn get_or_connect(
        &self,
        endpoint: &EndpointConfig,
    ) -> Result<(Arc<EndpointSession>, bool)> {
        let signature = endpoint_signature(endpoint)?;

        if let Some(existing) = self.sessions.lock().await.get(&endpoint.id).cloned() {
            if existing.signature == signature {
                let is_closed = existing.client.lock().await.is_closed();
                if !is_closed {
                    return Ok((existing, true));
                }
            }
            self.invalidate(&endpoint.id).await;
        }

        let client = McpClient::connect(endpoint).await?;
        let session = Arc::new(EndpointSession {
            signature,
            client: Mutex::new(client),
        });
        self.sessions
            .lock()
            .await
            .insert(endpoint.id.clone(), session.clone());
        Ok((session, false))
    }

    async fn invalidate(&self, endpoint_id: &str) {
        let removed = self.sessions.lock().await.remove(endpoint_id);
        if let Some(session) = removed {
            let mut client = session.client.lock().await;
            let _ = client.close().await;
        }
    }
}

fn endpoint_signature(endpoint: &EndpointConfig) -> Result<String> {
    Ok(serde_json::to_string(endpoint)?)
}
