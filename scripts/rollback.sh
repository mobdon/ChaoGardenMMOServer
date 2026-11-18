#!/usr/bin/env sh
# Manual rollback to the last known-good image. Operator-initiated; see
# docs/runbooks/deployment.md.
set -eu
IMAGE="$1"
ENVIRONMENT="$2"
case "$ENVIRONMENT" in
  staging)    APP="chao-api-staging" ;;
  production) APP="chao-api-production" ;;
  *) echo "Unknown environment: $ENVIRONMENT" >&2; exit 2 ;;
esac
: "${API_DEPLOY_TOKEN:?API_DEPLOY_TOKEN must be set}"
[ -n "$IMAGE" ] || { echo "Usage: scripts/rollback.sh <image> <env>" >&2; exit 2; }

echo "==> Rolling $APP back to $IMAGE"
flyctl deploy --app "$APP" --image "$IMAGE" --strategy rolling --wait-timeout 300
scripts/smoke-test.sh "$ENVIRONMENT"
echo "Rollback complete."
