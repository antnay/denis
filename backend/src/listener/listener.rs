use std::{net::SocketAddr, sync::Arc};

use tokio::net::{TcpListener, UdpSocket};

use crate::listener::ListenerConfig;

#[derive(thiserror::Error, Debug)]
pub enum ListenerError {
    #[error("failed to bind udp (addr: {0}): {1}")]
    BindUdp(String, std::io::Error),
    #[error("failed to bind tcp (addr: {0}): {1}")]
    BindTcp(String, std::io::Error),
    #[error("socket error: {0}")]
    Socket(std::io::Error),
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

    pub async fn run(&self) -> Result<(), ListenerError> {
        let udp = UdpSocket::bind(self.config.bind_addr)
            .await
            .map_err(|e| ListenerError::BindUdp(self.config.bind_addr.to_string(), e))?;
        let tcp = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ListenerError::BindTcp(self.config.bind_addr.to_string(), e))?;

        tokio::select! {
            r = self.serve_udp(udp) => {r},
            r = self.serve_tcp(tcp) => {r},
        }
    }

    async fn serve_udp(&self, udp_socket: UdpSocket) -> Result<(), ListenerError> {
        let socket = Arc::new(udp_socket);
        let mut buf = vec![0u8; self.config.udp_buffer_size];
        loop {
            let (len, src) = socket
                .recv_from(&mut buf)
                .await
                .map_err(|e| ListenerError::Socket(e))?;

            let handler = Arc::clone(&self.handler);
            let socket = Arc::clone(&socket);
            let data = buf[..len].to_vec();

            tokio::spawn(async move {
                Self::handle_udp(socket, handler, data, src).await;
            });
        }
    }

    async fn handle_udp(
        socket: Arc<UdpSocket>,
        handler: Arc<QueryHandler>,
        data: Vec<u8>,
        src: SocketAddr,
    ) {
    }

    async fn serve_tcp(&self, socket: TcpListener) -> Result<(), ListenerError> {
        todo!("serve tcp")
    }
}
