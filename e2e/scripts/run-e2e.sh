#!/usr/bin/env bash
# Run every Maestro mobile journey on a booted simulator/emulator against a
# locally started server with disposable data. Requires the Maestro CLI.
# Usage: e2e/scripts/run-e2e.sh [--app-id <id>] [--filter <name>]
set -eu

cd "$(dirname "$0")/../.."
REPORT_DIR="$PWD/e2e/reports/maestro"
mkdir -p "$REPORT_DIR"

command -v maestro >/dev/null 2>&1 || {
  echo "maestro not found: install https://maestro.mobile.dev (e.g. brew tap mobile-devops/tap && brew install maestro)"; exit 2; }

APP_ID="mobdon.chaogarden.mobile"
FILTER=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --app-id) APP_ID="$2"; shift 2 ;;
    --filter) FILTER="$2"; shift 2 ;;
    *) echo "Unknown flag: $1"; exit 2 ;;
  esac
done

# 1. Disposable server data dir + ephemeral port.
PID="$$"
DATA_DIR="$PWD/e2e/.tmp-data-maestro-$PID"
export TEST_DATA_DIR="$DATA_DIR"
node e2e/fixtures/seed.js
cleanup() { rm -rf "$DATA_DIR"; }
trap cleanup EXIT

LOG="$PWD/e2e/reports/logs/maestro-server.log"
mkdir -p "$(dirname "$LOG")"
CHAO_DATA_DIR="$DATA_DIR" GAME_HOST=127.0.0.1 GAME_PORT=55441 \
  CHAO_ALLOWED_ORIGINS=http://127.0.0.1:55441 \
  cargo run --quiet --locked >"$LOG" 2>&1 &
SERVER_PID=$!
BASE_URL="http://127.0.0.1:55441"
sleep 4
curl -fsS -m 10 "$BASE_URL/api/status" >/dev/null || {
  echo "Server not ready; log tail:"; tail -n 50 "$LOG"; exit 1; }

# 2. Point the app build at the local server (env injected into Maestro).
set +e
maestro test \
  --env BASE_URL="$BASE_URL" \
  --env APP_ID="$APP_ID" \
  ${FILTER:+--filter "$FILTER"} \
  e2e/maestro/flows \
  2>&1 | tee "$REPORT_DIR/maestro.log"
FAIL=${PIPESTATUS[0]}
set -e

kill "$SERVER_PID" 2>/dev/null || true

echo "Maestro suite finished (fail=$FAIL); log: $REPORT_DIR/maestro.log"
exit "$FAIL"
