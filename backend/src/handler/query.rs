use std::{net::IpAddr, time::Instant};

use tokio::sync::mpsc;

use crate::handler::resolver::Resolver;

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("failed to parse query: {0}")]
    Parse(String),
    #[error("resolver error: {0}")]
    Resolver(#[from] crate::handler::resolver::ResolverError),
}

pub enum QueryType {
    A,
}

pub struct QueryHandler {
    resolver: Resolver,
    // logger_tx: mpsc::Sender<Event>,
}

impl QueryHandler {
    pub fn new(
        resolver: Resolver,
        // _logger_tx: mpsc::Sender<Event>
    ) -> Self {
        Self {
            resolver,
            // logger_tx,
        }
    }

    pub async fn handle(&self, data: &[u8], client: IpAddr) -> Result<Vec<u8>, HandlerError> {
        println!("incoming client: {}", client);
        let begin = Instant::now();
        let query = self.resolver.parse(data).await?;
        self.resolver.resolve(&query).await?;
        let delta = begin.elapsed();
        println!("query time: {:?}", delta);
        // record statistics
        // log
        Ok("response".as_bytes().to_vec())
    }
}

pub struct Query {
    pub id: u16,
    pub name: String,
    pub query_type: QueryType,
    pub raw: Vec<u8>,
}
