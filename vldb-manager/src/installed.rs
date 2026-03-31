use std::cmp::Ordering;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use tempfile::TempDir;
use zip::ZipArchive;

use crate::service::{ServiceId, format_exit_code};

const REPO_SLUG: &str = "OpenVulcan/vulcan-local-db";
const WINSW_VERSION: &str = "v2.12.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredConfig {
    language: String,
    install_dir: String,
    release_tag: String,
    script_version: String,
    lancedb_root: String,
    duckdb_root: String,
    initialized: bool,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            install_dir: String::new(),
            release_tag: String::new(),
            script_version: env!("CARGO_PKG_VERSION").to_string(),
            lancedb_root: String::new(),
            duckdb_root: String::new(),
            initialized: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagerPaths {
    pub global_home: PathBuf,
    pub global_config_path: PathBuf,
    pub run_dir: PathBuf,
    pub install_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub config_dir: PathBuf,
    pub examples_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub tools_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ManagerState {
    pub language: String,
    pub release_tag: Option<String>,
    pub manager_version: String,
    pub lancedb_root: PathBuf,
    pub duckdb_root: PathBuf,
    pub initialized: bool,
}

#[derive(Debug, Clone)]
pub struct InstallManager {
    pub paths: ManagerPaths,
    pub state: ManagerState,
    client: Client,
}

#[derive(Debug, Clone)]
pub struct InstalledInstance {
    pub service: ServiceId,
    pub instance_name: String,
    pub config_path: PathBuf,
    pub host: String,
    pub port: u16,
    pub db_path: PathBuf,
    pub service_name: String,
    pub registered: bool,
    pub running: bool,
}

impl InstalledInstance {
    pub fn display_name(&self) -> String {
        format!("{} / {}", self.service.label(), self.instance_name)
    }
}

#[derive(Debug, Clone)]
pub struct InstanceRequest {
    pub service: ServiceId,
    pub instance_name: String,
    pub bind_host: String,
    pub port: u16,
    pub data_path: PathBuf,
    pub service_name: String,
}

#[derive(Debug, Clone)]
pub struct InitRequest {
    pub lancedb_root: PathBuf,
    pub duckdb_root: PathBuf,
    pub bind_host: String,
    pub lancedb_port: u16,
    pub duckdb_port: u16,
    pub lancedb_service_name: String,
    pub duckdb_service_name: String,
}

#[derive(Debug, Clone)]
pub struct UpdateCheck {
    pub current_manager_version: String,
    pub latest_release_tag: Option<String>,
    pub installed_release_tag: Option<String>,
    pub manager_update_available: bool,
    pub binary_update_available: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstanceConfig {
    host: String,
    port: u16,
    db_path: String,
    #[serde(default)]
    memory_limit: Option<String>,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    service_name: Option<String>,
    #[serde(default)]
    logging: serde_json::Value,
}

impl InstallManager {
    pub fn load(default_install_dir: &Path) -> Result<Self> {
        let home_dir =
            home_dir().ok_or_else(|| anyhow!("failed to resolve user home directory"))?;
        let global_home = home_dir.join(".vulcan").join("vldb");
        let global_config_path = global_home.join("config.json");
        let run_dir = global_home.join("run");
        let logs_dir = global_home.join("logs");

        let stored = if global_config_path.is_file() {
            let raw = fs::read_to_string(&global_config_path)
                .with_context(|| format!("failed to read {}", global_config_path.display()))?;
            serde_json::from_str::<StoredConfig>(&raw).unwrap_or_default()
        } else {
            StoredConfig::default()
        };

        let install_dir = if !stored.install_dir.trim().is_empty() {
            PathBuf::from(&stored.install_dir)
        } else {
            default_install_dir.to_path_buf()
        };
        let install_dir = normalize_path(install_dir);
        let bin_dir = install_dir.join("bin");
        let config_dir = install_dir.join("config");
        let examples_dir = install_dir.join("share").join("examples");
        let tools_dir = install_dir.join("tools");
        let lancedb_root = if !stored.lancedb_root.trim().is_empty() {
            PathBuf::from(&stored.lancedb_root)
        } else {
            global_home.join("lancedb")
        };
        let duckdb_root = if !stored.duckdb_root.trim().is_empty() {
            PathBuf::from(&stored.duckdb_root)
        } else {
            global_home.join("duckdb")
        };

        let client = Client::builder()
            .user_agent(format!("vldb-manager/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to create HTTP client")?;

        Ok(Self {
            paths: ManagerPaths {
                global_home,
                global_config_path,
                run_dir,
                install_dir,
                bin_dir,
                config_dir,
                examples_dir,
                logs_dir,
                tools_dir,
            },
            state: ManagerState {
                language: stored.language,
                release_tag: string_option(stored.release_tag),
                manager_version: env!("CARGO_PKG_VERSION").to_string(),
                lancedb_root: normalize_path(lancedb_root),
                duckdb_root: normalize_path(duckdb_root),
                initialized: stored.initialized,
            },
            client,
        })
    }

    pub fn ensure_launcher(&self, manager_exe: &Path) -> Result<()> {
        fs::create_dir_all(&self.paths.bin_dir)?;
        if cfg!(windows) {
            let launcher_cmd = self.paths.bin_dir.join("vldb.cmd");
            let launcher_ps1 = self.paths.bin_dir.join("vldb.ps1");
            let exe_text = manager_exe.display().to_string();
            fs::write(
                launcher_cmd,
                format!("@echo off\r\n\"{}\" %*\r\n", exe_text.replace('"', "\"\"")),
            )?;
            fs::write(
                launcher_ps1,
                format!("& \"{}\" @args\r\n", exe_text.replace('"', "\"\"")),
            )?;
        } else {
            let launcher = global_launcher_path(&self.paths.install_dir);
            if let Some(parent) = launcher.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                &launcher,
                format!(
                    "#!/usr/bin/env bash\nexec \"{}\" \"$@\"\n",
                    manager_exe.display()
                ),
            )?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&launcher)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&launcher, perms)?;
            }
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.paths.global_home)?;
        let stored = StoredConfig {
            language: self.state.language.clone(),
            install_dir: self.paths.install_dir.display().to_string(),
            release_tag: self.state.release_tag.clone().unwrap_or_default(),
            script_version: self.state.manager_version.clone(),
            lancedb_root: self.state.lancedb_root.display().to_string(),
            duckdb_root: self.state.duckdb_root.display().to_string(),
            initialized: self.state.initialized,
        };
        let json = serde_json::to_string_pretty(&stored)?;
        fs::write(&self.paths.global_config_path, json)?;
        Ok(())
    }

    pub fn list_instances(&self) -> Result<Vec<InstalledInstance>> {
        let mut items = Vec::new();
        for config_path in self.instance_config_files()? {
            let (service, instance_name) = parse_instance_meta(&config_path)?;
            let config = self.read_instance_config(&config_path)?;
            let service_name = config
                .service_name
                .clone()
                .unwrap_or_else(|| legacy_service_name(service, &instance_name));
            let registered = self.is_registered_by_name(&service_name)?;
            let running = self.is_running_by_name(&service_name)?;
            items.push(InstalledInstance {
                service,
                instance_name,
                config_path,
                host: config.host,
                port: config.port,
                db_path: normalize_path(PathBuf::from(config.db_path)),
                service_name,
                registered,
                running,
            });
        }
        items.sort_by(|left, right| left.display_name().cmp(&right.display_name()));
        Ok(items)
    }

    pub fn is_initialized(&self) -> Result<bool> {
        if self.state.initialized {
            return Ok(true);
        }
        Ok(!self.instance_config_files()?.is_empty())
    }

    pub fn default_data_root(&self, service: ServiceId) -> PathBuf {
        match service {
            ServiceId::LanceDb => self.state.lancedb_root.clone(),
            ServiceId::DuckDb => self.state.duckdb_root.clone(),
        }
    }

    pub fn default_instance_data_path(&self, service: ServiceId, instance_name: &str) -> PathBuf {
        match service {
            ServiceId::LanceDb => self.default_data_root(service).join(instance_name),
            ServiceId::DuckDb => self
                .default_data_root(service)
                .join(instance_name)
                .join("duckdb.db"),
        }
    }

    pub fn uses_chinese(&self) -> bool {
        self.state.language.starts_with("zh")
    }

    pub fn text<'a>(&self, zh: &'a str, en: &'a str) -> &'a str {
        if self.uses_chinese() { zh } else { en }
    }

    pub fn toggle_language(&mut self) -> Result<String> {
        self.state.language = if self.state.language.starts_with("zh") {
            "en".to_string()
        } else {
            "zh-CN".to_string()
        };
        self.save()?;
        Ok(format!(
            "{} {}",
            self.text("语言已切换到", "Language switched to"),
            self.state.language
        ))
    }

    pub fn new_unique_service_name(
        &self,
        service: ServiceId,
        instance_name: &str,
        preferred_name: Option<&str>,
        current_name: Option<&str>,
    ) -> Result<String> {
        let base_name = preferred_name
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| legacy_service_name(service, instance_name));
        let mut suffix = 2usize;
        let mut candidate = base_name.clone();

        loop {
            if self
                .validate_service_name(&candidate, service, instance_name, current_name)
                .is_ok()
            {
                return Ok(candidate);
            }
            candidate = format!("{base_name}-{suffix}");
            suffix += 1;
        }
    }

    pub fn check_updates(&self) -> Result<UpdateCheck> {
        let latest = self.fetch_latest_release()?;
        let current_manager = self.state.manager_version.clone();
        let installed_release = self.state.release_tag.clone();
        let latest_tag = Some(latest.tag_name.clone());
        let manager_update_available =
            version_compare(latest_tag.as_deref().unwrap_or(""), &current_manager)
                == Ordering::Greater;
        let binary_update_available = match (&installed_release, &latest_tag) {
            (Some(installed), Some(latest)) => {
                version_compare(latest, installed) == Ordering::Greater
            }
            (None, Some(_)) => true,
            _ => false,
        };

        Ok(UpdateCheck {
            current_manager_version: current_manager,
            latest_release_tag: latest_tag,
            installed_release_tag: installed_release,
            manager_update_available,
            binary_update_available,
        })
    }

    pub fn update_binaries_to_latest(&mut self) -> Result<String> {
        let latest = self.fetch_latest_release()?;
        self.update_binaries_to_tag(&latest.tag_name)
    }

    pub fn update_binaries_to_tag(&mut self, tag: &str) -> Result<String> {
        let release = self.fetch_release(Some(tag))?;
        let instances = self.list_instances()?;
        let running_before: Vec<(ServiceId, String, String)> = instances
            .iter()
            .filter(|instance| instance.running)
            .map(|instance| {
                (
                    instance.service,
                    instance.instance_name.clone(),
                    instance.service_name.clone(),
                )
            })
            .collect();

        let _ = self.stop_all_instances();
        for service in self.installed_service_kinds()? {
            self.install_service_binary(service, &release)?;
        }
        for (service, instance_name, service_name) in running_before {
            self.start_instance(service, &instance_name, &service_name)?;
        }

        self.state.release_tag = Some(release.tag_name.clone());
        self.save()?;
        Ok(format!(
            "{} {}",
            self.text("应用二进制已更新到", "Service binaries updated to"),
            release.tag_name
        ))
    }

    fn validate_instance_name(&self, candidate: &str) -> Result<()> {
        if candidate.is_empty() {
            bail!(
                "{}",
                self.text("实例名称不能为空", "Instance name cannot be empty")
            );
        }
        let valid = candidate.chars().enumerate().all(|(idx, ch)| match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => true,
            '-' | '_' => idx != 0,
            _ => false,
        }) && candidate.len() <= 32;
        if !valid {
            bail!(
                "{}",
                self.text(
                    "实例名只能包含字母、数字、连字符和下划线，且长度不超过 32",
                    "Instance names may only contain letters, digits, hyphens, and underscores, and must be at most 32 characters",
                )
            );
        }
        Ok(())
    }

    fn validate_data_roots(&self, lance_root: &Path, duck_root: &Path) -> Result<()> {
        self.validate_absolute_dir(
            lance_root,
            self.text("LanceDB 数据根目录", "LanceDB data root"),
        )?;
        self.validate_absolute_dir(
            duck_root,
            self.text("DuckDB 数据根目录", "DuckDB data root"),
        )?;
        if paths_overlap(lance_root, duck_root) {
            bail!(
                "{}",
                self.text(
                    "LanceDB 和 DuckDB 的数据根目录不能重叠",
                    "LanceDB and DuckDB data roots must not overlap",
                )
            );
        }
        if paths_overlap(&self.paths.install_dir, lance_root) {
            bail!(
                "{}",
                self.text(
                    "LanceDB 数据根目录必须位于安装目录之外",
                    "LanceDB data root must stay outside the installation directory",
                )
            );
        }
        if paths_overlap(&self.paths.install_dir, duck_root) {
            bail!(
                "{}",
                self.text(
                    "DuckDB 数据根目录必须位于安装目录之外",
                    "DuckDB data root must stay outside the installation directory",
                )
            );
        }
        Ok(())
    }

    fn validate_absolute_dir(&self, path: &Path, label: &str) -> Result<()> {
        if !path.is_absolute() {
            bail!(
                "{}",
                if self.uses_chinese() {
                    format!("{label} 必须是绝对路径")
                } else {
                    format!("{label} must be an absolute path")
                }
            );
        }
        if path.exists() && !path.is_dir() {
            bail!(
                "{}",
                if self.uses_chinese() {
                    format!("{label} 已存在且不是目录")
                } else {
                    format!("{label} already exists and is not a directory")
                }
            );
        }
        Ok(())
    }

    fn validate_bind_ip(&self, candidate: &str) -> Result<()> {
        if parse_ipv4(candidate).is_some() {
            return Ok(());
        }
        bail!(
            "{}",
            self.text(
                "绑定 IP 格式不正确，请输入合法的 IPv4 地址",
                "Bind IP is invalid. Please enter a valid IPv4 address",
            )
        )
    }

    fn validate_distinct_ports(&self, left: u16, right: u16) -> Result<()> {
        if left == right {
            bail!(
                "{}",
                self.text(
                    "LanceDB 和 DuckDB 必须使用不同端口",
                    "LanceDB and DuckDB must use different ports",
                )
            );
        }
        Ok(())
    }

    fn validate_port(
        &self,
        candidate: u16,
        service: ServiceId,
        instance_name: &str,
        current_port: Option<u16>,
        current_service_name: Option<&str>,
    ) -> Result<()> {
        if candidate == 0 {
            bail!(
                "{}",
                self.text(
                    "端口必须在 1 到 65535 之间",
                    "Port must be between 1 and 65535",
                )
            );
        }
        if let Some(message) = self.port_conflict_message(candidate, service, instance_name)? {
            bail!(message);
        }

        if current_port == Some(candidate)
            && let Some(service_name) = current_service_name
        {
            if self.is_running_by_name(service_name)? || test_port_available(candidate) {
                return Ok(());
            }
        }

        if !test_port_available(candidate) {
            bail!(
                "{}",
                if self.uses_chinese() {
                    format!("端口 {candidate} 已被其他服务、容器或进程占用，请更换端口")
                } else {
                    format!(
                        "Port {candidate} is already in use by another service, container, or process. Please choose another port"
                    )
                }
            );
        }
        Ok(())
    }

    fn port_conflict_message(
        &self,
        candidate_port: u16,
        service: ServiceId,
        instance_name: &str,
    ) -> Result<Option<String>> {
        for instance in self.list_instances()? {
            if instance.service == service && instance.instance_name == instance_name {
                continue;
            }
            if instance.port == candidate_port {
                return Ok(Some(format!(
                    "{}",
                    if self.uses_chinese() {
                        format!(
                            "端口 {} 已被 {}/{} 预留，请更换端口",
                            candidate_port,
                            instance.service.label(),
                            instance.instance_name
                        )
                    } else {
                        format!(
                            "Port {} is already reserved by {}/{}, please choose another port",
                            candidate_port,
                            instance.service.label(),
                            instance.instance_name
                        )
                    }
                )));
            }
        }
        Ok(None)
    }

    fn validate_data_path(
        &self,
        candidate: &Path,
        service: ServiceId,
        instance_name: &str,
    ) -> Result<()> {
        if !candidate.is_absolute() {
            bail!(
                "{}",
                self.text("请使用绝对数据路径", "Please use an absolute data path")
            );
        }
        if paths_overlap(&self.paths.install_dir, candidate) {
            bail!(
                "{}",
                self.text(
                    "数据库路径必须位于安装目录之外",
                    "Database path must stay outside the installation directory",
                )
            );
        }
        for instance in self.list_instances()? {
            if instance.service == service && instance.instance_name == instance_name {
                continue;
            }
            if paths_overlap(candidate, &instance.db_path) {
                bail!(
                    "{}",
                    if self.uses_chinese() {
                        format!(
                            "数据路径与 {}/{} 冲突：{}",
                            instance.service.label(),
                            instance.instance_name,
                            instance.db_path.display()
                        )
                    } else {
                        format!(
                            "Data path conflicts with {}/{}: {}",
                            instance.service.label(),
                            instance.instance_name,
                            instance.db_path.display()
                        )
                    }
                );
            }
        }
        Ok(())
    }

    fn validate_service_name(
        &self,
        candidate: &str,
        service: ServiceId,
        instance_name: &str,
        current_name: Option<&str>,
    ) -> Result<()> {
        let valid = candidate.chars().enumerate().all(|(idx, ch)| match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => true,
            '.' | '-' | '_' => idx != 0,
            _ => false,
        }) && !candidate.is_empty()
            && candidate.len() <= 64;
        if !valid {
            bail!(
                "{}",
                self.text(
                    "服务名只能包含字母、数字、点、连字符和下划线，且长度不超过 64",
                    "Service names may only contain letters, digits, dots, hyphens, and underscores, and must be at most 64 characters",
                )
            );
        }

        for instance in self.list_instances()? {
            if instance.service == service && instance.instance_name == instance_name {
                continue;
            }
            if instance.service_name.eq_ignore_ascii_case(candidate) {
                bail!(
                    "{}",
                    if self.uses_chinese() {
                        format!(
                            "服务名与 {}/{} 冲突：{}",
                            instance.service.label(),
                            instance.instance_name,
                            instance.service_name
                        )
                    } else {
                        format!(
                            "Service name conflicts with {}/{}: {}",
                            instance.service.label(),
                            instance.instance_name,
                            instance.service_name
                        )
                    }
                );
            }
        }

        if self.is_registered_by_name(candidate)?
            && current_name
                .map(|value| !value.eq_ignore_ascii_case(candidate))
                .unwrap_or(true)
        {
            bail!(
                "{}",
                if self.uses_chinese() {
                    format!("系统中已经存在同名服务：{}", candidate)
                } else {
                    format!("A service with the same name already exists: {}", candidate)
                }
            );
        }
        Ok(())
    }

    fn installed_service_kinds(&self) -> Result<Vec<ServiceId>> {
        let mut kinds = Vec::new();
        for service in [ServiceId::LanceDb, ServiceId::DuckDb] {
            let has_binary = self.service_binary_path(service).exists();
            let has_instance = self
                .list_instances()?
                .iter()
                .any(|instance| instance.service == service);
            if (has_binary || has_instance) && !kinds.contains(&service) {
                kinds.push(service);
            }
        }
        Ok(kinds)
    }

    fn validate_instance_request(
        &self,
        request: &InstanceRequest,
        current: Option<&InstalledInstance>,
    ) -> Result<()> {
        self.validate_bind_ip(&request.bind_host)?;
        self.validate_instance_name(&request.instance_name)?;
        self.validate_port(
            request.port,
            request.service,
            &request.instance_name,
            current.map(|value| value.port),
            current.map(|value| value.service_name.as_str()),
        )?;
        self.validate_data_path(&request.data_path, request.service, &request.instance_name)?;
        self.validate_service_name(
            &request.service_name,
            request.service,
            &request.instance_name,
            current.map(|value| value.service_name.as_str()),
        )?;
        Ok(())
    }

    pub fn initialize_installation(&mut self, request: InitRequest) -> Result<String> {
        self.validate_data_roots(&request.lancedb_root, &request.duckdb_root)?;
        self.validate_bind_ip(&request.bind_host)?;
        self.validate_distinct_ports(request.lancedb_port, request.duckdb_port)?;
        self.validate_service_name(
            &request.lancedb_service_name,
            ServiceId::LanceDb,
            "default",
            None,
        )?;
        self.validate_service_name(
            &request.duckdb_service_name,
            ServiceId::DuckDb,
            "default",
            None,
        )?;

        self.state.lancedb_root = normalize_path(request.lancedb_root.clone());
        self.state.duckdb_root = normalize_path(request.duckdb_root.clone());
        fs::create_dir_all(&self.state.lancedb_root)?;
        fs::create_dir_all(&self.state.duckdb_root)?;

        let latest = self.fetch_latest_release()?;
        let latest_tag = latest.tag_name.clone();
        self.install_service_binary(ServiceId::LanceDb, &latest)?;
        self.install_service_binary(ServiceId::DuckDb, &latest)?;

        let lance_request = InstanceRequest {
            service: ServiceId::LanceDb,
            instance_name: "default".to_string(),
            bind_host: request.bind_host.clone(),
            port: request.lancedb_port,
            data_path: self.default_instance_data_path(ServiceId::LanceDb, "default"),
            service_name: request.lancedb_service_name.clone(),
        };
        let duck_request = InstanceRequest {
            service: ServiceId::DuckDb,
            instance_name: "default".to_string(),
            bind_host: request.bind_host,
            port: request.duckdb_port,
            data_path: self.default_instance_data_path(ServiceId::DuckDb, "default"),
            service_name: request.duckdb_service_name.clone(),
        };

        self.write_instance_config(&lance_request)?;
        self.write_instance_config(&duck_request)?;
        self.register_instance(ServiceId::LanceDb, "default")?;
        self.register_instance(ServiceId::DuckDb, "default")?;

        self.state.initialized = true;
        self.state.release_tag = Some(latest_tag.clone());
        self.save()?;
        Ok(format!(
            "{} {latest_tag}",
            self.text(
                "首次安装完成，当前 release:",
                "Initialization complete. Current release:",
            )
        ))
    }

    pub fn install_single_instance(&mut self, request: InstanceRequest) -> Result<String> {
        self.validate_instance_request(&request, None)?;
        let latest = if let Some(tag) = self.state.release_tag.as_ref() {
            self.fetch_release(Some(tag))?
        } else {
            let latest = self.fetch_latest_release()?;
            self.state.release_tag = Some(latest.tag_name.clone());
            latest
        };

        self.install_service_binary(request.service, &latest)?;
        self.write_instance_config(&request)?;
        self.register_instance(request.service, &request.instance_name)?;
        self.state.initialized = true;
        self.save()?;
        Ok(format!(
            "{} {} / {}",
            self.text("已安装实例", "Installed instance"),
            request.service.label(),
            request.instance_name
        ))
    }

    pub fn configure_instance(
        &mut self,
        original: &InstalledInstance,
        request: InstanceRequest,
    ) -> Result<String> {
        self.validate_instance_request(&request, Some(original))?;
        let backup_raw = fs::read_to_string(&original.config_path)
            .with_context(|| format!("failed to backup {}", original.config_path.display()))?;

        if request.service_name != original.service_name {
            if original.running {
                self.stop_instance(
                    original.service,
                    &original.instance_name,
                    &original.service_name,
                )?;
            }
            self.write_instance_config(&request)?;
            if let Err(error) = self.unregister_instance(
                original.service,
                &original.instance_name,
                Some(&original.service_name),
            ) {
                fs::write(&original.config_path, &backup_raw).ok();
                return Err(error);
            }
            if let Err(error) = self.register_instance(original.service, &original.instance_name) {
                fs::write(&original.config_path, &backup_raw).ok();
                let _ = self.register_instance(original.service, &original.instance_name);
                return Err(error);
            }
            self.save()?;
            return Ok(self
                .text(
                    "配置已更新，服务注册名已刷新。",
                    "Configuration updated, and the service registration name has been refreshed.",
                )
                .to_string());
        }

        if original.running {
            self.stop_instance(
                original.service,
                &original.instance_name,
                &original.service_name,
            )?;
            if let Err(error) = self.write_instance_config(&request) {
                let _ = self.start_instance(
                    original.service,
                    &original.instance_name,
                    &original.service_name,
                );
                return Err(error);
            }
            if let Err(error) = self.start_instance(
                original.service,
                &original.instance_name,
                &request.service_name,
            ) {
                fs::write(&original.config_path, &backup_raw).ok();
                let _ = self.start_instance(
                    original.service,
                    &original.instance_name,
                    &original.service_name,
                );
                return Err(error);
            }
            self.save()?;
            return Ok(self
                .text(
                    "配置已更新，服务已自动重启。",
                    "Configuration updated, and the service has been restarted automatically.",
                )
                .to_string());
        }

        self.write_instance_config(&request)?;
        if original.registered && !self.is_registered_by_name(&request.service_name)? {
            self.register_instance(original.service, &original.instance_name)?;
        }
        self.save()?;
        Ok(self
            .text(
                "配置已更新，新设置会在下次启动时生效。",
                "Configuration updated. The new settings will take effect on the next start.",
            )
            .to_string())
    }

    pub fn start_registered_instance(&mut self, instance: &InstalledInstance) -> Result<String> {
        self.start_instance(
            instance.service,
            &instance.instance_name,
            &instance.service_name,
        )?;
        Ok(format!(
            "{} {}",
            self.text("已启动", "Started"),
            instance.display_name()
        ))
    }

    pub fn stop_registered_instance(&mut self, instance: &InstalledInstance) -> Result<String> {
        self.stop_instance(
            instance.service,
            &instance.instance_name,
            &instance.service_name,
        )?;
        Ok(format!(
            "{} {}",
            self.text("已停止", "Stopped"),
            instance.display_name()
        ))
    }

    pub fn start_all_instances(&mut self) -> Result<String> {
        let instances = self.list_instances()?;
        for instance in &instances {
            self.start_instance(
                instance.service,
                &instance.instance_name,
                &instance.service_name,
            )?;
        }
        Ok(format!(
            "{} {} {}",
            self.text("已启动", "Started"),
            instances.len(),
            self.text("个实例", "instance(s)")
        ))
    }

    pub fn stop_all_instances(&mut self) -> Result<String> {
        let instances = self.list_instances()?;
        for instance in &instances {
            self.stop_instance(
                instance.service,
                &instance.instance_name,
                &instance.service_name,
            )?;
        }
        Ok(format!(
            "{} {} {}",
            self.text("已停止", "Stopped"),
            instances.len(),
            self.text("个实例", "instance(s)")
        ))
    }

    pub fn uninstall_single_instance(&mut self, instance: &InstalledInstance) -> Result<String> {
        if instance.registered {
            self.unregister_instance(
                instance.service,
                &instance.instance_name,
                Some(&instance.service_name),
            )?;
        }
        fs::remove_file(&instance.config_path).ok();
        if self.instance_config_files()?.is_empty() {
            self.state.initialized = false;
        }
        self.save()?;
        Ok(format!(
            "{} {}{} {}",
            self.text("已卸载实例", "Uninstalled instance"),
            instance.display_name(),
            self.text("，数据库文件已保留在", ". Database files are preserved at"),
            instance.db_path.display()
        ))
    }

    pub fn uninstall_all(&mut self) -> Result<String> {
        let instances = self.list_instances()?;
        for instance in &instances {
            if instance.registered {
                self.unregister_instance(
                    instance.service,
                    &instance.instance_name,
                    Some(&instance.service_name),
                )?;
            }
        }
        self.remove_launcher_only()?;
        if self.paths.install_dir.exists() {
            fs::remove_dir_all(&self.paths.install_dir).ok();
        }
        if self.paths.run_dir.exists() {
            fs::remove_dir_all(&self.paths.run_dir).ok();
        }
        if self.paths.logs_dir.exists() {
            fs::remove_dir_all(&self.paths.logs_dir).ok();
        }
        if self.paths.global_config_path.exists() {
            fs::remove_file(&self.paths.global_config_path).ok();
        }
        self.state.initialized = false;
        Ok(self
            .text(
                "程序文件已移除，数据库目录已保留。",
                "Program files have been removed, and database directories were preserved.",
            )
            .to_string())
    }

    pub fn remove_launcher_only(&self) -> Result<()> {
        if cfg!(windows) {
            for path in [
                self.paths.bin_dir.join("vldb.cmd"),
                self.paths.bin_dir.join("vldb.ps1"),
            ] {
                if path.exists() {
                    fs::remove_file(path).ok();
                }
            }
        } else {
            let launcher = global_launcher_path(&self.paths.install_dir);
            if launcher.exists() {
                fs::remove_file(launcher).ok();
            }
        }
        Ok(())
    }

    fn instance_config_files(&self) -> Result<Vec<PathBuf>> {
        if !self.paths.config_dir.exists() {
            return Ok(Vec::new());
        }
        let mut files = fs::read_dir(&self.paths.config_dir)?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| {
                        (name.starts_with("vldb-lancedb-") || name.starts_with("vldb-duckdb-"))
                            && name.ends_with(".json")
                    })
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        files.sort();
        Ok(files)
    }

    fn read_instance_config(&self, path: &Path) -> Result<InstanceConfig> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: InstanceConfig = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    fn write_instance_config(&self, request: &InstanceRequest) -> Result<()> {
        fs::create_dir_all(&self.paths.config_dir)?;
        let config_path = self.instance_config_path(request.service, &request.instance_name);
        let existing_logging = if config_path.exists() {
            self.read_instance_config(&config_path)
                .ok()
                .map(|config| config.logging)
        } else {
            None
        };

        if request.service == ServiceId::LanceDb {
            fs::create_dir_all(&request.data_path)?;
        } else if let Some(parent) = request.data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut root = serde_json::json!({
            "host": request.bind_host,
            "port": request.port,
            "db_path": request.data_path.display().to_string(),
            "service_name": request.service_name,
            "logging": existing_logging.unwrap_or_else(|| default_logging_value(request.service)),
        });
        if request.service == ServiceId::DuckDb {
            root["memory_limit"] = serde_json::json!("2GB");
            root["threads"] = serde_json::json!(4);
        }

        let json = serde_json::to_string_pretty(&root)?;
        fs::write(config_path, json)?;
        Ok(())
    }

    fn instance_config_path(&self, service: ServiceId, instance_name: &str) -> PathBuf {
        self.paths
            .config_dir
            .join(format!("{}-{}.json", service.label(), instance_name))
    }

    fn service_binary_path(&self, service: ServiceId) -> PathBuf {
        let base = service.label();
        if cfg!(windows) {
            self.paths.bin_dir.join(format!("{base}.exe"))
        } else {
            self.paths.bin_dir.join(base)
        }
    }

    fn fetch_latest_release(&self) -> Result<GitHubRelease> {
        self.fetch_release(None)
    }

    fn fetch_release(&self, tag: Option<&str>) -> Result<GitHubRelease> {
        let url = if let Some(tag) = tag {
            format!("https://api.github.com/repos/{REPO_SLUG}/releases/tags/{tag}")
        } else {
            format!("https://api.github.com/repos/{REPO_SLUG}/releases/latest")
        };
        let response = self.client.get(url).send()?.error_for_status()?;
        let release = response.json::<GitHubRelease>()?;
        Ok(release)
    }

    fn install_service_binary(&self, service: ServiceId, release: &GitHubRelease) -> Result<()> {
        let temp_dir = TempDir::new()?;
        let target = detect_target_triple()?;
        let service_name = service.label();
        let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
        let archive_name = format!("{service_name}-{}-{target}.{extension}", release.tag_name);
        let checksum_name = format!("{archive_name}.sha256");

        let archive_asset = release
            .assets
            .iter()
            .find(|asset| asset.name == archive_name)
            .ok_or_else(|| anyhow!("release does not provide asset {}", archive_name))?;
        let checksum_asset = release
            .assets
            .iter()
            .find(|asset| asset.name == checksum_name)
            .ok_or_else(|| anyhow!("release does not provide checksum {}", checksum_name))?;

        let archive_path = temp_dir.path().join(&archive_name);
        let checksum_path = temp_dir.path().join(&checksum_name);

        self.download_to_path(&archive_asset.browser_download_url, &archive_path)?;
        self.download_to_path(&checksum_asset.browser_download_url, &checksum_path)?;
        verify_sha256(&archive_path, &checksum_path)?;

        fs::create_dir_all(&self.paths.bin_dir)?;
        fs::create_dir_all(&self.paths.examples_dir)?;

        if cfg!(windows) {
            extract_zip_service_archive(
                &archive_path,
                service_name,
                &self.service_binary_path(service),
                &self
                    .paths
                    .examples_dir
                    .join(format!("{service_name}.json.example")),
            )?;
        } else {
            extract_tar_service_archive(
                &archive_path,
                service_name,
                &self.service_binary_path(service),
                &self
                    .paths
                    .examples_dir
                    .join(format!("{service_name}.json.example")),
            )?;
        }
        Ok(())
    }

    fn download_to_path(&self, url: &str, out_path: &Path) -> Result<()> {
        let mut response = self.client.get(url).send()?.error_for_status()?;
        let mut file = fs::File::create(out_path)?;
        io::copy(&mut response, &mut file)?;
        Ok(())
    }

    fn winsw_template_path(&self) -> PathBuf {
        self.paths
            .tools_dir
            .join("winsw")
            .join("winsw-template.exe")
    }

    fn ensure_windows_service_builder(&self) -> Result<PathBuf> {
        let template = self.winsw_template_path();
        if template.exists() {
            return Ok(template);
        }
        if !cfg!(windows) {
            bail!(
                "{}",
                self.text(
                    "WinSW 仅可在 Windows 上使用",
                    "WinSW is only available on Windows"
                )
            );
        }
        if env::consts::ARCH != "x86_64" {
            bail!(
                "{}",
                self.text(
                    "内置 WinSW 引导目前只支持 x64 Windows",
                    "The bundled WinSW bootstrap currently supports only x64 Windows",
                )
            );
        }
        let temp_dir = TempDir::new()?;
        let download_path = temp_dir.path().join("WinSW-x64.exe");
        let url = format!(
            "https://github.com/winsw/winsw/releases/download/{}/WinSW-x64.exe",
            WINSW_VERSION
        );
        self.download_to_path(&url, &download_path)?;
        if let Some(parent) = template.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(download_path, &template)?;
        Ok(template)
    }

    fn register_instance(&self, service: ServiceId, instance_name: &str) -> Result<()> {
        if cfg!(windows) {
            self.register_windows_service(service, instance_name)
        } else if cfg!(target_os = "macos") {
            self.register_launchd_service(service, instance_name)
        } else {
            self.register_systemd_service(service, instance_name)
        }
    }

    fn unregister_instance(
        &self,
        service: ServiceId,
        instance_name: &str,
        registered_name_override: Option<&str>,
    ) -> Result<()> {
        if cfg!(windows) {
            self.unregister_windows_service(service, instance_name, registered_name_override)
        } else if cfg!(target_os = "macos") {
            self.unregister_launchd_service(service, instance_name, registered_name_override)
        } else {
            self.unregister_systemd_service(service, instance_name, registered_name_override)
        }
    }

    fn start_instance(
        &self,
        service: ServiceId,
        instance_name: &str,
        service_name: &str,
    ) -> Result<()> {
        if !self.is_registered_by_name(service_name)? {
            self.register_instance(service, instance_name)?;
            return Ok(());
        }
        if cfg!(windows) {
            run_powershell(&format!(
                "Start-Service -Name '{name}' -ErrorAction SilentlyContinue; \
                 $svc = Get-Service -Name '{name}' -ErrorAction Stop; \
                 $svc.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(20))",
                name = ps_literal(service_name),
            ))?;
        } else if cfg!(target_os = "macos") {
            run_command(
                "launchctl",
                &[
                    "load",
                    "-w",
                    self.launchd_plist_path(service_name)
                        .to_string_lossy()
                        .as_ref(),
                ],
            )?;
        } else if is_root_user()? {
            run_command("systemctl", &["start", &format!("{service_name}.service")])?;
        } else {
            run_command(
                "systemctl",
                &["--user", "start", &format!("{service_name}.service")],
            )?;
        }
        Ok(())
    }

    fn stop_instance(
        &self,
        _service: ServiceId,
        _instance_name: &str,
        service_name: &str,
    ) -> Result<()> {
        if !self.is_registered_by_name(service_name)? {
            return Ok(());
        }
        if cfg!(windows) {
            run_powershell(&format!(
                "Stop-Service -Name '{name}' -Force -ErrorAction SilentlyContinue; \
                 $svc = Get-Service -Name '{name}' -ErrorAction Stop; \
                 $svc.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(20))",
                name = ps_literal(service_name),
            ))?;
        } else if cfg!(target_os = "macos") {
            run_command(
                "launchctl",
                &[
                    "unload",
                    "-w",
                    self.launchd_plist_path(service_name)
                        .to_string_lossy()
                        .as_ref(),
                ],
            )?;
        } else if is_root_user()? {
            run_command("systemctl", &["stop", &format!("{service_name}.service")])?;
        } else {
            run_command(
                "systemctl",
                &["--user", "stop", &format!("{service_name}.service")],
            )?;
        }
        Ok(())
    }

    fn is_registered_by_name(&self, service_name: &str) -> Result<bool> {
        if cfg!(windows) {
            let output = Command::new("sc.exe")
                .args(["query", service_name])
                .output()
                .context("failed to query Windows service")?;
            return Ok(output.status.success());
        }
        if cfg!(target_os = "macos") {
            return Ok(self.launchd_plist_path(service_name).exists());
        }
        let path = self.systemd_unit_path(service_name)?;
        Ok(path.exists())
    }

    fn is_running_by_name(&self, service_name: &str) -> Result<bool> {
        if cfg!(windows) {
            let output = run_powershell_capture(&format!(
                "$svc = Get-Service -Name '{name}' -ErrorAction SilentlyContinue; \
                 if ($svc -and $svc.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running) {{ 'running' }}",
                name = ps_literal(service_name),
            ))?;
            return Ok(output.trim() == "running");
        }
        if cfg!(target_os = "macos") {
            let domain = if is_root_user()? {
                "system".to_string()
            } else {
                format!("gui/{}", current_uid()?)
            };
            let output = Command::new("launchctl")
                .args(["print", &format!("{domain}/{service_name}")])
                .output()
                .context("failed to query launchd service")?;
            return Ok(output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("pid = "));
        }
        let service_unit = format!("{service_name}.service");
        let args = if is_root_user()? {
            vec!["is-active", "--quiet", service_unit.as_str()]
        } else {
            vec!["--user", "is-active", "--quiet", service_unit.as_str()]
        };
        Ok(Command::new("systemctl").args(args).status()?.success())
    }

    fn systemd_unit_path(&self, service_name: &str) -> Result<PathBuf> {
        if is_root_user()? {
            Ok(PathBuf::from(format!(
                "/etc/systemd/system/{service_name}.service"
            )))
        } else {
            let home =
                home_dir().ok_or_else(|| anyhow!("failed to resolve user home directory"))?;
            Ok(home
                .join(".config")
                .join("systemd")
                .join("user")
                .join(format!("{service_name}.service")))
        }
    }

    fn launchd_plist_path(&self, service_name: &str) -> PathBuf {
        if cfg!(target_os = "macos") && current_uid().unwrap_or(1) == 0 {
            PathBuf::from(format!("/Library/LaunchDaemons/{service_name}.plist"))
        } else {
            home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Library")
                .join("LaunchAgents")
                .join(format!("{service_name}.plist"))
        }
    }

    fn service_registration_name(&self, service: ServiceId, instance_name: &str) -> Result<String> {
        let config_path = self.instance_config_path(service, instance_name);
        let config = self.read_instance_config(&config_path)?;
        Ok(config
            .service_name
            .unwrap_or_else(|| legacy_service_name(service, instance_name)))
    }

    fn windows_wrapper_dir(&self, service: ServiceId, instance_name: &str) -> PathBuf {
        self.paths
            .run_dir
            .join("services")
            .join(format!("{}-{}", service.label(), instance_name))
    }

    fn register_windows_service(&self, service: ServiceId, instance_name: &str) -> Result<()> {
        let registered_name = self.service_registration_name(service, instance_name)?;
        self.remove_legacy_startup_task(&registered_name)?;
        self.remove_windows_service_by_name(&registered_name)?;
        let wrapper_exe = self.write_windows_wrapper(service, instance_name, &registered_name)?;
        run_command(wrapper_exe.to_string_lossy().as_ref(), &["install"])?;
        run_command(wrapper_exe.to_string_lossy().as_ref(), &["start"])?;
        Ok(())
    }

    fn unregister_windows_service(
        &self,
        service: ServiceId,
        instance_name: &str,
        registered_name_override: Option<&str>,
    ) -> Result<()> {
        let registered_name = if let Some(value) = registered_name_override {
            value.to_string()
        } else {
            self.service_registration_name(service, instance_name)?
        };
        self.remove_legacy_startup_task(&registered_name)?;
        self.remove_windows_service_by_name(&registered_name)?;
        let wrapper_dir = self.windows_wrapper_dir(service, instance_name);
        if wrapper_dir.exists() {
            fs::remove_dir_all(wrapper_dir).ok();
        }
        Ok(())
    }

    fn write_windows_wrapper(
        &self,
        service: ServiceId,
        instance_name: &str,
        registered_name: &str,
    ) -> Result<PathBuf> {
        let wrapper_dir = self.windows_wrapper_dir(service, instance_name);
        let wrapper_exe = wrapper_dir.join(format!("{}-{}.exe", service.label(), instance_name));
        let wrapper_xml = wrapper_dir.join(format!("{}-{}.xml", service.label(), instance_name));
        let binary_path = self.service_binary_path(service);
        let json_config = self.instance_config_path(service, instance_name);
        let log_dir = self
            .paths
            .logs_dir
            .join(format!("{}-{}", service.label(), instance_name));

        fs::create_dir_all(&wrapper_dir)?;
        fs::create_dir_all(&log_dir)?;
        fs::copy(self.ensure_windows_service_builder()?, &wrapper_exe)?;

        let xml = format!(
            "<service>\n  <id>{name}</id>\n  <name>{name}</name>\n  <description>{name}</description>\n  <executable>{binary}</executable>\n  <arguments>--config &quot;{config}&quot;</arguments>\n  <workingdirectory>{workdir}</workingdirectory>\n  <startmode>Automatic</startmode>\n  <stoptimeout>15 sec</stoptimeout>\n  <onfailure action=\"restart\" delay=\"10 sec\" />\n  <onfailure action=\"restart\" delay=\"10 sec\" />\n  <onfailure action=\"restart\" delay=\"30 sec\" />\n  <logpath>{logdir}</logpath>\n  <log mode=\"roll\" />\n</service>\n",
            name = xml_escape(registered_name),
            binary = xml_escape(&binary_path.display().to_string()),
            config = xml_escape(&json_config.display().to_string()),
            workdir = xml_escape(&self.paths.install_dir.display().to_string()),
            logdir = xml_escape(&log_dir.display().to_string()),
        );
        fs::write(&wrapper_xml, xml)?;
        Ok(wrapper_exe)
    }

    fn remove_legacy_startup_task(&self, registered_name: &str) -> Result<()> {
        if cfg!(windows) {
            let _ = run_powershell(&format!(
                "Unregister-ScheduledTask -TaskName '{name}' -Confirm:$false -ErrorAction SilentlyContinue",
                name = ps_literal(registered_name)
            ));
        }
        Ok(())
    }

    fn remove_windows_service_by_name(&self, registered_name: &str) -> Result<()> {
        if !cfg!(windows) {
            return Ok(());
        }
        let _ = run_powershell(&format!(
            "$svc = Get-Service -Name '{name}' -ErrorAction SilentlyContinue; \
             if ($svc) {{ if ($svc.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {{ Stop-Service -Name '{name}' -Force -ErrorAction SilentlyContinue; try {{ $svc.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(20)) }} catch {{ }} }} }}",
            name = ps_literal(registered_name),
        ));
        let output = Command::new("sc.exe")
            .args(["delete", registered_name])
            .output()
            .context("failed to delete Windows service")?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() && !stderr.to_lowercase().contains("does not exist") {
            bail!(
                "failed to delete Windows service {}: {}",
                registered_name,
                stderr
            );
        }
        Ok(())
    }

    fn register_systemd_service(&self, service: ServiceId, instance_name: &str) -> Result<()> {
        let registered_name = self.service_registration_name(service, instance_name)?;
        let runner_file = self.write_runner_script(service, instance_name)?;
        let unit_path = self.systemd_unit_path(&registered_name)?;
        if let Some(parent) = unit_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let wanted_by = if is_root_user()? {
            "multi-user.target"
        } else {
            "default.target"
        };
        let unit = format!(
            "[Unit]\nDescription={name}\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={runner}\nRestart=always\nRestartSec=3\nWorkingDirectory={workdir}\n\n[Install]\nWantedBy={wanted_by}\n",
            name = registered_name,
            runner = runner_file.display(),
            workdir = self.paths.install_dir.display(),
        );
        fs::write(&unit_path, unit)?;
        let daemon_args = if is_root_user()? {
            vec!["daemon-reload"]
        } else {
            vec!["--user", "daemon-reload"]
        };
        run_command("systemctl", &daemon_args)?;
        let service_unit = format!("{registered_name}.service");
        let enable_args = if is_root_user()? {
            vec!["enable", "--now", service_unit.as_str()]
        } else {
            vec!["--user", "enable", "--now", service_unit.as_str()]
        };
        run_command("systemctl", &enable_args)?;
        Ok(())
    }

    fn unregister_systemd_service(
        &self,
        service: ServiceId,
        instance_name: &str,
        registered_name_override: Option<&str>,
    ) -> Result<()> {
        let registered_name = if let Some(value) = registered_name_override {
            value.to_string()
        } else {
            self.service_registration_name(service, instance_name)?
        };
        let unit_path = self.systemd_unit_path(&registered_name)?;
        let service_unit = format!("{registered_name}.service");
        let disable_args = if is_root_user()? {
            vec!["disable", "--now", service_unit.as_str()]
        } else {
            vec!["--user", "disable", "--now", service_unit.as_str()]
        };
        let _ = run_command("systemctl", &disable_args);
        if unit_path.exists() {
            fs::remove_file(unit_path).ok();
        }
        let daemon_args = if is_root_user()? {
            vec!["daemon-reload"]
        } else {
            vec!["--user", "daemon-reload"]
        };
        let _ = run_command("systemctl", &daemon_args);
        let runner = self.runner_path(service, instance_name);
        if runner.exists() {
            fs::remove_file(runner).ok();
        }
        Ok(())
    }

    fn register_launchd_service(&self, service: ServiceId, instance_name: &str) -> Result<()> {
        let registered_name = self.service_registration_name(service, instance_name)?;
        let runner_file = self.write_runner_script(service, instance_name)?;
        let plist_path = self.launchd_plist_path(&registered_name);
        if let Some(parent) = plist_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n  <dict>\n    <key>Label</key>\n    <string>{name}</string>\n    <key>ProgramArguments</key>\n    <array>\n      <string>{runner}</string>\n    </array>\n    <key>RunAtLoad</key>\n    <true/>\n    <key>KeepAlive</key>\n    <true/>\n    <key>WorkingDirectory</key>\n    <string>{workdir}</string>\n  </dict>\n</plist>\n",
            name = xml_escape(&registered_name),
            runner = xml_escape(&runner_file.display().to_string()),
            workdir = xml_escape(&self.paths.install_dir.display().to_string()),
        );
        fs::write(&plist_path, plist)?;
        let _ = run_command(
            "launchctl",
            &["unload", "-w", plist_path.to_string_lossy().as_ref()],
        );
        run_command(
            "launchctl",
            &["load", "-w", plist_path.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    fn unregister_launchd_service(
        &self,
        service: ServiceId,
        instance_name: &str,
        registered_name_override: Option<&str>,
    ) -> Result<()> {
        let registered_name = if let Some(value) = registered_name_override {
            value.to_string()
        } else {
            self.service_registration_name(service, instance_name)?
        };
        let plist_path = self.launchd_plist_path(&registered_name);
        let _ = run_command(
            "launchctl",
            &["unload", "-w", plist_path.to_string_lossy().as_ref()],
        );
        if plist_path.exists() {
            fs::remove_file(plist_path).ok();
        }
        let runner = self.runner_path(service, instance_name);
        if runner.exists() {
            fs::remove_file(runner).ok();
        }
        Ok(())
    }

    fn runner_path(&self, service: ServiceId, instance_name: &str) -> PathBuf {
        self.paths
            .run_dir
            .join(format!("{}-{}.sh", service.label(), instance_name))
    }

    fn write_runner_script(&self, service: ServiceId, instance_name: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.paths.run_dir)?;
        let runner = self.runner_path(service, instance_name);
        let binary = self.service_binary_path(service);
        let config = self.instance_config_path(service, instance_name);
        let script = format!(
            "#!/usr/bin/env bash\nexec \"{}\" --config \"{}\"\n",
            binary.display(),
            config.display()
        );
        fs::write(&runner, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&runner)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&runner, perms)?;
        }
        Ok(runner)
    }
}

