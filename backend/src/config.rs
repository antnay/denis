use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use ftlog::info;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::cli::Cli;

pub type SharedConfig = Arc<ArcSwap<RuntimeConfig>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingMode {
    NxDomain,
    Refused,
    ZeroIp,
}

impl Default for BlockingMode {
    fn default() -> Self {
        Self::NxDomain
    }
}

fn default_neg_ttl() -> u32 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub upstreams: Vec<SocketAddr>,
    pub upstream_timeout_ms: u64,
    pub blocking_mode: BlockingMode,
    pub cache_min_ttl: u32,
    pub cache_max_ttl: u32,
    /// TTL cap for cached negative answers (NXDOMAIN / NODATA). serde default
    /// keeps configs persisted before this field existed loadable.
    #[serde(default = "default_neg_ttl")]
    pub neg_ttl: u32,
}

impl RuntimeConfig {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            upstreams: cli.upstream.servers.clone(),
            upstream_timeout_ms: cli.upstream.timeout_ms,
            blocking_mode: BlockingMode::default(),
            cache_min_ttl: 0,
            cache_max_ttl: 86_400,
            neg_ttl: default_neg_ttl(),
        }
    }

    pub fn upstream_timeout(&self) -> Duration {
        Duration::from_millis(self.upstream_timeout_ms)
    }

    pub fn clamp_ttl(&self, ttl: u32) -> u32 {
        ttl.clamp(
            self.cache_min_ttl,
            self.cache_max_ttl.max(self.cache_min_ttl),
        )
    }

    pub fn zero_ip(qtype: hickory_proto::rr::RecordType) -> Option<IpAddr> {
        use hickory_proto::rr::RecordType;
        match qtype {
            RecordType::A => Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            RecordType::AAAA => Some(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RuntimeConfigPatch {
    pub upstreams: Option<Vec<SocketAddr>>,
    pub upstream_timeout_ms: Option<u64>,
    pub blocking_mode: Option<BlockingMode>,
    pub cache_min_ttl: Option<u32>,
    pub cache_max_ttl: Option<u32>,
    pub neg_ttl: Option<u32>,
}

impl RuntimeConfigPatch {
    pub fn apply_to(self, base: &RuntimeConfig) -> RuntimeConfig {
        RuntimeConfig {
            upstreams: self.upstreams.unwrap_or_else(|| base.upstreams.clone()),
            upstream_timeout_ms: self.upstream_timeout_ms.unwrap_or(base.upstream_timeout_ms),
            blocking_mode: self.blocking_mode.unwrap_or(base.blocking_mode),
            cache_min_ttl: self.cache_min_ttl.unwrap_or(base.cache_min_ttl),
            cache_max_ttl: self.cache_max_ttl.unwrap_or(base.cache_max_ttl),
            neg_ttl: self.neg_ttl.unwrap_or(base.neg_ttl),
        }
    }
}

pub async fn pls_table(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS settings (
            id     INT PRIMARY KEY DEFAULT 1,
            config JSONB NOT NULL,
            CONSTRAINT settings_single_row CHECK (id = 1)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_or_init(
    pool: &PgPool,
    defaults: RuntimeConfig,
) -> Result<SharedConfig, sqlx::Error> {
    pls_table(pool).await?;

    let existing: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT config FROM settings WHERE id = 1")
            .fetch_optional(pool)
            .await?;

    let config = match existing.and_then(|v| serde_json::from_value::<RuntimeConfig>(v).ok()) {
        Some(cfg) => {
            info!("loaded runtime config from postgres");
            cfg
        }
        None => {
            info!("no persisted runtime config; seeding from CLI defaults");
            persist(pool, &defaults).await?;
            defaults
        }
    };

    Ok(Arc::new(ArcSwap::from_pointee(config)))
}

pub async fn persist(pool: &PgPool, config: &RuntimeConfig) -> Result<(), sqlx::Error> {
    let json = serde_json::to_value(config).expect("RuntimeConfig serializes");
    sqlx::query(
        "INSERT INTO settings (id, config) VALUES (1, $1)
         ON CONFLICT (id) DO UPDATE SET config = EXCLUDED.config",
    )
    .bind(json)
    .execute(pool)
    .await?;
    Ok(())
}
