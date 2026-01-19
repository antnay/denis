use std::sync::Arc;

use tokio::net::{TcpListener, UdpSocket};

use crate::listener::ListenerConfig;

struct QueryHandler {}

#[derive(thiserror::Error, Debug)]
enum ServerError {
    #[error("failed to bind udp (addr: {0}): {1}")]
    BindUdp(String, std::io::Error),
    #[error("failed to bind tcp (addr: {0}): {1}")]
    BindTcp(String, std::io::Error),
    #[error("unknown server error")]
    Unknown,
}

pub struct Listener {
    config: ListenerConfig,
    handler: Arc<QueryHandler>,
}

impl Listener {
    pub fn new(config: ListenerConfig, handler: Arc<QueryHandler>) -> Self {
        Self { config, handler }
    }

    pub async fn run(&self) -> Result<(), ServerError> {
        let udp = UdpSocket::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindUdp(self.config.bind_addr.to_string(), e))?;
        let tcp = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindTcp(self.config.bind_addr.to_string(), e))?;

        tokio::select! {
            r = self.serve_udp(udp) => {r},
            r = self.serve_tcp(tcp) => {r},
        }
    }

    async fn serve_udp(&self, socket: UdpSocket) -> Result<(), ServerError> {
        todo!("serve udp")
    }

    async fn serve_tcp(&self, socket: TcpListener) -> Result<(), ServerError> {
        todo!("serve tcp")
    }
}
