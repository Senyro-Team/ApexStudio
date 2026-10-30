# Apex Studio team sync — architecture proposal

Status: planning, 2026-10-30. No server or sync code is implemented by this document.

## Product shape

- Each design team runs its own Apex Sync Server on a Linux host using Docker.
- Apex Studio remains local-first. A designer enters the team's server URL and joins with an invitation; the app does not depend on an Apex-hosted service.
- The URL is an HTTPS origin such as `https://studio-sync.example.org`. A domain is a convenient, stable address. It is not a database connection and does not provide authentication by itself.
- For development, LAN use, or VPN-only deployments, an internal DNS name or reachable host address can be used. For internet access, publish DNS to a reverse proxy and expose HTTPS only. Do not expose PostgreSQL or a filesystem share to desktop clients.

## Storage and sync model

- Keep the existing per-device SQLite workspace and managed-file store. Never have desktop clients open one shared SQLite file over SMB, NFS, or a network drive.
- The server owns team membership, shared project records, revisions, sync cursors, deletion tombstones, and shared asset metadata.
- Use PostgreSQL for server metadata and a server-side content store for immutable file blobs, addressed and verified by SHA-256. Docker volumes must persist both. An S3-compatible store can be considered later without changing client-facing sync semantics.
- The desktop sync adapter sends versioned, authenticated HTTPS requests. It first exchanges project manifests and revision cursors, then uploads/downloads only missing blobs. Large files need resumable, bounded transfers.
- Every changed record has a server revision. A stale update returns a conflict; it must not silently replace another designer's edit. Initial conflict handling should preserve both versions and let the user resolve the conflict in the app.
- Deletion is synchronized as a tombstone so offline devices do not resurrect removed content. Permanent deletion waits until retention and all active sync cursors allow collection.
- Local-only credentials (Roblox API keys and browser sessions) are never uploaded. Personal statistics and sales data are private by default; sharing those needs a separate explicit setting.

## Server deployment

- Docker Compose contains the API service and PostgreSQL; persistent volumes hold database data and file blobs.
- A reverse proxy terminates TLS on port 443. The database listens only on the private Docker network.
- The first connection flow needs an administrator-created team and a one-time invitation. Team roles should start with Owner, Admin, Designer, and Read-only.
- Health/readiness endpoints, API version reporting, migration status, and clear backup/restore instructions are needed before teams rely on a deployment.
- Server backups cover PostgreSQL and the content store at one consistent point. Restore must validate the database schema and referenced blob hashes before accepting connections.

## Delivery slices

1. **Contract and server foundation:** versioned API contract, Rust HTTP service, PostgreSQL migrations, Docker Compose, configuration validation, health/readiness and version endpoints.
2. **Team access:** administrator bootstrap, one-time invitations, sessions, membership and role checks. Password storage and recovery policy must be decided before implementation.
3. **Project sync:** project and note revisions, manifest exchange, tombstones, conflict responses, resumable content-addressed blob transfer, audit trail.
4. **Desktop connection:** server URL and connection check in Settings, sign-in/join flow, sync status and explicit conflict resolution; local-only workspaces remain supported.
5. **Operations:** tested consistent backups/restores, upgrades, retention, logs without secrets, and a documented manual update path.

## Identity decision

Version 1 uses server-managed accounts and one-time invitations. An OIDC provider such as Authentik/Keycloak can be a later optional adapter.

## Foundation implementation

The initial Rust service exposes liveness, database readiness, and API/server version endpoints. The server-info response includes a stable per-installation ID so the desktop can later identify the configured server. Docker Compose provides PostgreSQL and Caddy HTTPS termination. This is infrastructure groundwork only: authentication, team management, and synchronization endpoints are not implemented, so do not connect a team workspace or expose it as a production sync service yet.