fn default_logging_value(service: ServiceId) -> serde_json::Value {
    match service {
        ServiceId::LanceDb => serde_json::json!({
            "enabled": true,
            "file_enabled": true,
            "stderr_enabled": true,
            "request_log_enabled": true,
            "slow_request_log_enabled": true,
            "slow_request_threshold_ms": 1000,
            "include_request_details_in_slow_log": true,
            "request_preview_chars": 160,
            "log_dir": "",
            "log_file_name": "vldb-lancedb.log"
        }),
        ServiceId::DuckDb => serde_json::json!({
            "enabled": true,
            "file_enabled": true,
            "stderr_enabled": true,
            "request_log_enabled": true,
            "slow_query_log_enabled": true,
            "slow_query_threshold_ms": 1000,
            "slow_query_full_sql_enabled": true,
            "sql_preview_chars": 160,
            "log_dir": "",
            "log_file_name": "vldb-duckdb.log"
        }),
    }
}

fn verify_sha256(archive_path: &Path, checksum_path: &Path) -> Result<()> {
    let checksum_text = fs::read_to_string(checksum_path)?;
    let expected = checksum_text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("invalid checksum file"))?
        .to_lowercase();
    let mut file = fs::File::open(archive_path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        bail!("checksum verification failed");
    }
    Ok(())
}

