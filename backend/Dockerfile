FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

# ── Planner: compute the dependency recipe ────────────────────────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Builder: cook deps (cached layer), then build the binary ─────────────────
FROM chef AS builder
RUN apt-get update && apt-get install -y \
    cmake libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
# This layer is only invalidated when dependencies change
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin dnsserver

# ── Runtime: minimal image ────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y \
    libssl3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/dnsserver /usr/local/bin/dnsserver
EXPOSE 53/udp 8080/tcp
ENTRYPOINT ["/usr/local/bin/dnsserver"]
