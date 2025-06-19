# Multi-stage Docker build for HFT Trading System
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static

# Set up workspace
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ ./crates/

# Copy source code
COPY src/ ./src/
COPY tests/ ./tests/

# Build with optimizations for production
RUN cargo build --release --locked

# Runtime stage - minimal image
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    tzdata

# Create non-root user for security
RUN addgroup -g 1000 hft && \
    adduser -D -s /bin/sh -u 1000 -G hft hft

# Create directories
RUN mkdir -p /app/config /app/logs /app/data && \
    chown -R hft:hft /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/hft /app/hft
COPY deployment/config/ /app/config/

# Set ownership
RUN chown -R hft:hft /app

# Switch to non-root user
USER hft

WORKDIR /app

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ./hft --health-check || exit 1

# Expose metrics port
EXPOSE 8080

# Set environment variables
ENV RUST_LOG=info
ENV HFT_CONFIG_PATH=/app/config/production.toml
ENV HFT_LOG_LEVEL=info
ENV HFT_METRICS_PORT=8080

# Run the application
CMD ["./hft"]