fn extract_zip_service_archive(
    archive_path: &Path,
    service_name: &str,
    binary_out: &Path,
    example_out: &Path,
) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut binary_bytes = None;
    let mut example_bytes = None;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with(&format!("{service_name}.exe")) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            binary_bytes = Some(bytes);
        } else if name.ends_with(&format!("{service_name}.json.example")) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            example_bytes = Some(bytes);
        }
    }

    let binary_bytes =
        binary_bytes.ok_or_else(|| anyhow!("missing {}.exe in archive", service_name))?;
    let example_bytes =
        example_bytes.ok_or_else(|| anyhow!("missing {}.json.example in archive", service_name))?;
    fs::write(binary_out, binary_bytes)?;
    fs::write(example_out, example_bytes)?;
    Ok(())
}

fn extract_tar_service_archive(
    archive_path: &Path,
    service_name: &str,
    binary_out: &Path,
    example_out: &Path,
) -> Result<()> {
    let tar_gz = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(decoder);
    let mut binary_bytes = None;
    let mut example_bytes = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        if path.ends_with(&format!("/{service_name}")) || path == service_name {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            binary_bytes = Some(bytes);
        } else if path.ends_with(&format!("/{service_name}.json.example"))
            || path == format!("{service_name}.json.example")
        {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            example_bytes = Some(bytes);
        }
    }

    let binary_bytes =
        binary_bytes.ok_or_else(|| anyhow!("missing {} in archive", service_name))?;
    let example_bytes =
        example_bytes.ok_or_else(|| anyhow!("missing {}.json.example in archive", service_name))?;
    fs::write(binary_out, binary_bytes)?;
    fs::write(example_out, example_bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(binary_out)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(binary_out, perms)?;
    }
    Ok(())
}

