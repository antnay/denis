use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::Parser;
use hdrhistogram::Histogram;
use tokio::{net::UdpSocket, time::timeout};

#[derive(Parser, Debug)]
#[command(about = "Closed-loop UDP DNS load generator for the denis datapath")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:5356")]
    server: SocketAddr,

    #[arg(long, default_value_t = 10)]
    duration: u64,

    #[arg(long, default_value_t = 200)]
    concurrency: usize,

    #[arg(long, default_value = "example.com")]
    domain: String,

    #[arg(long, default_value_t = 2000)]
    timeout_ms: u64,
}

struct Stats {
    completed: u64,
    errors: u64,
    hist: Histogram<u64>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let query = Arc::new(build_query(&args.domain));

    // Warm the cache so we measure the hot (L1 hit) datapath, not the first miss.
    warmup(args.server, &query).await;

    let deadline = Instant::now() + Duration::from_secs(args.duration);
    let started = Instant::now();
    let timeout_dur = Duration::from_millis(args.timeout_ms);

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..args.concurrency {
        let server = args.server;
        let query = query.clone();
        set.spawn(async move { worker(server, query, deadline, timeout_dur).await });
    }

    let mut total = Stats {
        completed: 0,
        errors: 0,
        hist: new_hist(),
    };
    while let Some(res) = set.join_next().await {
        if let Ok(s) = res {
            total.completed += s.completed;
            total.errors += s.errors;
            total.hist.add(s.hist).ok();
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    let qps = total.completed as f64 / elapsed;

    println!("denis load test");
    println!(
        "  server={} duration={:.1}s concurrency={} domain={}",
        args.server, elapsed, args.concurrency, args.domain
    );
    println!("  completed = {}", total.completed);
    println!("  errors    = {}", total.errors);
    println!("  QPS       = {:.0}", qps);
    println!(
        "  latency us: p50={} p90={} p99={} p99.9={} max={}",
        total.hist.value_at_quantile(0.50),
        total.hist.value_at_quantile(0.90),
        total.hist.value_at_quantile(0.99),
        total.hist.value_at_quantile(0.999),
        total.hist.max(),
    );
}

async fn worker(
    server: SocketAddr,
    query: Arc<Vec<u8>>,
    deadline: Instant,
    timeout_dur: Duration,
) -> Stats {
    let mut stats = Stats {
        completed: 0,
        errors: 0,
        hist: new_hist(),
    };
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return stats,
    };
    if socket.connect(server).await.is_err() {
        return stats;
    }

    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        let t = Instant::now();
        if socket.send(&query).await.is_err() {
            stats.errors += 1;
            continue;
        }
        match timeout(timeout_dur, socket.recv(&mut buf)).await {
            Ok(Ok(_)) => {
                stats.completed += 1;
                stats.hist.record(t.elapsed().as_micros() as u64).ok();
            }
            _ => stats.errors += 1,
        }
    }
    stats
}

async fn warmup(server: SocketAddr, query: &[u8]) {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await {
        if socket.connect(server).await.is_ok() {
            let mut buf = [0u8; 512];
            for _ in 0..3 {
                let _ = socket.send(query).await;
                let _ = timeout(Duration::from_secs(2), socket.recv(&mut buf)).await;
            }
        }
    }
}

fn new_hist() -> Histogram<u64> {
    Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap()
}

fn build_query(domain: &str) -> Vec<u8> {
    let mut p = Vec::with_capacity(32);
    p.extend_from_slice(&0x1234u16.to_be_bytes()); // id
    p.extend_from_slice(&[0x01, 0x00]); // flags: RD
    p.extend_from_slice(&[0x00, 0x01]); // qdcount
    p.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // an/ns/ar
    for label in domain.split('.') {
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    p.push(0x00);
    p.extend_from_slice(&[0x00, 0x01]); // qtype A
    p.extend_from_slice(&[0x00, 0x01]); // qclass IN
    p
}
