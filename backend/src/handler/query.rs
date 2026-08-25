use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use ftlog::{debug, info};
use hickory_proto::op::ResponseCode;

use crate::{
    analytics::{AnalyticsProducer, DnsQueryEvent},
    config::SharedConfig,
    handler::{ParseError, Parser, UpstreamError, UpstreamPool, UpstreamResponse},
    repo::{Cache, CacheError},
};

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("parser error: {0}")]
    Parser(ParseError),
    #[error("cache error: {0}")]
    Cache(CacheError),
    #[error("upstream error: {0}")]
    Upstream(UpstreamError),
}

impl From<CacheError> for HandlerError {
    fn from(err: CacheError) -> Self {
        HandlerError::Cache(err)
    }
}

impl From<UpstreamError> for HandlerError {
    fn from(err: UpstreamError) -> Self {
        HandlerError::Upstream(err)
    }
}

impl From<ParseError> for HandlerError {
    fn from(err: ParseError) -> Self {
        HandlerError::Parser(err)
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    pub name: String,
    pub query_type: hickory_proto::rr::RecordType,
    pub raw: Vec<u8>,
    pub answer_offset: usize,
}

pub struct QueryHandler {
    cache: Arc<Cache>,
    upstream: UpstreamPool,
    analytics: Arc<AnalyticsProducer>,
    config: SharedConfig,
}

impl QueryHandler {
    pub fn new(
        cache: Arc<Cache>,
        upstream: UpstreamPool,
        analytics: Arc<AnalyticsProducer>,
        config: SharedConfig,
    ) -> Self {
        Self {
            cache,
            upstream,
            analytics,
            config,
        }
    }

    pub async fn handle(&self, data: &[u8]) -> Result<Vec<u8>, HandlerError> {
        let total = Instant::now();
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let query = Parser::parse_udp(data).await?;
        if cfg!(debug_assertions) {
            info!("parse time: {:?}", total.elapsed());
        }

        let served = match self.cache.check_and_get(&query).await? {
            (true, _) => {
                if cfg!(debug_assertions) {
                    debug!("blocked");
                }
                let response = UpstreamResponse::blocked(&query, self.config.load().blocking_mode);
                Served {
                    response_code: u16::from(response.code),
                    raw: response.raw,
                    cache_hit: false,
                    blocked: true,
                    latency_us: total.elapsed().as_micros() as u64,
                }
            }
            (false, Some(cached)) => {
                if cfg!(debug_assertions) {
                    debug!("cached");
                }
                let latency_us = total.elapsed().as_micros() as u64;
                self.cache.add_dns_query_moka(&query, &cached).await;
                Served {
                    response_code: u16::from(ResponseCode::NoError),
                    raw: UpstreamResponse::cached(&query, cached).raw,
                    cache_hit: true,
                    blocked: false,
                    latency_us,
                }
            }
            (false, None) => {
                let res = self.upstream.resolve(&query).await?;
                let latency_us = total.elapsed().as_micros() as u64;

                if res.code == ResponseCode::NoError {
                    let ttl = self
                        .config
                        .load()
                        .clamp_ttl(Parser::parse_ttl(&res.raw, query.answer_offset));
                    self.cache.add_dns_query_moka(&query, &res.raw).await;
                    self.cache.add_dns_query_redis(&query, &res.raw, ttl).await;
                }
                Served {
                    response_code: u16::from(res.code),
                    raw: res.raw,
                    cache_hit: false,
                    blocked: false,
                    latency_us,
                }
            }
        };

        self.analytics.send(DnsQueryEvent {
            timestamp_ms,
            domain: query.name,
            query_type: u16::from(query.query_type),
            response_code: served.response_code,
            cache_hit: served.cache_hit,
            blocked: served.blocked,
            latency_us: served.latency_us,
        });
        if cfg!(debug_assertions) {
            info!("total time: {:?}", total.elapsed());
        }
        Ok(served.raw)
    }
}

struct Served {
    raw: Vec<u8>,
    response_code: u16,
    cache_hit: bool,
    blocked: bool,
    latency_us: u64,
}
