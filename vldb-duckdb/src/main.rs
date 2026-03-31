mod config;
mod db_lock;
mod logging;
mod service;

pub mod pb {
    tonic::include_proto!("vldb.duckdb.v1");
}

use crate::config::{BoxError, load_config};
use crate::db_lock::DatabaseFileLock;
use crate::logging::ServiceLogger;
use crate::pb::duck_db_service_server::DuckDbServiceServer;
use crate::service::{DuckDbGrpcService, apply_connection_pragmas};
use duckdb::Connection;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let loaded = load_config()?;
    let config = loaded.config;

    if let Some(source) = &loaded.source {
        println!("loaded config from {}", source.display());
    } else {
        println!(
            "no config file found; using defaults (or provide -config <path> / place vldb-duckdb.json in the working directory or executable directory)"
        );
    }

    if let Some(parent) = config.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let db_file_lock = if config.hardening.enforce_db_file_lock
        && config.db_path.to_string_lossy() != ":memory:"
    {
        Some(DatabaseFileLock::acquire(&config.db_path)?)
    } else {
        None
    };

    let logger = ServiceLogger::new("vldb-duckdb", &config.logging)?;

    let conn = Connection::open(&config.db_path)?;
    apply_connection_pragmas(&conn, &config)?;

    let addr = format!("{}:{}", config.host, config.port).parse()?;

    println!("duckdb database: {}", config.db_path.display());
    println!(
        "memory_limit: {} | threads: {}",
        config.memory_limit, config.threads
    );
    println!(
        "hardening: db_file_lock={} external_access={} lock_configuration={} checkpoint_on_shutdown={} allowed_directories={} allowed_paths={} autoload_known_extensions={} autoinstall_known_extensions={} allow_community_extensions={}",
        if config.hardening.enforce_db_file_lock {
            "enabled"
        } else {
            "disabled"
        },
        if config.hardening.enable_external_access {
            "enabled"
        } else {
            "disabled"
        },
        if config.hardening.lock_configuration {
            "enabled"
        } else {
            "disabled"
        },
        if config.hardening.checkpoint_on_shutdown {
            "enabled"
        } else {
            "disabled"
        },
        config.hardening.allowed_directories.len(),
        config.hardening.allowed_paths.len(),
        if config.hardening.autoload_known_extensions {
            "enabled"
        } else {
            "disabled"
        },
        if config.hardening.autoinstall_known_extensions {
            "enabled"
        } else {
            "disabled"
        },
        if config.hardening.allow_community_extensions {
            "enabled"
        } else {
            "disabled"
        },
    );
    if let Some(db_file_lock) = &db_file_lock {
        println!("database lock file: {}", db_file_lock.path().display());
    }
    if config.hardening.enable_external_access {
        eprintln!(
            "warning: hardening.enable_external_access=true allows SQL to read/write external files and ATTACH other databases"
        );
    }
    if let Some(log_path) = logger.log_path() {
        println!("request log file: {}", log_path.display());
    } else if config.logging.enabled {
        println!("request log file: disabled");
    }
    println!("gRPC listening on {addr}");

    let svc = DuckDbGrpcService::new(conn, logger, config.clone());

    Server::builder()
        .add_service(DuckDbServiceServer::new(svc))
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    drop(db_file_lock);

    Ok(())
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        println!("shutdown signal received, stopping vldb-duckdb");
    }
}
