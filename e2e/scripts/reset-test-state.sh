#!/usr/bin/env sh
# Safely reset local test state used by the release test suites.
# Only touches directories that carry the `.test-seed` marker written by
# e2e/fixtures/seed.js, so a real/owner data directory can never be deleted.
set -eu

cd "$(dirname "$0")/../.."
DATA_ROOT="$PWD"
removed=0

for dir in "$DATA_ROOT"/e2e/.tmp-data-* "$DATA_ROOT"/e2e/.tmp-data-maestro-*; do
  [ -e "$dir" ] || continue
  if [ ! -f "$dir/.test-seed" ]; then
    echo "SKIP (no test marker): $dir" >&2
    continue
  fi
  # Belt and braces: refuse anything outside e2e/.tmp-data-*.
  case "$dir" in
    "$DATA_ROOT"/e2e/.tmp-data-*) rm -rf "$dir"; removed=$((removed + 1)) ;;
    *) echo "REFUSING unexpected path: $dir" >&2; exit 1 ;;
  esac
done

# Kill any stale integration server we started (recorded pid files only).
for pidfile in "$DATA_ROOT"/e2e/reports/.server-pid; do
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    kill "$(cat "$pidfile")" 2>/dev/null || true
    echo "Stopped stale test server pid $(cat "$pidfile")"
  fi
  rm -f "$pidfile"
done

# Drop leftover report token files (bearer credentials on disk).
find "$DATA_ROOT/e2e/reports" -name '.test-token' -delete 2>/dev/null || true

echo "Reset complete: $removed test data directory(ies) removed."
