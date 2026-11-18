#!/usr/bin/env bash
# Run only on a dedicated reviewed Linux deployment, as root. No remote upload.
set -euo pipefail
if [[ $# != 2 ]]; then
    echo 'usage: backup.sh /absolute/data-directory /absolute/new-backup.zip' >&2
    exit 2
fi
[[ "$1" = /* && "$2" = /* ]] || { echo 'Absolute paths required' >&2; exit 2; }
exec 9>/run/lock/chao-backup.lock
flock -n 9 || { echo 'Another backup is in progress' >&2; exit 1; }
unit=chao.service
was_active=false
if systemctl is-active --quiet "$unit" || [[ "$(systemctl show -p MainPID --value "$unit")" != 0 ]]; then
    was_active=true
fi
restart() {
    if "$was_active"; then systemctl start "$unit"; fi
}
trap restart EXIT
systemctl stop "$unit"
if systemctl is-active --quiet "$unit" || [[ "$(systemctl show -p MainPID --value "$unit")" != 0 ]]; then
    echo 'Authority did not stop; refusing backup' >&2
    exit 1
fi
python3 "$(dirname "$0")/backup.py" create "$1" "$2" --offline
