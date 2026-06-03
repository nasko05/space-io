# SpaceIO · Hearth

A self-hosted personal repository — a private corner of the internet for notes,
documents, photos, and small videos. Hearth is the calm literary direction:
single-column journal, warm paper tones, big Fraunces serif.

## Status

**Phase 1: vertical slice.** Boot the binary → enter passphrase → see one real
encrypted note rendered in Hearth typography. Phase 2+ adds the rest of the
six screens, search, upload/download, version history, and WebAuthn.

## Deploy (anywhere)

### Recommended: Docker Compose (CI/CD)

The simplest durable deploy is [`docker-compose.yml`](./docker-compose.yml):

```sh
docker compose up -d --build
```

This rebuilds the image from the current source and recreates the container,
while the named volume `hearth-data` keeps every user's notes, documents, and
uploads. Re-run the same command to ship new code — your data is reused.

### One script: `deploy.sh`

There's also one script that builds everything and serves it — on your laptop,
a $5 VPS, a Raspberry Pi, or inside Docker. No cloud account required.

```sh
./deploy.sh
```

It auto-detects what's available: if Docker is installed it builds an image
and runs a container; otherwise it builds natively (needs Rust + Node). Force
a path with `--docker` or `--native`. Useful flags:

```sh
./deploy.sh --data /srv/hearth     # persist to a host directory (bind mount)
./deploy.sh --port 8080            # custom port
./deploy.sh --docker --detach      # background container
./deploy.sh --build-only           # build, don't run
./deploy.sh --help                 # all options
```

For anything reachable off-localhost, front it with TLS (Caddy, nginx, or a
Cloudflare Tunnel) and pass `--secure-cookies` — Hearth itself speaks plain
HTTP and is single-tenant by design.

### Data persistence

User data is **never lost on redeploy**:

- **It lives in a persistent volume.** By default that's the Docker named volume
  `hearth-data` (used by both `docker compose` and `./deploy.sh`). Pass
  `./deploy.sh --data /abs/path` to use a host directory (bind mount) instead.
  Either way it's mounted at `/data` inside the container.
- **It survives image rebuilds and container recreation.** Redeploying
  (`docker compose up -d --build`, or re-running `./deploy.sh`) stops and removes
  the old container only — it never removes the volume or the host data dir.
- **The app never overwrites existing data on startup.** Space initialization is
  idempotent: it only creates what's missing, so restarts and upgrades keep all
  existing notes, documents, and uploads.
- A named volume is **cwd-independent**, so running the deploy from any directory
  reuses the same data — unlike a relative `./data` bind mount, which would
  silently create a fresh empty dir if you deployed from elsewhere.

## Build manually

```sh
# The frontend bundle is embedded into the Rust binary at compile time,
# so it must build first.
cd web && npm install && npm run build && cd ..
cargo build --release

# Start the server. The data dir is created on demand; no init step needed.
./target/release/hearth serve --space-dir ./data --listen 127.0.0.1:7777
```

Open `http://127.0.0.1:7777/` and the first visit lands on a
**registration** page: pick an email + passphrase. The email is mapped
to a UUID-named subdirectory under `./data/` and the mapping is
persisted to `./data/.users.toml`, so everything survives restarts.
Additional users can register from the login screen via the "Register"
link.

## Layout

```
src/        Rust backend (axum)
web/        Vite + React + TypeScript frontend
data/       Run-time root (gitignored)
  ├── .users.toml          # email -> uuid mapping
  └── <uuid>/              # one folder per registered user
       ├── .space.toml      # salt + verifier hash + (optional) passkey
       └── space/           # git repo with encrypted .age blobs
```

The bundle from the UI team that this project was built from is in `SPEC.md`
(forthcoming) and informed all six artboards' visual identity.
