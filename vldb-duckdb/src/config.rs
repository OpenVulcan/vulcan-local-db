use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

const PRIMARY_CONFIG_NAME: &str = "vldb-duckdb.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub db_path: PathBuf,
    pub memory_limit: String,
    pub threads: usize,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub file_enabled: bool,
    pub stderr_enabled: bool,
    pub request_log_enabled: bool,
    pub slow_query_log_enabled: bool,
    pub slow_query_threshold_ms: u64,
    pub slow_query_full_sql_enabled: bool,
    pub sql_preview_chars: usize,
    pub log_dir: PathBuf,
    pub log_file_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 50052,
            db_path: PathBuf::from("./data/duckdb.db"),
            memory_limit: "2GB".to_string(),
            threads: 4,
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
            slow_query_log_enabled: true,
            slow_query_threshold_ms: 1_000,
            slow_query_full_sql_enabled: true,
            sql_preview_chars: 160,
            log_dir: PathBuf::new(),
            log_file_name: "vldb-duckdb.log".to_string(),
        }
    }
}

impl Config {
    fn validate(&self) -> Result<(), BoxError> {
        if self.host.trim().is_empty() {
            return Err(invalid_input("config.host must not be empty"));
        }
        if self.port == 0 {
            return Err(invalid_input("config.port must be greater than 0"));
        }
        if self.memory_limit.trim().is_empty() {
            return Err(invalid_input("config.memory_limit must not be empty"));
        }
        if self.threads == 0 {
            return Err(invalid_input("config.threads must be greater than 0"));
        }
        self.logging.validate()?;
        Ok(())
    }
}

impl LoggingConfig {
    fn validate(&self) -> Result<(), BoxError> {
        if self.sql_preview_chars == 0 {
            return Err(invalid_input(
                "config.logging.sql_preview_chars must be greater than 0",
            ));
        }
        if self.slow_query_threshold_ms == 0 {
            return Err(invalid_input(
                "config.logging.slow_query_threshold_ms must be greater than 0",
            ));
        }
        if self.file_enabled && self.log_file_name.trim().is_empty() {
            return Err(invalid_input(
                "config.logging.log_file_name must not be empty when file logging is enabled",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub source: Option<PathBuf>,
}

pub fn load_config() -> Result<LoadedConfig, BoxError> {
    if let Some(explicit_path) = parse_config_arg()? {
        return load_config_file(explicit_path);
    }

    for candidate in default_search_paths()? {
        if candidate.is_file() {
            return load_config_file(candidate);
        }
    }

    let cwd = env::current_dir()?;
    let mut config = Config::default();
    config.db_path = expand_path(&config.db_path, &cwd)?;
    config.logging.log_dir = resolve_log_dir(&config.logging.log_dir, &config.db_path, &cwd)?;
    config.validate()?;

    Ok(LoadedConfig {
        config,
        source: None,
    })
}

fn load_config_file(path: PathBuf) -> Result<LoadedConfig, BoxError> {
    let cwd = env::current_dir()?;
    let resolved_path = expand_path(path, &cwd)?;

    let raw = fs::read_to_string(&resolved_path)?;
    let mut config: Config = serde_json::from_str(&raw)?;

    let base_dir = resolved_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cwd.clone());

    config.db_path = expand_path(&config.db_path, &base_dir)?;
    config.logging.log_dir = resolve_log_dir(&config.logging.log_dir, &config.db_path, &base_dir)?;
    config.validate()?;

    Ok(LoadedConfig {
        config,
        source: Some(resolved_path),
    })
}

fn parse_config_arg() -> Result<Option<PathBuf>, BoxError> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];
        if arg == "-config" || arg == "--config" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| invalid_input("missing file path after -config/--config"))?;
            return Ok(Some(PathBuf::from(value)));
        }

        if let Some(value) = arg.strip_prefix("-config=") {
            return Ok(Some(PathBuf::from(value)));
        }

        if let Some(value) = arg.strip_prefix("--config=") {
            return Ok(Some(PathBuf::from(value)));
        }

        i += 1;
    }

    Ok(None)
}

fn default_search_paths() -> Result<Vec<PathBuf>, BoxError> {
    let cwd = env::current_dir()?;
    let mut candidates = vec![cwd.join(PRIMARY_CONFIG_NAME)];

    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let exe_config = dir.join(PRIMARY_CONFIG_NAME);
        if exe_config != candidates[0] {
            candidates.push(exe_config);
        }
    }

    Ok(candidates)
}

fn expand_path<P: AsRef<Path>>(path: P, base_dir: &Path) -> Result<PathBuf, BoxError> {
    let with_home = expand_tilde(path.as_ref())?;
    let absolute_or_relative = if with_home.is_absolute() {
        with_home
    } else {
        base_dir.join(with_home)
    };

    Ok(normalize_path(absolute_or_relative))
}

fn resolve_log_dir(
    configured_dir: &Path,
    db_path: &Path,
    base_dir: &Path,
) -> Result<PathBuf, BoxError> {
    if configured_dir.as_os_str().is_empty() {
        return Ok(derive_default_log_dir(db_path));
    }

    expand_path(configured_dir, base_dir)
}

fn derive_default_log_dir(db_path: &Path) -> PathBuf {
    let parent = db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    match db_path.file_stem().or_else(|| db_path.file_name()) {
        Some(stem) => parent.join(format!("{}_log", stem.to_string_lossy())),
        None => parent.join("duckdb_log"),
    }
}

fn expand_tilde(path: &Path) -> Result<PathBuf, BoxError> {
    let raw = path.to_string_lossy();

    if raw == "~" {
        let home = home_dir().ok_or_else(|| invalid_input("cannot expand '~': HOME is not set"))?;
        return Ok(home);
    }

    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        let home = home_dir()
            .ok_or_else(|| invalid_input("cannot expand '~/' or '~\\': HOME is not set"))?;
        return Ok(home.join(rest));
    }

    Ok(path.to_path_buf())
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(
            || match (env::var_os("HOMEDRIVE"), env::var_os("HOMEPATH")) {
                (Some(drive), Some(path)) => {
                    let mut buf = PathBuf::from(drive);
                    buf.push(path);
                    Some(buf)
                }
                _ => None,
            },
        )
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    normalized
}

fn invalid_input(message: impl Into<String>) -> BoxError {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{derive_default_log_dir, resolve_log_dir};
    use std::path::{Path, PathBuf};

    #[test]
    fn default_log_dir_uses_db_file_stem() {
        let db_path = PathBuf::from("/srv/vldb/duckdb.db");
        assert_eq!(
            derive_default_log_dir(&db_path),
            PathBuf::from("/srv/vldb/duckdb_log")
        );
    }

    #[test]
    fn explicit_relative_log_dir_is_resolved_from_config_dir() {
        let resolved = resolve_log_dir(
            Path::new("./logs"),
            Path::new("/srv/vldb/duckdb.db"),
            Path::new("/etc/vldb"),
        )
        .expect("resolve log dir");

        assert_eq!(resolved, PathBuf::from("/etc/vldb/logs"));
    }
}
