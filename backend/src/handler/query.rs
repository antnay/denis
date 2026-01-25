use std::{net::IpAddr, sync::Arc, time::Instant};

use ftlog::{debug, info};
use hickory_proto::op::ResponseCode;

use crate::{
    cache::{BlocklistError, Cache, CacheError},
    handler::{UpstreamResponse, resolver::Resolver},
};

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("resolver error: {0}")]
    Resolver(#[from] crate::handler::resolver::ResolverError),
    #[error("cache error: {0}")]
    Cache(CacheError),
    #[error("blocklist error: {0}")]
    Blocklist(BlocklistError),
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

#[derive(Debug, Clone)]
pub struct Query {
    pub id: u16,
    pub name: String,
    pub query_type: hickory_proto::rr::RecordType,
    pub raw: Vec<u8>,
    pub answer_offset: usize,
}

pub struct QueryHandler {
    cache: Arc<Cache>,
    resolver: Resolver,
}

impl QueryHandler {
    pub fn new(cache: Arc<Cache>, resolver: Resolver) -> Self {
        Self { cache, resolver }
    }

    pub async fn handle(&self, data: &[u8], client: IpAddr) -> Result<Vec<u8>, HandlerError> {
        // parse
        let total = Instant::now();
        if cfg!(debug_assertions) {
            debug!("incoming client: {}", client);
        }
        let begin = Instant::now();
        let query = self.resolver.parse(data).await?;
        let delta = begin.elapsed();
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
                let res = self.resolver.resolve(&query).await?;
                let delta = begin.elapsed();
                if cfg!(debug_assertions) {
                    info!("resolve time: {:?}", delta);
                }
                if res.code == ResponseCode::NoError {
                    let cache = self.cache.clone();
                    let query_clone = query.clone();
                    let raw = res.raw.clone();
                    let answer_offset = query.answer_offset;

                    tokio::spawn(async move {
                        let ttl = Resolver::parse_ttl(&raw, answer_offset);
                        let _ = cache.add_query(&query_clone, &raw, ttl).await;
                    });
                }
                if cfg!(debug_assertions) {
                    info!("total time: {:?}", total.elapsed());
                }
                Ok(res.raw)
            }
        }
    }
}
