use std::{sync::Arc, time::Duration};

use bytes::BytesMut;
use ftlog::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Mutex,
    time::timeout,
};

use crate::{dns::ServerConfig, handler::QueryHandler};

const TCP_MAX_MSG: usize = 65535;
const TCP_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("failed to bind udp (addr: {0}): {1}")]
    BindUdp(String, std::io::Error),
    #[error("failed to bind tcp (addr: {0}): {1}")]
    BindTcp(String, std::io::Error),
    #[error("socket error: {0}")]
    Socket(std::io::Error),
}

// todo: use disruptor
#[derive(Debug)]
pub struct BufferPool {
    pool: Mutex<Vec<BytesMut>>,
    buffer_size: usize,
}

impl BufferPool {
    pub fn new(buffer_size: usize, initial_count: usize) -> Arc<Self> {
        let pool = (0..initial_count)
            .map(|_| BytesMut::zeroed(buffer_size))
            .collect();
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
        let tcp = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindTcp(self.config.bind_addr.to_string(), e))?;

        if cfg!(debug_assertions) {
            info!("server running on {}", self.config.bind_addr);
        }
        tokio::select! {
            r = self.serve_udp(udp) => r,
            r = self.serve_tcp(tcp) => r,
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
                    Err(e) => error!("query handling failed: {}", e),
                }
            });
        }
    }

    async fn serve_tcp(&self, listener: TcpListener) -> Result<(), ServerError> {
        if cfg!(debug_assertions) {
            info!("tcp server running");
        }
        loop {
            let (stream, src) = listener.accept().await.map_err(ServerError::Socket)?;
            let handler = Arc::clone(&self.handler);
            tokio::spawn(async move {
                if cfg!(debug_assertions) {
                    info!("tcp connection from {}", src);
                }
                if let Err(e) = handle_tcp_conn(stream, handler).await {
                    error!("tcp connection error from {}: {}", src, e);
                }
            });
        }
    }
}

async fn handle_tcp_conn(
    mut stream: TcpStream,
    handler: Arc<QueryHandler>,
) -> Result<(), std::io::Error> {
    let mut len_buf = [0u8; 2];

    loop {
        match timeout(TCP_IDLE_TIMEOUT, stream.read_exact(&mut len_buf)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Ok(Err(e)) => return Err(e),
            Err(_) => break,
        }

        let msg_len = u16::from_be_bytes(len_buf) as usize;
        if msg_len == 0 || msg_len > TCP_MAX_MSG {
            break;
        }

        let mut msg = vec![0u8; msg_len];
        stream.read_exact(&mut msg).await?;

        match handler.handle(&msg).await {
            Ok(response) => {
                let resp_len = (response.len() as u16).to_be_bytes();
                stream.write_all(&resp_len).await?;
                stream.write_all(&response).await?;
            }
            Err(e) => {
                error!("tcp query handling failed: {}", e);
                break;
            }
        }
    }

    Ok(())
}
