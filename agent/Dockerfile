FROM rust:1.94-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates/buba-machine-telemetry/ crates/buba-machine-telemetry/
COPY agent/ agent/
# Create stub workspace members
RUN mkdir -p bots/paint/src && echo 'fn main() {}' > bots/paint/src/main.rs && \
    mkdir -p dashboard/server/src && echo 'fn main() {}' > dashboard/server/src/main.rs
COPY bots/paint/Cargo.toml bots/paint/Cargo.toml
COPY dashboard/server/Cargo.toml dashboard/server/Cargo.toml
RUN cargo build --release -p buba-agent && rm -rf bots/ dashboard/

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/buba-agent /usr/local/bin/buba-agent
EXPOSE 9090
ENTRYPOINT ["buba-agent"]
CMD ["--db-path", "/data/paint.db", "--port", "9090", "--monitor-only"]
