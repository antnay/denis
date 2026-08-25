use std::net::SocketAddr;

use clap::{Args, Parser};

/// Denis DNS proxy.
///
/// Flags below are *startup* configuration: things that require a restart to
/// change (bind addresses, backend connections, socket tuning). Runtime-tunable
/// settings (upstreams, blocking mode, cache TTL clamps) live in [`RuntimeConfig`]
/// and are mutated through the management API — see `crate::config`.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[command(flatten)]
    pub dns: DnsArgs,
    #[command(flatten)]
    pub api: ApiArgs,
    #[command(flatten)]
    pub upstream: UpstreamArgs,
    #[command(flatten)]
    pub redis: RedisArgs,
    #[command(flatten)]
    pub postgres: PostgresArgs,
    #[command(flatten)]
    pub kafka: KafkaArgs,
    #[command(flatten)]
    pub clickhouse: ClickhouseArgs,

    /// Increase log verbosity: -v = debug, -vv = trace.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "DNS server")]
pub struct DnsArgs {
    /// Address the UDP + TCP DNS listener binds to.
    #[arg(long, env = "DENIS_DNS_BIND", default_value = "0.0.0.0:53")]
    pub dns_bind: SocketAddr,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Management API")]
pub struct ApiArgs {
    /// Address the HTTP management/stats API binds to.
    #[arg(long, env = "DENIS_API_BIND", default_value = "0.0.0.0:8080")]
    pub api_bind: SocketAddr,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Upstream resolvers (initial defaults; tunable via API)")]
pub struct UpstreamArgs {
    /// Upstream resolvers, comma-separated or repeated. These seed the runtime
    /// config the first time Denis starts; afterwards the persisted value wins.
    #[arg(
        long = "upstream",
        env = "DENIS_UPSTREAMS",
        value_delimiter = ',',
        default_value = "9.9.9.9:53,1.1.1.1:53"
    )]
    pub servers: Vec<SocketAddr>,

    /// Per-query upstream timeout in milliseconds.
    #[arg(long, env = "DENIS_UPSTREAM_TIMEOUT_MS", default_value_t = 5000)]
    pub timeout_ms: u64,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Redis / cache")]
pub struct RedisArgs {
    #[arg(long, env = "REDIS_URL", default_value = "redis://localhost:6379")]
    pub redis_url: String,

    /// Maximum number of entries in the in-process (L1) cache.
    #[arg(long, env = "DENIS_CACHE_CAPACITY", default_value_t = 10_000)]
    pub cache_capacity: u64,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "PostgreSQL")]
pub struct PostgresArgs {
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://postgres:postgres@localhost:5433/denis"
    )]
    pub database_url: String,

    #[arg(long, env = "DENIS_PG_MAX_CONNECTIONS", default_value_t = 16)]
    pub pg_max_connections: u32,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Kafka")]
pub struct KafkaArgs {
    #[arg(long, env = "DENIS_KAFKA_BROKERS", default_value = "localhost:9092")]
    pub kafka_brokers: String,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "ClickHouse")]
pub struct ClickhouseArgs {
    #[arg(
        long,
        env = "DENIS_CLICKHOUSE_URL",
        default_value = "http://localhost:8123"
    )]
    pub clickhouse_url: String,
    #[arg(long, env = "DENIS_CLICKHOUSE_USER", default_value = "default")]
    pub clickhouse_user: String,
    #[arg(long, env = "DENIS_CLICKHOUSE_PASSWORD", default_value = "clickhouse")]
    pub clickhouse_password: String,
}
