FROM rust:1.94-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
ARG BUBA_GIT_SHA=unknown
ENV BUBA_GIT_SHA=$BUBA_GIT_SHA
COPY Cargo.toml Cargo.lock ./
COPY crates/buba-machine-telemetry/ crates/buba-machine-telemetry/
COPY bots/paint/ bots/paint/
# Create stub workspace members so cargo doesn't fail
RUN mkdir -p agent/src && echo 'fn main() {}' > agent/src/main.rs && \
    mkdir -p dashboard/server/src && echo 'fn main() {}' > dashboard/server/src/main.rs
COPY agent/Cargo.toml agent/Cargo.toml
COPY dashboard/server/Cargo.toml dashboard/server/Cargo.toml
RUN cargo build --release -p buba-paint && rm -rf agent/ dashboard/

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates sqlite3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/buba-paint /usr/local/bin/buba-paint
ENTRYPOINT ["buba-paint"]
CMD ["live", "--db-path", "/data/paint.db", "--balance", "200"]
