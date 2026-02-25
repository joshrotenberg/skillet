FROM rust:1.90-slim AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release --bin skillet

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/skillet /usr/local/bin/skillet

ENTRYPOINT ["skillet"]
