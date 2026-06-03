#!/usr/bin/env bash
# SpaceIO · Hearth — one-command build & deploy.
#
# Runs anywhere: your laptop, a $5 VPS, a Raspberry Pi, or inside Docker.
# No cloud provider, no account, no IAM, no CloudFormation. Two ways to run:
#
#   ./deploy.sh             # auto: use Docker if installed, else build natively
#   ./deploy.sh --native    # force a native build (needs Rust + Node on the host)
#   ./deploy.sh --docker     # force the Docker path (needs Docker only)
#
# The frontend bundle is embedded into the Rust binary at compile time, so the
# build order (web → cargo) is handled for you in both paths.
#
# Server-side variables — cookie mode and the AI-assistant keys
# (HEARTH_OPENROUTER_API_KEY, …) — can live in a .env file next to this
# script instead of being exported by hand. A ./.env is picked up
# automatically (Docker: --env-file, native: sourced). See .env.example.
#
# Common flags:
#   --port N         port to serve on            (default 7777)
#   --host ADDR      listen address              (default 0.0.0.0)
#   --data DIR       data directory for the vault (default ./data)
#   --build-only     build, don't run
#   --detach         (docker) run in the background
#   --name NAME      (docker) container name      (default hearth)
#   --secure-cookies require HTTPS for cookies (set when behind a TLS proxy)
#   --env-file PATH  load server env vars from a file (default: ./.env if any)
#   --no-env         ignore a ./.env file even if present
#   -h, --help       this help
#
# Put TLS in front (Caddy, nginx, Cloudflare Tunnel) for anything reachable
# off-localhost — Hearth speaks plain HTTP and is single-tenant by design.

set -euo pipefail

cd "$(dirname "$0")"

# ---- defaults -------------------------------------------------------------
MODE="auto"          # auto | native | docker
PORT="${HEARTH_PORT:-7777}"
HOST="${HEARTH_HOST:-0.0.0.0}"
DATA_DIR="${HEARTH_DATA:-./data}"
IMAGE="${HEARTH_IMAGE:-hearth}"
CONTAINER="${HEARTH_CONTAINER:-hearth}"
BUILD_ONLY=0
DETACH=0
INSECURE_COOKIES=1   # plain HTTP by default; flip with --secure-cookies
ENV_FILE="${HEARTH_ENV_FILE:-}"   # --env-file PATH; else auto-detect ./.env
NO_ENV=0             # --no-env: ignore ./.env even if present

die() { echo "error: $*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# Print the leading comment block (everything from line 2 to the first blank
# line) as help, so the flag list stays in sync with the header above.
usage() { sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'; }

# ---- arg parse ------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    --native) MODE="native"; shift ;;
    --docker) MODE="docker"; shift ;;
    --port)   PORT="${2:?--port needs a value}"; shift 2 ;;
    --port=*) PORT="${1#--port=}"; shift ;;
    --host)   HOST="${2:?--host needs a value}"; shift 2 ;;
    --host=*) HOST="${1#--host=}"; shift ;;
    --data)   DATA_DIR="${2:?--data needs a value}"; shift 2 ;;
    --data=*) DATA_DIR="${1#--data=}"; shift ;;
    --name)   CONTAINER="${2:?--name needs a value}"; shift 2 ;;
    --name=*) CONTAINER="${1#--name=}"; shift ;;
    --build-only) BUILD_ONLY=1; shift ;;
    --detach|-d) DETACH=1; shift ;;
    --secure-cookies) INSECURE_COOKIES=0; shift ;;
    --env-file) ENV_FILE="${2:?--env-file needs a value}"; shift 2 ;;
    --env-file=*) ENV_FILE="${1#--env-file=}"; shift ;;
    --no-env) NO_ENV=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown argument: $1 (try --help)" ;;
  esac
done

# ---- env file -------------------------------------------------------------
# Resolve which env file (if any) to feed the server. Auto-pick ./.env unless
# the caller passed one explicitly or opted out with --no-env. Docker reads it
# verbatim via --env-file; the native path sources it. Either way: write plain
# KEY=value lines, no surrounding quotes (Docker keeps them literally).
if [ -z "$ENV_FILE" ] && [ "$NO_ENV" -eq 0 ] && [ -f .env ]; then
  ENV_FILE=".env"
