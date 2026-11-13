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
- The current Sync Server test suite does **not** exercise those route authorization checks against a database-backed membership or mock upstream. Add that coverage before treating gateway authorization as fully verified. The live test only confirmed the current team’s campaign list is empty; it did not verify rendering a populated list. No public image upload has been tested live.
- The goal is still in progress. Resume with authorization integration coverage, then complete the remaining offline/manual-fallback and media-version/reuse checks listed above. Preserve the no-write live-test boundary unless a specific campaign or public upload test is explicitly approved.
