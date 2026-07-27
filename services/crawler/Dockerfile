FROM rust:1.88-slim-bookworm as builder

WORKDIR /usr/src/crawler
COPY Cargo.toml ./

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/crawler/target/release/crawler .
# Queue snapshot dir — must exist even when no volume is mounted (the binary
# also mkdir -p's it defensively at boot).
RUN mkdir -p /app/queue_data

EXPOSE 5001
CMD ["./crawler"]
