use clap::Parser;
use std::sync::Arc;

use crate::{
    handler::{QueryHandler, Resolver, UpstreamConfig, UpstreamPool},
    server::{Server, ServerConfig},
};

mod handler;
mod server;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "0.0.0.0:53")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let upstream = UpstreamPool::new(UpstreamConfig::default());
    let resolver = Resolver::new(upstream);
    let handler = Arc::new(QueryHandler::new(resolver));

    let mut config = ServerConfig::default();
    config.bind_addr = cli.bind.parse()?;

    let server = Server::new(config, handler);

    println!("Starting server on {}", cli.bind);

    if let Err(e) = server.run().await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
