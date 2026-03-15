use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    mcphub::run_cli().await
}
