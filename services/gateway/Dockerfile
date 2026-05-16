FROM rust:1.88-slim-bookworm as builder

WORKDIR /usr/src/gateway
COPY Cargo.toml ./

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/gateway/target/release/gateway .

EXPOSE 4000
CMD ["./gateway"]
