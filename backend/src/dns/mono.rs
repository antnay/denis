use std::{
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use ftlog::error;
use monoio::net::udp::UdpSocket;
use socket2::{Domain, Protocol, Socket, Type};

use crate::{
    analytics::{AnalyticsProducer, DnsQueryEvent},
    config::SharedConfig,
    handler::{Parser, Query, UpstreamResponse},
    repo::Cache,
};

const RECV_BUF: usize = 512;

pub struct ColdRequest {
    pub query: Query,
    pub total: Instant,
    pub timestamp_ms: u64,
    pub reply: flume::Sender<Vec<u8>>,
}

struct Worker {
    cache: Arc<Cache>,
    config: SharedConfig,
    analytics: Arc<AnalyticsProducer>,
    cold: flume::Sender<ColdRequest>,
}

pub fn spawn_workers(
    addr: SocketAddr,
    pins: Vec<Option<core_affinity::CoreId>>,
    cache: Arc<Cache>,
    config: SharedConfig,
    analytics: Arc<AnalyticsProducer>,
    cold: flume::Sender<ColdRequest>,
) {
    for (id, pin) in pins.into_iter().enumerate() {
        let worker = Worker {
            cache: cache.clone(),
            config: config.clone(),
            analytics: analytics.clone(),
            cold: cold.clone(),
        };
        std::thread::Builder::new()
            .name(format!("monoio-{id}"))
            .spawn(move || {
                if let Some(core) = pin {
                    core_affinity::set_for_current(core);
                }
                let std_sock = match bind_reuseport(addr) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("monoio worker {id} bind failed: {e}");
                        return;
                    }
                };
                let mut rt = monoio::RuntimeBuilder::<monoio::FusionDriver>::new()
                    .build()
                    .expect("build monoio runtime");
                rt.block_on(worker.run(std_sock));
            })
            .expect("spawn monoio worker thread");
    }
}

fn bind_reuseport(addr: SocketAddr) -> std::io::Result<std::net::UdpSocket> {
    let sock = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&addr.into())?;
    Ok(sock.into())
}

impl Worker {
    async fn run(self, std_sock: std::net::UdpSocket) {
        let socket = match UdpSocket::from_std(std_sock) {
            Ok(s) => Rc::new(s),
            Err(e) => {
                error!("monoio from_std failed: {e}");
                return;
            }
        };
        let this = Rc::new(self);

        loop {
            let buf = vec![0u8; RECV_BUF];
            let (res, mut buf) = socket.recv_from(buf).await;
            let (len, src) = match res {
                Ok(v) => v,
                Err(e) => {
                    error!("monoio recv: {e}");
                    continue;
                }
            };
            buf.truncate(len);

            // Hot path (blocked / L1 hit) runs inline on this core — no task
            // spawn, run-to-completion. Only a cache miss, which must await the
            // tokio cold path, is spawned so it doesn't stall recv.
            match this.classify(buf).await {
                Outcome::Reply(resp) => {
                    let (r, _) = socket.send_to(resp, src).await;
                    if let Err(e) = r {
                        error!("monoio send: {e}");
                    }
                }
                Outcome::Miss {
                    query,
                    total,
                    timestamp_ms,
                } => {
                    let this = this.clone();
                    let socket = socket.clone();
                    monoio::spawn(async move {
                        if let Some(resp) = this.serve_miss(query, total, timestamp_ms).await {
                            let (r, _) = socket.send_to(resp, src).await;
                            if let Err(e) = r {
                                error!("monoio send: {e}");
                            }
                        }
                    });
                }
                Outcome::Drop => {}
            }
        }
    }

    async fn classify(&self, packet: Vec<u8>) -> Outcome {
        let total = Instant::now();
        let timestamp_ms = now_ms();

        let query = match Parser::parse_udp(packet).await {
            Ok(q) => q,
            Err(_) => return Outcome::Drop,
        };

        if self.cache.is_blocked(&query.name) {
            let resp = UpstreamResponse::blocked(&query, self.config.load().blocking_mode);
            self.analytics.send(DnsQueryEvent {
                timestamp_ms,
                domain: query.name,
                query_type: u16::from(query.query_type),
                response_code: u16::from(resp.code),
                cache_hit: false,
                blocked: true,
                latency_us: total.elapsed().as_micros() as u64,
            });
            return Outcome::Reply(resp.raw);
        }

        if let Some(cached) = self.cache.l1_get(&query).await {
            let resp = UpstreamResponse::cached(&query, cached);
            self.analytics.send(DnsQueryEvent {
                timestamp_ms,
                domain: query.name,
                query_type: u16::from(query.query_type),
                response_code: u16::from(resp.code),
                cache_hit: true,
                blocked: false,
                latency_us: total.elapsed().as_micros() as u64,
            });
            return Outcome::Reply(resp.raw);
        }

        Outcome::Miss {
            query,
            total,
            timestamp_ms,
        }
    }

    async fn serve_miss(&self, query: Query, total: Instant, timestamp_ms: u64) -> Option<Vec<u8>> {
        let (tx, rx) = flume::bounded(1);
        self.cold
            .send_async(ColdRequest {
                query,
                total,
                timestamp_ms,
                reply: tx,
            })
            .await
            .ok()?;
        rx.recv_async().await.ok()
    }
}

enum Outcome {
    Reply(Vec<u8>),
    Miss {
        query: Query,
        total: Instant,
        timestamp_ms: u64,
    },
    Drop,
}

pub async fn cold_path(rx: flume::Receiver<ColdRequest>, handler: Arc<crate::handler::QueryHandler>) {
    while let Ok(req) = rx.recv_async().await {
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Ok(resp) = handler
                .handle_parsed(req.query, req.total, req.timestamp_ms)
                .await
            {
                let _ = req.reply.send_async(resp).await;
            }
        });
    }
}

#[inline]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
