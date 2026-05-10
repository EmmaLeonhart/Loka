# Build stage
FROM rust:1.82-slim AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p loka-cli

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/loka /usr/local/bin/loka

# Default data directory
RUN mkdir -p /data
VOLUME /data

EXPOSE 3030

ENTRYPOINT ["loka"]
CMD ["serve", "--port", "3030", "--data-dir", "/data"]
