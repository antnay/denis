mod cache;
mod handler;
mod server;

use clap::Parser;
use deadpool_redis::{Config, Runtime};
use ftlog::{error, info};
use std::sync::Arc;

use crate::{
    cache::{Blocklist, Cache, RedisConfig},
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

    let def = RedisConfig::default();
    let conf = Config::from_url(&def.url);
    let pool = conf.create_pool(Some(Runtime::Tokio1))?;

    let blocklist = Arc::new(Blocklist::new(pool.clone()));
    let cache = Arc::new(Cache::new(pool.clone()));
    let upstream = UpstreamPool::new(UpstreamConfig::default());
    let resolver = Resolver::new(blocklist.clone(), cache.clone(), upstream);
    let handler = Arc::new(QueryHandler::new(resolver));

    // todo: get rid of me
    blocklist.add_block_domain("ads.google.com").await?;
    blocklist.add_block_domain("doubleclick.net").await?;
    blocklist.add_block_domain("tracking.facebook.com").await?;
    blocklist.add_block_domain("analytics.google.com").await?;
    blocklist.add_block_domain("ad.doubleclick.net").await?;

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