fn global_launcher_path(install_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        install_dir.join("bin").join("vldb")
    } else if is_root_user().unwrap_or(false) {
        PathBuf::from("/usr/local/bin/vldb")
    } else {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("bin")
            .join("vldb")
    }
}

fn detect_target_triple() -> Result<String> {
    let target = match (env::consts::OS, env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        (os, arch) => bail!("unsupported platform {os}/{arch}"),
    };
    Ok(target.to_string())
}

fn legacy_service_name(service: ServiceId, instance_name: &str) -> String {
    format!("{}-{}", service.label(), instance_name)
}

fn parse_instance_meta(path: &Path) -> Result<(ServiceId, String)> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("invalid config file name"))?;
    if let Some(value) = stem.strip_prefix("vldb-lancedb-") {
        return Ok((ServiceId::LanceDb, value.to_string()));
    }
    if let Some(value) = stem.strip_prefix("vldb-duckdb-") {
        return Ok((ServiceId::DuckDb, value.to_string()));
    }
    bail!("unsupported instance config file {}", path.display())
}

fn parse_ipv4(candidate: &str) -> Option<[u8; 4]> {
    let parts = candidate.split('.').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let mut values = [0u8; 4];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return None;
        }
        let parsed = part.parse::<u8>().ok()?;
        values[index] = parsed;
    }
    Some(values)
}

