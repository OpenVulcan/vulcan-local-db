use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Local};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceId {
    LanceDb,
    DuckDb,
}

impl ServiceId {
    pub fn label(self) -> &'static str {
        match self {
            Self::LanceDb => "vldb-lancedb",
            Self::DuckDb => "vldb-duckdb",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::LanceDb => "LanceDB",
            Self::DuckDb => "DuckDB",
        }
    }
}

impl fmt::Display for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    pub fn toggle(self) -> Self {
        match self {
            Self::Debug => Self::Release,
            Self::Release => Self::Debug,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }

    pub fn cargo_flag(self) -> Option<&'static str> {
        match self {
            Self::Debug => None,
            Self::Release => Some("--release"),
        }
    }
}

impl fmt::Display for BuildProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceSpec {
    pub id: ServiceId,
    pub folder_name: &'static str,
    pub default_host: &'static str,
    pub default_port: u16,
    pub default_db_path: &'static str,
    pub default_log_file_name: &'static str,
}

impl ServiceSpec {
    pub fn directory(self, workspace_root: &Path) -> PathBuf {
        workspace_root.join(self.folder_name)
    }

    pub fn manifest_path(self, workspace_root: &Path) -> PathBuf {
        self.directory(workspace_root).join("Cargo.toml")
    }

    pub fn example_config_path(self, workspace_root: &Path) -> PathBuf {
        self.directory(workspace_root)
            .join(format!("{}.json.example", self.folder_name))
    }

    pub fn workspace_config_path(self, workspace_root: &Path) -> PathBuf {
        self.directory(workspace_root)
            .join(format!("{}.json", self.folder_name))
    }

    pub fn binary_path(self, workspace_root: &Path, profile: BuildProfile) -> PathBuf {
        let executable_name = executable_name(self.folder_name);
        let profile_dir = match profile {
            BuildProfile::Debug => "debug",
            BuildProfile::Release => "release",
        };

        self.directory(workspace_root)
            .join("target")
            .join(profile_dir)
            .join(executable_name)
    }
}

pub fn service_specs() -> [ServiceSpec; 2] {
    [
        ServiceSpec {
            id: ServiceId::LanceDb,
            folder_name: "vldb-lancedb",
            default_host: "127.0.0.1",
            default_port: 19301,
            default_db_path: "./data",
            default_log_file_name: "vldb-lancedb.log",
        },
        ServiceSpec {
            id: ServiceId::DuckDb,
            folder_name: "vldb-duckdb",
            default_host: "0.0.0.0",
            default_port: 19401,
            default_db_path: "./data/duckdb.db",
            default_log_file_name: "vldb-duckdb.log",
        },
    ]
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub services: Vec<ServiceSpec>,
}

