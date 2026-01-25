use std::{net::IpAddr, time::Instant};

use ftlog::{debug, info};

use crate::handler::resolver::Resolver;

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("resolver error: {0}")]
    Resolver(#[from] crate::handler::resolver::ResolverError),
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
    resolver: Resolver,
}

impl QueryHandler {
    pub fn new(resolver: Resolver) -> Self {
        Self { resolver }
    }

    pub async fn handle(&self, data: &[u8], client: IpAddr) -> Result<Vec<u8>, HandlerError> {
        debug!("incoming client: {}", client);
        let begin = Instant::now();
        let query = self.resolver.parse(data).await?;
        // debug!("query debug: {:#?}", query);
        let res = self.resolver.resolve(&query).await?;
        // debug!("response debug: {:#?}", res);
        let delta = begin.elapsed();
        info!("query time: {:?}", delta);
        // record statistics
        Ok(res.raw)
    }
}
