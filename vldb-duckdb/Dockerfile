FROM rust:1.94-bookworm AS builder

WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        clang \
        cmake \
        libprotobuf-dev \
        libssl-dev \
        protobuf-compiler \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock build.rs rust-toolchain.toml ./
COPY proto ./proto
COPY src ./src

RUN cargo build --locked --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN mkdir -p /app/config /app/data

COPY --from=builder /workspace/target/release/vldb-duckdb /usr/local/bin/vldb-duckdb
COPY docker/vldb-duckdb.json /app/config/vldb-duckdb.json

EXPOSE 50052
VOLUME ["/app/data"]

ENTRYPOINT ["/usr/local/bin/vldb-duckdb"]
CMD ["--config", "/app/config/vldb-duckdb.json"]
