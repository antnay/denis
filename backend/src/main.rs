mod analytics;
mod api;
mod cli;
mod config;
mod dns;
mod handler;
mod repo;

use clap::Parser;
use ftlog::{LevelFilter, error, info};
use redis::Client;
use sqlx::postgres;
use std::sync::Arc;

use crate::{
    analytics::{AnalyticsConsumer, AnalyticsProducer, StatsClient},
    api::ApiState,
    cli::Cli,
    config::RuntimeConfig,
    dns::Server,
    handler::{QueryHandler, UpstreamPool},
    repo::Cache,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let level = match cli.verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    let _guard = ftlog::builder()
        .max_log_level(level)
        .try_init()
        .expect("failed to init logger");

    let rds_conn = Client::open(cli.redis.redis_url.clone())
        .expect("Cannot connect to redis")
        .get_multiplexed_async_connection()
        .await?;

    let pg_pool = postgres::PgPoolOptions::new()
        .max_connections(cli.postgres.pg_max_connections)
        .connect(&cli.postgres.database_url)
        .await
        .expect("Cannot connect to postgres");

    sqlx::query(
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

    sqlx::query(
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

    let runtime_config = config::load_or_init(&pg_pool, RuntimeConfig::from_cli(&cli))
        .await
        .expect("failed to load runtime config");

    let analytics_producer = Arc::new(AnalyticsProducer::new(&cli.kafka.kafka_brokers));
    let analytics_consumer = AnalyticsConsumer::new(
        &cli.kafka.kafka_brokers,
        "denis-analytics",
        &cli.clickhouse.clickhouse_url,
        &cli.clickhouse.clickhouse_user,
        &cli.clickhouse.clickhouse_password,
    );
    tokio::spawn(analytics_consumer.run());

    let cache = Arc::new(Cache::new(rds_conn, pg_pool, cli.redis.cache_capacity));
    cache.read_blocklist_db_memory().await;
    cache.read_allowlist_db_memory().await;

    let upstream = UpstreamPool::new(runtime_config.clone());
    let handler = Arc::new(QueryHandler::new(
        cache.clone(),
        upstream,
        analytics_producer,
        runtime_config.clone(),
    ));

    let stats_client = Arc::new(StatsClient::new(
        &cli.kafka.kafka_brokers,
        &cli.clickhouse.clickhouse_url,
        &cli.clickhouse.clickhouse_user,
        &cli.clickhouse.clickhouse_password,
    ));
    let api_router = api::router(ApiState {
        cache: cache.clone(),
        stats: stats_client,
        config: runtime_config.clone(),
    });
    let api_bind = cli.api.api_bind;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(api_bind)
            .await
            .expect("Cannot bind API listener");
        info!("Management API listening on {}", api_bind);
        axum::serve(listener, api_router)
            .await
            .expect("API server error");
    });

    let server_config = dns::ServerConfig {
        bind_addr: cli.dns.dns_bind,
        udp_buffer_size: dns::UDP_BUFFER_SIZE,
        udp_buffer_count: dns::UDP_BUFFER_COUNT,
    };
    let dns_server = Server::new(server_config, handler);
    info!("Starting DNS server on {}", cli.dns.dns_bind);
    if let Err(e) = dns_server.run().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
