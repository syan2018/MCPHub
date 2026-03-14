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

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
