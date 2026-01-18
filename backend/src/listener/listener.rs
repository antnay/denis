use std::{error::Error, sync::Arc};

use crate::listener::ListenerConfig;

pub struct Listener {
    config: ListenerConfig,
    handler: Arc<QueryHandler>,
}

impl Listener {
    pub fn new(config: ListenerConfig, handler: Arc<QueryHandler>) -> Self {
        Self { config, handler }
    }

    // pub async fn run(&self) -> Result<(), Error> {
    //
    // }
}


