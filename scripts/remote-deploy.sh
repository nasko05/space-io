#!/usr/bin/env bash
# Runs ON the VPS (as the deploy user), invoked over SSH by
# .github/workflows/deploy.yml AFTER the repo has been synced to origin/main.
#
# There is deliberately NO server-side .env. Runtime config is rendered on the
# GitHub runner from Secrets + Variables, base64-packed, and sent over the
# encrypted SSH stdin to this script — it is never written to disk and never
# placed in a process argument (so it can't leak via `ps`). Whatever is not
# injected falls back to the ${VAR:-default} defaults baked into
# docker-compose.yml, which is also what a plain local `docker compose up` uses.
set -euo pipefail
cd "$(dirname "$0")/.."

# Materialise the injected config into THIS shell only, so `docker compose`
# interpolates it. Splitting on the FIRST '=' keeps values that themselves
# contain '=' or spaces intact (`val` gets the remainder). A missing/empty line
# leaves the compose defaults in force.
read -r env_b64 || true
if [ -n "${env_b64:-}" ]; then
  while IFS='=' read -r key val; do
    [ -n "$key" ] && export "$key=$val"
  done < <(printf '%s' "$env_b64" | base64 -d)
fi

docker compose up -d --build
docker image prune -f

# Verify the app actually came up before calling this a success; otherwise the
# previous container is still serving and we fail loud.
ok=""
for _ in $(seq 1 30); do
  if curl -fsS http://127.0.0.1:8001/healthz >/dev/null 2>&1; then ok=1; break; fi
  sleep 5
done
if [ -z "$ok" ]; then
  echo "::error::App did not become healthy after deploy"
  docker compose ps
  docker compose logs --tail=80 space-io
  exit 1
fi

echo "Deploy OK — app healthy."
docker compose ps
