mod cli;
mod config;
mod daemon;
mod facade;
mod mcp_client;
mod models;
mod registry;
mod runtime;
mod schema_utils;
mod service;

pub mod api;

pub use api::SyncHttpEndpointResult;
pub use config::{DAEMON_HOST, DAEMON_PORT};
pub use facade::{HubFacade, serve_stdio};
pub use models::{
    EndpointConfig, EndpointHealth, EndpointSummary, EndpointTransport, RegistryState,
    ResolvedToolTarget, ToolCallOutput, ToolCatalogEntry, ToolInspection,
};
pub use runtime::HubRuntime;
pub use service::HubService;

pub async fn run_cli() -> anyhow::Result<()> {
    cli::run().await
}
