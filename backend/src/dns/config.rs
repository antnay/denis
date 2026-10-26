use std::net::SocketAddr;

#[derive(Clone, Copy)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub udp_buffer_size: usize,
    pub udp_buffer_count: usize,
}
