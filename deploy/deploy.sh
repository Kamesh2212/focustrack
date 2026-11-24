#!/usr/bin/env bash
# Deploys (or updates) the FocusTrack sync server on the Oracle server.
# Run from the repository on your PC (Git Bash):
#
#   deploy/deploy.sh standalone   # its own Caddy on ports 80/443
#   deploy/deploy.sh shared       # the server's shared Caddy (exchange project)
#
# Settings (environment): FT_DEPLOY_HOST, FT_DEPLOY_KEY, SYNC_SITE.
set -euo pipefail

MODE=${1:-standalone}
HOST=${FT_DEPLOY_HOST:-ubuntu@140.238.91.202}
KEY=${FT_DEPLOY_KEY:-$HOME/.ssh/oracle_exchange}
SITE=${SYNC_SITE:-sync.140-238-91-202.sslip.io}
case "$MODE" in standalone | shared) ;; *) echo "usage: $0 standalone|shared" >&2; exit 2 ;; esac

cd "$(git rev-parse --show-toplevel)"
ssh_() { ssh -i "$KEY" -o BatchMode=yes "$HOST" "$@"; }

echo "Uploading the server's source to $HOST:~/focustrack"
ssh_ 'rm -rf ~/focustrack/server ~/focustrack/crates ~/focustrack/deploy && mkdir -p ~/focustrack'
tar czf - server/Cargo.toml server/Cargo.lock server/src server/Dockerfile \
  crates/sync-proto/Cargo.toml crates/sync-proto/Cargo.lock crates/sync-proto/src deploy |
  ssh_ 'tar xzf - -C ~/focustrack'

echo "Building and starting ($MODE)"
ssh_ "set -e
cd ~/focustrack/deploy
chmod +x backup.sh
printf 'SYNC_SITE=%s\n' '$SITE' > .env
if [ '$MODE' = standalone ]; then
  sudo docker compose --profile standalone up -d --build
else
  sudo docker network inspect exchange_default >/dev/null 2>&1 || { echo 'The shared Caddy (exchange project) is not running.' >&2; exit 1; }
  sudo docker compose --profile standalone stop caddy 2>/dev/null || true
  sudo docker compose -f docker-compose.yml -f docker-compose.shared.yml up -d --build sync
  mkdir -p ~/caddy-extra
  printf '%s {\n\tencode gzip\n\treverse_proxy focustrack-sync:8787\n}\n' '$SITE' > ~/caddy-extra/focustrack.caddy
  sudo docker exec caddy caddy reload --config /etc/caddy/Caddyfile
fi
# Daily backup at 03:30 (server time), replacing any earlier entry.
( crontab -l 2>/dev/null | grep -v focustrack-backup || true; echo '30 3 * * * \$HOME/focustrack/deploy/backup.sh >> \$HOME/focustrack/backup.log 2>&1 # focustrack-backup' ) | crontab -
sudo docker image prune -f >/dev/null
"

echo "Checking https://$SITE/v1/status"
for _ in $(seq 1 30); do
  if curl -fsS "https://$SITE/v1/status"; then echo; echo "Done: https://$SITE"; exit 0; fi
  sleep 5
done
echo "The server didn't answer over HTTPS yet. Logs: ssh $HOST 'sudo docker logs focustrack-sync; sudo docker logs focustrack-caddy'" >&2
exit 1
