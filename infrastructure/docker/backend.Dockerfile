ARG RUST_VERSION=1.97.1
# Keep the builder on Bookworm so its dynamically linked binaries are compatible
# with the Debian Bookworm runtime image below.
FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /workspace

RUN apt-get update \
    && apt-get install --yes --no-install-recommends build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY database ./database

RUN cargo build --locked --release \
        -p techatlas-api \
        -p techatlas-cli \
        -p techatlas-scheduler \
        -p techatlas-worker \
    && install -D target/release/techatlas-api /out/usr/local/bin/techatlas-api \
    && install -D target/release/techatlas-cli /out/usr/local/bin/techatlas-cli \
    && install -D target/release/techatlas-scheduler /out/usr/local/bin/techatlas-scheduler \
    && install -D target/release/techatlas-worker /out/usr/local/bin/techatlas-worker

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home techatlas

COPY --from=builder /out/ /
COPY --chmod=0555 infrastructure/docker/backend-entrypoint.sh /usr/local/bin/techatlas-entrypoint

USER techatlas
ENTRYPOINT ["/usr/local/bin/techatlas-entrypoint"]
