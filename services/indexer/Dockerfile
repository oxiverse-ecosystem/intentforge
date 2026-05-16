FROM rust:1.88-slim-bookworm as builder

WORKDIR /usr/src/indexer
COPY Cargo.toml ./
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
COPY src ./src
RUN touch src/main.rs && RUSTFLAGS="-C target-cpu=native" cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /usr/src/indexer/target/release/indexer /usr/local/bin/indexer
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
EXPOSE 6000
CMD ["indexer"]
