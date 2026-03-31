use serde::Deserialize;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_FILE: &str = "vldb-lancedb.json";
const LEGACY_CONFIG_FILE: &str = "lancedb.json";

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub db_path: String,
    pub read_consistency_interval_ms: Option<u64>,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub file_enabled: bool,
    pub stderr_enabled: bool,
    pub request_log_enabled: bool,
    pub slow_request_log_enabled: bool,
    pub slow_request_threshold_ms: u64,
    pub include_request_details_in_slow_log: bool,
    pub request_preview_chars: usize,
    pub log_dir: PathBuf,
    pub log_file_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 50051,
            db_path: "./data".to_string(),
            read_consistency_interval_ms: Some(0),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            file_enabled: true,
            stderr_enabled: true,
            request_log_enabled: true,
            slow_request_log_enabled: true,
            slow_request_threshold_ms: 1_000,
            include_request_details_in_slow_log: true,
            request_preview_chars: 160,
            log_dir: PathBuf::new(),
            log_file_name: "vldb-lancedb.log".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub host: String,
    pub port: u16,
    pub db_path: String,
    pub read_consistency_interval_ms: Option<u64>,
    pub source: Option<PathBuf>,
    pub logging: LoggingConfig,
}

impl ResolvedConfig {
    pub fn is_local_db_path(&self) -> bool {
        !looks_like_uri(&self.db_path)
    }

    pub fn read_consistency_interval(&self) -> Option<std::time::Duration> {
        self.read_consistency_interval_ms
            .map(std::time::Duration::from_millis)
    }

    pub fn concurrent_write_warning(&self) -> Option<&'static str> {
        if is_plain_s3_uri(&self.db_path) {
            Some(
                "plain s3:// storage is not safe for concurrent LanceDB writers; use s3+ddb:// for multi-writer deployments or ensure this service is the only writer for each table",
            )
        } else {
            None
        }
    }
}

pub fn load() -> Result<ResolvedConfig, BoxError> {
    let explicit_config = parse_config_arg()?;
    let config_path = explicit_config.or(find_default_config_file()?);

    let (mut config, source) = match config_path {
        Some(path) => {
            let content = fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&content)?;
            (config, Some(path))
        }
        None => (Config::default(), None),
    };

    let base_dir = source
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or(env::current_dir()?);

    let resolved_db_path = resolve_data_path(&config.db_path, &base_dir);
    config.logging.log_dir = resolve_log_dir(&config.logging.log_dir, &resolved_db_path, &base_dir);
    validate_config(&config)?;

    Ok(ResolvedConfig {
        host: config.host,
        port: config.port,
        db_path: resolved_db_path,
        read_consistency_interval_ms: config.read_consistency_interval_ms,
        source,
        logging: config.logging,
    })
}

fn validate_config(config: &Config) -> Result<(), BoxError> {
    if config.host.trim().is_empty() {
        return Err(invalid_input("config.host must not be empty"));
    }
    if config.port == 0 {
        return Err(invalid_input("config.port must be greater than 0"));
    }
    if config.db_path.trim().is_empty() {
        return Err(invalid_input("config.db_path must not be empty"));
    }
    if config.logging.request_preview_chars == 0 {
        return Err(invalid_input(
            "config.logging.request_preview_chars must be greater than 0",
        ));
    }
    if config.logging.slow_request_threshold_ms == 0 {
        return Err(invalid_input(
            "config.logging.slow_request_threshold_ms must be greater than 0",
        ));
    }
    if config.logging.file_enabled && config.logging.log_file_name.trim().is_empty() {
        return Err(invalid_input(
            "config.logging.log_file_name must not be empty when file logging is enabled",
        ));
    }
    Ok(())
}

fn parse_config_arg() -> Result<Option<PathBuf>, BoxError> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-config" || arg == "--config" {
            let next = args
                .next()
                .ok_or_else(|| invalid_input("missing value after -config / --config"))?;
            let current_dir = env::current_dir()?;
            return Ok(Some(resolve_path_like_shell(&next, &current_dir)));
        }
    }
    Ok(None)
}

fn find_default_config_file() -> Result<Option<PathBuf>, BoxError> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe()
        && let Some(exe_dir) = current_exe.parent()
    {
        candidates.push(exe_dir.join(DEFAULT_CONFIG_FILE));
        candidates.push(exe_dir.join(LEGACY_CONFIG_FILE));
    }

    let current_dir = env::current_dir()?;
    candidates.push(current_dir.join(DEFAULT_CONFIG_FILE));
    candidates.push(current_dir.join(LEGACY_CONFIG_FILE));

    Ok(candidates.into_iter().find(|p| p.is_file()))
}

