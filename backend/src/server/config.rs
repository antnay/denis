use std::{net::SocketAddr, time::Duration};

pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub tcp_timeout: Duration,
    pub max_tcp_connections: usize,
    pub udp_buffer_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:53".parse().unwrap(),
            tcp_timeout: Duration::from_secs(10),
            max_tcp_connections: 100,
            udp_buffer_size: 4096,
        }
    }
}
