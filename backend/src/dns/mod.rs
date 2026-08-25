mod config;
mod server;

pub use config::ServerConfig;
pub use server::Server;

pub const UDP_BUFFER_SIZE: usize = 512;
pub const UDP_BUFFER_COUNT: usize = 1000;
