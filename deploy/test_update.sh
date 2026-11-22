#!/usr/bin/env bash
# Offline check of update.sh with a stand-in systemctl and synthetic files.
# A "server" whose file says bad never becomes healthy.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin" "$T/data" "$T/src/deploy" "$T/src/web"
cat > "$T/bin/systemctl" <<STUB
#!/usr/bin/env bash
echo "\$1" >> "$T/calls"
rm -f "$T/healthy"
if [[ \$1 == start ]]; then
    grep -q good "$T/root/current/chao-game-server" && touch "$T/healthy"
    grep -q bad "$T/root/current/chao-game-server" && echo wrecked > "$T/data/profiles.json"
fi
exit 0
STUB
chmod +x "$T/bin/systemctl"
export PATH="$T/bin:$PATH" CHAO_ROOT="$T/root" CHAO_DATA_DIR="$T/data" CHAO_BACKUPS="$T/backups" \
    CHAO_LOCK="$T/lock" CHAO_HEALTH_URL="file://$T/healthy" CHAO_HEALTH_TRIES=2 CHAO_HEALTH_WAIT=0
cp "$here/backup.py" "$T/src/deploy/"
release() { # release <binary text> <page text>
    printf '#!/bin/sh\n# %s\n' "$1" > "$T/src/chao-game-server"; chmod +x "$T/src/chao-game-server"
    echo "$2" > "$T/src/web/index.html"
    tar -czf "$T/release.tar.gz" -C "$T/src" chao-game-server deploy web
}
update() { bash "$here/update.sh" "$T/release.tar.gz"; }
is() { [[ "$2" == "$3" ]] || { echo "FAIL $1: got '$2' want '$3'" >&2; exit 1; }; echo "ok   $1"; }
page() { cat "$T/root/current/web/index.html"; }
SAVE='[]'   # synthetic empty save

release good-1 page-1; update
is 'first install is served' "$(page)" page-1
echo "$SAVE" > "$T/data/profiles.json"

release good-2 page-2; update
is 'new server release is served' "$(page)" page-2
is 'saves were backed up first' "$(ls "$T/backups" | grep -c pre-update)" 1
is 'saves untouched by a good update' "$(cat "$T/data/profiles.json")" "$SAVE"

: > "$T/calls"; release good-2 page-3; update
is 'page-only update is served' "$(page)" page-3
is 'page-only update never stops the server' "$(cat "$T/calls")" ''

release bad-3 page-4
if update 2>/dev/null; then echo 'FAIL: a bad release reported success' >&2; exit 1; fi
is 'bad release rolls back to the old pages' "$(page)" page-3
is 'bad release rolls back the saves' "$(cat "$T/data/profiles.json")" "$SAVE"
is 'old server is running again' "$(tail -1 "$T/calls")" start
[[ -e "$T/healthy" ]] || { echo 'FAIL: not healthy after rollback' >&2; exit 1; }

printf 'x' > "$T/evil"; tar -czf "$T/release.tar.gz" -C "$T" evil
if update 2>/dev/null; then echo 'FAIL: unknown archive member accepted' >&2; exit 1; fi
is 'unknown archive member is refused' "$(page)" page-3
echo 'All update checks passed.'
