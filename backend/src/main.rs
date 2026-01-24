mod cache;
mod handler;
mod server;

use clap::Parser;
use ftlog::{error, info};
use std::sync::Arc;

use crate::{
    cache::{RdsCache, RedisCacheConfig},
    handler::{QueryHandler, Resolver, UpstreamConfig, UpstreamPool},
    server::{Server, ServerConfig},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "0.0.0.0:53")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let _guard = ftlog::builder()
        .max_log_level(ftlog::LevelFilter::Trace)
        .try_init()
        .unwrap();

    let cache = RdsCache::new(RedisCacheConfig::default())?;
    let upstream = UpstreamPool::new(UpstreamConfig::default());
    let resolver = Resolver::new(cache, upstream);
    let handler = Arc::new(QueryHandler::new(resolver));

    let mut config = ServerConfig::default();
    config.bind_addr = cli.bind.parse()?;

    let server = Server::new(config, handler);

    info!("Starting server on {}", cli.bind);

    if let Err(e) = server.run().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