fn resolve_data_path(raw: &str, base_dir: &Path) -> String {
    if looks_like_uri(raw) {
        return raw.to_string();
    }

    resolve_path_like_shell(raw, base_dir)
        .to_string_lossy()
        .to_string()
}

fn resolve_log_dir(configured_dir: &Path, db_path: &str, base_dir: &Path) -> PathBuf {
    if !configured_dir.as_os_str().is_empty() {
        return resolve_path_like_shell(&configured_dir.to_string_lossy(), base_dir);
    }

    if looks_like_uri(db_path) {
        return base_dir.join("vldb-lancedb-logs");
    }

    let db_path = PathBuf::from(db_path);
    db_path.join("logs")
}

fn resolve_path_like_shell(raw: &str, base_dir: &Path) -> PathBuf {
    let expanded = expand_tilde(raw);
    let path = PathBuf::from(expanded);

    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn expand_tilde(raw: &str) -> String {
    if raw == "~" {
        return home_dir_string().unwrap_or_else(|| raw.to_string());
    }

    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = home_dir_string()
    {
        return PathBuf::from(home).join(rest).to_string_lossy().to_string();
    }

    if let Some(rest) = raw.strip_prefix("~\\")
        && let Some(home) = home_dir_string()
    {
        return PathBuf::from(home).join(rest).to_string_lossy().to_string();
    }

    raw.to_string()
}

fn home_dir_string() -> Option<String> {
    env::var("HOME")
        .ok()
        .or_else(|| env::var("USERPROFILE").ok())
}

fn looks_like_uri(value: &str) -> bool {
    value.contains("://")
}

fn is_plain_s3_uri(value: &str) -> bool {
    value.to_ascii_lowercase().starts_with("s3://")
}

fn invalid_input(message: impl Into<String>) -> BoxError {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{Config, LoggingConfig, ResolvedConfig, resolve_log_dir};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn default_log_dir_uses_logs_subdir_for_local_db_path() {
        let resolved = resolve_log_dir(Path::new(""), "/srv/vldb/lancedb", Path::new("/etc/vldb"));
        assert_eq!(resolved, PathBuf::from("/srv/vldb/lancedb/logs"));
    }

    #[test]
    fn explicit_relative_log_dir_is_resolved_from_config_dir() {
        let resolved = resolve_log_dir(
            Path::new("./logs"),
            "/srv/vldb/lancedb",
            Path::new("/etc/vldb"),
        );
        assert_eq!(resolved, PathBuf::from("/etc/vldb/logs"));
    }

    #[test]
    fn uri_db_path_falls_back_to_config_relative_log_dir() {
        let resolved =
            resolve_log_dir(Path::new(""), "s3://bucket/lancedb", Path::new("/etc/vldb"));
        assert_eq!(resolved, PathBuf::from("/etc/vldb/vldb-lancedb-logs"));
    }

    #[test]
    fn default_read_consistency_interval_is_strong() {
        assert_eq!(Config::default().read_consistency_interval_ms, Some(0));
    }

    #[test]
    fn plain_s3_uri_emits_concurrent_write_warning() {
        let cfg = ResolvedConfig {
            host: "127.0.0.1".to_string(),
            port: 50051,
            db_path: "s3://bucket/lancedb".to_string(),
            read_consistency_interval_ms: Some(0),
            source: None,
            logging: LoggingConfig::default(),
        };

        assert_eq!(
            cfg.read_consistency_interval(),
            Some(Duration::from_millis(0))
        );
        assert!(cfg.concurrent_write_warning().is_some());
    }

    #[test]
    fn s3_ddb_uri_does_not_emit_concurrent_write_warning() {
        let cfg = ResolvedConfig {
            host: "127.0.0.1".to_string(),
            port: 50051,
            db_path: "s3+ddb://bucket/lancedb".to_string(),
            read_consistency_interval_ms: Some(250),
            source: None,
            logging: LoggingConfig::default(),
        };

        assert_eq!(
            cfg.read_consistency_interval(),
            Some(Duration::from_millis(250))
        );
        assert!(cfg.concurrent_write_warning().is_none());
    }
}
