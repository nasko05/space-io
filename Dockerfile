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
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/hearth /usr/local/bin/hearth

# The vault lives here; mount a volume to persist it across container restarts.
VOLUME /data
EXPOSE 7777

# This image serves plain HTTP; cookies can't be flagged Secure without TLS.
# Front the container with a TLS proxy and unset this for production.
ENV HEARTH_INSECURE_COOKIES=1

ENTRYPOINT ["hearth", "serve", "--space-dir", "/data", "--listen", "0.0.0.0:7777"]
