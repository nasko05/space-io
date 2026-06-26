# SpaceIO · Hearth — portable multi-stage build.
# Stage 1 builds the frontend, stage 2 compiles the Rust binary with the
# bundle embedded (rust-embed), stage 3 is a slim runtime with just the
# binary. Produces a single self-contained image that runs anywhere.

# ---- stage 1: frontend bundle --------------------------------------------
FROM node:24-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# ---- stage 2: rust binary ------------------------------------------------
FROM rust:1-slim AS build
WORKDIR /app
# libgit2-sys vendors + compiles libgit2 from C, so we need a toolchain;
# libssl-dev/pkg-config cover the crates that link against system OpenSSL.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
# The Rust crate embeds web/dist/ at compile time, so it must exist first.
COPY --from=web /web/dist ./web/dist
RUN cargo build --release

# ---- stage 3: runtime ----------------------------------------------------
FROM debian:bookworm-slim AS runtime
# ca-certificates + libssl3 cover the binary's TLS/link needs; curl is only here
# for the container HEALTHCHECK below (the Rust image has no python/wget to fall
# back on, unlike the sibling FastAPI app).
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/hearth /usr/local/bin/hearth

# Run as a non-root user. Create /data and hand it to that user before the
# VOLUME is declared, so the named volume inherits writable ownership when
# Docker first initialises it. uid 10001 matches the sibling app on this host.
RUN useradd -r -u 10001 -m appuser \
    && mkdir -p /data \
    && chown -R appuser:appuser /data
USER appuser

# The vault lives here; mount a volume to persist it across container restarts.
VOLUME /data
EXPOSE 7777

# Liveness probe for `docker compose`/the orchestrator. Hits the unauthenticated
# /healthz route; a boot that never binds or panics flips the container to
# unhealthy instead of silently serving errors.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:7777/healthz || exit 1

# No HEARTH_INSECURE_COOKIES here: in production the app runs behind the shared
# TLS proxy, so it must mark session cookies Secure. The cookie code only sets
# Secure when the request arrived over HTTPS (read from X-Forwarded-Proto), so
# plain-HTTP local runs still work. For local HTTP testing without a proxy, pass
# HEARTH_INSECURE_COOKIES=1 explicitly (deploy.sh does this for you).
ENTRYPOINT ["hearth", "serve", "--space-dir", "/data", "--listen", "0.0.0.0:7777"]
