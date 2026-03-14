use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::config::{DAEMON_HOST, DAEMON_PORT};
use crate::models::{EndpointHealth, ToolCallOutput, ToolCatalogEntry};
use crate::runtime::HubRuntime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    Ping,
    Shutdown,
    Discover {
        endpoint_id: String,
    },
    Call {
        endpoint_id: String,
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    },
    Health {
        endpoint_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    Pong { version: String },
    DiscoverResult { tools: Vec<ToolCatalogEntry> },
    CallResult { output: ToolCallOutput },
    HealthResult { status: EndpointHealth },
    Ack,
    Error { message: String },
}

pub async fn start() -> Result<()> {
    if ping().await.is_ok() {
        println!("daemon already running on {}", daemon_addr());
        return Ok(());
    }

    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("daemon")
        .arg("run")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .context("failed to spawn background daemon process")?;

    let started = tokio::time::Instant::now();
    let timeout = Duration::from_secs(5);
    while started.elapsed() < timeout {
        if ping().await.is_ok() {
            println!("daemon started on {}", daemon_addr());
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    bail!("daemon did not become ready within {:?}", timeout)
}

pub async fn run() -> Result<()> {
    let addr = daemon_socket_addr()?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind daemon on {}", addr))?;
    let runtime = HubRuntime::new();
    loop {
        let (mut stream, _) = listener.accept().await?;
        let request = read_request(&mut stream).await?;
        let response = handle_request(&runtime, request).await?;
        write_response(&mut stream, &response).await?;
        if matches!(response, DaemonResponse::Ack) {
            break;
        }
    }
    Ok(())
}

pub async fn status() -> Result<()> {
    let response = ping().await?;
    match response {
        DaemonResponse::Pong { version } => {
            println!("daemon running on {} version={}", daemon_addr(), version);
            Ok(())
        }
        other => Err(anyhow!("unexpected daemon response: {:?}", other)),
    }
}

pub async fn stop() -> Result<()> {
    let response = request(DaemonRequest::Shutdown).await?;
    match response {
        DaemonResponse::Ack => {
            println!("daemon stopped");
            Ok(())
        }
        DaemonResponse::Error { message } => Err(anyhow!(message)),
        other => Err(anyhow!("unexpected daemon response: {:?}", other)),
    }
}

pub async fn request(request: DaemonRequest) -> Result<DaemonResponse> {
    let addr = daemon_socket_addr()?;
    let mut stream = TcpStream::connect(addr)
        .await
        .with_context(|| format!("failed to connect to daemon at {}", addr))?;
    write_request(&mut stream, &request).await?;
    read_response(&mut stream).await
}

pub async fn ping() -> Result<DaemonResponse> {
    request(DaemonRequest::Ping).await
}

pub fn daemon_addr() -> String {
    format!("{}:{}", DAEMON_HOST, DAEMON_PORT)
}

fn daemon_socket_addr() -> Result<SocketAddr> {
    daemon_addr()
        .parse()
        .with_context(|| format!("invalid daemon address {}", daemon_addr()))
}

async fn handle_request(runtime: &HubRuntime, request: DaemonRequest) -> Result<DaemonResponse> {
    let response = match request {
        DaemonRequest::Ping => DaemonResponse::Pong {
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        DaemonRequest::Shutdown => DaemonResponse::Ack,
        DaemonRequest::Discover { endpoint_id } => DaemonResponse::DiscoverResult {
            tools: runtime.discover_tools(&endpoint_id).await?,
        },
        DaemonRequest::Call {
            endpoint_id,
            tool_name,
            arguments,
        } => DaemonResponse::CallResult {
            output: runtime
                .call_tool(&endpoint_id, &tool_name, arguments)
                .await?,
        },
        DaemonRequest::Health { endpoint_id } => DaemonResponse::HealthResult {
            status: runtime.health_check(&endpoint_id).await?,
        },
    };
    Ok(response)
}

async fn read_request(stream: &mut TcpStream) -> Result<DaemonRequest> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;
    serde_json::from_slice(&bytes).context("failed to decode daemon request")
}

async fn write_request(stream: &mut TcpStream, request: &DaemonRequest) -> Result<()> {
    let bytes = serde_json::to_vec(request)?;
    stream.write_all(&bytes).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn read_response(stream: &mut TcpStream) -> Result<DaemonResponse> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;
    serde_json::from_slice(&bytes).context("failed to decode daemon response")
}

async fn write_response(stream: &mut TcpStream, response: &DaemonResponse) -> Result<()> {
    let bytes = serde_json::to_vec(response)?;
    stream.write_all(&bytes).await?;
    stream.shutdown().await?;
    Ok(())
}
