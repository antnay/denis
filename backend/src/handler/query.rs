
use std::{iter::Inspect, net::IpAddr, time::Instant};

use tokio::sync::mpsc;

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
}

pub struct QueryHandler {
    resolver: Resolver,
    logger_tx: mpsc::Sender<Event>,
}

impl QueryHandler {
    pub fn new(resolver: Resolver, logger_tx: mpsc::Sender<Event>) -> Self {
        Self {
            resolver, logger_tx
        }
    }

    pub async fn handle(&self, data: &[u8], client: IpAddr) {
        // let begin = Instant::now();

        // blocklist
        // cache
        // upstream
        
        // record statistics
        // log
    }
}
