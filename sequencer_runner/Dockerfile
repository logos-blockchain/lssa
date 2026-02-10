# Chef stage - uses pre-built cargo-chef image
FROM lukemathwalker/cargo-chef:latest-rust-1.91.1-slim-trixie AS chef

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libclang-dev \
    clang \
    cmake \
    ninja-build \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install r0vm (manual build as it's portable across different host platforms)
RUN git clone --depth 1 --branch release-3.0 https://github.com/risc0/risc0.git
RUN git clone --depth 1 --branch r0.1.91.1 https://github.com/risc0/rust.git
WORKDIR /risc0
RUN cargo install --path rzup
RUN rzup build --path /rust rust --verbose
RUN cargo install --path risc0/cargo-risczero
ENV PATH="/root/.cargo/bin:/root/.risc0/bin:${PATH}"
RUN cp "$(which r0vm)" /usr/local/bin/r0vm
RUN test -x /usr/local/bin/r0vm
RUN r0vm --version

# Install logos blockchain circuits
RUN curl -sSL https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/main/scripts/setup-logos-blockchain-circuits.sh | bash

WORKDIR /sequencer_runner

# Planner stage - generates dependency recipe
FROM chef AS planner
COPY . .
RUN cargo chef prepare --bin sequencer_runner --recipe-path recipe.json

# Builder stage - builds dependencies and application
FROM chef AS builder
COPY --from=planner /sequencer_runner/recipe.json recipe.json
# Build dependencies only (this layer will be cached)
RUN cargo chef cook --bin sequencer_runner --release --recipe-path recipe.json

# Copy source code
COPY . .

# Build the actual application
RUN cargo build --release --bin sequencer_runner

# Strip debug symbols to reduce binary size
RUN strip /sequencer_runner/target/release/sequencer_runner

# Runtime stage - minimal image
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update \
 && apt-get install -y gosu jq \
 && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 -s /bin/bash sequencer_user && \
    mkdir -p /sequencer_runner /etc/sequencer_runner && \
    chown -R sequencer_user:sequencer_user /sequencer_runner /etc/sequencer_runner

# Copy binary from builder
COPY --from=builder --chown=sequencer_user:sequencer_user /sequencer_runner/target/release/sequencer_runner /usr/local/bin/sequencer_runner

# Copy r0vm binary from builder
COPY --from=builder --chown=sequencer_user:sequencer_user /usr/local/bin/r0vm /usr/local/bin/r0vm

# Copy logos blockchain circuits from builder
COPY --from=builder --chown=sequencer_user:sequencer_user /root/.logos-blockchain-circuits /home/sequencer_user/.logos-blockchain-circuits

# Copy entrypoint script
COPY sequencer_runner/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

# Volume for configuration directory
VOLUME ["/etc/sequencer_runner"]

# Expose default port
EXPOSE 3040

# Health check (TODO #244: Replace when a real health endpoint is available)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl http://localhost:3040 \
    -H "Content-Type: application/json" \
    -d "{ \
        \"jsonrpc\": \"2.0\", \
        \"method\": \"hello\", \
        \"params\": {}, \
        \"id\": 1 \
    }" || exit 1

# Run the application
ENV RUST_LOG=info

# Set explicit location for r0vm binary
ENV RISC0_SERVER_PATH=/usr/local/bin/r0vm

USER root

ENTRYPOINT ["/docker-entrypoint.sh"]

WORKDIR /sequencer_runner
CMD ["sequencer_runner", "/etc/sequencer_runner"]
