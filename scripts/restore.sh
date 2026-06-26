#!/usr/bin/env bash
# SpaceIO · Hearth — restore the vault volume from a backup snapshot.
#
#   ./scripts/restore.sh ~/hearth-backups/hearth-20260626T030000Z.tar.gz
#
# Stops the app, REPLACES the contents of the `hearth-data` volume with the
# snapshot, then brings the app back. Because this overwrites live data, it
# refuses to run without an explicit archive argument and prints what it is
# about to do.
#
# Tunables (env vars):
#   HEARTH_VOLUME   Docker volume to restore into (default: hearth-data)
#   COMPOSE_DIR     dir containing docker-compose.yml (default: script's repo root)

set -euo pipefail

archive="${1:-}"
HEARTH_VOLUME="${HEARTH_VOLUME:-hearth-data}"
COMPOSE_DIR="${COMPOSE_DIR:-$(cd "$(dirname "$0")/.." && pwd)}"

die() { echo "error: $*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

[ -n "$archive" ] || die "usage: $0 <path-to-hearth-*.tar.gz>"
[ -f "$archive" ] || die "archive not found: $archive"
have docker || die "docker not found"

echo "About to restore:"
echo "  archive : $archive"
echo "  volume  : $HEARTH_VOLUME  (its current contents will be REPLACED)"
echo "  compose : $COMPOSE_DIR"
printf "Type 'restore' to proceed: "
read -r confirm
[ "$confirm" = "restore" ] || die "aborted"

# Stop the app so nothing writes mid-restore. `|| true` so a not-yet-created
# stack doesn't abort the restore.
echo ">> stopping app"
( cd "$COMPOSE_DIR" && docker compose stop hearth ) || true

echo ">> wiping and repopulating volume '$HEARTH_VOLUME'"
docker run --rm \
  -v "$HEARTH_VOLUME":/data \
  -v "$(cd "$(dirname "$archive")" && pwd)":/backup:ro \
  alpine:3.20 \
  sh -c "rm -rf /data/* /data/..?* /data/.[!.]* 2>/dev/null; tar xzf /backup/$(basename "$archive") -C /data"

echo ">> starting app"
( cd "$COMPOSE_DIR" && docker compose up -d hearth )

echo ">> restore done. Tail logs with: docker compose logs -f hearth"
