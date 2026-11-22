#!/usr/bin/env bash
# One-time droplet setup (Ubuntu 24.04, as root), run from the server checkout
# at /srv/chao-src/server after the build user and its deploy keys exist:
#   bash deploy/setup.sh play.example.com
# Safe to run again. It never touches /var/lib/chao (the saves) and never
# overwrites an existing /etc/chao/chao.env.
set -euo pipefail
[[ $# == 1 && $EUID == 0 ]] || { echo 'usage (as root): setup.sh <domain>' >&2; exit 2; }
domain=$1
here=$(cd "$(dirname "$0")" && pwd)
SRC=/srv/chao-src
CLIENT_REPO=${CLIENT_REPO:-git@github-client:mobdon/ChaoGardenMMOClient.git}
export DEBIAN_FRONTEND=noninteractive

apt-get update -q
apt-get install -yq caddy git python3 curl build-essential pkg-config ufw
if ! command -v node >/dev/null || [[ $(node -p 'process.versions.node.split(".")[0]') -lt 22 ]]; then
    curl -fsSL https://deb.nodesource.com/setup_24.x | bash -
    apt-get install -yq nodejs
fi

# Release builds of the server need more memory than a small droplet has.
if ! swapon --show | grep -q .; then
    fallocate -l 4G /swapfile && chmod 600 /swapfile && mkswap -q /swapfile && swapon /swapfile
    echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

id chao >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin chao
[[ -x /home/chaobuild/.cargo/bin/cargo ]] ||
    runuser -l chaobuild -c 'curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal'
[[ -d $SRC/client/.git ]] || runuser -l chaobuild -c "git clone -q '$CLIENT_REPO' '$SRC/client'"

mkdir -p /etc/chao /opt/chao /var/backups/chao
chmod 700 /var/backups/chao
if [[ ! -f /etc/chao/chao.env ]]; then
    sed -e "s|^CHAO_ALLOWED_ORIGINS=.*|CHAO_ALLOWED_ORIGINS=https://$domain|" \
        -e "s|^CHAO_PUBLIC_URL=.*|CHAO_PUBLIC_URL=https://$domain|" "$here/chao.env.example" > /etc/chao/chao.env
    chmod 600 /etc/chao/chao.env
fi
touch /etc/chao/enable-private-playtest   # the service refuses to start without it
sed "s|example.invalid|$domain|" "$here/Caddyfile" > /etc/caddy/Caddyfile
cp "$here"/chao.service "$here"/chao-pull.service "$here"/chao-pull.timer \
    "$here"/chao-backup.service "$here"/chao-backup.timer /etc/systemd/system/
systemctl daemon-reload

ufw allow OpenSSH >/dev/null
ufw allow 80/tcp >/dev/null
ufw allow 443 >/dev/null
ufw --force enable >/dev/null

bash "$here/pull.sh"   # first build and install; takes several minutes
systemctl enable -q chao.service caddy.service
systemctl enable -q --now chao-backup.timer
systemctl reload caddy || systemctl restart caddy
echo "Set up. Open https://$domain once DNS points here."
echo 'To turn on automatic updates:  systemctl enable --now chao-pull.timer'
echo 'To update by hand in one step: /opt/chao/current/deploy/pull.sh'
