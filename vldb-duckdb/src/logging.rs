use crate::config::{BoxError, LoggingConfig};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct ServiceLogger {
    service_name: &'static str,
    config: LoggingConfig,
    file: Option<Mutex<File>>,
    log_path: Option<PathBuf>,
}

impl ServiceLogger {
    pub fn new(service_name: &'static str, config: &LoggingConfig) -> Result<Arc<Self>, BoxError> {
        if !config.enabled {
            return Ok(Arc::new(Self {
                service_name,
                config: config.clone(),
                file: None,
                log_path: None,
            }));
        }

        let (file, log_path) = if config.file_enabled {
            fs::create_dir_all(&config.log_dir)?;
            let log_path = config.log_dir.join(&config.log_file_name);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?;
            (Some(Mutex::new(file)), Some(log_path))
        } else {
            (None, None)
        };

        Ok(Arc::new(Self {
            service_name,
            config: config.clone(),
            file,
            log_path,
        }))
    }

    pub fn config(&self) -> &LoggingConfig {
        &self.config
    }

    pub fn log_path(&self) -> Option<&PathBuf> {
        self.log_path.as_ref()
    }

    pub fn log(&self, category: &str, message: impl AsRef<str>) {
        if !self.config.enabled {
            return;
        }

        let line = format!(
            "[{}][{}][{}] {}",
            unix_millis_timestamp(),
            self.service_name,
            category,
            message.as_ref()
        );

        if self.config.stderr_enabled {
            eprintln!("{line}");
        }

        if let Some(file) = &self.file
            && let Ok(mut guard) = file.lock()
        {
            let _ = writeln!(guard, "{line}");
        }
    }
}

fn unix_millis_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}.{}",
        now.as_secs(),
        format!("{:03}", now.subsec_millis())
    )
}
