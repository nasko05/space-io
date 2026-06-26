#!/usr/bin/env bash
# SpaceIO · Hearth — off-the-container backup of the vault volume.
#
# All user data (per-user git repos of encrypted .age blobs + .users.toml) lives
# in the Docker named volume `hearth-data`. This script snapshots that volume to
# a timestamped tar.gz on the host, prunes old snapshots to a retention count,
# and — if an rclone remote is configured — copies the snapshot OFF-SITE. A
# backup that sits on the same disk as the data is not disaster recovery; the
# rclone step is what makes it one.
#
# Designed to run from cron on the VPS (see DEPLOYMENT.md):
#   0 3 * * *  /home/deploy/space-io/scripts/backup.sh >> /home/deploy/hearth-backup.log 2>&1
#
# Tunables (env vars, all optional):
#   HEARTH_VOLUME     Docker volume to back up        (default: hearth-data)
#   BACKUP_DIR        host dir for local snapshots     (default: ~/hearth-backups)
#   BACKUP_RETENTION  how many local snapshots to keep (default: 14)
#   RCLONE_REMOTE     rclone target, e.g. b2:my-bucket/hearth (default: unset → off-site skipped)

set -euo pipefail

HEARTH_VOLUME="${HEARTH_VOLUME:-hearth-data}"
BACKUP_DIR="${BACKUP_DIR:-$HOME/hearth-backups}"
BACKUP_RETENTION="${BACKUP_RETENTION:-14}"
RCLONE_REMOTE="${RCLONE_REMOTE:-}"

have() { command -v "$1" >/dev/null 2>&1; }
have docker || { echo "error: docker not found" >&2; exit 1; }

# Confirm the volume exists before doing anything, so a typo doesn't silently
# produce an empty archive.
if ! docker volume inspect "$HEARTH_VOLUME" >/dev/null 2>&1; then
  echo "error: docker volume '$HEARTH_VOLUME' does not exist" >&2
  exit 1
fi

mkdir -p "$BACKUP_DIR"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
archive="$BACKUP_DIR/hearth-${stamp}.tar.gz"

echo ">> snapshotting volume '$HEARTH_VOLUME' → $archive"
# Mount the volume read-only into a throwaway alpine and tar it to the host dir,
# which is bind-mounted at /backup. No dependency on the app image or a running
# container; consistent enough for a file vault (atomic per-file git writes).
docker run --rm \
  -v "$HEARTH_VOLUME":/data:ro \
  -v "$BACKUP_DIR":/backup \
  alpine:3.20 \
  tar czf "/backup/$(basename "$archive")" -C /data .

echo ">> wrote $(du -h "$archive" | cut -f1) to $archive"

# Prune local snapshots beyond the retention count (newest kept).
echo ">> pruning local snapshots, keeping newest $BACKUP_RETENTION"
ls -1t "$BACKUP_DIR"/hearth-*.tar.gz 2>/dev/null \
  | tail -n +"$((BACKUP_RETENTION + 1))" \
  | while read -r old; do
      echo "   removing $old"
      rm -f "$old"
    done

# Off-site copy. Without a remote this is a same-disk backup only — not DR.
if [ -n "$RCLONE_REMOTE" ]; then
  have rclone || { echo "error: RCLONE_REMOTE set but rclone not installed" >&2; exit 1; }
  echo ">> copying off-site → $RCLONE_REMOTE"
  rclone copy "$archive" "$RCLONE_REMOTE" --no-traverse
  echo ">> off-site copy complete"
else
  echo ">> RCLONE_REMOTE unset — skipping off-site copy (local snapshot only)."
  echo "   Set RCLONE_REMOTE for real disaster recovery; see DEPLOYMENT.md."
fi

echo ">> backup done."
