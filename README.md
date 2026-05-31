# SpaceIO · Hearth

A self-hosted personal repository — a private corner of the internet for notes,
documents, photos, and small videos. Hearth is the calm literary direction:
single-column journal, warm paper tones, big Fraunces serif.

## Status

**Phase 1: vertical slice.** Boot the binary → enter passphrase → see one real
encrypted note rendered in Hearth typography. Phase 2+ adds the rest of the
six screens, search, upload/download, version history, and WebAuthn.

## Deploy (anywhere)

One script builds everything and serves it — on your laptop, a $5 VPS, a
Raspberry Pi, or inside Docker. No cloud account required.

```sh
./deploy.sh
```

It auto-detects what's available: if Docker is installed it builds an image
and runs a container; otherwise it builds natively (needs Rust + Node). Force
a path with `--docker` or `--native`. Useful flags:

```sh
./deploy.sh --port 8080 --data /srv/hearth     # custom port + data dir
./deploy.sh --docker --detach                  # background container
./deploy.sh --build-only                       # build, don't run
./deploy.sh --help                             # all options
```

For anything reachable off-localhost, front it with TLS (Caddy, nginx, or a
Cloudflare Tunnel) and pass `--secure-cookies` — Hearth itself speaks plain
HTTP and is single-tenant by design.

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
