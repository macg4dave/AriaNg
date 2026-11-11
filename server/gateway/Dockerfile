# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/ariang-gateway /usr/local/bin/ariang-gateway
USER www-data
EXPOSE 8081
HEALTHCHECK --interval=5s --timeout=3s --start-period=5s --retries=5 \
    CMD ["curl", "--fail", "--silent", "--max-time", "2", "http://127.0.0.1:8081/healthz", "--output", "/dev/null"]
ENTRYPOINT ["ariang-gateway"]
CMD ["/run/secrets/gateway_config"]
