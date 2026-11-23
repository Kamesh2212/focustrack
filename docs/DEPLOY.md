# Deploying the sync server

FocusTrack's hosted sync server runs on the project's Oracle Cloud Always Free server (Ubuntu 24.04, ARM, Docker),
at **https://sync.140-238-91-202.sslip.io**. sslip.io names point at the IP inside them, so no domain or DNS
setup is needed; Caddy gets a normal HTTPS certificate for the name.

```
phones & PCs ──HTTPS──▶ Caddy :443 ──▶ focustrack-sync :8787 (container) ──▶ /data/focustrack-sync.db
                                                     └─ daily copy ─▶ /data/backups (14 days)
```

The server only stores end-to-end-encrypted records: it can't read anyone's screen time.

## Deploy or update

From the repository on a PC that can SSH into the server (Git Bash):

```sh
deploy/deploy.sh standalone
```

It uploads the server's source (`server/`, `crates/sync-proto/`, `deploy/`) to `~/focustrack`, builds the image on
the server, starts the containers, installs the daily backup and checks `https://…/v1/status`. Running it again
updates the server; data stays in the `focustrack_sync_data` volume.

Settings, as environment variables when running the script: `FT_DEPLOY_HOST` (default `ubuntu@140.238.91.202`),
`FT_DEPLOY_KEY` (default `~/.ssh/oracle_exchange`), `SYNC_SITE` (default `sync.140-238-91-202.sslip.io`).

### Sharing HTTPS with another project on the server

`standalone` runs FocusTrack's own Caddy on ports 80/443. The server's `exchange` project has a Caddy of its own
that serves other sites from `~/caddy-extra/*.caddy`. To run both:

1. `ssh ubuntu@140.238.91.202 'cd ~/focustrack/deploy && sudo docker compose --profile standalone stop caddy'`
2. Start the exchange project as usual (its Caddy takes ports 80/443).
3. `deploy/deploy.sh shared`: the sync server joins exchange's network and `~/caddy-extra/focustrack.caddy`
   adds the site to its Caddy.

## Limits (in `deploy/docker-compose.yml`)

| Setting | Default | |
|---|---|---|
| `FT_SYNC_QUOTA_MB` | 500 | most data one account may store |
| `FT_SYNC_MAX_ACCOUNTS` | 1000 | sign-ups stop at this many accounts |

Sign-in, sign-up, token refresh, password change and account deletion are limited to 20 per minute per visitor,
and 5 wrong passwords lock an account for a while. Requests are at most 4 MB.

## Everyday commands (on the server)

```sh
sudo docker logs --tail 50 focustrack-sync       # server log
sudo docker logs --tail 50 focustrack-caddy      # HTTPS / certificate log
~/focustrack/deploy/backup.sh                    # back up now
sudo docker exec focustrack-sync ls -l /data/backups
```

Restore a backup: stop the server, copy the backup over the database, start it again:

```sh
cd ~/focustrack/deploy
sudo docker compose stop sync
sudo docker run --rm -v focustrack_sync_data:/data debian:bookworm-slim \
  cp /data/backups/focustrack-YYYY-MM-DD.db /data/focustrack-sync.db
sudo docker compose start sync
```

## Not done yet

- **Off-server backups**: backups live on the same disk. Copy them to Oracle Object Storage (free 20 GB) once a
  bucket exists.
- **Idle reclaim**: Oracle may reclaim Always Free servers it considers idle. Upgrading the account to
  Pay-As-You-Go (with a £1 budget alert) prevents that and still costs nothing.
- **Public IP**: the address is tied to the server's public IP. Make it a *reserved* IP in the Oracle console so it
  doesn't change; if it ever does, the sync address changes with it.
- **A real domain** (optional): set `SYNC_SITE` to it and deploy again; apps then need the new address.
