# ---- Build stage: static musl binary ----
FROM rust:1.99-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app

# Cache dependencies separately from application sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY static ./static
RUN touch src/main.rs && cargo build --release --locked

# ---- Runtime stage ----
FROM alpine:3.22
RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/fetcherr /usr/local/bin/fetcherr

ARG BUILD_ARCH=amd64
ARG BUILD_VERSION=dev
LABEL \
    io.hass.name="Fetcherr" \
    io.hass.description="Browse and request movies and series; download with Prowlarr and qBittorrent into Jellyfin" \
    io.hass.type="addon" \
    io.hass.arch="${BUILD_ARCH}" \
    io.hass.version="${BUILD_VERSION}" \
    org.opencontainers.image.source="https://github.com/luflax/fetcherr" \
    org.opencontainers.image.version="${BUILD_VERSION}"

EXPOSE 8734
CMD ["/usr/local/bin/fetcherr"]
