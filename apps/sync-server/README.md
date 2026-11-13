# Apex Sync Server

This directory contains the Apex Sync Server API and its Docker Compose deployment. It provides team accounts, invitations and roles, optional project synchronization, a scoped gateway for optional image and invite services, and shared Roblox decal codes. In Apex Studio, users connect to an administrator-provided server from the **Server** area; they do not install this software on their own computers.

The normal Apex Studio installer does not install or manage server containers. An administrator self-hosting the Sync Server needs a Linux Docker host, or can use the separate Cloudflare Tunnel instructions for a temporary PC deployment. The Sync Server is the only server application included in this repository. Image Provider and Invite Tracker source and deployment projects are maintained separately; they are optional and are not bundled into either this Compose stack or the Apex Studio installer.

Livery image blobs for project sync remain on the server and are streamed only when a member requests a preview; the desktop client resizes the preview in memory. Production backup/restore remains future work. Do not store important team data until an independently tested backup and restore process is in place. Review the [API contract](../../docs/planning/TEAM-SYNC-API.md) before integrating another client.

The original server source code is licensed under [Apache-2.0](../../LICENSE). The licence does not grant use of Apex Studio branding; see the [brand policy](../../BRANDING.md). Keep deployment secrets in `.env` and out of source control.

## Optional service gateway

Project synchronization and the two service integrations are independent. To enable Image Hosting, deploy the separate Image Provider project, then set `APEX_IMAGE_PROVIDER_URL` and `APEX_IMAGE_PROVIDER_KEY` on the Sync Server and the matching `APEX_SERVICE_KEY` on Image Provider. To enable Invite Tracking, deploy the separate Invite Tracker project, then set `APEX_INVITE_TRACKER_URL` and `APEX_INVITE_TRACKER_KEY` on the Sync Server and the matching `APEX_SERVICE_KEY` on Invite Tracker. Both integrations are optional; leave each service's URL and key blank to disable it. Their source and deployment instructions are outside this repository; no upstream repository link is configured in this checkout.

Use independent random keys of at least 32 characters. Keep them only in the corresponding server environment files; never put service keys in Apex Studio, source control, or logs. Public service URLs must use HTTPS so gateway keys are not sent in cleartext. HTTP is accepted only for loopback, private IP addresses, and local service hostnames (for example, Docker Compose service names). The gateway checks the signed-in user's team membership and requires a write-capable role to create invites.

Image Provider exposes `GET/POST /api/service/images` and `PATCH /api/service/images/{id}/sharing`; the browser and desktop app must call only the authenticated Sync Server routes. Invite Tracker exposes service-key-protected `GET/POST /api/service/campaigns`; campaign records are scoped to the Apex team and creation is idempotent. Apex Studio accesses invites through `GET/POST /api/v1/teams/{team_id}/invite-campaigns`. Authenticated image routes are `GET/POST /api/v1/teams/{team_id}/image-library` and `PATCH /api/v1/teams/{team_id}/image-library/{image_id}/sharing`; service availability is returned by `GET /api/v1/teams/{team_id}/services`. Upstream connection failures return distinct `image_hosting_unavailable` and `invite_tracking_unavailable` codes so clients can identify which optional service failed.

The service availability response checks the authenticated image-list and invite-list routes using the configured service keys. It sends reserved synthetic user/team IDs and discards the response body, so service status does not retrieve a real member's library or invite campaigns. Public health endpoints alone are not used to report these integrations as ready.

## Requirements

For a temporary Windows PC deployment without incoming ports, use the
[Cloudflare Tunnel setup](TUNNEL.md) instead of the Caddy deployment below.

- A Linux server with Docker Engine and Docker Compose.
- A domain or subdomain whose DNS `A`/`AAAA` record points to this server.
- Inbound TCP ports 80 and 443 available so Caddy can obtain and renew the HTTPS certificate. UDP 443 is optional and enables HTTP/3.

## Configure and start

Copy `.env.example` to `.env`, set `APEX_SERVER_DOMAIN`, and generate a random 64-character hexadecimal value for both `APEX_SYNC_DB_PASSWORD` and the temporary `APEX_SYNC_BOOTSTRAP_TOKEN`. On a Linux host with OpenSSL, generate each value with a separate `openssl rand -hex 32` invocation. Set both values in the private `.env` file before starting the stack; otherwise the first-owner setup cannot be completed.

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

Create the first team Owner exactly once. Install the Apex Studio CLI on a Windows administrator workstation, then run the setup command against the public HTTPS URL:

```powershell
apex-studio-cli sync setup https://YOUR_DOMAIN `
  --team-name "Your team" `
  --email "owner@example.org" `
  --display-name "Team owner"
```

The command securely prompts for the one-time bootstrap token and Owner password, then saves the resulting desktop session in Windows Credential Manager. After setup succeeds, remove `APEX_SYNC_BOOTSTRAP_TOKEN` from `.env` and restart the API (`docker compose up -d api`). Keep the database password unchanged for an existing database volume. If the CLI is unavailable, use the documented `POST /api/v1/setup` request in the [API contract](../../docs/planning/TEAM-SYNC-API.md).

The server binds only inside the Compose network; Caddy is the only service publishing internet-facing ports. PostgreSQL and its persistent volume are not exposed to desktop clients. Keep `.env` private and outside backups that are shared with designers. This Compose project starts only PostgreSQL, the Sync Server API, and Caddy; it does not start Image Provider or Invite Tracker.

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
