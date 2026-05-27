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
# First-run: initialise a new space with a passphrase
./target/release/hearth init --space-dir ./data

# Serve it
./target/release/hearth serve --space-dir ./data --listen 127.0.0.1:7777
```

Open `http://127.0.0.1:7777/` and unlock with your passphrase.

## Layout

```
src/        Rust backend (axum)
web/        Vite + React + TypeScript frontend
data/       Run-time space (gitignored; created by `hearth init`)
```

The bundle from the UI team that this project was built from is in `SPEC.md`
(forthcoming) and informed all six artboards' visual identity.
