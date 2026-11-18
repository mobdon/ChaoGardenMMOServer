#!/usr/bin/env sh
# Rolling release with automatic rollback to the previously running image.
# Usage: scripts/release.sh <image> <staging|production>
set -eu
IMAGE="$1"
ENVIRONMENT="$2"
case "$ENVIRONMENT" in
  staging)    APP="chao-api-staging" ;;
  production) APP="chao-api-production" ;;
  *) echo "Unknown environment: $ENVIRONMENT" >&2; exit 2 ;;
esac
: "${API_DEPLOY_TOKEN:?API_DEPLOY_TOKEN must be set}"

PREVIOUS="$(flyctl releases list --app "$APP" --image --json \
  | grep -m1 '"imageRef"' | sed 's/.*: "//;s/".*//')" || PREVIOUS=""
echo "==> Previous image: ${PREVIOUS:-none (first release)}"

echo "==> Rolling release of $IMAGE onto $APP"
flyctl deploy --app "$APP" --image "$IMAGE" --strategy rolling --wait-timeout 300

echo "==> Post-release health gate"
ATTEMPT=0
while [ "$ATTEMPT" -lt 10 ]; do
  STATUS="$(flyctl status --app "$APP" --json | grep -m1 '"healthy"' || true)"
  case "$STATUS" in *true*) echo "Machines healthy."; exit 0 ;; esac
  ATTEMPT=$((ATTEMPT + 1))
  sleep 5
done

echo "!! Health gate failed; rolling back to $PREVIOUS" >&2
if [ -n "$PREVIOUS" ]; then
  flyctl deploy --app "$APP" --image "$PREVIOUS" --strategy rolling --wait-timeout 300
fi
exit 1
