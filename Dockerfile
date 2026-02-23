# Build stage
FROM rust:1.75-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock* ./

# Create dummy source to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/pop3ctl.rs

# Build dependencies only
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src
COPY tests ./tests

# Build the actual binary
RUN touch src/main.rs src/lib.rs src/bin/pop3ctl.rs && \
    cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Create non-root user
RUN useradd -r -s /bin/false pop3

# Copy binaries from builder
COPY --from=builder /app/target/release/pop3-server /usr/local/bin/
COPY --from=builder /app/target/release/pop3ctl /usr/local/bin/

# Copy default configuration
COPY config.toml /app/config.toml.example
COPY users.toml /app/users.toml.example

# Create directories
RUN mkdir -p /app/certs /app/domains /var/mail && \
    chown -R pop3:pop3 /app /var/mail

# Expose ports
# 995 - POP3 over TLS
# 8080 - Management API
EXPOSE 995 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -sf http://localhost:8080/api/v1/health || exit 1

USER pop3

# Default command
CMD ["pop3-server", "--config", "/app/config.toml", "--users", "/app/users.toml"]
