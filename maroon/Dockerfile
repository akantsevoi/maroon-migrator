FROM rust:1.86 AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y protobuf-compiler

COPY Cargo.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock

RUN mkdir src && \
    echo 'fn main() { println!("Dummy implementation"); }' > src/main.rs && \
    # cargo build --release && \
    cargo build && \
    rm -rf src

COPY src ./src

RUN find src -type f -name "*.rs" -exec touch {} \; && \
    # cargo build --release
    cargo build

FROM gcr.io/distroless/cc
# COPY --from=builder /app/target/release/maroon-migrator /
COPY --from=builder /app/target/debug/maroon-migrator /
ENV RUST_LOG=info
CMD ["./maroon-migrator"]