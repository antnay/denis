use std::sync::Arc;

use bytes::BytesMut;
use ftlog::{debug, error, info};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::Mutex,
};

use crate::{handler::QueryHandler, server::ServerConfig};

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("failed to bind udp (addr: {0}): {1}")]
    BindUdp(String, std::io::Error),
    #[error("failed to bind tcp (addr: {0}): {1}")]
    BindTcp(String, std::io::Error),
    #[error("socket error: {0}")]
    Socket(std::io::Error),
    // #[error("unknown server error")]
    // Unknown,
}

// todo: use disruspter
#[derive(Debug)]
pub struct BufferPool {
    pool: Mutex<Vec<BytesMut>>,
    buffer_size: usize,
}

impl BufferPool {
    pub fn new(buffer_size: usize, initial_count: usize) -> Arc<Self> {
        let mut pool = Vec::with_capacity(initial_count);
        for _ in 0..initial_count {
            pool.push(BytesMut::with_capacity(buffer_size));
        }
        Arc::new(Self {
            pool: Mutex::new(pool),
            buffer_size,
        })
    }

    pub async fn get(&self) -> BytesMut {
        self.pool
            .lock()
            .await
            .pop()
            .unwrap_or_else(|| BytesMut::with_capacity(self.buffer_size))
    }

    pub async fn put(&self, mut buf: BytesMut) {
        buf.clear();
        if buf.capacity() <= self.buffer_size * 2 {
            self.pool.lock().await.push(buf);
        }
    }
}

pub struct Server {
    config: ServerConfig,
    handler: Arc<QueryHandler>,
    buffer_pool: Arc<BufferPool>,
}

impl Server {
    pub fn new(config: ServerConfig, handler: Arc<QueryHandler>) -> Self {
        let buffer_pool = BufferPool::new(config.udp_buffer_size, config.udp_buffer_count);
        debug!("buffer pool {:#?}", buffer_pool);
        Self {
            config,
            handler,
            buffer_pool,
        }
    }

    pub async fn run(&self) -> Result<(), ServerError> {
        let udp = UdpSocket::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindUdp(self.config.bind_addr.to_string(), e))?;
        let _tcp = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindTcp(self.config.bind_addr.to_string(), e))?;

        if cfg!(debug_assertions) {
            info!("server running: {}", self.config.bind_addr);
        }
        tokio::select! {
            r = self.serve_udp(udp) => {r},
            // r = self.serve_tcp(tcp) => {r},
        }
    }

    async fn serve_udp(&self, udp_socket: UdpSocket) -> Result<(), ServerError> {
        if cfg!(debug_assertions) {
            info!("udp server running");
        }
        let socket = Arc::new(udp_socket);

        loop {
            let mut buf = self.buffer_pool.get().await;
            buf.resize(self.config.udp_buffer_size, 0);
            let (len, src) = socket
                .recv_from(&mut buf)
                .await
                .map_err(ServerError::Socket)?;

            let handler = Arc::clone(&self.handler);
            let socket = Arc::clone(&socket);

            let pool = Arc::clone(&self.buffer_pool);

            buf.truncate(len);

            tokio::spawn(async move {
                let result = handler.handle(&buf).await;

                pool.put(buf).await;

                match result {
                    Ok(res) => {
                        if let Err(e) = socket.send_to(&res, src).await {
                            error!("cannot send udp: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("query handling failed: {}", e);
                    }
                }
            });
        }
    }

    // async fn serve_tcp(&self, socket: TcpListener) -> Result<(), ServerError> {
    //     todo!("serve tcp")
    // }
}
