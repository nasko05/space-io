# SpaceIO · Hearth

A self-hosted personal repository — a private corner of the internet for notes,
documents, photos, and small videos. Hearth is the calm literary direction:
single-column journal, warm paper tones, big Fraunces serif.

## Status

**Phase 1: vertical slice.** Boot the binary → enter passphrase → see one real
encrypted note rendered in Hearth typography. Phase 2+ adds the rest of the
six screens, search, upload/download, version history, and WebAuthn.

## Build

```sh
# Build the frontend bundle (embedded into the Rust binary in release mode)
cd web && npm install && npm run build && cd ..

# Build the server
cargo build --release
```

## Use

```sh
# Start the server. The data dir is created on demand; no init step needed.
./target/release/hearth serve --space-dir ./data --listen 127.0.0.1:7777
```

Open `http://127.0.0.1:7777/` and the first visit lands on a
**registration** page: pick an email + passphrase. The email is mapped
to a UUID-named subdirectory under `./data/` and the mapping is
persisted to `./data/.users.toml`, so everything survives restarts.

For scripted setups you can pre-register a user from the CLI:

```sh
./target/release/hearth init --space-dir ./data \
  --email you@home.lan --passphrase 'correct horse battery staple'
```

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
