# Optional Server and Post Workflow — Handoff

Date: 2026-11-11

## Where work stopped

- The optional Server module, user-owned Image Library, ERLCX Markdown composer, reusable Markdown blocks, and neutral overlay workflow are implemented in the current source. The detailed product contract and earlier verification records are in [`OPTIONAL-SERVER-PUBLISHING-WORKFLOW.md`](../planning/OPTIONAL-SERVER-PUBLISHING-WORKFLOW.md).
- A fresh optimized Windows build completed at `target/release/apex-desktop.exe`. It opened the existing **Tiki Workspace** and reused its saved session for `https://apexstd.senyro.de`.
- The app authenticated to team **ApexStudio** as Owner. Project Sync, Image Hosting, and Invite Tracking all reported **Online**.
- The marked `APEX STUDIO TEST - DO NOT PUBLISH` draft was opened at its Markdown step. The authenticated campaign-list request completed and the app showed **No tracked campaigns yet**. No campaign was created; no media upload, project sync, post save, or publish action was performed.
- The frontend production build and native optimized build succeeded. Vite emitted the existing large Notes chunk warning.
- The handoff is on branch `codex/optional-server-progress-2026-11-11`, based on the current local implementation branch. The untracked `target-native-diagnostic/` directory was intentionally left untouched and excluded.

## Next work when resuming

1. Audit the full goal against the current code and tests: confirm local-only project/notes/assets/overlay/Markdown work, service-by-service optionality, gateway team/role authorization, and separation of service credentials and data.
2. Keep the live invite-list result scoped accurately: the team currently has no listed campaigns. To verify rendering of an existing campaign, use one already present or get explicit approval before creating a test invite; do not create one automatically.
3. Finish end-to-end verification of reusable media and explicit public uploads: unchanged bytes reuse their provider mapping, link-destination edits do not re-upload, pixel/overlay changes create a new version, and team sharing remains opt-in. No live image upload has been performed in this review.
4. Check disconnected and unavailable service states in the running optimized build and confirm manual image URL and invite URL paths still work. Do not synchronize project data or publish a post as part of these checks.
5. Record any remaining implementation gaps here and in the planning document, then commit each scoped code change. This handoff branch is temporary; do not merge or push to `main` unless requested.

## Repository snapshot

- Source branch before handoff: `codex/apex-cli-phase-1`.
- Last source commit: `53eb314 docs: verify live invite campaign listing`.
- Local branch contained 125 commits not present in `origin/main` and included `origin/main` at handoff time. The temporary branch therefore preserves the complete current local history, not just the handoff note.
- The only untracked item at handoff time was `target-native-diagnostic/`; it was not staged or committed.

## Review update (2026-11-13)

- Re-ran the full desktop suite on this branch: 117 tests passed across 23 files. Focused Publishing, Media Library, Team Sync Settings, and Overlay Studio suites passed 31 tests across 4 files.
- Sync Server passed all 5 unit tests and `cargo fmt --all --check`. `npm run build` passed; Vite still reports the Notes bundle size warning.
- Source review confirms service routes call `membership_role` for team reads; image upload, image sharing, and invite creation additionally require a write-capable role. Service keys are held in the Sync Server configuration and forwarded server-to-service. The Image Provider and Invite Tracker compare their gateway key in constant time and scope queries by the asserted user/team identity.
- Database-backed handler tests now cover nonmember denial, read-only list access reaching the configured-service boundary, read-only write denial, and designer write access. GitHub Actions now provides PostgreSQL and runs the standalone Sync Server suite. The local run passed six tests but skipped the database-backed test because `APEX_SYNC_TEST_DATABASE_URL` is unset here; verify the next GitHub Actions run actually executes it. Full router/session middleware and mock-upstream behavior remain outside this test.
- GitHub Actions run `43463625056` on commit `d3306fe` completed: frontend and desktop jobs passed; the core job ran the PostgreSQL-backed Sync Server suite and `service_gateway_enforces_team_membership_and_write_roles` passed (6/6 server tests). Core then failed at strict workspace Clippy because existing `apex-storage/src/media_library.rs` functions `insert_media_item` and `update_media_library_item` exceed the argument-count lint; the standalone Sync Server check was skipped after that failure. These Clippy findings are outside the gateway-test change and remain unresolved.
- The live test only confirmed the current team’s campaign list is empty; it did not verify rendering a populated list. No public image upload has been tested live. The goal is still in progress; finish the remaining offline/manual-fallback and media-version/reuse checks listed above. Preserve the no-write live-test boundary unless a specific campaign or public upload test is explicitly approved.

