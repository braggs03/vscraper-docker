# -----------------------------
# 1️⃣ Build client (Vite)
# -----------------------------
FROM node:20-alpine AS client-builder

WORKDIR /client
COPY client/package*.json ./
RUN npm install

COPY client/ .
RUN npm run build

# -----------------------------
# 2️⃣ Build Rust server
# -----------------------------
FROM rust:bullseye AS server-builder

WORKDIR /app

# Install build dependencies
RUN apt update
RUN apt install -y musl-dev

# Copy manifests first (for better caching)
COPY server/Cargo.toml server/Cargo.lock ./

# Dummy main to cache deps
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy real source
COPY server/src ./src
COPY server/.sqlx ./.sqlx
COPY server/migrations ./migrations

RUN cargo build --release

# -----------------------------
# 3️⃣ Final runtime image
# -----------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=server-builder /app/target/release/server ./server
COPY --from=client-builder /client/dist ./static

EXPOSE 3000

CMD ["./server"]
