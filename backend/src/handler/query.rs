use std::{string::ParseError, sync::Arc, time::Instant};

use ftlog::{debug, info};
use hickory_proto::op::ResponseCode;

use crate::{
    cache::{BlocklistError, Cache, CacheError},
    handler::{Parser, UpstreamError, UpstreamPool, UpstreamResponse},
};

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("parser error: {0}")]
    Parser(ParseError),
    #[error("cache error: {0}")]
    Cache(CacheError),
    #[error("blocklist error: {0}")]
    Blocklist(BlocklistError),
    #[error("upstream error: {0}")]
    Upstream(UpstreamError),
}

impl From<CacheError> for HandlerError {
    fn from(err: CacheError) -> Self {
        HandlerError::Cache(err)
    }
}

impl From<BlocklistError> for HandlerError {
    fn from(err: BlocklistError) -> Self {
        HandlerError::Blocklist(err)
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
}

impl QueryHandler {
    pub fn new(cache: Arc<Cache>, upstream: UpstreamPool) -> Self {
        Self { cache, upstream }
    }

    pub async fn handle(&self, data: &[u8]) -> Result<Vec<u8>, HandlerError> {
        let total = Instant::now();
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
                return Ok(UpstreamResponse::nxdomain(&query).raw);
            }
            (false, Some(cached)) => {
                if cfg!(debug_assertions) {
                    debug!("cached");
                    info!("total time: {:?}", total.elapsed());
                }
                return Ok(UpstreamResponse::cached(&query, cached).raw);
            }
            (false, None) => {
                let begin = Instant::now();
                let res = self.upstream.resolve(&query).await?;
                let delta = begin.elapsed();
                if cfg!(debug_assertions) {
                    info!("resolve time: {:?}", delta);
                }
                // handle better
                // if res.code == ResponseCode::NoError {
                //     let cache = self.cache.clone();
                //     let query_clone = query.clone();
                //     let raw = res.raw.clone();
                //     let answer_offset = query.answer_offset;
                //
                //     tokio::spawn(async move {
                //         let ttl = Resolver::parse_ttl(&raw, answer_offset);
                //         let _ = cache.add_query(&query_clone, &raw, ttl).await;
                //     });
                // }
                //
                if res.code == ResponseCode::NoError {
                    let ttl = Parser::parse_ttl(&res.raw, query.answer_offset);
                    let _ = self.cache.add_query(&query, &res.raw, ttl).await;
                }
                if cfg!(debug_assertions) {
                    info!("total time: {:?}", total.elapsed());
                }
                Ok(res.raw)
            }
        }
    }
}
