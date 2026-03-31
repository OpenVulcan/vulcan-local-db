mod config;
mod logging;
mod service;

pub mod pb {
    tonic::include_proto!("vldb.lancedb.v1");
}

use config::{BoxError, load};
use logging::ServiceLogger;
use service::LanceDbGrpcService;
use tokio::net::lookup_host;
use tonic::transport::Server;

use crate::pb::lance_db_service_server::LanceDbServiceServer;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let cfg = load()?;
    let logger = ServiceLogger::new("vldb-lancedb", &cfg.logging)?;

    if cfg.is_local_db_path() {
        tokio::fs::create_dir_all(&cfg.db_path).await?;
    }

    if let Some(warning) = cfg.concurrent_write_warning() {
        eprintln!("warning: {warning}");
        logger.log("warning", warning);
    }

    let mut db_builder = lancedb::connect(&cfg.db_path);
    if let Some(interval) = cfg.read_consistency_interval() {
        db_builder = db_builder.read_consistency_interval(interval);
    }
    let db = db_builder.execute().await?;

    let bind_text = format!("{}:{}", cfg.host, cfg.port);
    let mut addrs = lookup_host(bind_text.as_str()).await?;
    let addr = addrs
        .next()
        .ok_or_else(|| format!("failed to resolve bind address: {bind_text}"))?;

    if let Some(source) = cfg.source.as_ref() {
        println!("using config file: {}", source.display());
    } else {
        println!("using default config");
    }
    println!("lancedb uri: {}", cfg.db_path);
    match cfg.read_consistency_interval_ms {
        Some(0) => println!("lancedb read consistency interval: 0 ms (strong)"),
        Some(ms) => println!("lancedb read consistency interval: {} ms", ms),
        None => println!("lancedb read consistency interval: disabled"),
    }
    if let Some(log_path) = logger.log_path() {
        println!("request log file: {}", log_path.display());
    } else if cfg.logging.enabled {
        println!("request log file: disabled");
    }
    println!("grpc listen: {}", addr);

    Server::builder()
        .add_service(LanceDbServiceServer::new(LanceDbGrpcService::new(
            db, logger,
        )))
        .serve(addr)
        .await?;

    Ok(())
}
