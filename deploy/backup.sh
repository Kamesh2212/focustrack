#!/bin/sh
# Daily backup of the sync database (run by cron on the server; deploy.sh installs it).
# Keeps 14 days of copies in the sync_data volume under /data/backups.
set -eu
day=$(date +%F)
sudo docker exec focustrack-sync sh -c "
  mkdir -p /data/backups &&
  rm -f /data/backups/focustrack-$day.db &&
  focustrack-server backup /data/backups/focustrack-$day.db &&
  find /data/backups -name 'focustrack-*.db' -mtime +14 -delete
"
