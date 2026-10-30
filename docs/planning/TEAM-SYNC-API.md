# Apex Sync Server API contract

Status: implemented server-side contract, 2026-10-30. API base path: `/api/v1`. All identifiers are UUIDs. JSON field names use camelCase unless noted. Error responses have the form `{"error":"stable_code"}`.

## Public endpoints

| Method and path | Purpose |
| --- | --- |
| `GET /health/live` | Process liveness; no database check. |
| `GET /health/ready` | Database readiness. |
| `GET /api/v1/server-info` | Product, API/server versions, stable server-instance ID. |
| `POST /api/v1/setup` | Create the first team Owner, team and session. Requires the configured one-time bootstrap token. Available only until setup succeeds. |
| `POST /api/v1/sessions` | Sign in with email and password. |
| `POST /api/v1/invitations/accept` | Consume a one-time invitation code, create the account and return a session. |

The setup request contains `bootstrapToken`, `teamName`, `email`, `displayName`, and `password`. Invitation acceptance contains `invitationToken`, `email`, `displayName`, and `password`. Passwords must be 12–128 characters. Emails are trimmed and lowercased. The server stores Argon2 password hashes and hashes bearer-session and invitation tokens before persistence. Session lifetime is 30 days. An invitation code is a 256-bit random bearer secret, valid for 72 hours and usable once. The raw code is returned only once when created; it must be shared privately. It grants access to the selected team and role but does not verify ownership of the email address entered during registration. No email is sent.

## Authenticated team endpoints

Send `Authorization: Bearer <accessToken>` on every route below.

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/session` | Current user and their teams/roles. |
| `DELETE /api/v1/sessions/current` | Revoke the current session. |
| `GET /api/v1/teams/{teamId}/members` | List active members. |
| `POST /api/v1/teams/{teamId}/invitations` | Create a one-time invitation code with role `admin`, `designer`, or `read_only`. |
| `PATCH /api/v1/teams/{teamId}/members/{userId}` | Change a member role. |
| `DELETE /api/v1/teams/{teamId}/members/{userId}` | Remove a member from this team. |

Roles are `owner`, `admin`, `designer`, and `read_only`. Designers can change project data; read-only members cannot. Owners and admins can manage members; only Owners can grant or revoke Admin. Ownership transfer, invite revocation/listing, email delivery, password reset, and account recovery are not implemented. Invitation codes are not bound to an email address; share them only with the intended person.

## Project records and conflict handling

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/teams/{teamId}/projects` | List active projects. |
| `POST /api/v1/teams/{teamId}/projects` | Create a project using `{ "id": "uuid", "data": { "name": "..." } }`. |
| `PUT /api/v1/teams/{teamId}/projects/{projectId}/records/{recordType}/{recordId}` | Create/update a record or write a tombstone. |
| `GET /api/v1/teams/{teamId}/projects/{projectId}/changes?cursor=0&limit=200` | Read ordered changes after a cursor. `limit` is 1–500. |

Supported record types: `project`, `note`, `folder`, `livery`, `asset`, `placement`, `post`, and `post_media`. Records are JSON objects capped at 2 MiB. A write contains `baseRevision`, `data`, and optional `deleted`. For a new record use `baseRevision: null`; for an update use its current server revision. For a deletion send `deleted: true` and `data: null`. The first revision is 1 and subsequent successful writes increment it. A stale or duplicate create returns HTTP 409 with `revision_conflict`; clients must retain local changes and offer a conflict-resolution workflow. Deletions are retained as tombstones in the change feed.

## Project blobs

Blob addresses are lowercase SHA-256 hex digests. Uploads are project-scoped, and clients should verify content against the digest before use.

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/teams/{teamId}/projects/{projectId}/blobs` | List known hashes and byte sizes. |
| `PUT /api/v1/teams/{teamId}/projects/{projectId}/blobs/{sha256}` | Upload raw bytes up to 64 MiB. |
| `GET /api/v1/teams/{teamId}/projects/{projectId}/blobs/{sha256}` | Stream a stored blob. |
| `POST /api/v1/teams/{teamId}/projects/{projectId}/blob-uploads` | Start/reuse an upload session with `contentHash` and `totalBytes`. |
| `GET /api/v1/teams/{teamId}/projects/{projectId}/blob-uploads/{uploadId}` | Read chunk receipt state. |
| `PUT /api/v1/teams/{teamId}/projects/{projectId}/blob-uploads/{uploadId}/chunks/{index}` | Send one 8 MiB-or-smaller chunk. |
| `POST /api/v1/teams/{teamId}/projects/{projectId}/blob-uploads/{uploadId}/complete` | Verify and commit all chunks. |
| `DELETE /api/v1/teams/{teamId}/projects/{projectId}/blob-uploads/{uploadId}` | Cancel an incomplete upload. |

Chunked uploads support blobs up to 512 MiB and expire after 24 hours. The completion endpoint validates received indexes, byte counts, and SHA-256 before associating the blob with the project. Clients may retry chunks using the same index. Completed upload sessions remain queryable until their expiry. Server startup removes expired session metadata and staging directories.

## Deployment and current limitations

Use HTTPS at the public origin. Cloudflare Tunnel only transports requests; it does not replace server authentication. Keep the bootstrap token at least 32 characters, use it once to create the initial Owner, then remove it from the deployment environment and restart the API. Keep database and blob volumes together and back them up consistently.

The desktop Settings surface checks a server, signs in, creates and accepts invitation codes, performs one-time first-team setup, and displays account/team membership. Session tokens are stored in Windows Credential Manager and bound to both the normalized server origin and its stable instance ID. The active team selection is a local device preference. Project sync execution, local/server project mapping, and conflict-resolution UI are not implemented yet.

Production safeguards still needed include tested PostgreSQL/blob backup and restore, operational rate limiting, audit/retention policy, account recovery, and integration testing against a freshly deployed server. The API contract may evolve before a stable public release; check `server-info` and API version before synchronizing.
