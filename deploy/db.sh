#!/usr/bin/env bash
# Look at the cloud sync database (read-only) from your PC (Git Bash):
#
#   deploy/db.sh                 # summary: accounts, devices, records per kind, latest records
#   deploy/db.sh watch           # the same, refreshed every 5 seconds (Ctrl+C to stop)
#   deploy/db.sh shell           # sqlite3 prompt (.tables, .schema records, SELECT …; .quit)
#   deploy/db.sh "SELECT …"      # one query
#
# Records are end-to-end encrypted: the server (and this) sees only metadata such
# as kind, device, day and size, never the screen time inside them.
set -euo pipefail

HOST=${FT_DEPLOY_HOST:-ubuntu@140.238.91.202}
KEY=${FT_DEPLOY_KEY:-$HOME/.ssh/oracle_exchange}
DB=/var/lib/docker/volumes/focustrack_sync_data/_data/focustrack-sync.db
SQL="sudo sqlite3 -readonly -header -column $DB"

ssh_() { ssh -i "$KEY" -o BatchMode=yes "$HOST" "$@"; }
# Ubuntu's sqlite3 package, installed the first time.
ssh_ 'command -v sqlite3 >/dev/null || { sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=l apt-get install -y -qq sqlite3 >/dev/null 2>&1; }'

summary() {
  cat <<'EOF'
.print '== Accounts'
SELECT substr(id, 1, 8) AS account, email, datetime(created, 'unixepoch') AS created_utc,
       retention_days, failed_logins,
       (SELECT COUNT(*) FROM devices d WHERE d.account = a.id AND d.revoked = 0) AS devices,
       (SELECT COUNT(*) FROM records r WHERE r.account = a.id) AS records,
       (SELECT printf('%.1f KB', COALESCE(SUM(length(payload)), 0) / 1024.0) FROM records r WHERE r.account = a.id) AS stored
FROM accounts a ORDER BY created;
.print ''
.print '== Devices'
SELECT substr(id, 1, 8) AS device, substr(account, 1, 8) AS account, platform, app_version,
       datetime(last_seen, 'unixepoch') AS last_seen_utc, revoked
FROM devices ORDER BY last_seen DESC;
.print ''
.print '== Records per kind'
SELECT kind, COUNT(*) AS records, SUM(deleted) AS deleted, MAX(seq) AS latest_seq FROM records GROUP BY kind ORDER BY records DESC;
.print ''
.print '== Latest 15 records'
SELECT seq, kind, substr(device, 1, 8) AS device, day, version, deleted, length(payload) AS bytes
FROM records ORDER BY seq DESC LIMIT 15;
EOF
}

case "${1:-}" in
  "")
    summary | ssh_ "$SQL"
    ;;
  watch)
    while true; do
      out=$(summary | ssh_ "$SQL" 2>&1)
      clear
      echo "FocusTrack sync database — $(date +%T) (refreshes every 5 s, Ctrl+C to stop)"
      echo "$out"
      sleep 5
    done
    ;;
  shell)
    ssh -t -i "$KEY" "$HOST" "$SQL"
    ;;
  *)
    printf '%s\n' "$1" | ssh_ "$SQL"
    ;;
esac