impl Workspace {
    pub fn discover() -> Result<Self> {
        let current_dir = std::env::current_dir().context("failed to read current directory")?;
        let mut cursor = current_dir.clone();

        loop {
            if cursor.join("vldb-lancedb").is_dir() && cursor.join("vldb-duckdb").is_dir() {
                return Ok(Self {
                    root: cursor,
                    services: service_specs().to_vec(),
                });
            }

            if !cursor.pop() {
                bail!(
                    "failed to locate workspace root from {}",
                    current_dir.display()
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceConfigSummary {
    pub source_path: PathBuf,
    pub source_label: &'static str,
    pub raw_text: String,
    pub host: String,
    pub probe_host: String,
    pub port: u16,
    pub db_path: String,
    pub log_dir: PathBuf,
    pub log_file_name: String,
    pub log_path: PathBuf,
}

#[derive(Debug)]
pub struct ManagedProcess {
    pub child: Child,
    pub pid: u32,
    pub started_at: Instant,
    pub started_label: String,
    pub profile: BuildProfile,
}

#[derive(Debug, Clone)]
pub struct BuildRecord {
    pub profile: BuildProfile,
    pub finished_at: String,
    pub success: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ExitRecord {
    pub finished_at: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug)]
pub enum BackgroundEvent {
    LogLine {
        service: ServiceId,
        scope: LogScope,
        line: String,
    },
    BuildFinished {
        service: ServiceId,
        profile: BuildProfile,
        success: bool,
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum LogScope {
    Build,
    Stdout,
    Stderr,
}

impl LogScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug)]
pub struct ServiceState {
    pub spec: ServiceSpec,
    pub config: Option<ServiceConfigSummary>,
    pub managed_process: Option<ManagedProcess>,
    pub build_running: bool,
    pub build_started_at: Option<Instant>,
    pub last_build: Option<BuildRecord>,
    pub last_exit: Option<ExitRecord>,
    pub last_error: Option<String>,
    pub last_probe_ok: bool,
}

impl ServiceState {
    pub fn new(spec: ServiceSpec) -> Self {
        Self {
            spec,
            config: None,
            managed_process: None,
            build_running: false,
            build_started_at: None,
            last_build: None,
            last_exit: None,
            last_error: None,
            last_probe_ok: false,
        }
    }

    pub fn status(&self) -> ServiceStatus {
        if self.build_running {
            return ServiceStatus::Building;
        }

        if self.managed_process.is_some() {
            if self.last_probe_ok {
                return ServiceStatus::Running;
            }

            return ServiceStatus::Starting;
        }

        if self.last_probe_ok {
            return ServiceStatus::External;
        }

        if self.last_error.is_some() {
            return ServiceStatus::Failed;
        }

        ServiceStatus::Stopped
    }

    pub fn uptime_label(&self) -> Option<String> {
        self.managed_process
            .as_ref()
            .map(|process| format_duration(process.started_at.elapsed()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Building,
    Starting,
    Running,
    External,
    Failed,
    Stopped,
}

pub fn ensure_workspace_config(spec: ServiceSpec, workspace_root: &Path) -> Result<PathBuf> {
    let target = spec.workspace_config_path(workspace_root);
    if target.is_file() {
        return Ok(target);
    }

    let source = spec.example_config_path(workspace_root);
    let content = fs::read_to_string(&source)
        .with_context(|| format!("failed to read example config {}", source.display()))?;
    fs::write(&target, content)
        .with_context(|| format!("failed to write workspace config {}", target.display()))?;

    Ok(target)
}

pub fn load_service_config(
    spec: ServiceSpec,
    workspace_root: &Path,
) -> Result<ServiceConfigSummary> {
    let workspace_config = spec.workspace_config_path(workspace_root);
    let example_config = spec.example_config_path(workspace_root);

    let (source_path, source_label) = if workspace_config.is_file() {
        (workspace_config, "workspace")
    } else if example_config.is_file() {
        (example_config, "example")
    } else {
        return Err(anyhow!("missing config files for {}", spec.folder_name));
    };

    let raw_text = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let json: Value = serde_json::from_str(&raw_text)
        .with_context(|| format!("failed to parse {}", source_path.display()))?;

    let host = json
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or(spec.default_host)
        .to_string();
    let port = json
        .get("port")
        .and_then(Value::as_u64)
        .map(|value| value as u16)
        .unwrap_or(spec.default_port);
    let db_path_raw = json
        .get("db_path")
        .and_then(Value::as_str)
        .unwrap_or(spec.default_db_path);
    let base_dir = source_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace_root.to_path_buf());
    let db_path = if looks_like_uri(db_path_raw) {
        db_path_raw.to_string()
    } else {
        normalize_path(base_dir.join(db_path_raw))
            .to_string_lossy()
            .to_string()
    };

    let logging = json.get("logging").and_then(Value::as_object);
    let log_file_name = logging
        .and_then(|value| value.get("log_file_name"))
        .and_then(Value::as_str)
        .unwrap_or(spec.default_log_file_name)
        .to_string();

    let configured_log_dir = logging
        .and_then(|value| value.get("log_dir"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    let log_dir = resolve_log_dir(spec, configured_log_dir, &db_path, &base_dir);
    let log_path = log_dir.join(&log_file_name);

    Ok(ServiceConfigSummary {
        source_path,
        source_label,
        raw_text,
        probe_host: normalize_probe_host(&host),
        host,
        port,
        db_path,
        log_dir: log_dir.clone(),
        log_file_name,
        log_path,
    })
}

pub fn probe_service(summary: &ServiceConfigSummary) -> bool {
    let timeout = Duration::from_millis(80);
    let address_text = format!("{}:{}", summary.probe_host, summary.port);
    let addresses = match address_text.to_socket_addrs() {
        Ok(value) => value,
        Err(_) => return false,
    };

    for address in addresses {
        if TcpStream::connect_timeout(&address, timeout).is_ok() {
            return true;
        }
    }

    false
}

pub fn spawn_build(
    spec: ServiceSpec,
    workspace_root: &Path,
    profile: BuildProfile,
    tx: Sender<BackgroundEvent>,
) {
    let service_dir = spec.directory(workspace_root);
    thread::spawn(move || {
        let mut command = Command::new("cargo");
        command.current_dir(&service_dir).arg("build");
        if let Some(flag) = profile.cargo_flag() {
            command.arg(flag);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = tx.send(BackgroundEvent::LogLine {
                    service: spec.id,
                    scope: LogScope::Build,
                    line: format!("failed to spawn cargo build: {error}"),
                });
                let _ = tx.send(BackgroundEvent::BuildFinished {
                    service: spec.id,
                    profile,
                    success: false,
                    exit_code: None,
                });
                return;
            }
        };

        let stdout_handle = child
            .stdout
            .take()
            .map(|stdout| spawn_reader(stdout, tx.clone(), spec.id, LogScope::Build));
        let stderr_handle = child
            .stderr
            .take()
            .map(|stderr| spawn_reader(stderr, tx.clone(), spec.id, LogScope::Build));

        let status = child.wait();

        if let Some(handle) = stdout_handle {
            let _ = handle.join();
        }
        if let Some(handle) = stderr_handle {
            let _ = handle.join();
        }

        match status {
            Ok(status) => {
                let _ = tx.send(BackgroundEvent::BuildFinished {
                    service: spec.id,
                    profile,
                    success: status.success(),
                    exit_code: status.code(),
                });
            }
            Err(error) => {
                let _ = tx.send(BackgroundEvent::LogLine {
                    service: spec.id,
                    scope: LogScope::Build,
                    line: format!("cargo build wait failed: {error}"),
                });
                let _ = tx.send(BackgroundEvent::BuildFinished {
                    service: spec.id,
                    profile,
                    success: false,
                    exit_code: None,
                });
            }
        }
    });
}

pub fn start_service(
    spec: ServiceSpec,
    workspace_root: &Path,
    profile: BuildProfile,
    tx: Sender<BackgroundEvent>,
) -> Result<ManagedProcess> {
    let binary_path = spec.binary_path(workspace_root, profile);
    if !binary_path.is_file() {
        bail!(
            "{} binary not found at {}. Build it first.",
            spec.folder_name,
            binary_path.display()
        );
    }

    let config_path = spec.workspace_config_path(workspace_root);
    if !config_path.is_file() {
        bail!(
            "workspace config missing for {} at {}",
            spec.folder_name,
            config_path.display()
        );
    }

    let service_dir = spec.directory(workspace_root);

    let mut command = Command::new(&binary_path);
    command
        .current_dir(service_dir)
        .arg("--config")
        .arg(&config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {}", spec.folder_name))?;

    let pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        spawn_reader(stdout, tx.clone(), spec.id, LogScope::Stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(stderr, tx, spec.id, LogScope::Stderr);
    }

    Ok(ManagedProcess {
        child,
        pid,
        started_at: Instant::now(),
        started_label: timestamp_label(Local::now()),
        profile,
    })
}

pub fn stop_process(process: &mut ManagedProcess) -> Result<Option<i32>> {
    process
        .child
        .kill()
        .context("failed to terminate managed process")?;
    let status = process
        .child
        .wait()
        .context("failed to wait for managed process shutdown")?;
    Ok(status.code())
}

pub fn timestamp_label(value: DateTime<Local>) -> String {
    value.format("%H:%M:%S").to_string()
}

pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remain = seconds % 60;

    if hours > 0 {
        return format!("{hours}h {minutes:02}m");
    }
    if minutes > 0 {
        return format!("{minutes}m {remain:02}s");
    }

    format!("{remain}s")
}

pub fn format_exit_code(code: Option<i32>) -> String {
    match code {
        Some(value) => value.to_string(),
        None => "signal/unknown".to_string(),
    }
}

fn resolve_log_dir(
    spec: ServiceSpec,
    configured_log_dir: &str,
    db_path: &str,
    base_dir: &Path,
) -> PathBuf {
    if !configured_log_dir.trim().is_empty() {
        return normalize_path(base_dir.join(configured_log_dir));
    }

    match spec.id {
        ServiceId::LanceDb => {
            if looks_like_uri(db_path) {
                return normalize_path(base_dir.join("vldb-lancedb-logs"));
            }

            normalize_path(PathBuf::from(db_path).join("logs"))
        }
        ServiceId::DuckDb => {
            let db_path = PathBuf::from(db_path);
            let parent = db_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));

            match db_path.file_stem().or_else(|| db_path.file_name()) {
                Some(stem) => {
                    normalize_path(parent.join(format!("{}_log", stem.to_string_lossy())))
                }
                None => normalize_path(parent.join("duckdb_log")),
            }
        }
    }
}

fn normalize_probe_host(host: &str) -> String {
    match host.trim() {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
        value if value.is_empty() => "127.0.0.1".to_string(),
        value => value.to_string(),
    }
}

fn looks_like_uri(value: &str) -> bool {
    value.contains("://")
}

fn executable_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

fn spawn_reader<R>(
    reader: R,
    tx: Sender<BackgroundEvent>,
    service: ServiceId,
    scope: LogScope,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();

        loop {
            buffer.clear();
            let bytes_read = match reader.read_until(b'\n', &mut buffer) {
                Ok(bytes_read) => bytes_read,
                Err(_) => break,
            };

            if bytes_read == 0 {
                break;
            }

            let line = String::from_utf8_lossy(&buffer)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if line.is_empty() {
                continue;
            }

            let _ = tx.send(BackgroundEvent::LogLine {
                service,
                scope,
                line,
            });
        }
    })
}
