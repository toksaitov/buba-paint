#!/usr/bin/env bash
set -euo pipefail

echo "=== Installing host packages ==="
sudo apt update
sudo apt install -y \
  build-essential \
  curl \
  git \
  pkg-config \
  sqlite3 \
  libssl-dev \
  nodejs \
  npm \
  python3 \
  python3-matplotlib \
  python3-numpy \
  python3-pandas

echo "=== Verifying toolchains ==="
rustc --version
cargo --version
node --version
npm --version

echo "=== Installing dashboard dependencies ==="
cd ~/buba-paint/dashboard/client
npm install

echo "=== Installing sidecar dependencies ==="
cd ~/buba-paint/polymarket-sidecar
npm install

echo "=== Building Rust workspace ==="
cd ~/buba-paint
cargo build --release --workspace

echo "=== Building dashboard client ==="
cd ~/buba-paint/dashboard/client
npm run build

echo "=== Building Polymarket sidecar ==="
cd ~/buba-paint/polymarket-sidecar
npm run build

echo "=== Done ==="
