mod cache;
mod handler;
mod server;

use clap::Parser;
use deadpool_redis::{Config, Runtime};
use ftlog::{error, info};
use std::sync::Arc;

use crate::{
    cache::{Cache, RedisConfig},
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
    if cfg!(debug_assertions) {
        let _guard = ftlog::builder()
            .max_log_level(ftlog::LevelFilter::Trace)
            // .max_log_level(ftlog::LevelFilter::Error)
            .try_init()
            .unwrap();
    }

    let def = RedisConfig::default();
    let conf = Config::from_url(&def.url);
    let pool = conf.create_pool(Some(Runtime::Tokio1))?;

    let cache = Arc::new(Cache::new(pool.clone()));
    let upstream = UpstreamPool::new(UpstreamConfig::default());
    let resolver = Resolver::new(upstream);
    let handler = Arc::new(QueryHandler::new(cache.clone(), resolver));

    // todo: get rid of me
    cache.add_block_domain("ads.google.com").await?;
    cache.add_block_domain("doubleclick.net").await?;
    cache.add_block_domain("tracking.facebook.com").await?;
    cache.add_block_domain("analytics.google.com").await?;
    cache.add_block_domain("ad.doubleclick.net").await?;

    let mut config = ServerConfig::default();
    config.bind_addr = cli.bind.parse()?;
    let dns_server = Server::new(config, handler);
    // axum
    if cfg!(debug_assertions) {
        info!("Starting dns server on {}", cli.bind);
    }
    if let Err(e) = dns_server.run().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
