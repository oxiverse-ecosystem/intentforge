FROM rust:1.88-slim-bookworm as builder

WORKDIR /usr/src/intent-engine
COPY Cargo.toml ./

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev g++ && rm -rf /var/lib/apt/lists/*

# We initialize a dummy project to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Now copy the real source and build (CACHE_BUST forces recompilation)
ARG CACHE_BUST=1
COPY src ./src
COPY setup.sh .
RUN touch src/main.rs && RUSTFLAGS="-C target-cpu=native" cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/intent-engine/target/release/intent-engine .
COPY setup.sh .
RUN sed -i 's/\r$//' setup.sh && chmod +x setup.sh

EXPOSE 3005
ENTRYPOINT ["./setup.sh"]
