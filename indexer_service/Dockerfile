# Chef stage - uses pre-built cargo-chef image
FROM lukemathwalker/cargo-chef:latest-rust-1.91.1-slim-trixie AS chef

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libclang-dev \
    clang \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /indexer_service

# Planner stage - generates dependency recipe
FROM chef AS planner
COPY . .
RUN cargo chef prepare --bin indexer_service --recipe-path recipe.json

# Builder stage - builds dependencies and application
FROM chef AS builder
COPY --from=planner /indexer_service/recipe.json recipe.json
# Build dependencies only (this layer will be cached)
RUN cargo chef cook --bin indexer_service --release --recipe-path recipe.json

# Copy source code
COPY . .

# Build the actual application
RUN cargo build --release --bin indexer_service

# Strip debug symbols to reduce binary size
RUN strip /indexer_service/target/release/indexer_service

# Runtime stage - minimal image
FROM debian:trixie-slim

# Create non-root user for security
RUN useradd -m -u 1000 -s /bin/bash indexer_service_user

# Copy binary from builder
COPY --from=builder --chown=indexer_service_user:indexer_service_user /indexer_service/target/release/indexer_service /usr/local/bin/indexer_service

# Expose default port
EXPOSE 8779

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl http://localhost:8779 \
    -H "Content-Type: application/json" \
    -d "{ \
        \"jsonrpc\": \"2.0\", \
        \"method\": \"get_schema\", \
        \"params\": {}, \
        \"id\": 1 \
    }" || exit 1

# Run the application
ENV RUST_LOG=info

USER indexer_service_user

WORKDIR /indexer_service
CMD ["indexer_service"]
