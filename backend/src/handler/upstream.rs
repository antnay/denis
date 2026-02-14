use bytes::BytesMut;
use ftlog::debug;
use hickory_proto::op::ResponseCode;
use std::{net::SocketAddr, time::Duration, vec};
use tokio::{
    net::UdpSocket,
    time::{error::Elapsed, timeout},
};

use crate::handler::query::Query;

pub enum LoadBalancer {
    RoundRobin,
    // WeightedRR,
}

pub struct UpstreamConfig {
    pub servers: Vec<SocketAddr>,
    pub timeout: tokio::time::Duration,
    pub loadbalancer: LoadBalancer,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            servers: vec![
                "10.0.69.1:53".parse().unwrap(),
                "9.9.9.9:53".parse().unwrap(),
                "1.1.1.1:53".parse().unwrap(),
            ],
            timeout: Duration::from_secs(5),
            loadbalancer: LoadBalancer::RoundRobin,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpstreamResponse {
    pub code: ResponseCode,
    pub raw: BytesMut,
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

    pub fn cached(query: &Query, mut raw: BytesMut) -> Self {
        if raw.len() >= 2 && query.raw.len() >= 2 {
            raw[0] = query.raw[0];
            raw[1] = query.raw[1];
        }

        Self {
            code: ResponseCode::NoError,
            raw,
        }
    }

    pub fn nxdomain(query: &mut Query) -> Self {
        let mut raw = query.raw.split_off(query.answer_offset);
        if query.raw.len() >= 12 {
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
    config: UpstreamConfig,
}

impl UpstreamPool {
    pub fn new(config: UpstreamConfig) -> Self {
        Self { config }
    }

    pub async fn resolve(&self, query: &Query) -> Result<UpstreamResponse, UpstreamError> {
        match self.config.loadbalancer {
            LoadBalancer::RoundRobin => self.rr(query).await,
            // LoadBalancer::WeightedRR => self.weighted_rr(query).await,
        }
    }

    #[inline]
    async fn rr(&self, query: &Query) -> Result<UpstreamResponse, UpstreamError> {
        let mut err = None;
        for attempt in 0..5 {
            let server = &self.config.servers[attempt % self.config.servers.len()];
            if cfg!(debug_assertions) {
                debug!("using server: {}", server);
            }
            match self.query_dns(server, query).await {
                Ok(response) => return Ok(response),
                Err(e) => err = Some(e),
            }
        }

        Err(err.unwrap_or_else(|| UpstreamError::Upstream("all upsteams failed".into())))
    }

    // todo: weighted round robin and maybe other load balancing methods
    // #[inline]
    // async fn weighted_rr(&self, _query: &Query) -> Result<UpstreamResponse, UpstreamError> {
    //     todo!("weighted round robin")
    // }

    async fn query_dns(
        &self,
        server: &SocketAddr,
        query: &Query,
    ) -> Result<UpstreamResponse, UpstreamError> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(server).await?;
        socket.send(&query.raw[..]).await?;
        let mut buf = bytes::BytesMut::zeroed(4096);
        let n = timeout(self.config.timeout, socket.recv(&mut buf)).await??;
        buf.truncate(n);
        if cfg!(debug_assertions) {
            debug!("UpstreamResponse: {:?}", buf);
        }
        let code = if buf.len() >= 4 {
            match buf[3] & 0x0F {
                0 => ResponseCode::NoError,
                2 => ResponseCode::ServFail,
                3 => ResponseCode::NXDomain,
                5 => ResponseCode::Refused,
                n => ResponseCode::Unknown(n.into()),
            }
        } else {
            ResponseCode::ServFail
        };

        Ok(UpstreamResponse { code, raw: buf })
    }
}
