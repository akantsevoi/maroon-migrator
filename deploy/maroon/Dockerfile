FROM rust:1.86 AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y protobuf-compiler

COPY Cargo.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock
COPY common ./common
COPY gateway ./gateway
COPY maroon ./maroon

RUN cargo build -p maroon

FROM gcr.io/distroless/cc
# COPY --from=builder /app/target/release/maroon-migrator /
COPY --from=builder /app/target/debug/maroon /
ENV RUST_LOG=info
CMD ["./maroon"]
