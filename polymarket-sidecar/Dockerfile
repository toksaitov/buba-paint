FROM node:22-slim AS builder

WORKDIR /app
COPY polymarket-sidecar/package.json polymarket-sidecar/package-lock.json ./
RUN npm ci
COPY polymarket-sidecar/ ./
RUN npm run build && npm prune --omit=dev

FROM node:22-slim

WORKDIR /app
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/package.json /app/package-lock.json ./
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/dist ./dist

EXPOSE 3210
CMD ["node", "dist/index.js"]
