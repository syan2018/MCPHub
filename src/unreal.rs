use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::sleep;

use crate::mcp_client::McpClient;
use crate::models::{EndpointConfig, EndpointHealth, EndpointTransport};
use crate::service::HubService;

const DEFAULT_TRANSPORT: &str = "http";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 19840;
const DEFAULT_PATH: &str = "/mcp";
const SETTINGS_SECTION: &str = "[/Script/UnrealCopilot.UnrealCopilotSettings]";

#[derive(Debug, Clone)]
pub struct UnrealStatusOptions {
    pub project: Option<PathBuf>,
    pub endpoint_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnrealLaunchOptions {
    pub project: Option<PathBuf>,
    pub endpoint_id: Option<String>,
    pub engine_dir: Option<PathBuf>,
    pub wait_seconds: u64,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct UnrealConnectOptions {
    pub project: Option<PathBuf>,
    pub endpoint_id: Option<String>,
    pub engine_dir: Option<PathBuf>,
    pub launch: bool,
    pub wait_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnrealCopilotSettings {
    pub transport: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub auto_start_mcp_server: bool,
    pub config_sources: Vec<PathBuf>,
}

impl Default for UnrealCopilotSettings {
    fn default() -> Self {
        Self {
            transport: DEFAULT_TRANSPORT.to_string(),
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            path: DEFAULT_PATH.to_string(),
            auto_start_mcp_server: false,
            config_sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UnrealProjectInfo {
    pub project_name: String,
    pub project_path: PathBuf,
    pub project_dir: PathBuf,
    pub engine_association: Option<String>,
    pub engine_dir: Option<PathBuf>,
    pub editor_exe: Option<PathBuf>,
    pub endpoint_id: String,
    pub endpoint_url: String,
    pub copilot_settings: UnrealCopilotSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnrealStatusReport {
    pub project: UnrealProjectInfo,
    pub endpoint_registered: bool,
    pub health: Option<EndpointHealth>,
    pub health_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnrealLaunchReport {
    pub project: UnrealProjectInfo,
    pub pid: u32,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
    pub health: Option<EndpointHealth>,
    pub health_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnrealConnectReport {
    pub project: UnrealProjectInfo,
    pub launched: bool,
    pub launch_pid: Option<u32>,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
    pub registered: bool,
    pub health: EndpointHealth,
    pub discovered_tool_count: usize,
    pub tools: Vec<String>,
}

#[derive(Debug)]
struct LaunchArtifacts {
    pid: u32,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

pub async fn status(options: UnrealStatusOptions) -> Result<UnrealStatusReport> {
    let project = resolve_project_info(options.project, options.endpoint_id, None)?;
    let endpoint = build_http_endpoint(&project)?;
    let service = HubService::load()?;
    let endpoint_registered = service.get_endpoint(&project.endpoint_id).is_some();
    let (health, health_error) = match McpClient::health_check(&endpoint).await {
        Ok(status) => (Some(status), None),
        Err(error) => (None, Some(error.to_string())),
    };

    Ok(UnrealStatusReport {
        project,
        endpoint_registered,
        health,
        health_error,
    })
}

pub async fn launch(options: UnrealLaunchOptions) -> Result<UnrealLaunchReport> {
    let project = resolve_project_info(options.project, options.endpoint_id, options.engine_dir)?;
    let endpoint = build_http_endpoint(&project)?;
    let launch = launch_editor(
        &project,
        options.stdout_log.as_deref(),
        options.stderr_log.as_deref(),
    )
    .await?;

    let (health, health_error) = if options.wait_seconds == 0 {
        (None, None)
    } else {
        match wait_for_endpoint(&endpoint, Duration::from_secs(options.wait_seconds)).await {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error.to_string())),
        }
    };

    Ok(UnrealLaunchReport {
        project,
        pid: launch.pid,
        stdout_log: launch.stdout_log,
        stderr_log: launch.stderr_log,
        health,
        health_error,
    })
}

pub async fn connect(options: UnrealConnectOptions) -> Result<UnrealConnectReport> {
    let project = resolve_project_info(options.project, options.endpoint_id, options.engine_dir)?;
    let endpoint = build_http_endpoint(&project)?;

    let mut launched = false;
    let mut launch_pid = None;
    let mut stdout_log = None;
    let mut stderr_log = None;

    let health = match McpClient::health_check(&endpoint).await {
        Ok(status) => status,
        Err(initial_error) => {
            if !options.launch {
                bail!(
                    "failed to connect to {}: {}. Re-run with --launch to start UnrealEditor first.",
                    project.endpoint_url,
                    initial_error
                );
            }

            let launch = launch_editor(&project, None, None).await?;
            launched = true;
            launch_pid = Some(launch.pid);
            stdout_log = Some(launch.stdout_log.clone());
            stderr_log = Some(launch.stderr_log.clone());

            wait_for_endpoint(&endpoint, Duration::from_secs(options.wait_seconds))
                .await
                .with_context(|| {
                    format!(
                        "UnrealEditor launched but MCP endpoint {} never became healthy",
                        project.endpoint_url
                    )
                })?
        }
    };

    let mut service = HubService::load()?;
    service.register_http_endpoint(
        &project.endpoint_id,
        &project.endpoint_url,
        Vec::new(),
        &format!("{} UnrealCopilot", project.project_name),
    )?;
    let tools = service.discover_tools(&project.endpoint_id).await?;

    Ok(UnrealConnectReport {
        project,
        launched,
        launch_pid,
        stdout_log,
        stderr_log,
        registered: true,
        health,
        discovered_tool_count: tools.len(),
        tools: tools.into_iter().map(|tool| tool.qualified_name).collect(),
    })
}

fn resolve_project_info(
    requested_project: Option<PathBuf>,
    endpoint_id: Option<String>,
    requested_engine_dir: Option<PathBuf>,
) -> Result<UnrealProjectInfo> {
    let project_path = resolve_project_path(requested_project)?;
    let project_dir = project_path
        .parent()
        .context("uproject is missing a parent directory")?
        .to_path_buf();
    let project_name = project_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("uproject filename is invalid UTF-8")?
        .to_string();
    let engine_association = read_engine_association(&project_path)?;
    let copilot_settings = load_copilot_settings(&project_dir)?;
    let engine_dir =
        requested_engine_dir.or_else(|| find_engine_dir(engine_association.as_deref()));
    let editor_exe = engine_dir
        .as_ref()
        .map(|engine_dir| {
            engine_dir
                .join("Engine")
                .join("Binaries")
                .join("Win64")
                .join("UnrealEditor.exe")
        })
        .filter(|path| path.is_file());
    let endpoint_id = endpoint_id.unwrap_or_else(|| default_endpoint_id(&project_name));
    let endpoint_url = format!(
        "http://{}:{}{}",
        copilot_settings.host,
        copilot_settings.port,
        normalize_mcp_path(&copilot_settings.path)
    );

    Ok(UnrealProjectInfo {
        project_name,
        project_path,
        project_dir,
        engine_association,
        engine_dir,
        editor_exe,
        endpoint_id,
        endpoint_url,
        copilot_settings,
    })
}

fn resolve_project_path(requested_project: Option<PathBuf>) -> Result<PathBuf> {
    match requested_project {
        Some(path) if path.is_file() => {
            if path.extension().and_then(|ext| ext.to_str()) == Some("uproject") {
                Ok(path)
            } else {
                bail!("project path must point to a .uproject file");
            }
        }
        Some(path) if path.is_dir() => find_uproject_in_dir_or_ancestors(&path),
        Some(path) => bail!("project path '{}' does not exist", path.display()),
        None => {
            let cwd = std::env::current_dir().context("failed to resolve current directory")?;
            find_uproject_in_dir_or_ancestors(&cwd)
        }
    }
}

fn find_uproject_in_dir_or_ancestors(start: &Path) -> Result<PathBuf> {
    for directory in start.ancestors() {
        let mut matches = fs::read_dir(directory)
            .with_context(|| format!("failed to read {}", directory.display()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("uproject"))
            .collect::<Vec<_>>();
        matches.sort();
        if let Some(path) = matches.into_iter().next() {
            return Ok(path);
        }
    }

    bail!(
        "could not find a .uproject by walking upward from {}",
        start.display()
    )
}

fn read_engine_association(project_path: &Path) -> Result<Option<String>> {
    let raw = fs::read_to_string(project_path)
        .with_context(|| format!("failed to read {}", project_path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", project_path.display()))?;
    Ok(value
        .get("EngineAssociation")
        .and_then(Value::as_str)
        .map(str::to_string))
}

fn load_copilot_settings(project_dir: &Path) -> Result<UnrealCopilotSettings> {
    let mut settings = UnrealCopilotSettings::default();
    let config_files = [
        project_dir
            .join("Config")
            .join("DefaultEditorPerProjectUserSettings.ini"),
        project_dir
            .join("Saved")
            .join("Config")
            .join("WindowsEditor")
            .join("EditorPerProjectUserSettings.ini"),
    ];

    for path in config_files {
        if path.is_file() && apply_copilot_settings_ini(&mut settings, &path)? {
            settings.config_sources.push(path);
        }
    }

    Ok(settings)
}

fn apply_copilot_settings_ini(settings: &mut UnrealCopilotSettings, path: &Path) -> Result<bool> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut in_section = false;
    let mut matched_section = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed.eq_ignore_ascii_case(SETTINGS_SECTION);
            matched_section |= in_section;
            continue;
        }

        if !in_section {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "Transport" => settings.transport = normalize_transport(value),
            "McpHost" => {
                if !value.is_empty() {
                    settings.host = value.to_string();
                }
            }
            "McpPort" => {
                if let Ok(port) = value.parse::<u16>() {
                    settings.port = port;
                }
            }
            "McpPath" => {
                if !value.is_empty() {
                    settings.path = value.to_string();
                }
            }
            "bAutoStartMcpServer" => settings.auto_start_mcp_server = parse_ini_bool(value),
            _ => {}
        }
    }

    Ok(matched_section)
}

fn normalize_transport(value: &str) -> String {
    let lowered = value.trim().to_ascii_lowercase();
    if lowered.contains("stdio") {
        "stdio".to_string()
    } else if lowered.contains("sse") {
        "sse".to_string()
    } else {
        "http".to_string()
    }
}

fn parse_ini_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn default_endpoint_id(project_name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in project_name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "unreal-local".to_string()
    } else {
        format!("{slug}-local")
    }
}

fn normalize_mcp_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        DEFAULT_PATH.to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn build_http_endpoint(project: &UnrealProjectInfo) -> Result<EndpointConfig> {
    if project.copilot_settings.transport != "http" {
        bail!(
            "Unreal helper currently supports only http transport, but project is configured for '{}'",
            project.copilot_settings.transport
        );
    }

    Ok(EndpointConfig {
        id: project.endpoint_id.clone(),
        name: format!("{} UnrealCopilot", project.project_name),
        transport: EndpointTransport::Http,
        url: Some(project.endpoint_url.clone()),
        headers: Vec::new(),
        command: None,
        args: Vec::new(),
        env: Vec::new(),
        cwd: None,
    })
}

async fn launch_editor(
    project: &UnrealProjectInfo,
    requested_stdout_log: Option<&Path>,
    requested_stderr_log: Option<&Path>,
) -> Result<LaunchArtifacts> {
    let editor_exe = project.editor_exe.as_ref().ok_or_else(|| {
        anyhow!(
            "could not locate UnrealEditor.exe for {}",
            project.project_name
        )
    })?;
    if !editor_exe.is_file() {
        bail!(
            "editor executable '{}' does not exist",
            editor_exe.display()
        );
    }

    let logs_dir = project.project_dir.join("Saved").join("Logs");
    fs::create_dir_all(&logs_dir)
        .with_context(|| format!("failed to create {}", logs_dir.display()))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let stdout_log = requested_stdout_log
        .map(PathBuf::from)
        .unwrap_or_else(|| logs_dir.join(format!("mcphub-unreal-stdout-{timestamp}.log")));
    let stderr_log = requested_stderr_log
        .map(PathBuf::from)
        .unwrap_or_else(|| logs_dir.join(format!("mcphub-unreal-stderr-{timestamp}.log")));

    let stdout = File::create(&stdout_log)
        .with_context(|| format!("failed to create {}", stdout_log.display()))?;
    let stderr = File::create(&stderr_log)
        .with_context(|| format!("failed to create {}", stderr_log.display()))?;

    let child = Command::new(editor_exe)
        .arg(&project.project_path)
        .arg("-stdout")
        .arg("-FullStdOutLogOutput")
        .arg("-NoSplash")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("failed to launch {}", editor_exe.display()))?;

    let pid = child
        .id()
        .ok_or_else(|| anyhow!("launched UnrealEditor but could not read its process id"))?;
    drop(child);

    Ok(LaunchArtifacts {
        pid,
        stdout_log,
        stderr_log,
    })
}

async fn wait_for_endpoint(endpoint: &EndpointConfig, timeout: Duration) -> Result<EndpointHealth> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_error = None;

    while tokio::time::Instant::now() < deadline {
        match McpClient::health_check(endpoint).await {
            Ok(status) => return Ok(status),
            Err(error) => {
                last_error = Some(error);
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    match last_error {
        Some(error) => Err(error).with_context(|| {
            format!(
                "timed out after {}s waiting for {}",
                timeout.as_secs(),
                endpoint.url.as_deref().unwrap_or("<unknown>")
            )
        }),
        None => bail!(
            "timed out after {}s waiting for {}",
            timeout.as_secs(),
            endpoint.url.as_deref().unwrap_or("<unknown>")
        ),
    }
}

fn find_engine_dir(engine_association: Option<&str>) -> Option<PathBuf> {
    let association = engine_association?;

    let registry_candidates = [
        (
            format!(r"HKLM\SOFTWARE\EpicGames\Unreal Engine\{association}"),
            "InstalledDirectory".to_string(),
        ),
        (
            r"HKCU\SOFTWARE\Epic Games\Unreal Engine\Builds".to_string(),
            association.to_string(),
        ),
    ];

    for (key, value_name) in registry_candidates {
        if let Some(path) = query_registry_value(&key, &value_name) {
            let path = PathBuf::from(path);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    let fallback_roots = [
        PathBuf::from(format!(r"D:\Epic Games\UE_{association}")),
        PathBuf::from(format!(r"C:\Program Files\Epic Games\UE_{association}")),
        PathBuf::from(format!(r"C:\Epic Games\UE_{association}")),
    ];
    fallback_roots.into_iter().find(|path| path.is_dir())
}

fn query_registry_value(key: &str, value_name: &str) -> Option<String> {
    let output = std::process::Command::new("reg")
        .args(["query", key, "/v", value_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with(value_name) {
            return None;
        }

        let mut parts = trimmed.split_whitespace();
        let name = parts.next()?;
        if name != value_name {
            return None;
        }
        let _kind = parts.next()?;
        let value = parts.collect::<Vec<_>>().join(" ");
        if value.is_empty() { None } else { Some(value) }
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        SETTINGS_SECTION, UnrealCopilotSettings, apply_copilot_settings_ini, default_endpoint_id,
        find_uproject_in_dir_or_ancestors, normalize_mcp_path,
    };

    #[test]
    fn finds_uproject_by_walking_upward() {
        let dir = tempdir().unwrap();
        let project_dir = dir.path().join("Project");
        let plugin_dir = project_dir.join("Plugins").join("UnrealCopilot");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(project_dir.join("Demo.uproject"), "{}").unwrap();

        let resolved = find_uproject_in_dir_or_ancestors(&plugin_dir).unwrap();
        assert_eq!(
            resolved.file_name().and_then(|name| name.to_str()),
            Some("Demo.uproject")
        );
    }

    #[test]
    fn parses_copilot_settings_from_ini_section() {
        let dir = tempdir().unwrap();
        let ini = dir.path().join("EditorPerProjectUserSettings.ini");
        std::fs::write(
            &ini,
            format!(
                "{section}\nTransport=Sse\nMcpHost=0.0.0.0\nMcpPort=21234\nMcpPath=custom\nbAutoStartMcpServer=True\n",
                section = SETTINGS_SECTION
            ),
        )
        .unwrap();

        let mut settings = UnrealCopilotSettings::default();
        let matched = apply_copilot_settings_ini(&mut settings, &ini).unwrap();

        assert!(matched);
        assert_eq!(settings.transport, "sse");
        assert_eq!(settings.host, "0.0.0.0");
        assert_eq!(settings.port, 21234);
        assert_eq!(settings.path, "custom");
        assert!(settings.auto_start_mcp_server);
    }

    #[test]
    fn normalizes_endpoint_id_and_path() {
        assert_eq!(
            default_endpoint_id("LyraStarterGame"),
            "lyrastartergame-local"
        );
        assert_eq!(normalize_mcp_path("mcp"), "/mcp");
        assert_eq!(normalize_mcp_path("/rpc"), "/rpc");
    }
}
