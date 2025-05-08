FROM rust:1.86 AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y protobuf-compiler

COPY Cargo.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock

RUN mkdir -p maroon/src && \
    echo 'fn main() { println!("Dummy implementation"); }' > maroon/src/main.rs && \
    # cargo build -p maroon --release && \
    cargo build -p maroon && \
    rm -rf maroon/src

COPY maroon ./maroon

RUN find src -type f -name "*.rs" -exec touch {} \; && \
    # cargo build -p maroon --release
    cargo build -p maroon

FROM gcr.io/distroless/cc
# COPY --from=builder /app/target/release/maroon-migrator /
COPY --from=builder /app/target/debug/maroon-migrator /
ENV RUST_LOG=info
CMD ["./maroon-migrator"]