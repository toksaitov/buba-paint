#!/usr/bin/env bash
set -euo pipefail

# Setup script for buba-paint on Ubuntu 24.04
# Usage: ssh buba-paint 'bash -s' < scripts/setup-ubuntu.sh

echo "=== Installing system packages ==="
sudo apt update
sudo apt install -y nodejs npm sqlite3 python3 python3-pip python3-matplotlib python3-pandas python3-numpy

echo "=== Node version ==="
node --version

echo "=== Installing npm dependencies ==="
cd ~/buba-paint
npm install

echo "=== Creating data directory ==="
mkdir -p data

echo "=== Verifying typecheck ==="
npx tsc --noEmit

echo "=== Done ==="
