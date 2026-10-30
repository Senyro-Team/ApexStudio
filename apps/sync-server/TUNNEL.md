# Temporary PC deployment with Cloudflare Tunnel

This is a standalone alternative to `compose.yaml`. It publishes no host ports
and requires no router port forwarding. Caddy is not used. Docker Desktop must
run Linux containers (for example using WSL 2 on Windows).

Traffic flows through Cloudflare HTTPS, the outbound Cloudflared tunnel, and then
`http://api:8080` inside Docker. PostgreSQL is on a separate internal network;
Cloudflared cannot access that network. Container logs rotate at 10 MB, retaining
up to three files per service.

The server API includes one-time first-owner setup, password sign-in, invitations,
team roles, revisioned project data, and content-addressed file transfer. The
Apex Studio desktop client is not yet connected to it. Do not store real team
work until client integration and a tested PostgreSQL/blob backup and restore
process are available. The API routes and current limits are in
[`docs/planning/TEAM-SYNC-API.md`](../../docs/planning/TEAM-SYNC-API.md).

## 1. Create a remotely managed tunnel

In the Cloudflare dashboard, open Tunnels (under Networking or Cloudflare One,
depending on the dashboard), create a Cloudflared tunnel, and select Docker for
the connector instructions. Copy the value following `--token`; this is the
connector token, not a Cloudflare API token. Do not run the dashboard's Docker
command separately: Compose will run the connector.

Add a published application route:

| Field | Value |
| --- | --- |
| Hostname | `sync.your-domain.example` (replace with your own hostname) |
| Service type | HTTP |
| Service URL | `api:8080` |

Leave the path empty to route all paths. Do not use `localhost`, HTTPS, or a
Windows host IP for the origin. Cloudflare serves public HTTPS; the origin uses
HTTP within Docker.

Use a domain active in your Cloudflare account. The dashboard creates the tunnel
DNS record when you publish the route. Resolve any existing DNS record conflict
for that hostname first. No A/AAAA record pointing to your home IP is required.

For a private browser test, protect the hostname with a Cloudflare Access
self-hosted application and an Allow policy restricted to your email/identity.
Configure Access before publishing the route if it must never be public.
The tunnel alone does not authenticate visitors. Future desktop API clients
will need compatible authentication; a browser Access login is not automatically
supported by an API client. Do not embed an Access service-token secret in a
distributed desktop app.

## 2. Configure local secrets

From PowerShell:

```powershell
cd D:\ChatGPT\ApexStudio\apps\sync-server
Copy-Item .env.tunnel.example .env.tunnel

# Generate a 64-character hexadecimal database password.
$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$rng.Dispose()
[BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
```

Do not overwrite an existing `.env.tunnel`. Edit the copied file and set the
database password and tunnel token. Generate a separate random bootstrap token
of at least 32 characters and set it temporarily. Keep the database password
stable once the database volume exists:
changing the environment variable does not change an existing PostgreSQL user's
password. No `APEX_SERVER_DOMAIN` setting is needed for this deployment.

The local `.env` files are ignored by Git and excluded from Docker build context.
Tokens in environment variables are still visible to users with Docker access.
Keep the file private; rotate the tunnel token in Cloudflare if it is exposed.

After starting the API, send `POST /api/v1/setup` once with the bootstrap token,
team name, Owner email/display name, and a 12–128 character password. The
response contains the first session token. Remove `APEX_SYNC_BOOTSTRAP_TOKEN`
from `.env.tunnel` and restart the API container after setup. Setup becomes
unavailable after the first successful account creation even if the token is
still configured. Keep session tokens private; use them only as bearer tokens
over HTTPS. A Cloudflare Tunnel does not itself authenticate API requests.

## 3. Start

If the original Caddy deployment is running in this directory, stop it first:

```powershell
docker compose down
```

Do not add `-v`: it deletes database volumes. With the same default Compose
project name, both deployments reuse `database-data`; keep an existing database
password when switching. If the previous deployment used `-p` or a custom project
name, use the same project name consistently to reuse its volume.

```powershell
docker compose --env-file .env.tunnel -f compose.tunnel.yaml config --quiet
docker compose --env-file .env.tunnel -f compose.tunnel.yaml up --build -d
docker compose --env-file .env.tunnel -f compose.tunnel.yaml ps
docker compose --env-file .env.tunnel -f compose.tunnel.yaml logs --tail 100 cloudflared api
```

Always use this explicit file and env-file pair. Do not combine the two Compose
files using multiple `-f` options: that could retain Caddy's published ports.
Avoid sharing the output of plain `docker compose config`, which includes secrets.

## 4. Check and stop

Replace the hostname before running:

```powershell
curl.exe https://sync.your-domain.example/health/live
curl.exe https://sync.your-domain.example/health/ready
curl.exe https://sync.your-domain.example/api/v1/server-info
```

`/health/live` should return `{"status":"alive"}`. With Access enabled,
authenticate in a browser and open these URLs there instead. Unauthenticated
curl requests will encounter Access rather than the API response.

```powershell
# Stop temporarily, retaining containers and data.
docker compose --env-file .env.tunnel -f compose.tunnel.yaml stop
# Resume the existing containers.
docker compose --env-file .env.tunnel -f compose.tunnel.yaml start
# Remove containers and networks, retaining database data.
docker compose --env-file .env.tunnel -f compose.tunnel.yaml down
```

Keep the PC awake, online, and Docker Desktop running. No incoming firewall rule
is needed. A restrictive firewall must allow Cloudflared's outbound connections
to Cloudflare on port 7844 (UDP for QUIC or TCP for HTTP/2), as well as normal DNS
resolution and HTTPS access for image pulls/builds.

A 502 response usually means the connector cannot reach `http://api:8080`:
check the route's type/URL and API logs. An unhealthy database can prevent the API
from starting. Check `database` logs without sharing credentials. A healthy tunnel
status only proves the connector is online, not that the API is ready.

To update the connector image, run `docker compose --env-file .env.tunnel -f
compose.tunnel.yaml pull cloudflared`, then repeat the `up --build -d` command.
The temporary setup uses `latest`; use a tested pinned image version/digest for
reproducible long-term deployment. A Docker volume is persistence, not a backup;
establish and test a consistent backup/restore process before using real data.

References: [Cloudflare Tunnel setup](https://developers.cloudflare.com/tunnel/get-started/)
and [published application routing](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/routing-to-tunnel/).