fi
ENV_FILE_ABS=""
if [ -n "$ENV_FILE" ]; then
  [ -f "$ENV_FILE" ] || die "env file not found: $ENV_FILE"
  ENV_FILE_ABS="$(cd "$(dirname "$ENV_FILE")" && pwd)/$(basename "$ENV_FILE")"
  echo ">> env file: $ENV_FILE_ABS"
fi

# ---- mode resolution ------------------------------------------------------
if [ "$MODE" = "auto" ]; then
  if have docker; then
    MODE="docker"
  elif have cargo && have npm; then
    MODE="native"
  else
    die "need either Docker, or Rust (cargo) + Node (npm) installed. Install one and re-run."
  fi
fi
echo ">> mode: $MODE"

# =========================================================================
# Native path: build web → build binary → run
# =========================================================================
deploy_native() {
  have cargo || die "cargo not found — install Rust from https://rustup.rs"
  have npm   || die "npm not found — install Node 18+ from https://nodejs.org"

  echo ">> building frontend (web/dist)"
  (
    cd web
    if [ -f package-lock.json ]; then npm ci; else npm install; fi
    npm run build
  )

  echo ">> building release binary"
  cargo build --release

  local bin="./target/release/hearth"
  [ -x "$bin" ] || die "build succeeded but $bin is missing"

  if [ "$BUILD_ONLY" -eq 1 ]; then
    echo ">> build complete: $bin"
    return 0
  fi

  mkdir -p "$DATA_DIR"
  echo ">> serving on http://$HOST:$PORT  (data: $DATA_DIR)"
  echo "   first visit shows the registration page — pick an email + passphrase there."
  # Load the env file first so an explicit deploy flag (below) still wins.
  if [ -n "$ENV_FILE_ABS" ]; then
    set -a
    # The env file path is user-supplied and dynamic, so shellcheck genuinely
    # cannot follow the source; SC1090 here is unavoidable, not a shortcut.
    # shellcheck disable=SC1090
    . "$ENV_FILE_ABS"
    set +a
  fi
  [ "$INSECURE_COOKIES" -eq 1 ] && export HEARTH_INSECURE_COOKIES=1
  exec "$bin" serve --space-dir "$DATA_DIR" --listen "$HOST:$PORT"
}

# =========================================================================
# Docker path: build image → run container
# =========================================================================
deploy_docker() {
  have docker || die "docker not found — install it or re-run with --native"

  echo ">> building image: $IMAGE"
  docker build -t "$IMAGE" .

  if [ "$BUILD_ONLY" -eq 1 ]; then
    echo ">> image built: $IMAGE"
    return 0
  fi

  # Absolute path for the bind mount; Docker won't accept a relative one.
  mkdir -p "$DATA_DIR"
  local abs_data
  abs_data="$(cd "$DATA_DIR" && pwd)"

  # Replace any prior container with the same name so re-runs are idempotent.
  if docker ps -aq -f "name=^${CONTAINER}$" | grep -q .; then
    echo ">> removing existing container: $CONTAINER"
    docker rm -f "$CONTAINER" >/dev/null
  fi

  local run=(docker run --name "$CONTAINER"
    -p "$PORT:7777"
    -v "$abs_data:/data"
    --restart unless-stopped)
  # --env-file is read at run time, so secrets never get baked into the image.
  # A trailing -e wins over the file, keeping the deploy flag authoritative.
  [ -n "$ENV_FILE_ABS" ] && run+=(--env-file "$ENV_FILE_ABS")
  [ "$INSECURE_COOKIES" -eq 1 ] && run+=(-e HEARTH_INSECURE_COOKIES=1)

  if [ "$DETACH" -eq 1 ]; then
    run+=(-d)
    "${run[@]}" "$IMAGE"
    echo ">> running in background as '$CONTAINER' → http://$HOST:$PORT"
    echo "   logs:  docker logs -f $CONTAINER"
    echo "   stop:  docker rm -f $CONTAINER"
  else
    run+=(-it --rm)
    echo ">> serving on http://$HOST:$PORT  (data: $abs_data)  — Ctrl-C to stop"
    exec "${run[@]}" "$IMAGE"
  fi
}

case "$MODE" in
  native) deploy_native ;;
  docker) deploy_docker ;;
  *) die "unreachable mode: $MODE" ;;
esac
