use std::time::Duration;

use ftlog::debug;
use ftlog::info;
use hickory_proto::op::ResponseCode;

use crate::cache::CacheError;
use crate::cache::RdsCache;
use crate::handler::UpstreamError;
use crate::handler::UpstreamPool;
use crate::handler::UpstreamResponse;

use crate::handler::query::Query;

const QUESTION: usize = 12;

#[derive(thiserror::Error, Debug)]
pub enum ResolverError {
    #[error("upstream error: {0}")]
    Upstream(UpstreamError),
    #[error("cache error: {0}")]
    Cache(CacheError),
}

impl From<UpstreamError> for ResolverError {
    fn from(err: UpstreamError) -> Self {
        ResolverError::Upstream(err)
    }
}

impl From<CacheError> for ResolverError {
    fn from(err: CacheError) -> Self {
        ResolverError::Cache(err)
    }
}

enum ParseState {
    Length,
    Scan,
}

pub struct Resolver {
    cache: RdsCache,
    // blocklist: Arc<Blocklist>
    upstream: UpstreamPool,
}

impl Resolver {
    pub fn new(cache: RdsCache, upstream: UpstreamPool) -> Self {
        Self { cache, upstream }
    }

    pub async fn parse(&self, data: &[u8]) -> Result<Query, ResolverError> {
        let id = u16::from_be_bytes([data[0], data[1]]);
        let (qname, idx) = self.parse_question(data);
        let qtype = self.parse_qtype(data, idx);

        let qname_str = String::from_utf8_lossy(&qname);

        // debug!("qname bytes: {:?}", qname);
        // debug!("qname string: {}", qname_str.to_string());
        // debug!("qype string: {:02x}", qtype);

        Ok(Query {
            id,
            name: qname_str.to_string(),
            query_type: hickory_proto::rr::RecordType::from(qtype),
            raw: data.to_vec(),
            answer_offset: idx + 5,
        })
    }

    // #[inline]
    // async fn parse_header(&self, data: &[u8]) {
    //     let id = u16::from_be_bytes([data[0], data[1]]);
    // }

    // Returns a Vec<u8> containing the dns packet qname and pointer to the last index of qname
    #[inline]
    fn parse_question(&self, data: &[u8]) -> (Vec<u8>, usize) {
        let mut idx = QUESTION;
        let mut len = 0;
        let mut state = ParseState::Length;
        let mut buf = Vec::with_capacity(64);

        while data[idx] != 0x00 {
            match state {
                ParseState::Length => {
                    len = data[idx];
                    idx += 1;
                    state = ParseState::Scan
                }
                ParseState::Scan => {
                    let stop = idx + len as usize;
                    for i in idx..stop {
                        buf.push(data[i as usize]);
                    }
                    idx += len as usize;
                    // fixme: easy branchless
                    if data[idx] != 0x00 {
                        buf.push(46);
                    }
                    state = ParseState::Length;
                }
            }
        }
        (buf, idx)
    }

    #[inline]
    fn parse_qtype(&self, data: &[u8], idx: usize) -> u16 {
        u16::from_be_bytes([data[idx + 1], data[idx + 2]])
    }

    #[inline]
    fn parse_ttl(&self, data: &[u8], mut idx: usize) -> u32 {
        idx += 6;
        u32::from_be_bytes([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]])
    }

    pub async fn resolve(&self, query: &Query) -> Result<UpstreamResponse, ResolverError> {
        // blocklist
        if let Some(mut cached) = self.cache.get_query(&query).await? {
            debug!("using cache");
            if cached.len() >= 2 {
                cached[0] = query.raw[0];
                cached[1] = query.raw[1];
            }
            return Ok(UpstreamResponse {
                code: ResponseCode::NoError,
                cached: true,
                blocked: false,
                raw: cached,
            });
        }

        debug!("using upstream");
        let res = self.upstream.resolve(query).await?;
        if res.code == ResponseCode::NoError {
            let ttl = self.parse_ttl(&res.raw, query.answer_offset);
            self.cache.add_query(&query, &res.raw, ttl).await?;
        }
        Ok(res)
    }
}