fn test_port_available(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

fn string_option(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn version_compare(left: &str, right: &str) -> Ordering {
    let normalize = |value: &str| -> Vec<u64> {
        value
            .trim_start_matches('v')
            .split('.')
            .map(|part| {
                part.chars()
                    .filter(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect()
    };

    let left = normalize(left);
    let right = normalize(right);
    let count = left.len().max(right.len());
    for index in 0..count {
        let l = *left.get(index).unwrap_or(&0);
        let r = *right.get(index).unwrap_or(&0);
        match l.cmp(&r) {
            Ordering::Equal => {}
            value => return value,
        }
    }
    Ordering::Equal
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    let left = normalize_path(left.to_path_buf());
    let right = normalize_path(right.to_path_buf());
    left == right || left.starts_with(&right) || right.starts_with(&left)
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

fn current_uid() -> Result<u32> {
    #[cfg(unix)]
    {
        return Ok(libc_geteuid() as u32);
    }
    #[cfg(not(unix))]
    {
        Ok(1)
    }
}

#[cfg(unix)]
fn libc_geteuid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

fn is_root_user() -> Result<bool> {
    if cfg!(windows) {
        return Ok(false);
    }
    Ok(current_uid()? == 0)
}

fn run_command(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to start command: {} {}", program, args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "command failed: {} {} (exit {})\n{}",
            program,
            args.join(" "),
            format_exit_code(output.status.code()),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn run_powershell(script: &str) -> Result<()> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", script])
        .output()
        .context("failed to start PowerShell")?;
    if !output.status.success() {
        bail!(
            "PowerShell command failed (exit {})\n{}",
            format_exit_code(output.status.code()),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn run_powershell_capture(script: &str) -> Result<String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", script])
        .output()
        .context("failed to start PowerShell")?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ps_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
