---
description: Install Windfenster, the self-hosted wind forecast, with Docker Compose or a Podman Quadlet; reverse proxy, Cloudflare Tunnel, updates, backups and disk space.
---

# Installation

Windfenster is one container: the server, the web app and the database (SQLite) in one image,
`tristanteu/windfenster` on Docker Hub (also `ghcr.io/firsttris/windfenster`), for **amd64 and
arm64**: a Linux server, a NAS, a mini PC, a Raspberry Pi 4 or 5. All it keeps lives in one data
folder.

## Docker Compose

```bash
mkdir windfenster && cd windfenster
curl -O https://raw.githubusercontent.com/firsttris/windfenster/main/compose.yaml
docker compose up -d
```

Open <http://localhost:8080>. The data lands in `./data`. The settings go into the
`environment:` part of `compose.yaml`; see [Configuration](configuration.md). To build the image
from source instead, run `docker compose up --build` in a checkout.

## Podman Quadlet (a server with systemd)

As a normal user with [Podman](https://podman.io/) 4.4 or newer.

**1. Folders and the unit.** The data folder must exist before the start; Podman refuses to mount
a missing one.

```bash
mkdir -p ~/.config/containers/systemd ~/windfenster/data
curl -o ~/.config/containers/systemd/windfenster.container \
  https://raw.githubusercontent.com/firsttris/windfenster/main/windfenster.container
```

**2. The settings.** Edit the `[Container]` part of
`~/.config/containers/systemd/windfenster.container`. An example for a server with room to spare
(a large SSD; Windfenster needs no memory setting):

```ini
[Container]
Image=docker.io/tristanteu/windfenster:latest
ContainerName=windfenster
AutoUpdate=registry
# Only the reverse proxy on this host reaches it; 8080 taken? 127.0.0.1:8081:8080
PublishPort=127.0.0.1:8080:8080
Volume=%h/windfenster/data:/data:Z

# The public address: canonical links, meta tags and the passkeys' domain
Environment=WF_PUBLIC_URL=https://windfenster.example
# Basemap: the world to zoom 8, Europe to zoom 12 with every town's streets (about 7 GB)
Environment=WF_BASEMAP_WORLD_ZOOM=8
Environment=WF_BASEMAP_ZOOM=12
# Full detail (zoom 14) for up to 500 spots where people look (2 to 17 MB each)
Environment=WF_BASEMAP_SPOTS=500
# Relief of the whole world to zoom 8 (estimated 3 to 4 GB)
Environment=WF_TERRAIN_ZOOM=8
# Stations kept current and verified even when nobody opens them
Environment=WF_TRACKED_STATIONS=edsb,lgrp
# Alarm e-mails (without them alarms go by push only)
# Environment=WF_SMTP_URL=smtps://alarm%40example.org:PASSWORD@smtp.example.org:465
# Environment=WF_MAIL_FROM=Windfenster <alarm@example.org>
Environment=RUST_LOG=info
```

Leave `[Service]` and `[Install]` as they are.

**3. Start it, and at boot.**

```bash
systemctl --user daemon-reload
systemctl --user start windfenster
loginctl enable-linger $USER                               # start at boot without a login
systemctl --user enable --now podman-auto-update.timer     # pull new releases daily
```

**4. Check it.**

```bash
curl http://127.0.0.1:8080/api/health    # {"version":…,"schemaVersion":…,"stations":…}
journalctl --user -u windfenster -f      # or: podman logs -f windfenster
```

As root instead: put the unit into `/etc/containers/systemd/`, use a path like
`Volume=/srv/windfenster:/data:Z` and `systemctl` without `--user`; linger is not needed.

## Reaching it from outside

Set `WF_PUBLIC_URL` to the address people use **before the first sign-up**: passkeys belong to
that domain, and changing it later makes them useless.

=== "Reverse proxy (Caddy)"

    Caddy fetches the TLS certificate itself. `/etc/caddy/Caddyfile`:

    ```
    windfenster.example {
        reverse_proxy 127.0.0.1:8080
    }
    ```

    Ports 80 and 443 must reach the server.

=== "Cloudflare Tunnel"

    No port forwarding: `cloudflared` on the server opens an outgoing connection to Cloudflare,
    and Cloudflare serves the domain with HTTPS. The domain's DNS must be at Cloudflare.

    1. In the Cloudflare dashboard, *Zero Trust → Networks → Tunnels*: create a tunnel and copy
       its token.
    2. Add a *public hostname*, e.g. `windfenster.example`, with the service
       `http://localhost:8080` (or `http://windfenster:8080` when `cloudflared` runs in the same
       Compose project).
    3. Run `cloudflared` next to Windfenster, e.g. in `compose.yaml`:

        ```yaml
          cloudflared:
            image: cloudflare/cloudflared:latest
            restart: unless-stopped
            command: tunnel run
            environment:
              - TUNNEL_TOKEN=<the token>
        ```

    Push messages need HTTPS, which the tunnel brings; a `.app` domain works only over HTTPS
    anyway.

    With the tunnel only, nothing needs port 8080 on the network: leave `ports:` out (or publish
    on `127.0.0.1` only), so the way in is the tunnel and Cloudflare's protection in front of it.
    The per-client limits read the client's address from `CF-Connecting-IP`, which Cloudflare
    sets ([`WF_CLIENT_IP_HEADER`](configuration.md)). In the Cloudflare dashboard, a rate limiting
    rule on `/api/` stops floods before they reach the server.

=== "Both: Caddy at home, the tunnel from outside"

    The same server can be reached at home through Caddy and from the internet through the
    tunnel. Two things to know:

    - **Passkeys belong to one domain**, the one in `WF_PUBLIC_URL`. Use that domain at home
      too (a local DNS entry pointing it to Caddy, Caddy with a certificate for it), or sign in
      at home with e-mail and password.
    - **With an https `WF_PUBLIC_URL` the session cookie is Secure**: the browser keeps you
      signed in only over https, so open it at home through Caddy, not at `http://…:8080`.

    Requests through Caddy carry no `CF-Connecting-IP`; the limits then count Caddy's address,
    so everyone at home shares one count (30 new places an hour), plenty for a household.
    If Caddy itself is open to the internet, set `WF_CLIENT_IP_HEADER=X-Forwarded-For`.

## Without root

The server in the container does not run as root. Under Docker the container starts as root
only to hand the data folder to its user (uid 10001, or `WF_UID`/`WF_GID`) and then drops to it,
with no way back (`no-new-privileges`). A data folder from an older version is handed over once
on the first start. On the host the files then belong to uid 10001: set `WF_UID` to your own
(`id -u`) to keep them yours, or use `sudo` for backups. Under rootless Podman root in the
container is your user on the host already; nothing changes there unless `WF_UID` is set.

## What happens on the first start

- The server creates the database and fetches the station lists, the measurements and the newest
  model runs right away.
- The map's first hours are there once ICON-D2 and ECMWF are downloaded (about 150 and 115 MB per
  run); ICON global follows (about 1.1 GB per run).
