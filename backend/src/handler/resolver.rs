use crate::cache::BlocklistError;
use crate::cache::CacheError;
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
    #[error("blocklist error: {0}")]
    Blocklist(BlocklistError),
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

impl From<BlocklistError> for ResolverError {
    fn from(err: BlocklistError) -> Self {
        ResolverError::Blocklist(err)
    }
}

enum ParseState {
    Length,
    Scan,
}

pub struct Resolver {
    upstream: UpstreamPool,
}

impl Resolver {
    pub fn new(upstream: UpstreamPool) -> Self {
        Self { upstream }
    }

    pub async fn parse(&self, data: &[u8]) -> Result<Query, ResolverError> {
        // let id = self.parse_header(data);
        let (qname, idx) = self.parse_question(data);
        let qtype = self.parse_qtype(data, idx);

        let qname_str = String::from_utf8_lossy(&qname);

        // debug!("qname bytes: {:?}", qname);
        // debug!("qname string: {}", qname_str.to_string());
        // debug!("qype string: {:02x}", qtype);

        Ok(Query {
            name: qname_str.to_string(),
            query_type: hickory_proto::rr::RecordType::from(qtype),
            raw: data.to_vec(),
            answer_offset: idx + 5,
        })
    }

    // #[inline]
    // fn parse_header(&self, data: &[u8]) -> u16 {
    //     u16::from_be_bytes([data[0], data[1]])
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
                    if data[idx] != 0x00 {
                        buf.push(46);
                    }

                    // branchless
                    // let needs_push = (data[idx] != 0x00) as usize;
                    // let old_len = buf.len();
                    // buf.reserve(1);
                    // unsafe {
                    //     *buf.as_mut_ptr().add(old_len) = 46;
                    //     buf.set_len(old_len + needs_push);
                    // }

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

    pub async fn resolve(&self, query: &Query) -> Result<UpstreamResponse, ResolverError> {
        let res = self.upstream.resolve(query).await?;
        Ok(res)
    }

    #[inline]
    pub fn parse_ttl(data: &[u8], mut idx: usize) -> u32 {
        idx += 6;
        u32::from_be_bytes([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]])
    }
}
