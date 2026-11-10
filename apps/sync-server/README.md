# Apex Sync Server

This Docker deployment runs the Apex Sync Server API for team access and project synchronization. The API supports first-owner setup, password sign-in, one-time invitation codes, team roles and member removal, revisioned project records, tombstones, content-addressed file transfer, and immutable team-shared Roblox decal codes. Apex Studio Settings can check the server, sign in, list/remove team members, create and accept invitation codes, and synchronize projects. Livery image blobs remain server-side for team sync and are streamed only when a member requests a preview; the preview is resized in memory by the desktop client. Production backup/restore remains future work. Review the [API contract](../../docs/planning/TEAM-SYNC-API.md) before integrating a client.

The original server source code is licensed under [Apache-2.0](../../LICENSE). The licence does not grant use of Apex Studio branding; see the [brand policy](../../BRANDING.md). Keep deployment secrets in `.env` and out of source control.

## Optional service gateway

Project synchronization remains independent of optional services. To enable Image Hosting, set `APEX_IMAGE_PROVIDER_URL` and `APEX_IMAGE_PROVIDER_KEY` on the Sync Server and the matching `APEX_SERVICE_KEY` on Image Provider. To enable Invite Tracking, set `APEX_INVITE_TRACKER_URL` and `APEX_INVITE_TRACKER_KEY` on the Sync Server and the matching `APEX_SERVICE_KEY` on the Invite Tracker. Use random keys of at least 32 characters, keep them only in server environments, and never place them in Apex Studio or logs. The gateway checks signed-in team membership and requires a write-capable role to create invites. Both services remain optional; leave each service's URL and key blank to disable it.

Image Provider source and deployment instructions are maintained separately in the Image Provider project. The gateway expects its `GET/POST /api/service/images` and `PATCH /api/service/images/{id}/sharing` contract; the browser and desktop app must call only the authenticated Sync Server routes. Invite Tracker exposes service-key-protected `GET/POST /api/service/campaigns`; its campaign records are scoped to the Apex team and creation is idempotent. Apex Studio accesses invites through `GET/POST /api/v1/teams/{team_id}/invite-campaigns`. The authenticated image routes remain `GET/POST /api/v1/teams/{team_id}/image-library` and `PATCH /api/v1/teams/{team_id}/image-library/{image_id}/sharing`; service availability is returned by `GET /api/v1/teams/{team_id}/services`.

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
