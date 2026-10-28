# Global ERLCX, trash and Frontplate implementation

Baseline: a616cb1, schema 7. Work is split into Frontplate, trash (schema 8), then independent publishing drafts (schema 9) and Chrome native messaging. Check current Git state and schema numbering before each block; preserve concurrent changes. No automatic push.

## Interfaces and ownership

- Core side parser/storage slot validation/UI SlotEditor: canonical `Frontplate N`, default N=1 only for explicit front-plate aliases. Generic plate names stay unassigned. Existing side rules remain.
- Trash: backend-owned recursive deletion groups; typed preview/list/restore/purge IPC. Preserve immutable blob references, versions and upload history. Terminal upload hashes become metadata references independent of available managed bytes; active jobs keep content pinned. No automatic cleanup of user originals.
- Global publishing: typed list/create/read/save/export commands with independent draft ID and revision. Migrate legacy project drafts. Public bundle v1 stays unchanged. Global post drafts travel in workspace backups, not new project packages; legacy package drafts import globally.
- Browser connection: separate Windows native-messaging helper, user-local Chrome host registration, approved extension origin, bounded package chunks and named-pipe IPC. Native settings are machine-local. Explicit public snapshot handoff, one active run, manual final consent/publish.

## Block 1 — Frontplate

Added to filename recognition, storage slot validation and the manual side picker. Number defaults to 1 only for recognized Frontplate names; first following number wins. Downstream Roblox, Markdown and ERLCX use side strings and retain the canonical value without a format change.

## Block 2: project trash (schema 8)

Soft deletion keeps folders, placements, versions and covers. Active name/slot indexes exclude trash. Pending upload references are disabled, shared work stays active, and submitted operations remain traceable. Restore uses a unique name and falls back to the project root when the parent is unavailable. Permanent deletion collects only unreferenced content; note/post media, leases and active operations protect blobs. A workspace content gate serializes file promotion, backup and collection. Terminal upload history no longer owns file bytes.

Verification: 33 storage unit tests pass, including recursive restore, name conflicts, missing parents, shared blobs, interrupted queues and backup/restore. Existing 45 frontend tests and production frontend build pass. Rust doc-test invocation is restricted by Windows Application Control (rustdoc error 4551); unit/all-target checks do not require bypassing this policy.

## Block 3: global posts and Windows Chrome bridge (schema 9)

`posts` and `post_media` hold independent drafts with optimistic revisions and timestamps. Migration copies existing project drafts without changing JSON, saved codes or image ordering. Legacy API shadows are retained for compatibility and synchronized on save; newly exported project packages exclude posts. Older package drafts import into the global list. Workspace backups include both draft tables and media. Saved snapshots flag changed or missing sources; using them requires explicit review, while actual missing IDs still prevent export and preparation.

The global ERLCX editor has separate Liveries, Details, Previews and Review steps. Livery selection supports project filters, search, selection across projects and a twenty-livery limit. Preview asset selection uses an independent project. Temporary imported media use renewable backend content leases; save, discard and editor close release unused media safely. The two asset-manager Roblox shortcuts and the project ERLCX tab are removed.

### Bridge contract

- Standalone `apex-chrome-bridge.exe` links no workspace adapter and never starts the app, queue or migrations. Keep it beside `apex-desktop.exe`; `default-run` still selects the desktop binary for development.
- Per-user settings and host registration are outside the repository/workspace. Registration permits one exact extension origin. Profile/path/ID changes invalidate connection verification. Chrome connection verification uses a random sixty-second challenge.
- A saved revision is validated and exported to an immutable temporary public bundle. A random one-use handoff token expires after two minutes; a claimed session lasts up to two hours. Only one preparation or transfer is active. Creating the bundle does not hold the bridge state lock.
- Chrome stdin/stdout and named-pipe messages are length-prefixed JSON. Requests are limited to 16 KiB, responses to 1 MiB; bundle chunks are sequential 192 KiB raw bytes. Supported requests are check, claim, chunk and status. There is no arbitrary file/path or workspace command.
- The named-pipe server rejects remote clients and acquires the first instance before serving. The helper validates Chrome's caller origin and forwards only to the current app endpoint. Restart and workspace restore invalidate sessions. App-side Resume/Cancel reconcile through status requests.

### Extension behavior

Version 0.2.0 preserves the existing unpacked extension ID by keeping its installation folder; no manifest key is introduced. Permissions add nativeMessaging and access restricted to ERLCX. A dedicated handoff page reuses strict bundle validation, while a native-connected service worker owns automation. Existing popup ZIP import and manual filling remain available.

The automatic workflow verifies category selection, title/description/tags, completed preview images, vehicle/side/decal/color/opacity fields and empty free monetization before clicking only the exact Continue button. Unknown layouts, login, CAPTCHA, conflicting contents or validation errors pause. Images already attached are never blindly attached again. Submit returns before any click; agreement, scheduling, Save Draft and Publish are excluded. Native cancellation and worker restart are recoverable; extension-side state is kept in IndexedDB. The popup opens the assistant and prevents conflicting manual actions during an active run.

### Verification log

- Storage: 37 unit tests, including global revisions/backup, schema migration, old-package post import, source deletion with retained post media and explicit snapshot review.
- Frontend/extension: 56 tests, including cross-project selection with missing IDs, saved-snapshot review, global navigation, every automatic wizard boundary, media waiting and final-page non-submission.
- Native: 17 library tests plus standalone framing tests; token replay, chunk ordering/size, origin, expiry and connection challenges are covered. A Windows integration test launches the real helper against an isolated named pipe, verifies caller origin forwarding and a large framed response, and confirms that no workspace database is opened. Workspace Clippy with warnings denied passes.
- Production frontend and extension ZIP/unpacked builds pass. The existing large Notes chunk warning remains unrelated to this change.
- Live ERLCX Categories, Details and Media layout were read in logged-in Chrome using clearly marked demo text. No draft or post was submitted. The browser file API requires an unavailable file-access permission, so no banner upload was performed through that API. Internal Chrome pages are blocked by browser automation policy; installed extension ID/profile and a manual reload were requested from the user. Final native/Chrome end-to-end review remains pending those inputs and restoring the minimized app window.
- Optimized native library and standalone helper build succeeded. The final full release build reached linking but Windows denied replacing the running `target/release/apex-desktop.exe` (OS error 5). The existing minimized app was left running to preserve unknown unsaved edits. Once it is saved and closed, rebuild the desktop executable and launch through Explorer in the normal desktop context, verifying Tiki Workspace. This final app review is not yet complete.
