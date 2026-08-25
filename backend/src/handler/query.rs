use std::{
    string::ParseError,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use ftlog::{debug, info};
use hickory_proto::op::ResponseCode;

use crate::{
    analytics::{AnalyticsProducer, DnsQueryEvent},
    handler::{Parser, UpstreamError, UpstreamPool, UpstreamResponse},
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
}

impl QueryHandler {
    pub fn new(cache: Arc<Cache>, upstream: UpstreamPool, analytics: Arc<AnalyticsProducer>) -> Self {
        Self { cache, upstream, analytics }
    }

    pub async fn handle(&self, data: &[u8]) -> Result<Vec<u8>, HandlerError> {
        let total = Instant::now();
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let query = Parser::parse_udp(data).await;
        let delta = total.elapsed();
        if cfg!(debug_assertions) {
            info!("parse time: {:?}", delta);
        }

        match self.cache.check_and_get(&query).await? {
            (true, _) => {
                if cfg!(debug_assertions) {
                    debug!("blocked");
                    info!("total time: {:?}", total.elapsed());
                }
                self.analytics.send(DnsQueryEvent {
                    timestamp_ms,
                    domain: query.name.clone(),
                    query_type: query.query_type.to_string(),
                    response_code: ResponseCode::NXDomain.to_string(),
                    cache_hit: false,
                    blocked: true,
                    latency_us: total.elapsed().as_micros() as u64,
                });
                Ok(UpstreamResponse::nxdomain(&query).raw)
            }
            (false, Some(cached)) => {
                if cfg!(debug_assertions) {
                    debug!("cached");
                    info!("total time: {:?}", total.elapsed());
                }
                self.analytics.send(DnsQueryEvent {
                    timestamp_ms,
                    domain: query.name.clone(),
                    query_type: query.query_type.to_string(),
                    response_code: ResponseCode::NoError.to_string(),
                    cache_hit: true,
                    blocked: false,
                    latency_us: total.elapsed().as_micros() as u64,
                });
                let _ = self.cache.add_dns_query_moka(&query, &cached).await;
                Ok(UpstreamResponse::cached(&query, cached).raw)
            }
            (false, None) => {
                let begin = Instant::now();
                let res = self.upstream.resolve(&query).await?;
                let delta = begin.elapsed();
                if cfg!(debug_assertions) {
                    info!("resolve time: {:?}", delta);
                }

                self.analytics.send(DnsQueryEvent {
                    timestamp_ms,
                    domain: query.name.clone(),
                    query_type: query.query_type.to_string(),
                    response_code: res.code.to_string(),
                    cache_hit: false,
                    blocked: false,
                    latency_us: total.elapsed().as_micros() as u64,
                });

                if res.code == ResponseCode::NoError {
                    let ttl = Parser::parse_ttl(&res.raw, query.answer_offset);
                    let _ = self.cache.add_dns_query_moka(&query, &res.raw).await;
                    let _ = self.cache.add_dns_query_redis(&query, &res.raw, ttl).await;
                }
                if cfg!(debug_assertions) {
                    info!("total time: {:?}", total.elapsed());
                }
                Ok(res.raw)
            }
        }
    }
}
