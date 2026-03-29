mod config;
mod service;

pub mod pb {
    tonic::include_proto!("vldb.duckdb.v1");
}

use crate::config::{BoxError, load_config};
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

    let conn = Connection::open(&config.db_path)?;
    apply_connection_pragmas(&conn, &config)?;

    let addr = format!("{}:{}", config.host, config.port).parse()?;

    println!("duckdb database: {}", config.db_path.display());
    println!(
        "memory_limit: {} | threads: {}",
        config.memory_limit, config.threads
    );
    println!("gRPC listening on {addr}");

    let svc = DuckDbGrpcService::new(conn, config.clone());

    Server::builder()
        .add_service(DuckDbServiceServer::new(svc))
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        println!("shutdown signal received, stopping vldb-duckdb");
    }
}
