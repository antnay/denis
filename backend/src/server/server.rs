use std::{net::SocketAddr, sync::Arc};

use ftlog::{error, info};
use tokio::net::{TcpListener, UdpSocket};

use crate::{handler::QueryHandler, server::ServerConfig};

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("failed to bind udp (addr: {0}): {1}")]
    BindUdp(String, std::io::Error),
    #[error("failed to bind tcp (addr: {0}): {1}")]
    BindTcp(String, std::io::Error),
    #[error("socket error: {0}")]
    Socket(std::io::Error),
    #[error("unknown server error")]
    Unknown,
}

pub struct Server {
    config: ServerConfig,
    handler: Arc<QueryHandler>,
}

impl Server {
    pub fn new(config: ServerConfig, handler: Arc<QueryHandler>) -> Self {
        Self { config, handler }
    }

    pub async fn run(&self) -> Result<(), ServerError> {
        let udp = UdpSocket::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindUdp(self.config.bind_addr.to_string(), e))?;
        let _tcp = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindTcp(self.config.bind_addr.to_string(), e))?;

        info!("server running: {}", self.config.bind_addr);
        tokio::select! {
            r = self.serve_udp(udp) => {r},
            // r = self.serve_tcp(tcp) => {r},
        }
    }

    async fn serve_udp(&self, udp_socket: UdpSocket) -> Result<(), ServerError> {
        info!("udp server running");
        let socket = Arc::new(udp_socket);
        let mut buf = vec![0u8; self.config.udp_buffer_size];
        loop {
            let (len, src) = socket
                .recv_from(&mut buf)
                .await
                .map_err(|e| ServerError::Socket(e))?;

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
        match handler.handle(&data, src.ip()).await {
            Ok(res) => {
                if let Err(e) = socket.send_to(&res, src).await {
                    error!("cannot send udp: {}", e);
                }
            }
            Err(e) => {
                error!("query handling failed: {}", e);
            }
        }
    }

    async fn serve_tcp(&self, socket: TcpListener) -> Result<(), ServerError> {
        todo!("serve tcp")
    }
}
