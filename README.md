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

## AI assistant

Hearth ships an optional in-app AI assistant (the spark button, bottom-right).
It can **read, search, write, move, reorganise, and tag** your notes, and —
when enabled — **search the web**. It runs as a server-side tool-using agent
against an [OpenRouter](https://openrouter.ai)-compatible chat model.

Two things are deliberate:

- **You approve every change.** The assistant reads and searches on its own,
  but anything that *writes, moves, deletes, or retags* a file is shown to you
  as a proposal first. Approved changes flow through the same endpoints the UI
  uses, so every edit is a git commit and every delete lands in the trash —
  fully reversible.
- **Privacy.** The vault stays encrypted at rest, but when the assistant reads
  a note its plaintext is sent to your configured model provider (OpenRouter)
  so it can help. If that trade-off isn't for you, simply leave the key unset —
  the assistant stays hidden and nothing leaves the machine.

Enable it by setting an API key in the server's environment, then restart:

```sh
export HEARTH_OPENROUTER_API_KEY=sk-or-...     # required to turn the agent on
./target/release/hearth serve --space-dir ./data
```

All agent settings (server-side environment variables):

| Variable | Default | Purpose |
| --- | --- | --- |
| `HEARTH_OPENROUTER_API_KEY` | _(unset → agent off)_ | OpenRouter API key. |
| `HEARTH_AGENT_MODEL` | `qwen/qwen3.6-27b` | Any tool-calling model id on OpenRouter. |
| `HEARTH_OPENROUTER_BASE_URL` | `https://openrouter.ai/api/v1` | Point at any OpenAI-compatible endpoint. |
| `HEARTH_BRAVE_API_KEY` | _(unset)_ | If set, web search uses Brave; otherwise OpenRouter's built-in web plugin. |
| `HEARTH_AGENT_WEB_SEARCH` | `1` | Set to `0` to disable web search entirely. |
| `HEARTH_AGENT_MAX_STEPS` | `8` | Max tool rounds the agent runs per message. |

About web search: the Qwen model itself doesn't browse, but OpenRouter adds web
search to *any* model via its built-in `web` plugin — so search works out of
the box with just the OpenRouter key. Set `HEARTH_BRAVE_API_KEY` if you'd
rather the agent call Brave directly with your own key.

In Docker, pass the same variables with `-e`:

```sh
./deploy.sh --docker --detach        # then, or instead, run with:
docker run -e HEARTH_OPENROUTER_API_KEY=sk-or-... -p 7777:7777 -v hearth-data:/data hearth
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
