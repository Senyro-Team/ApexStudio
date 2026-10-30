# Apex Sync Server

This Docker deployment is the infrastructure foundation for team sync. It currently serves health and version information only. It has no sign-in, invitations, project access controls, or synchronization API, so it is not ready for real team data.

## Requirements

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

## Data volumes

- `database-data`: PostgreSQL server data.
- `proxy-data` and `proxy-config`: Caddy certificates and runtime state.

The first version has no user project blobs. A consistent backup/restore procedure for PostgreSQL and the future blob store must be implemented before storing team work here.
