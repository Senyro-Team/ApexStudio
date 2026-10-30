# Apex Studio team sync — architecture proposal

Status: server API implementation in progress, 2026-10-30. The Rust service now includes account setup and sign-in, invitations, role management, revisioned project records, tombstones, and content-addressed file transfer. The desktop sign-in and sync client, production operations, and backup/restore workflow remain future work.

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

1. **Contract and server foundation:** implemented; see [server API contract](../TEAM-SYNC-API.md).
2. **Team access:** initial implementation includes one-time bootstrap, Argon2id password hashes, bearer sessions, 72-hour invitations, membership and role checks. Account recovery and email delivery are not implemented.
3. **Project sync:** initial API includes revisioned records, change cursors, tombstones, blob manifests, direct small-file upload, and resumable chunked uploads. Client conflict resolution and end-to-end audit/retention operations remain outstanding.
4. **Desktop connection:** Settings supports server checks, sign-in, invitation code creation and acceptance, first-team setup, session storage and team selection. The actual project sync worker, local/server project mapping and explicit conflict-resolution UI remain outstanding.
5. **Operations:** tested consistent backups/restores, upgrades, retention, logs without secrets, and a documented manual update path.

## Identity decision

Version 1 uses server-managed accounts and one-time invitations. An OIDC provider such as Authentik/Keycloak can be a later optional adapter.

## Foundation implementation

The Rust service exposes liveness, database readiness, public server information, one-time initial setup, login, and invitation acceptance. Authenticated endpoints provide session identity, logout, team membership, invitation creation, role changes, member removal, project records, change feeds, and content-addressed blob transfer. The desktop Settings surface checks the server, signs in, accepts invitations or initializes the first team, and displays the account and team role. Session tokens are stored in Windows Credential Manager and bound to the server origin and instance ID. The project sync worker and conflict-resolution UI are not implemented yet. Do not rely on this for production team data until rate limiting, tested database/blob backups, restore validation, and upgrade operations are complete.
