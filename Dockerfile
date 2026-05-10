# ─── Build stage ─────────────────────────────
FROM rust:1.78-slim-bookworm AS builder

WORKDIR /app
COPY . .

# Install dependencies if needed (e.g., for headless browser libs)
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo build --release -p kitsune-cloud-mock

# ─── Runtime stage ───────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

# Install Chromium + deps for headless browser automation
RUN apt-get update && apt-get install -y \
    chromium \
    libglib2.0-0 libnss3 libgconf-2-4 libfontconfig1 \
    libxss1 libappindicator3-1 libatk-bridge2.0-0 \
    libgtk-3-0 libasound2 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/kitsune-cloud-mock /app/kitsune-cloud-mock

# Render injects $PORT
ENV PORT=8080
ENV RUST_LOG=info

EXPOSE 8080

CMD ["./kitsune-cloud-mock"]