- A minute after the start it cuts the [basemap and the relief](basemap.md) (with
  `WF_BASEMAP=ask` only once an admin says so). Until they are ready, the map shows Natural Earth
  and coarser tiles. Detail around a place comes a minute or two after it is opened.

## The first admin

Sign up in the app, then give your account the admin role on the server:

```bash
docker compose exec windfenster windfenster admin you@example.org
# Podman: podman exec windfenster windfenster admin you@example.org
```

The gear in the header (on a phone: the link on the account page) opens the
[admin page](admin.md): what the server is doing, the map's downloads, the forecast settings,
accounts and sign-up, backups.

## Disk and downloads

| | Defaults | With the example above |
|---|---|---|
| Basemap | 323 MB | about 7.5 GB |
| Relief | 948 MB | about 3–4 GB |
| Detail spots | up to 600 MB | a few GB |
| Map runs (two per model) | about 4 GB | about 4 GB |
| Database after a year | up to about 1 GB | up to about 1 GB |
| **Total** | **about 6 GB** | **about 18 GB** |

The big download is ICON global: about 1.1 GB per run at 00 and 12 UTC, 2.2 GB a day;
`WF_ICON_GLOBAL=false` leaves the world to ECMWF. While the basemap or relief is rebuilt, the old
file stays until the new one is complete, so plan room for both.

## Updating

- **Podman**: the auto-update timer pulls a new `:latest` once a day and restarts the container.
- **Compose**: `docker compose pull && docker compose up -d`.

The database migrates itself on start. Releases are listed on
[GitHub](https://github.com/firsttris/windfenster/releases). The admin page shows under *System*
when a newer one is out: the server asks GitHub's API for the newest release once a day, its only
request that is not about weather or map data.

## Backup

Only `windfenster.db` and `vapid.pem` (the push key) matter; the map data comes back by itself.
The admin page downloads both under *Storage and database*. On the server, while it runs:

```bash
sqlite3 ~/windfenster/data/windfenster.db "VACUUM INTO 'windfenster-backup.db'"
cp ~/windfenster/data/vapid.pem vapid-backup.pem
```

Or stop the container and copy `windfenster.db*` and `vapid.pem`. A new push key makes every
browser subscribe to alarms again.
