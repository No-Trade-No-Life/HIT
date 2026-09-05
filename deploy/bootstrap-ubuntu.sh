#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install --yes ca-certificates caddy curl
id hit >/dev/null 2>&1 || useradd --system --home-dir /var/lib/hit --shell /usr/sbin/nologin hit
install -d -m 0755 /opt/hit/releases
install -d -o hit -g hit -m 0700 /var/lib/hit

install -m 0644 /dev/stdin /etc/systemd/system/hit.service <<'UNIT'
[Unit]
Description=HIT algorithmic trading executor platform
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=hit
Group=hit
WorkingDirectory=/var/lib/hit
ExecStart=/opt/hit/current/hit
Restart=on-failure
RestartSec=5s
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/hit

[Install]
WantedBy=multi-user.target
UNIT

install -m 0644 /dev/stdin /etc/caddy/Caddyfile <<'CADDY'
hit.ntnl.io {
    reverse_proxy 127.0.0.1:8080
}
CADDY

systemctl daemon-reload
systemctl enable hit.service caddy.service
systemctl restart caddy.service
