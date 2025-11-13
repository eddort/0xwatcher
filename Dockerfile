# Build stage
FROM rust:latest as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src
COPY tests ./tests

# Build the application
RUN touch src/main.rs && \
    cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/Oxwatcher /app/oxwatcher

# Copy example config (user should mount their own)
COPY config.example.yaml /app/config.example.yaml

# Create data directory for state files
RUN mkdir -p /app/data

# The application expects:
# - config.yaml in /app/config.yaml (or specify custom path)
# - data_dir in config points to /app/data
# - state files (balances.json, telegram_chats.json, alert_states.json) will be created in /app/data
#
# Volume mounts:
# - Mount your config.yaml to /app/config.yaml
# - Mount data volume to /app/data to persist state files

CMD ["/app/oxwatcher"]
