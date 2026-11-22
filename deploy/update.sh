#!/usr/bin/env bash
# One-step update on the droplet, as root: back up the saves, swap in the new
# release, health-check it, and put the old release and saves back if it fails.
# usage: update.sh /absolute/release.tar.gz
# The archive holds any of these top-level entries; whatever it leaves out is
# carried over from the running release.
set -euo pipefail
ENTRIES='chao-game-server deploy web sa2 rev-server rev-client'
ROOT=${CHAO_ROOT:-/opt/chao}
DATA=${CHAO_DATA_DIR:-/var/lib/chao}
BACKUPS=${CHAO_BACKUPS:-/var/backups/chao}
HEALTH=${CHAO_HEALTH_URL:-http://127.0.0.1:3001/readyz}
LOCK=${CHAO_LOCK:-/run/lock/chao-backup.lock}
UNIT=chao.service
KEEP=5
here=$(cd "$(dirname "$0")" && pwd)

[[ $# == 1 && "$1" = /* && -f "$1" ]] || { echo 'usage: update.sh /absolute/release.tar.gz' >&2; exit 2; }
exec 9>"$LOCK"
flock -n 9 || { echo 'A backup or another update is in progress' >&2; exit 1; }

id=$(date -u +%Y%m%dT%H%M%S.%3NZ)
new=$ROOT/releases/$id
old=$(readlink -f "$ROOT/current" 2>/dev/null || true)
mkdir -p "$ROOT/releases" "$BACKUPS"
stage=$(mktemp -d "$ROOT/releases/.stage-XXXXXX")
trap 'rm -rf "$stage"' EXIT

# Archive members must be plain relative paths under a known entry.
tar -tzf "$1" | while IFS= read -r name; do
    top=${name%%/*}
    [[ "$name" != /* && "$name" != *..* && " $ENTRIES " == *" $top "* ]] ||
        { echo "Unexpected archive member: $name" >&2; exit 1; }
done
tar -xzf "$1" -C "$stage" --no-same-owner

if [[ -n "$old" ]]; then cp -al "$old" "$new"; else mkdir "$new"; fi
for entry in $ENTRIES; do
    [[ -e "$stage/$entry" ]] || continue
    rm -rf "${new:?}/$entry"
    mv "$stage/$entry" "$new/$entry"
done
[[ -x "$new/chao-game-server" ]] || { echo 'Release has no server binary' >&2; rm -rf "$new"; exit 1; }

swap() { ln -sfn "$1" "$ROOT/current.next" && mv -T "$ROOT/current.next" "$ROOT/current"; }
healthy() {
    for _ in $(seq "${CHAO_HEALTH_TRIES:-60}"); do
        curl -fsS -m 2 -o /dev/null "$HEALTH" 2>/dev/null && return 0
        sleep "${CHAO_HEALTH_WAIT:-1}"
    done
    return 1
}
prune() {
    ls -1d "$ROOT"/releases/*/ | head -n -"$KEEP" | xargs -r rm -rf --
    { ls -1t "$BACKUPS"/pre-update-*.zip 2>/dev/null || true; } | tail -n +11 | xargs -r rm -f --
}

# Only page and game-file changes: swap with the server left running.
if [[ -n "$old" ]] && cmp -s "$old/chao-game-server" "$new/chao-game-server" &&
    diff -rq "$old/deploy" "$new/deploy" >/dev/null 2>&1; then
    swap "$new"; prune
    echo "Updated to $id without a restart."
    exit 0
fi

systemctl stop "$UNIT"   # SIGTERM: the server writes its saves before it exits
backup=
if [[ -f "$DATA/profiles.json" ]]; then
    backup=$BACKUPS/pre-update-$id.zip
    if ! python3 "$here/backup.py" create "$DATA" "$backup" --offline; then
        echo 'Backup failed; keeping the old release.' >&2
        rm -rf "$new"; [[ -z "$old" ]] || systemctl start "$UNIT"; exit 1
    fi
fi
swap "$new"
systemctl start "$UNIT" || true
if healthy; then
    prune
    echo "Updated to $id."
    exit 0
fi

echo "Release $id failed its health check; rolling back." >&2
systemctl stop "$UNIT" || true
if [[ -z "$old" ]]; then rm -f "$ROOT/current"; rm -rf "$new"; exit 1; fi
swap "$old"
if [[ -n "$backup" ]]; then
    # The failed release may have rewritten the saves; put back the copy taken above.
    owner=$(stat -c %U:%G "$DATA")
    mv "$DATA" "$BACKUPS/failed-data-$id"
    python3 "$here/backup.py" restore "$backup" "$DATA" --offline
    chown -R "$owner" "$DATA"
fi
rm -rf "$new"
systemctl start "$UNIT"
healthy || echo 'The old release is not healthy either; look at: journalctl -u chao' >&2
exit 1
