#!/usr/bin/env sh
# Nightly logical backup of the Postgres cluster, uploaded to object storage
# (R2/S3) with retention pruning. Scheduled in CI with OIDC credentials; see
# docs/runbooks/database-restore.md for the restore procedure.
#
# Managed Flex Postgres also keeps continuous WAL archives plus daily
# snapshots (7 days staging / 30 days production); the logical dump below is
# the cross-provider safety net used for quarterly restore drills.
set -eu
ENVIRONMENT="${1:-staging}"
case "$ENVIRONMENT" in
  staging)    DB_APP="chao-api-staging-db";    BUCKET="chao-staging-backups";    KEEP_DAYS=7  ;;
  production) DB_APP="chao-api-production-db"; BUCKET="chao-production-backups"; KEEP_DAYS=30 ;;
  *) echo "Unknown environment: $ENVIRONMENT" >&2; exit 2 ;;
esac
: "${API_DEPLOY_TOKEN:?API_DEPLOY_TOKEN must be set}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
DUMP="./chao-$STAMP.dump"

echo "==> Logical dump of $DB_APP"
flyctl ssh console --app "$DB_APP" --command \
  "pg_dump -Fc --no-owner -f /tmp/chao-$STAMP.dump \$DATABASE_URL"
flyctl ssh sftp get --app "$DB_APP" "/tmp/chao-$STAMP.dump" "$DUMP"

echo "==> Uploading to s3://$BUCKET/$STAMP/"
aws s3 cp "$DUMP" "s3://$BUCKET/$STAMP/chao-$STAMP.dump"

echo "==> Pruning dumps older than $KEEP_DAYS days"
aws s3 ls "s3://$BUCKET/" --recursive \
  | awk -v cutoff="$(date -u -d "-$KEEP_DAYS days" +%Y%m%d)" \
      '{key=$4; if (key ~ /^([0-9]{8}T)/) { stamp=substr(key,1,8) } else { stamp=0 }
       if (stamp != 0 && stamp < cutoff) print key }' \
  | while IFS= read -r key; do
      echo "removing $key"
      aws s3 rm "s3://$BUCKET/$key"
    done

rm -f "$DUMP"
echo "Backup complete: $STAMP"
