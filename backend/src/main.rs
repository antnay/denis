mod analytics;
mod api;
mod dns;
mod handler;
mod repo;

use clap::Parser;
use ftlog::{error, info};
use redis::Client;
use sqlx::postgres;
use std::sync::Arc;

use crate::{
    analytics::{AnalyticsConsumer, AnalyticsProducer, StatsClient},
    api::ApiState,
    dns::Server,
    handler::{QueryHandler, UpstreamConfig, UpstreamPool},
    repo::{Cache, PGConfig, RedisConfig},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "0.0.0.0:53")]
    bind: String,

    #[arg(long, default_value = "0.0.0.0:8080")]
    api_bind: String,

    #[arg(long, default_value = "localhost:9092")]
    kafka_brokers: String,

    #[arg(long, default_value = "http://localhost:8123")]
    clickhouse_url: String,

    #[arg(long, default_value = "default")]
    clickhouse_user: String,

    #[arg(long, default_value = "clickhouse")]
    clickhouse_password: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if cfg!(debug_assertions) {
        let _guard = ftlog::builder()
            .max_log_level(ftlog::LevelFilter::Trace)
            .try_init()
            .unwrap();
    }

    let rds_config = RedisConfig::default();
    let rds_conn = Client::open(rds_config.url)
        .expect("Cannot connect to redis")
        .get_multiplexed_async_connection()
        .await?;

    let pg_config = PGConfig::default();
    let pg_pool = postgres::PgPoolOptions::new()
        .max_connections(pg_config.max_connections)
        .idle_timeout(pg_config.idle_timeout)
        .connect(&pg_config.url)
        .await
        .expect("Cannot connect to postgres");

    let _ = sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS allowed (
            id SERIAL PRIMARY KEY,
            domain VARCHAR(253) NOT NULL UNIQUE
        )
        ",
    )
    .execute(&pg_pool)
    .await
    .expect("failed to create allowed table");

    let _ = sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS blocked (
            id SERIAL PRIMARY KEY,
            list VARCHAR(50),
            domain VARCHAR(253) NOT NULL UNIQUE
        )
        ",
    )
    .execute(&pg_pool)
    .await
    .expect("failed to create blocked table");

    let analytics_producer = Arc::new(AnalyticsProducer::new(&cli.kafka_brokers));
    let analytics_consumer = AnalyticsConsumer::new(
        &cli.kafka_brokers,
        "denis-analytics",
        &cli.clickhouse_url,
        &cli.clickhouse_user,
        &cli.clickhouse_password,
    );
    tokio::spawn(analytics_consumer.run());

    let cache = Arc::new(Cache::new(rds_conn, pg_pool));
    cache.read_blocklist_db_memory().await;

    let upstream = UpstreamPool::new(UpstreamConfig::default());
    let handler = Arc::new(QueryHandler::new(cache.clone(), upstream, analytics_producer));

    let stats_client = Arc::new(StatsClient::new(
        &cli.kafka_brokers,
        &cli.clickhouse_url,
        &cli.clickhouse_user,
        &cli.clickhouse_password,
    ));
    let api_router = api::router(ApiState { cache: cache.clone(), stats: stats_client });
    let api_bind = cli.api_bind.clone();
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&api_bind)
            .await
            .expect("Cannot bind API listener");
        info!("Management API listening on {}", api_bind);
        axum::serve(listener, api_router)
            .await
            .expect("API server error");
    });

    let config = dns::ServerConfig {
        bind_addr: cli.bind.parse()?,
        ..Default::default()
    };
    let dns_server = Server::new(config, handler);
    info!("Starting DNS server on {}", cli.bind);
    if let Err(e) = dns_server.run().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
