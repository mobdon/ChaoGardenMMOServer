#!/usr/bin/env bash
# Run k6 load scenarios against TEST_BASE_URL (staging or a local server).
# Usage: e2e/scripts/run-load.sh [scenario...]   (default: all)
# Env:   TEST_BASE_URL (required), TEST_TOKEN_FILE (path, optional)
set -eu

cd "$(dirname "$0")/../.."
REPORT_DIR="$PWD/e2e/reports/k6"
mkdir -p "$REPORT_DIR"

: "${TEST_BASE_URL:?TEST_BASE_URL must be set (never point this at production)}"
echo "==> Load target: $TEST_BASE_URL"

# Fail early if k6 is missing, with a safe diagnostic.
command -v k6 >/dev/null 2>&1 || { echo "k6 not found: install https://k6.io (e.g. brew install k6)"; exit 2; }

SCENARIOS=(garden-sync care-command-burst websocket-connect room-fanout reconnect-storm)
if [ "$#" -gt 0 ]; then SCENARIOS=("$@"); fi

# Summaries go to JSON for verify-thresholds.js and CI artifacts. k6 exits
# non-zero if any threshold breaches, which this script propagates.
FAIL=0
for SCENARIO in "${SCENARIOS[@]}"; do
  FILE="e2e/k6/$SCENARIO.js"
  [ -f "$FILE" ] || { echo "Unknown scenario: $SCENARIO"; exit 2; }
  echo "==> $SCENARIO"
  TEST_BASE_URL="$TEST_BASE_URL" \
  TEST_TOKEN_FILE="${TEST_TOKEN_FILE:-}" \
  SCENARIO_NAME="$SCENARIO" \
    k6 run --quiet "$FILE" -o json="$REPORT_DIR/$SCENARIO.jsonl" || FAIL=1
done

echo "==> Threshold gate"
node e2e/scripts/verify-thresholds.js k6 || FAIL=1

echo "Load run finished (fail=$FAIL); reports: e2e/reports/k6/"
exit "$FAIL"