## Verification update (2026-11-13)

- Commit `0b63d93` resolves strict Clippy findings in the storage crate and CLI without changing user-visible behavior. Media-library metadata now travels as a typed value, invitation-block validation uses a guarded match arm, CLI sorts use `sort_by_key`, and the large Team Sync worker result is boxed.
- Local checks passed: full Rust workspace tests, Sync Server suite (6 tests), workspace formatting, strict workspace Clippy, desktop Tauri `cargo check`, all 117 frontend tests, and the frontend production build. Vite continues to report the large Notes chunk warning.
- GitHub Actions run `43466121744` for `0b63d93` passed all three jobs: core (including PostgreSQL-backed gateway authorization tests and Sync Server check), desktop, and frontend (including extension build).
- Added local HTTP mock-upstream tests for the Image Provider and Invite Tracker gateways. They check forwarded user/team scope and server-held service keys, and verify upstream-private headers and service credentials do not appear in responses to Apex. CI supplies `APEX_SYNC_TEST_DATABASE_URL`, so database-backed route tests run there. Local Sync Server formatting, tests, and strict Clippy pass.
- Added write-path mock-upstream coverage for image upload, image sharing, and invite campaign creation. These tests check the user/team scope, selected image scope, campaign idempotency key, request bodies, service-specific routing, and response-header sanitization. GitHub Actions run `43471450222` for commit `a960bb4` passed all three jobs; the PostgreSQL-backed Sync Server suite passed all 8 tests, including both database-backed authorization and read/write gateway proxy tests.
- Remaining release evidence is live/runtime focused: render a populated campaign list if an existing campaign is available; exercise public upload/reuse and explicit team sharing with a chosen test image; and verify disconnected/unavailable service fallback in the optimized app. No live invite creation or image upload has occurred. The objective remains in progress until those workflows are verified and remaining behavior gaps, if any, are fixed.

## Completion audit (2026-11-13)

- The full implementation scope in `OPTIONAL-SERVER-PUBLISHING-WORKFLOW.md` is present: local-first project, notes, assets, overlay and Markdown workflows; the main-navigation Server module; independently optional Project Sync, Image Hosting and Invite Tracking; the reusable Image Library; the ERLCX composer; manual URL fallbacks; explicit public-upload and sharing confirmations; and workspace-local templates, media versions and post snapshots.
- Current verification passed: 27 focused desktop tests (Publishing, Media Library and Team Sync Settings), 54 `apex-storage` tests, Invite Tracker TypeScript typecheck, and Image Provider `node --check`. The last GitHub Actions run for `a960bb4` passed all three jobs and ran eight Sync Server tests, including PostgreSQL-backed authorization and read/write gateway proxy tests. The prior optimized Windows review verified the signed-in team and all three capabilities online, loaded shared images and an empty campaign list, and did not create campaigns or upload media.
- Current code review confirms the Image Provider deduplicates within personal or team scope by SHA-256 content hash; the composer maps changed Markdown link destinations without re-uploading; overlay render versions are keyed to output bytes and preserve post references; and project/user/team scopes are enforced at the gateway. The public-upload UI test confirms declining its confirmation sends no upload request.
- No live public image upload was performed. It is an optional smoke test that would expose the selected test image at a public URL, so it remains separate from implementation completion and requires a specifically approved image and destination. No invite campaign was created.
- No further code gap was identified in the goal audit. The temporary branch contains the completed implementation and verification record; `target-native-diagnostic/` remains untracked and untouched.
