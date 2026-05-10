# syntax=docker/dockerfile:1

# --- Build stage ---
FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /build

# Cache dependency compilation separately from application code.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Build the real application.
COPY src ./src
# Touch main.rs so cargo knows the stub binary is stale.
RUN touch src/main.rs \
    && cargo build --release

# --- Runtime stage ---
FROM alpine:3.21

RUN apk add --no-cache ca-certificates \
    && addgroup -S dupl \
    && adduser -S -G dupl dupl

COPY --from=builder /build/target/release/dupl /usr/local/bin/dupl

# /data is the bind-mount point for dupl.json and actions.json.
VOLUME ["/data"]

EXPOSE 8080

USER dupl

ENTRYPOINT ["dupl"]
CMD ["--help"]
