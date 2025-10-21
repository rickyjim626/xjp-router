# ---- build stage ----
FROM rust:1 as builder
WORKDIR /app
COPY Cargo.toml Cargo.toml
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && cargo build --release
COPY src ./src
COPY config ./config
RUN cargo build --release

# ---- runtime ----
FROM gcr.io/distroless/cc-debian12
WORKDIR /app
COPY --from=builder /app/target/release/xjp-gateway /app/xjp-gateway
COPY --from=builder /app/config/xjp.example.toml /app/config/xjp.toml
EXPOSE 8080
USER 65532:65532
ENTRYPOINT ["/app/xjp-gateway"]
