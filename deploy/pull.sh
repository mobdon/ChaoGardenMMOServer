#!/usr/bin/env bash
# Run as root by chao-pull.timer (or by hand: one step). When main has a new
# commit in either repository, build it as the unprivileged build user at low
# priority and hand the result to update.sh. A commit that fails to build or
# fails its health check is remembered and not tried again.
set -euo pipefail
SRC=${CHAO_SRC:-/srv/chao-src}
ROOT=${CHAO_ROOT:-/opt/chao}
here=$(cd "$(dirname "$0")" && pwd)
exec 8>/run/lock/chao-pull.lock
flock -n 8 || exit 0
build() { runuser -l chaobuild -c "cd '$SRC/$1' && $2"; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/release"
tried=()
for repo in server client; do
    dir=$SRC/$repo
    build "$repo" 'git fetch -q origin main'
    rev=$(build "$repo" 'git rev-parse FETCH_HEAD')
    [[ "$rev" != "$(cat "$ROOT/current/rev-$repo" 2>/dev/null)" ]] || continue
    [[ "$rev" != "$(cat "$SRC/failed-$repo" 2>/dev/null)" ]] || continue
    echo "Building $repo $rev"
    if [[ $repo == server ]]; then
        steps='nice -n 19 cargo build --release --locked'
    else
        steps='npm ci --ignore-scripts --no-audit --no-fund && nice -n 19 npx vite build'
    fi
    if ! build "$repo" "git checkout -q --detach -f $rev && $steps"; then
        echo "Build of $repo $rev failed; the running release is unchanged." >&2
        echo "$rev" > "$SRC/failed-$repo"
        continue
    fi
    if [[ $repo == server ]]; then
        cp "$dir/target/release/chao-game-server" "$work/release/"
        cp -r "$dir/deploy" "$work/release/deploy"
    else
        cp -r "$dir/dist" "$work/release/web"
    fi
    echo "$rev" > "$work/release/rev-$repo"
    tried+=("$repo:$rev")
done
[[ ${#tried[@]} -gt 0 ]] || exit 0
(cd "$work/release" && tar -czf "$work/release.tar.gz" -- *)
if ! bash "$here/update.sh" "$work/release.tar.gz"; then
    for item in "${tried[@]}"; do echo "${item#*:}" > "$SRC/failed-${item%%:*}"; done
    exit 1
fi
