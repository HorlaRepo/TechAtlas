FROM rust:1.97.1-slim

WORKDIR /workspace

RUN rustup component add rustfmt clippy

CMD ["cargo", "run", "-p", "techatlas-api"]
