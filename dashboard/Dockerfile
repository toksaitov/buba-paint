# Stage 1: Build frontend
FROM node:22-slim AS frontend
WORKDIR /app/dashboard/client
COPY dashboard/client/package.json dashboard/client/package-lock.json ./
RUN npm ci
COPY dashboard/client/ .
RUN npm run build

# Stage 2: Build backend
FROM rust:1.94-slim AS backend
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates/buba-machine-telemetry/ crates/buba-machine-telemetry/
COPY dashboard/server/ dashboard/server/
# Create stub workspace members
RUN mkdir -p bots/paint/src && echo 'fn main() {}' > bots/paint/src/main.rs && \
    mkdir -p agent/src && echo 'fn main() {}' > agent/src/main.rs
COPY bots/paint/Cargo.toml bots/paint/Cargo.toml
COPY agent/Cargo.toml agent/Cargo.toml
RUN cargo build --release -p buba-dashboard && rm -rf bots/ agent/

# Stage 3: Runtime
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=backend /app/target/release/buba-dashboard /usr/local/bin/buba-dashboard
COPY --from=frontend /app/dashboard/client/dist /srv/static
EXPOSE 3001
ENTRYPOINT ["buba-dashboard"]
CMD ["--config", "/etc/dashboard.toml", "--static-dir", "/srv/static"]
