# Apex Sync Server

This Docker deployment runs the Apex Sync Server API for team access and project synchronization. The API supports first-owner setup, password sign-in, one-time invitation codes, team roles, revisioned project records, tombstones, and content-addressed file transfer. Apex Studio Settings can check the server, sign in, list team members, create and accept invitation codes, and synchronize projects. Production backup/restore remains future work. Review the [API contract](../../docs/planning/TEAM-SYNC-API.md) before integrating a client.

## Requirements

For a temporary Windows PC deployment without incoming ports, use the
[Cloudflare Tunnel setup](TUNNEL.md) instead of the Caddy deployment below.

- A Linux server with Docker Engine and Docker Compose.
- A domain or subdomain whose DNS `A`/`AAAA` record points to this server.
- Inbound TCP ports 80 and 443 available so Caddy can obtain and renew the HTTPS certificate. UDP 443 is optional and enables HTTP/3.

## Start

Copy `.env.example` to `.env`, set `APEX_SERVER_DOMAIN`, and replace the database password with a random 64-character hexadecimal value. On a Linux host with OpenSSL, generate one with `openssl rand -hex 32`.

From this directory, build and start the services:

```sh
docker compose up --build -d
```

Check the public endpoints after DNS has propagated and Caddy has issued its certificate:

```sh
curl https://YOUR_DOMAIN/health/live
curl https://YOUR_DOMAIN/health/ready
curl https://YOUR_DOMAIN/api/v1/server-info
```

The server binds only inside the Compose network; Caddy is the only service publishing internet-facing ports. PostgreSQL and its persistent volume are not exposed to desktop clients. Keep `.env` private and outside backups that are shared with designers.

## Sync logs

The API writes compact structured events and request failures to stdout/stderr. With Docker Compose, follow them with:

```sh
docker compose logs -f --tail=100 api
```

Events identify team, project, actor, record/revision or blob size, and the operation result. Successful empty change polls are omitted. Rejected requests include the matched route, HTTP status, and duration. Logs never include credentials, invitation codes, request bodies, project content, or file contents. The Compose JSON log rotation is limited to 10 MB across three files; retain or export logs separately if longer history is needed.

## Data volumes

- `database-data`: PostgreSQL server data.
- `proxy-data` and `proxy-config`: Caddy certificates and runtime state.

The `blob-data` volume stores project files. Back up PostgreSQL and this volume at the same consistent point. A tested backup/restore tool is not yet included; use only test data until that workflow is available.
