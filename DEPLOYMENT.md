# Deploying SpaceIO (production)

SpaceIO is co-hosted on the **same Contabo VPS** as the sibling
[`cloud-storage-system`](https://github.com/nasko05/cloud-storage-system) app,
behind **one shared, containerized Caddy** that already owns ports 80/443 and
terminates HTTPS via Let's Encrypt. This document mirrors that app's deployment
so day-2 ops are identical for both.

The model is **zero manual server steps**: you push to `main`, CI runs, and on
green CI the deploy workflow injects runtime config from GitHub straight into the
`docker compose up` shell (there is **no server-side `.env`**), rebuilds the
container, and verifies `/healthz`. **All runtime config lives in GitHub**
(Settings → Secrets and variables → Actions) — never hand-edit files on the box.

---

## Architecture on the shared host

```
                 Internet (:443)
                       │
              ┌────────▼─────────┐     external docker network: web
              │   caddy (shared) │◄───────────────┐
              │  owns 80/443,    │                │
              │  Let's Encrypt   │                │
              └───┬─────────┬────┘                │
   app:8000 ──────┘         └────── space-io:7777 ──┘
 (cloud-storage,        (THIS app, no published 80/443;
  its own project)       loopback 127.0.0.1:8001 for health only)
```

- The sibling app's Caddy is the **single TLS listener**. SpaceIO runs **no
  Caddy** and **never binds 80/443**.
- Caddy reaches SpaceIO by container name over the **external `web`** docker
  network. Both Caddy and SpaceIO attach to it.
- SpaceIO additionally publishes **`127.0.0.1:8001:7777`** (loopback only) so the
  deploy can poll `/healthz` over SSH. The sibling app uses `127.0.0.1:8000`, so
  there is no port collision.

### Confirmed: no collisions with the sibling app

| Resource          | cloud-storage-system                                   | SpaceIO (this app) |
|-------------------|--------------------------------------------------------|-------------------|
| Compose project   | `cloud-storage-system`                                 | `space-io`          |
| Containers        | `db`, `app`, `caddy`, `clamav`                         | `space-io`          |
| Public ports      | `caddy` → 80, 443                                       | none              |
| Loopback port     | `app` → 127.0.0.1:**8000**                              | 127.0.0.1:**8001** |
| Named volumes     | `db_data`, `blob_data`, `backup_data`, `clamav_data`, `caddy_data`, `caddy_config` | `space-io-data` |
| Networks          | project default (+`web` after the wiring below)         | external `web`    |

SpaceIO has **no database** (it is a filesystem vault) and **no app-wide signing
key** (keys are derived per-user from each passphrase; sessions are in-memory).
The only sensitive server value is the *optional* OpenRouter API key.

---

## One-time setup (manual, do once)

### 1. DNS — already covered by the wildcard

The Contabo zone already has a wildcard A record:

```
*.personal-drive-io.com.   A   31.220.94.82
```

That matches `personal-area.personal-drive-io.com` (a single label with no more
specific record), so **no DNS change is needed** — and any other subdomain works
too. Verify with `dig +short personal-area.personal-drive-io.com` → `31.220.94.82`.

(If you ever move off the wildcard, add an explicit `A` record to the VPS
**IPv4** — Caddy needs the name to resolve to the host for the ACME challenge,
and runners have no IPv6 route.)

### 2. The `deploy` user and SSH key (shared with the sibling app)

Same VPS, same `deploy` user (non-root, in the `docker` group), key-based login
only. If the sibling app already set this up, **reuse it** — the same
`DEPLOY_*` secrets work for both repos.

Generate a dedicated deploy keypair (if you don't already have one) and
authorize it:

```sh
ssh-keygen -t ed25519 -f deploy_key -C "github-actions deploy" -N ""
ssh-copy-id -i deploy_key.pub deploy@31.220.94.82      # or append to authorized_keys
```

Keep `deploy_key` (the **private** half) for the `DEPLOY_SSH_KEY` secret below.

### 3. Clone the repo on the server

The deploy syncs an existing checkout; create it once at `~/space-io`:

```sh
ssh deploy@31.220.94.82
git clone https://github.com/nasko05/space-io.git ~/space-io
```

### 4. Create the shared `web` network and attach the existing Caddy

The sibling Caddy currently talks to its app over its **own project network** —
there is no `web` network yet. Create it and put Caddy on it:

```sh
docker network create web        # idempotent; the deploy also runs this
```

Then, in **the sibling repo** (`cloud-storage-system`), attach Caddy to `web`
and add a site block for SpaceIO. This is the only cross-repo change; apply it
there and redeploy that app once.

`docker-compose.yml` — add the network to the `caddy` service and declare it:

```yaml
services:
  caddy:
    # ...existing config...
    networks:
      - default      # keep: this is how Caddy reaches its own `app`
      - web          # add:  this is how Caddy reaches SpaceIO

networks:
  web:
    external: true
```

`Caddyfile` — add a second site block (the existing
`{$DRIVE_SITE_ADDRESS::80}` block stays untouched):

```caddyfile
personal-area.personal-drive-io.com {
	reverse_proxy space-io:7777
}
```

After redeploying the sibling app, Caddy will provision a Let's Encrypt cert for
`personal-area.personal-drive-io.com` automatically and proxy it to SpaceIO.

> If you'd rather not edit the sibling repo, the alternative is to extract Caddy
> into its own standalone reverse-proxy stack that owns 80/443 + `web`, and have
> both apps register against it. That's a larger change to the sibling app —
> raise it before going that route.

### 5. GitHub Secrets and Variables (on **this** repo)

Settings → Secrets and variables → Actions.

**Secrets** (sensitive):

| Name                        | Required | What                                                                 |
|-----------------------------|----------|---------------------------------------------------------------------|
| `DEPLOY_HOST`               | ✅       | VPS **IPv4** (`31.220.94.82`). Never an IPv6 — runners can't route it. |
| `DEPLOY_USER`               | ✅       | `deploy`                                                             |
| `DEPLOY_SSH_KEY`            | ✅       | The **private** deploy key (full PEM, multi-line).                  |
| `SPACEIO_OPENROUTER_API_KEY` | ➖       | Turns the in-app AI assistant on. Omit → assistant stays hidden.    |
| `SPACEIO_BRAVE_API_KEY`      | ➖       | Direct Brave web search. Omit → OpenRouter's built-in web plugin.   |

**Variables** (non-sensitive, all optional). Set one to override; the **default
for each lives in `docker-compose.yml`** (`${VAR:-default}`) — the single source
of truth, also used by a plain local `docker compose up`. The deploy passes a
Variable through only when you set it.

| Name                     | Default (in compose) | What                                  |
|--------------------------|----------------------|---------------------------------------|
| `SPACEIO_AGENT_MODEL`     | `qwen/qwen3.6-27b`   | Any tool-calling OpenRouter model id. |
| `SPACEIO_AGENT_WEB_SEARCH`| `1`                  | `0` disables agent web search.        |
| `SPACEIO_AGENT_MAX_STEPS` | `8`                  | Max tool rounds per agent message.    |

The public **domain is not a SpaceIO Variable** — it's configured in the sibling
Caddy's site block (§4), and DNS is already covered by the `*.personal-drive-io.com`
wildcard A record.

The `DEPLOY_*` trio is **shared** with the sibling app — but GitHub secrets are
per-repo, so add them to this repo too (same values).

---

## How a deploy works (CI → CD)

- **CI** (`.github/workflows/ci.yml`) on every PR and push to `main`:
  `cargo fmt`/`clippy`, web `tsc` type-check + `vitest`, `cargo test` (unit +
  integration), and a **docker-smoke** job that builds the image, runs the real
  compose file, and waits for `/healthz`.
- **CD** (`.github/workflows/deploy.yml`) triggers on `workflow_run` **after CI
  succeeds on `main`**. There is **no server-side `.env`** — config is injected
  per-deploy. It:
  1. **guards** that `DEPLOY_HOST`/`DEPLOY_USER`/`DEPLOY_SSH_KEY` exist and
     aborts *before connecting* if not,
  2. packs runtime config from Secrets + Variables into one base64 line,
  3. SSHes in: `rm -f .env` (purge any legacy file), `docker network create web
     || true`, `git fetch` + `git reset --hard origin/main`, then pipes the
     packed config over the encrypted SSH **stdin** (never on disk, never in
     argv) to `scripts/remote-deploy.sh`,
  4. which **exports** it into the `docker compose up -d --build` shell —
     anything unset falls back to the `${VAR:-default}` defaults in
     `docker-compose.yml` — and runs `docker image prune -f`,
  5. polls `http://127.0.0.1:8001/healthz` (30×5s) and **fails the deploy** if
     the app doesn't come up — the previous container keeps serving.

Trigger a deploy by merging to `main`. To re-deploy after only changing a
Variable/Secret, re-run the latest `Deploy` workflow.

---

## Day-2 operations

All commands run as `deploy` on the VPS, in `~/space-io`.

**Logs**
```sh
docker compose logs -f space-io
```

**Status / health**
```sh
docker compose ps
curl -fsS http://127.0.0.1:8001/healthz && echo            # → ok
```

**Manual redeploy / restart** (normally automatic via CD)
```sh
git fetch origin main && git reset --hard origin/main
docker compose up -d --build && docker image prune -f
```

**Change config** — edit the GitHub Secret/Variable, then re-run the `Deploy`
workflow. There is no `~/space-io/.env` to edit: GitHub is the single source of
truth, and every deploy injects the current values (with `docker-compose.yml`
defaults as the fallback). A one-off manual `docker compose up` on the box uses
those compose defaults — to apply a changed secret, run the workflow.

### Backups

User data is the **`space-io-data`** volume (per-user git repos of encrypted
`.age` blobs + `.users.toml`). `scripts/backup.sh` snapshots it to a timestamped
`tar.gz`, prunes to a retention count, and — if `RCLONE_REMOTE` is set — copies
it **off-site** (a same-disk backup is not disaster recovery).

Daily cron on the VPS:
```sh
crontab -e
# 3am UTC daily; off-site to Backblaze B2 (configure `rclone config` first)
0 3 * * *  RCLONE_REMOTE=b2:my-bucket/space-io /home/deploy/space-io/scripts/backup.sh >> /home/deploy/space-io-backup.log 2>&1
```

Restore from a snapshot (stops the app, replaces the volume, restarts):
```sh
./scripts/restore.sh ~/space-io-backups/space-io-20260626T030000Z.tar.gz
```

---

## Lessons baked in (don't relearn these the hard way)

- **`DEPLOY_HOST` must be the IPv4.** GitHub-hosted runners have no IPv6 route;
  an IPv6 (or AAAA-only DNS) makes the SSH/SCP step hang then fail.
- **Container DNS / ACME fix.** Ubuntu's `systemd-resolved` hands containers a
  `127.0.0.53` stub that doesn't work inside them. The host's
  `/etc/docker/daemon.json` is set to `{"dns":["1.1.1.1","8.8.8.8"]}`; **don't
  undo it**. The SpaceIO service also pins those resolvers so the AI agent's
  outbound HTTPS resolves regardless.
- **Forwarded headers / HTTPS behind the proxy.** SpaceIO speaks plain HTTP
  behind Caddy. It reads **`X-Forwarded-Proto`** to know the request was HTTPS,
  so it marks session cookies `Secure` in production. (The frontend uses
  same-origin relative URLs, so there's no absolute-URL/mixed-content problem to
  fix — the FastAPI sibling needs `FORWARDED_ALLOW_IPS=*` for uvicorn; SpaceIO's
  axum server needs no equivalent.) Do **not** set `SPACEIO_INSECURE_COOKIES` in
  production.
- **Pin image tags.** Every image uses a maintained, non-floating tag
  (`node:24-slim`, `rust:1-slim`, `debian:bookworm-slim`, `alpine:3.20`). Avoid
  `latest`, and verify a tag still exists before pinning — EOL minor tags do get
  removed from registries (e.g. an old ClamAV minor).
- **Config changes go through GitHub, not SSH.** There is no server-side `.env`;
  each deploy injects config from Secrets + Variables over the SSH stdin, with
  the `docker-compose.yml` defaults as the fallback.
- **No port/volume/network collisions.** SpaceIO uses project `space-io`, container
  `space-io`, volume `space-io-data`, loopback `8001`, and the shared `web` network —
  none of which the sibling app uses for the same purpose.
