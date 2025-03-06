FROM rust:1.85 as builder
WORKDIR /app

COPY Cargo.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock
COPY src ./src
RUN cargo build --release

FROM gcr.io/distroless/cc
COPY --from=builder /app/target/release/maroon-migrator /
ENV RUST_LOG=info
CMD ["./maroon-migrator"]