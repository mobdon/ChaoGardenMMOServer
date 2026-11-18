#!/usr/bin/env bash
# Start isolated PostgreSQL + Redis, seed deterministic fixtures, run the
# integration + contract suites, then tear the services down.
# Usage: e2e/scripts/run-integration.sh [node --test args...]
set -eu

cd "$(dirname "$0")/../.."
ROOT="$PWD"
REPORT_DIR="$ROOT/e2e/reports"
mkdir -p "$REPORT_DIR/integration" "$REPORT_DIR/logs"

# Isolated service ports/containers; names include PID so parallel CI runs
# never collide and the real dev/production services are never touched.
PID="$$"
PG_CONTAINER="chao-test-pg-$PID"
REDIS_CONTAINER="chao-test-redis-$PID"
PG_PORT="${PG_PORT:-55432}"
REDIS_PORT="${REDIS_PORT:-55433}"
PG_URL="postgres://chao_test:chao_test@127.0.0.1:$PG_PORT/chao_test"
REDIS_URL="redis://127.0.0.1:$REDIS_PORT/0"
DATA_DIR="$ROOT/e2e/.tmp-data-$PID"
export TEST_DATA_DIR="$DATA_DIR"

cleanup() {
  docker rm -f "$PG_CONTAINER" "$REDIS_CONTAINER" >/dev/null 2>&1 || true
  rm -rf "$DATA_DIR"
}
trap cleanup EXIT

echo "==> Starting isolated Postgres + Redis (ports $PG_PORT/$REDIS_PORT)"
docker run -d --rm --name "$PG_CONTAINER" -e POSTGRES_USER=chao_test \
  -e POSTGRES_PASSWORD=chao_test -e POSTGRES_DB=chao_test \
  -p "127.0.0.1:$PG_PORT:5432" postgres:16-alpine >/dev/null
docker run -d --rm --name "$REDIS_CONTAINER" -p "127.0.0.1:$REDIS_PORT:6379" \
  redis:7-alpine >/dev/null

# Wait for readiness without printing DSNs.
for _ in $(seq 1 30); do
  if docker exec "$PG_CONTAINER" pg_isready -U chao_test >/dev/null 2>&1; then break; fi
  sleep 1
done
docker exec "$PG_CONTAINER" pg_isready -U chao_test >/dev/null

echo "==> Seeding deterministic fixtures"
node e2e/fixtures/seed.js

# Write the test bearer token to a file (never an inline env var) so runners
# and diagnostics only ever reference a path.
TOKEN_FILE="$REPORT_DIR/.test-token"
node -e '
  const fs = require("fs");
  const t = JSON.parse(fs.readFileSync(process.env.TEST_DATA_DIR + "/.test-seed-tokens", "utf8"));
  fs.writeFileSync(process.env.TOKEN_FILE, t.SeedOwner + "\n");
' TEST_DATA_DIR="$DATA_DIR" TOKEN_FILE="$TOKEN_FILE" 2>/dev/null \
  || node -e '
  const fs = require("fs");
  const t = JSON.parse(fs.readFileSync(process.env.TEST_DATA_DIR + "/.test-seed-tokens", "utf8"));
  fs.writeFileSync(process.env.TOKEN_FILE, t.SeedOwner + "\n");
' 2>/dev/null
chmod 600 "$TOKEN_FILE"

echo "==> Starting server authority on an ephemeral port with disposable data"
GAME_PORT=0
LOG="$REPORT_DIR/logs/integration-server.log"
( CHAO_DATA_DIR="$DATA_DIR" GAME_PORT=55440 GAME_HOST=127.0.0.1 \
  CHAO_ALLOWED_ORIGINS=http://127.0.0.1:5173,http://127.0.0.1:55440 \
  cargo run --quiet --locked >"$LOG" 2>&1 & echo $! > "$REPORT_DIR/.server-pid" )
sleep 4
BASE_URL="http://127.0.0.1:55440"
curl -fsS -m 10 "$BASE_URL/api/status" >/dev/null || {
  echo "Server did not become ready; log tail:"; tail -n 50 "$LOG"; exit 1; }

echo "==> Running integration + contract suites"
FAIL=0
TEST_BASE_URL="$BASE_URL" TEST_TOKEN_FILE="$TOKEN_FILE" \
TEST_DATABASE_URL="$PG_URL" TEST_REDIS_URL="$REDIS_URL" \
  node --test e2e/integration/*.test.js "$@" \
  --reporter=tap 2>&1 | tee "$REPORT_DIR/integration/integration.tap" || FAIL=1

if [ "$(kill -0 "$(cat "$REPORT_DIR/.server-pid")" 2>/dev/null && echo up || echo down)" = up ]; then
  kill "$(cat "$REPORT_DIR/.server-pid")" 2>/dev/null || true
fi
rm -f "$TOKEN_FILE" "$REPORT_DIR/.server-pid"

echo "==> Integration suite finished (fail=$FAIL); report: e2e/reports/integration/"
exit "$FAIL"
