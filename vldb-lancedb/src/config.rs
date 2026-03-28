use serde::Deserialize;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_FILE: &str = "vldb-lancedb.json";
const LEGACY_CONFIG_FILE: &str = "lancedb.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub db_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 50051,
            db_path: "./data/lancedb".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub host: String,
    pub port: u16,
    pub db_path: String,
    pub source: Option<PathBuf>,
}

impl ResolvedConfig {
    pub fn is_local_db_path(&self) -> bool {
        !looks_like_uri(&self.db_path)
    }
}

pub fn load() -> Result<ResolvedConfig, Box<dyn Error>> {
    let explicit_config = parse_config_arg()?;

    let config_path = if let Some(path) = explicit_config {
        Some(path)
    } else {
        find_default_config_file()?
    };

    let (config, source) = match config_path {
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

    Ok(ResolvedConfig {
        host: config.host,
        port: config.port,
        db_path: resolved_db_path,
        source,
    })
}

fn parse_config_arg() -> Result<Option<PathBuf>, Box<dyn Error>> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-config" || arg == "--config" {
            let next = args
                .next()
                .ok_or("missing value after -config / --config")?;
            let current_dir = env::current_dir()?;
            return Ok(Some(resolve_path_like_shell(&next, &current_dir)));
        }
    }
    Ok(None)
}

fn find_default_config_file() -> Result<Option<PathBuf>, Box<dyn Error>> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join(DEFAULT_CONFIG_FILE));
            candidates.push(exe_dir.join(LEGACY_CONFIG_FILE));
        }
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

    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home_dir_string() {
            return PathBuf::from(home).join(rest).to_string_lossy().to_string();
        }
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
