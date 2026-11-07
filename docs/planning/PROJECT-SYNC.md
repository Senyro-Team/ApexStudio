# Desktop project synchronization

Implemented 2026-11-03; team access and livery-code/blob handling extended 2026-11-07.

## Controls

- New/Edit Project: **Synchronize with team server**. New projects default to local-only. The destination is the team selected in Settings → Team sync.
- Project Details → Synchronization: enable/disable sync, see the last result and sync time, retry immediately, or resolve a conflict.
- Settings → Team sync → **Automatically synchronize projects** enables/disables background transfers on this device. The choice persists immediately; devices configured before the switch existed retain automatic sync by default. **Sync projects now** remains available when automatic sync is off.
- With automatic sync enabled, startup, saved changes, the end of editing, focus, and restored connectivity trigger a 250 ms debounced exchange. Remote checks run two seconds after the previous exchange completes while not editing. Slow transfers never accumulate overlapping automatic runs; failures back off up to 32 seconds. Actual transfer latency also depends on network and file size.
- Shared project records and ordinary project files download automatically to a newly connected device. Livery textures and livery covers are stored on the team server and are not added to a newly syncing member's managed-file storage. A preview request streams the selected image through memory, verifies its SHA-256, resizes it, and sends a no-store thumbnail response. This does not prevent screenshots or manual copying of a preview.
- Existing Roblox decal IDs are shared per team and image SHA-256. Sync imports a team's existing ID into the livery's shared-code status; successful local uploads are offered to the team catalog on the next project sync. The first ID stored for a hash is authoritative. A different ID is reported as a conflict rather than silently replacing it. Explicit re-upload remains available.
- Owners can remove any non-owner member. Admins can remove designers and read-only members, but not owners or admins. Removal and active-session revocation happen in one database transaction. The server denies later team access; files or copies already present on a device cannot be recalled. Existing workspace files from earlier full-download syncs are not deleted automatically because Apex cannot distinguish those copies from user-imported originals safely.
- A removed member's project metadata already present locally remains in their workspace, but server access and future streamed previews stop. Server access control cannot revoke Roblox decal IDs already exposed or files previously copied outside Apex Studio.
- If a later sync reports an expired session or removed membership, Apex clears the local team decal-code cache for that team. Until the device next contacts the server, cached Roblox codes may remain visible offline.
- Disabling sync retains local and server data. Deleting a local archived project disables its binding; it does not delete the team copy or cause an automatic re-download.
- A project with a stale team/server binding is reported as skipped with its name and reason; it is not counted as up to date. A locally owned project can be explicitly reconnected to the currently selected team. A project received from another team can be reconnected only when its local contents still match the last saved sync baseline. Apex asks for confirmation, keeps the prior server copy and resets the destination-specific cursor. Modified or unverified local copies remain untouched and require manual recovery.

## Data and persistence

Schema 15 adds local-only `project_sync` bindings, an availability marker for server-only file metadata, and project-scoped team decal-code cache. Existing projects stay local-only until opted in. These bindings and credentials are excluded from transferred project data.

The v1 server already accepts arbitrary versioned project record data. The desktop root record contains the project name and an `apexSnapshot` version 1 object. It includes project metadata/archive state, assets (including trashed assets), folder/livery structure, placements, texture versions, project notes and note image references, and project-linked publishing drafts/posts and media. Standalone posts, general notes, Roblox/browser credentials, upload queues, personal statistics, and device settings do not synchronize.

When sharing a locally owned project for the first time, the server may briefly contain a revision-1 root record with only its name and the `apexPendingSnapshot` marker. The next sync replaces that exact placeholder with the full snapshot. A member without the matching locally owned project cannot fill it; unrelated or incompatible server records remain untouched.

Project-scoped content hashes are checked before upload; downloaded bytes must match their declared SHA-256 and size before registration. Missing blobs upload before the snapshot becomes visible. Files over 64 MiB use the existing 8 MiB chunk protocol; the client accepts HTTP 204 chunk receipts. Transfers use a separate longer timeout from metadata requests. Livery-only blobs are distinguished in snapshots and skipped by the download path; previews use a team-authorized authenticated stream and are never written to the managed files directory.

Snapshots are deterministic, read from one SQLite read transaction, and capped by the existing 2 MiB server record limit. Local application uses one immediate transaction, rechecks the previously observed local fingerprint, defers foreign keys, inserts only fixed allowlisted tables/columns, and verifies the resulting project scope before commit. A disabled or differently bound project cannot be replaced. Failed validation rolls back visible records. Files are immutable and may remain unreferenced after interrupted transfers.

## Conflicts and recovery

Unchanged local projects receive server updates. Local-only changes upload with `baseRevision`; concurrent edits or HTTP 409 preserve local data and expose a conflict. An interrupted successful upload is recognized by matching fingerprints on retry.

**Use server version** creates a complete workspace `.apexbackup` under the workspace `backups` folder, then replaces the project only if the local fingerprint still matches. **Use this device's version** performs an explicit revision-checked update; previous server snapshots remain in the server change history. Neither resolution silently overwrites a newer server revision. A server tombstone/unavailable project retains the local copy and reports the situation.

Current limits: 512 MiB per file, 2 GiB total referenced blobs, 10,000 blobs and 30,000 snapshot rows. An oversized project reports an error and remains local. Conflict resolution is at project-snapshot granularity, not field merging. Initial download can replay the root record's change history; more efficient snapshot endpoints can be added later. Project deletion on the server is not initiated by the desktop in this slice.

## Verification

- Storage tests transfer project assets, livery folders, texture versions, and note images between two temporary workspaces, preserve stable IDs, and reject concurrent edits, cross-project rows, and disabled bindings.
- A local HTTP fixture implements the existing changes/records/blob contract. Two-device tests cover upload, download, subsequent edits, simultaneous-edit conflicts, and rejected writes without using real accounts or project data. A 65 MiB file checks chunk transfer and 204 handling.
- UI tests cover new-project opt-in, persisted disabling, and deliberate conflict resolution. Existing project/asset/UI suites remain applicable.
- Run Windows native library tests with `powershell -NoProfile -File scripts/test-desktop.ps1`. The runner activates common-controls v6 on the test artifact only; it leaves the application build manifest unchanged.

The deployed team server and a second physical device still need a deployment-specific smoke test. No server-side code/API change is required for snapshot-format version 1.
