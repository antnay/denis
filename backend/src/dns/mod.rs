mod config;
pub mod mono;
mod server;

pub use config::ServerConfig;
pub use server::{Server, serve_tcp};

pub const UDP_BUFFER_SIZE: usize = 512;
pub const UDP_BUFFER_COUNT: usize = 1000;
