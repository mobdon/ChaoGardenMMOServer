#!/usr/bin/env sh
# Post-deploy staging smoke test. Fails the deploy workflow if any gate fails.
# Full procedure and recorded results: docs/runbooks/staging-smoke-test.md
set -eu
ENVIRONMENT="${1:-staging}"
case "$ENVIRONMENT" in
  staging)    BASE="https://staging.chaogarden.example" ;;
  production) BASE="https://api.chaogarden.example" ;;
  *) echo "Unknown environment: $ENVIRONMENT" >&2; exit 2 ;;
esac
fail() { echo "SMOKE FAIL: $1" >&2; exit 1; }
check() {
  NAME="$1"; WANT="$2"; shift 2
  GOT="$(curl -fsS -m 10 -o /dev/null -w '%{http_code}' "$@" 2>/dev/null || true)"
  [ "$GOT" = "$WANT" ] || fail "$NAME expected $WANT got ${GOT:-none}"
  echo "ok   $NAME -> $WANT"
}

echo "==> Smoke test against $BASE"
check "liveness" 200 "$BASE/livez"
check "readiness" 200 "$BASE/readyz"
check "status" 200 "$BASE/api/status"
check "activities" 200 "$BASE/api/activities"
check "metrics" 200 "$BASE/metrics"
check "session denied without content-type" 415 -X POST "$BASE/api/session"
check "origin guard rejects bad origin" 403 -X POST "$BASE/api/session" \
  -H 'Content-Type: application/json' -H 'Origin: https://evil.example' -d '{}'

METRICS="$(curl -fsS -m 10 "$BASE/metrics")"
echo "$METRICS" | grep -q '^chao_http_requests_total' \
  || fail "metrics missing request series"
echo "ok   metrics expose request series"

echo "All smoke checks passed for $ENVIRONMENT."
