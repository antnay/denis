use std::net::SocketAddr;

use crossbeam_queue::ArrayQueue;
use ftlog::debug;
use hickory_proto::op::ResponseCode;
use tokio::{
    net::UdpSocket,
    time::{Duration, error::Elapsed, timeout},
};

use crate::{
    config::{BlockingMode, RuntimeConfig, SharedConfig},
    handler::query::Query,
};

const POOL_SIZE: usize = 256;
const RECV_BUF: usize = 4096;

#[derive(Debug, Clone)]
pub struct UpstreamResponse {
    pub code: ResponseCode,
    pub raw: Vec<u8>,
}

impl UpstreamResponse {
    // pub fn blocked() -> Self {
    //     Self {
    //         code: ResponseCode::NXDomain,
    //         // cached: false,
    //         // blocked: true,
    //         raw: vec![],
    //     }
    // }

    pub fn cached(query: &Query, mut raw: Vec<u8>) -> Self {
        if raw.len() >= 2 && query.raw.len() >= 2 {
            raw[0] = query.raw[0];
            raw[1] = query.raw[1];
        }

        Self {
            code: rcode_from_raw(&raw),
            raw,
        }
    }
    pub fn blocked(query: &Query, mode: BlockingMode) -> Self {
        match mode {
            BlockingMode::NxDomain => Self::nxdomain(query),
            BlockingMode::Refused => Self::refused(query),
            BlockingMode::ZeroIp => match RuntimeConfig::zero_ip(query.query_type) {
                Some(ip) => Self::zero_ip(query, ip),
                None => Self::nxdomain(query),
            },
        }
    }

    pub fn nxdomain(query: &Query) -> Self {
        let response_len = query.answer_offset;
        let mut raw = query.raw[..response_len.min(query.raw.len())].to_vec();

        if raw.len() >= 12 {
            let rd = raw[2] & 0x01;
            raw[2] = 0x84 | rd;
            raw[3] = 0x83;
            raw[6] = 0x00;
            raw[7] = 0x00;
            raw[8] = 0x00;
            raw[9] = 0x00;
            raw[10] = 0x00;
            raw[11] = 0x00;
        }

        Self {
            code: ResponseCode::NXDomain,
            raw,
        }
    }

    fn refused(query: &Query) -> Self {
        let response_len = query.answer_offset;
        let mut raw = query.raw[..response_len.min(query.raw.len())].to_vec();

        if raw.len() >= 12 {
            let rd = raw[2] & 0x01;
            raw[2] = 0x80 | rd; // QR=1, opcode 0, AA=0
            raw[3] = 0x05; // RCODE = REFUSED
            raw[6..12].fill(0); // AN/NS/AR counts = 0
        }

        Self {
            code: ResponseCode::Refused,
            raw,
        }
    }

    fn zero_ip(query: &Query, ip: std::net::IpAddr) -> Self {
        const TTL: u32 = 60;
        let response_len = query.answer_offset.min(query.raw.len());
        let mut raw = query.raw[..response_len].to_vec();

        if raw.len() < 12 {
            return Self::nxdomain(query);
        }

        let rd = raw[2] & 0x01;
        raw[2] = 0x84 | rd; // QR=1, AA=1
        raw[3] = 0x80; // RA=1, RCODE=0
        raw[6..8].copy_from_slice(&1u16.to_be_bytes()); // ANCOUNT = 1
        raw[8..12].fill(0); // NS/AR counts = 0

        raw.extend_from_slice(&0xC00Cu16.to_be_bytes());
        let (rtype, rdata): (u16, Vec<u8>) = match ip {
            std::net::IpAddr::V4(v4) => (1, v4.octets().to_vec()), // A
            std::net::IpAddr::V6(v6) => (28, v6.octets().to_vec()), // AAAA
        };
        raw.extend_from_slice(&rtype.to_be_bytes());
        raw.extend_from_slice(&1u16.to_be_bytes()); // CLASS IN
        raw.extend_from_slice(&TTL.to_be_bytes());
        raw.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        raw.extend_from_slice(&rdata);

        Self {
            code: ResponseCode::NoError,
            raw,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum UpstreamError {
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("timeout error: {0}")]
    Timeout(Elapsed),
}

impl From<std::io::Error> for UpstreamError {
    fn from(err: std::io::Error) -> Self {
        UpstreamError::Upstream(err.to_string())
    }
}
impl From<Elapsed> for UpstreamError {
    fn from(err: Elapsed) -> Self {
        UpstreamError::Timeout(err)
    }
}

pub struct UpstreamPool {
    config: SharedConfig,
    sockets: ArrayQueue<UdpSocket>,
}

impl UpstreamPool {
    pub async fn new(config: SharedConfig) -> Self {
        let sockets = ArrayQueue::new(POOL_SIZE);
        for _ in 0..POOL_SIZE {
            if let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await {
                let _ = sockets.push(sock);
            }
        }
        Self { config, sockets }
    }

    pub async fn resolve(&self, query: &Query) -> Result<UpstreamResponse, UpstreamError> {
        let cfg = self.config.load();
        let servers = &cfg.upstreams;
        if servers.is_empty() {
            return Err(UpstreamError::Upstream("no upstreams configured".into()));
        }
        let timeout = cfg.upstream_timeout();

        let mut err = None;
        for attempt in 0..servers.len().max(1) {
            let server = servers[attempt % servers.len()];
            if cfg!(debug_assertions) {
                debug!("using server: {}", server);
            }
            match self.query_dns(&server, query, timeout).await {
                Ok(response) => return Ok(response),
                Err(e) => err = Some(e),
            }
        }

        Err(err.unwrap_or_else(|| UpstreamError::Upstream("all upstreams failed".into())))
    }

    async fn query_dns(
        &self,
        server: &SocketAddr,
        query: &Query,
        query_timeout: Duration,
    ) -> Result<UpstreamResponse, UpstreamError> {
        let socket = match self.sockets.pop() {
            Some(sock) => sock,
            None => UdpSocket::bind("0.0.0.0:0").await?,
        };

        let result = self.exchange(&socket, server, query, query_timeout).await;

        if result.is_ok() {
            let _ = self.sockets.push(socket);
        }
        result
    }

    async fn exchange(
        &self,
        socket: &UdpSocket,
        server: &SocketAddr,
        query: &Query,
        query_timeout: Duration,
    ) -> Result<UpstreamResponse, UpstreamError> {
        socket.send_to(&query.raw, server).await?;
        let txid = [query.raw[0], query.raw[1]];

        let mut buf = [0u8; RECV_BUF];
        let len = timeout(query_timeout, async {
            loop {
                let (len, src) = socket.recv_from(&mut buf).await?;
                if src == *server && len >= 2 && buf[0] == txid[0] && buf[1] == txid[1] {
                    return Ok::<usize, std::io::Error>(len);
                }
            }
        })
        .await??;

        let bytes = buf[..len].to_vec();
        let code = rcode_from_raw(&bytes);
        Ok(UpstreamResponse { code, raw: bytes })
    }
}

/// Map a DNS message's RCODE (low nibble of byte 3) to a `ResponseCode`.
pub fn rcode_from_raw(raw: &[u8]) -> ResponseCode {
    match raw.get(3) {
        Some(b) => match b & 0x0F {
            0 => ResponseCode::NoError,
            2 => ResponseCode::ServFail,
            3 => ResponseCode::NXDomain,
            5 => ResponseCode::Refused,
            n => ResponseCode::Unknown(n.into()),
        },
        None => ResponseCode::ServFail,
    }
}
