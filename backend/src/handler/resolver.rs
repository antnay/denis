use crate::handler::UpstreamError;
use crate::handler::UpstreamPool;
use crate::handler::UpstreamResponse;

use crate::handler::query::Query;

const QNAME_START: usize = 12;

#[derive(thiserror::Error, Debug)]
pub enum ResolverError {
    #[error("upstream error: {0}")]
    Upstream(UpstreamError),
}

impl From<UpstreamError> for ResolverError {
    fn from(err: UpstreamError) -> Self {
        ResolverError::Upstream(err)
    }
}

enum ParseState {
    Length,
    Scan,
}

pub struct Resolver {
    // cache: Cache,
    // blocklist: Arc<Blocklist>
    upstream: UpstreamPool,
}

impl Resolver {
    pub fn new(upstream: UpstreamPool) -> Self {
        Self { upstream }
    }

    pub async fn parse(&self, data: &[u8]) -> Result<Query, ResolverError> {
        let id = u16::from_be_bytes([data[0], data[1]]);
        let (qname, qtype) = self.parse_question(data);

        let qname_str = String::from_utf8_lossy(&qname);

        if cfg!(debug_assertions) {
            println!("qname bytes: {:?}", qname);
            println!("qname string: {}", qname_str.to_string());
            println!("qype string: {:02x}", qtype);
        }

        Ok(Query {
            id,
            name: qname_str.to_string(),
            query_type: hickory_proto::rr::RecordType::from(qtype),
            raw: data.to_vec(),
        })
    }

    // #[inline]
    // async fn parse_header(&self, data: &[u8]) {
    //     let id = u16::from_be_bytes([data[0], data[1]]);
    // }

    #[inline]
    fn parse_question(&self, data: &[u8]) -> (Vec<u8>, u16) {
        let mut indx = QNAME_START;
        let mut len = 0;
        let mut state = ParseState::Length;
        let mut buf = Vec::with_capacity(64);

        while data[indx] != 0x00 {
            match state {
                ParseState::Length => {
                    len = data[indx];
                    indx += 1;
                    state = ParseState::Scan
                }
                ParseState::Scan => {
                    let stop = indx + len as usize;
                    for i in indx..stop {
                        // println!(
                        //     "indx:{} | i: {} | data: '{}' - {} | len: {}",
                        //     indx, i, data[i as usize] as char, data[i as usize], len
                        // );
                        buf.push(data[i as usize]);
                    }
                    indx += len as usize;
                    // fixme: easy branchless
                    if data[indx] != 0x00 {
                        buf.push(46);
                    }
                    state = ParseState::Length;
                }
            }
        }
        (buf, u16::from_be_bytes([data[indx + 1], data[indx + 2]]))
    }

    // #[inline]
    // async fn parse_answer(&self, data: &[u8]) {}

    pub async fn resolve(&self, query: &Query) -> Result<UpstreamResponse, ResolverError> {
        // bloclist
        // cache
        println!("querying {}", query.name);
        let res = self.upstream.resolve(query).await?;
        // insert into cache
        Ok(res)
    }
}
