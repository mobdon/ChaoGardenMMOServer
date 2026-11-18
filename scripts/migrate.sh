#!/usr/bin/env sh
# Controlled migration release step. Runs `chao-game-server migrate` against
# the database/data volume BEFORE the new image rolls. If it fails, the
# release is aborted and machines keep serving the previous image.
set -eu
IMAGE="$1"
ENVIRONMENT="$2"
case "$ENVIRONMENT" in
  staging)    APP="chao-api-staging" ;;
  production) APP="chao-api-production" ;;
  *) echo "Unknown environment: $ENVIRONMENT" >&2; exit 2 ;;
esac
: "${API_DEPLOY_TOKEN:?API_DEPLOY_TOKEN must be set (GitHub environment secret)}"

echo "==> Migrating $APP to $IMAGE"
flyctl deploy --app "$APP" --image "$IMAGE" --strategy immediate \
  --release-command /usr/local/bin/chao-game-server migrate --wait-timeout 300

echo "==> Verifying migration"
flyctl ssh console --app "$APP" --command \
  '/usr/local/bin/chao-game-server migrate && cat /data/.schema-version'
echo "Migration complete for $ENVIRONMENT."
