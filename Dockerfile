# Stage 1: Build
FROM rust:1.95-slim-bookworm AS builder

WORKDIR /build
COPY . .

# Build with testnet features
RUN cargo build --release --features testnet

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create ewatt user
RUN useradd -m -s /bin/bash ewatt

COPY --from=builder /build/target/release/ewatts-protocol /usr/local/bin/ewattsd
COPY --from=builder /build/ewatts_dashboard.html /etc/ewatts/dashboard.html
COPY --from=builder /build/testnet/entrypoint.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh
RUN mkdir -p /data && chown ewatt:ewatt /data

USER ewatt
WORKDIR /data

VOLUME ["/data"]

EXPOSE 9000 8080

ENTRYPOINT ["/entrypoint.sh"]
