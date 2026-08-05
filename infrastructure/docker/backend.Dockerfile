ARG RUST_VERSION=1.97.1
FROM rust:${RUST_VERSION}-slim AS builder

ARG BINARY
WORKDIR /workspace

RUN apt-get update \
    && apt-get install --yes --no-install-recommends build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY database ./database

RUN cargo build --locked --release -p "${BINARY}" \
    && install -D "target/release/${BINARY}" /out/techatlas

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home techatlas

COPY --from=builder /out/techatlas /usr/local/bin/techatlas

USER techatlas
ENTRYPOINT ["/usr/local/bin/techatlas"]
