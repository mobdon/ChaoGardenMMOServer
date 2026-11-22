#!/usr/bin/env bash
# Run as root by chao-backup.timer: one save backup a night, newest 14 kept.
set -euo pipefail
dir=/var/backups/chao
"$(dirname "$0")/backup.sh" /var/lib/chao "$dir/nightly-$(date -u +%Y%m%dT%H%M%SZ).zip"
find "$dir" -name 'nightly-*.zip' -mtime +14 -delete
