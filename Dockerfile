FROM rust:1.97-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/sub-forge/Cargo.toml ./crates/sub-forge/Cargo.toml
COPY crates/sub-forge/src ./crates/sub-forge/src
COPY frontend ./frontend
RUN cargo build --release --locked --package sub-forge

FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sub-forge /usr/local/bin/sub-forge

RUN mkdir -p /app/data && chown 65532:65532 /app/data

USER 65532:65532

EXPOSE 8080

CMD ["sub-forge"]
