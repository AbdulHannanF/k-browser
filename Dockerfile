# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# pkg-config is needed by some proc-macro crates; nothing else required because
# kitsune-cloud-mock uses reqwest with rustls-tls (pure Rust, no OpenSSL).
RUN apt-get update && apt-get install -y pkg-config && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release -p kitsune-cloud-mock

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

# ca-certificates lets reqwest verify HTTPS responses against Mozilla's root CAs.
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/kitsune-cloud-mock /app/kitsune-cloud-mock

ENV PORT=8080
ENV RUST_LOG=info

EXPOSE 8080
CMD ["./kitsune-cloud-mock"]
