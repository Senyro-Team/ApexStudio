# CLI Phase 4 — Roblox uploads and ERLCX publishing

Status: In progress.

## Implemented so far

- Shared application wrappers expose global ERLCX post drafts, source status, preview PNG import, `.apexpublish` export, Roblox livery summaries, and durable queue snapshots.
- `posts list|create|show|save|sources|add-images|import|export|handoff` provides JSON-editable global draft workflows. Saving uses the draft revision for conflict detection; adding media imports and pins PNGs through the storage service.
- `posts import <PATH>` validates and imports a public version-1 `.apexpublish` ZIP into a new global post draft. It checks the manifest schema, archive layout and size, PNG limits, file hashes, and normal publishing validation; source-project IDs and other private data are not part of the bundle.
- `codes export <PATH> --post <ID>...` combines up to 50 saved post snapshots into one escaped Markdown file, requires `--reviewed` for stale/missing source liveries, and refuses to overwrite files.
- `roblox liveries --account-id <ID>` and `roblox queue` expose read-only account-specific livery coverage and persisted queue state. These commands do not start network transfers or perform startup recovery.
- `roblox batch start` creates an explicitly confirmed durable batch; pause/resume/cancel/retry controls operate on the shared queue. The existing desktop worker handles network requests while the desktop app is running, using credentials configured in Settings.
- `roblox double-upload --enable|--disable` changes the persisted preference used by future batches.
- `roblox credentials status|set|clear` uses the same Windows Credential Manager entry as the desktop app. `set` reads the API key in a hidden terminal prompt, validates account ownership, expiration and asset scopes, and stores only after successful validation; queued batches are paused before replacing or clearing credentials.
- Shared Roblox transport and response validation now live in `apex-application::roblox_upload`: the desktop adapter and future standalone worker use the same Decal multipart request, bounded response parsing, operation-reference validation, and owner/asset-type checks.
- The TUI includes searchable ERLCX draft and Roblox queue views, with post snapshot details, batch status, texture pass state, and both decal IDs. On the draft list, `i` imports a `.apexpublish` bundle by path, shows validation errors in place, and opens the imported draft. `n` opens an ERLCX draft form and `e` edits title, multiline description, tags and YouTube URL while preserving preview media and livery/code snapshots. `m` opens preview-image management: import a PNG by path, reorder the cover-first list, refresh a conflicted revision, or remove an image after confirmation. Each change is saved through the revision-checked snapshot service; the original import file is preserved. `l` attaches one or more source liveries from across projects to the open post; each livery keeps a source link and only usable final decal IDs are copied, leaving missing codes blank for later upload. `v` opens the livery manager for vehicle/color/opacity edits, side-code creation and editing, row and livery reordering, and confirmed removal. Reordering source-linked code rows also reorders their source hashes to preserve the association. `Ctrl+S` runs the same validation and revision-checked save; invalid values and revision conflicts stay visible in editors. From the queue, `u` opens a cross-project livery picker with search, Space selection, select-all-filtered, clear, and re-upload mode. Enter shows texture/reusable-ID/pass counts; `y` creates the durable batch only after review. The desktop app must be running to process that batch. Queue actions use Tab to focus batch or job lists; `p` toggles the selected batch between pause/resume, `t` retries failed jobs in it, and `c` opens a separate cancel confirmation.
- Disposable integration coverage checks multi-post Markdown output, escaping, no-overwrite, `.apexpublish` export/import, livery listing, queue creation/pause, and the double-upload preference without any live Roblox requests. Local HTTP mocks exercise Roblox credential introspection, and a Clap test ensures API keys are rejected as command-line arguments; no real credentials are read by tests. TUI rendering coverage includes all post, media, livery, code, queue, picker and review screens at 40, 80, and 120 columns; dummy-workspace tests cover validated post creation, multiline text, tag and URL persistence, invalid-input recovery, bundle import and error recovery, source-linked livery selection, image import/reorder/confirmed removal with unchanged originals, livery property and decal-code editing, confirmed code removal, livery/code snapshot preservation, filtered livery multi-selection, and the explicit cancel-confirmation step.

## Current limitations

- An upload worker that can run without the desktop app has not been implemented in the CLI yet. Queue scheduling, rate-budget handling, retries and pass transitions still run in the Tauri adapter. A CLI-created batch is durable, but it waits for the existing desktop worker.
- TUI editing covers post metadata, source-linked and manual livery creation, livery properties/codes, and preview-image import/order/removal. Manual livery drafts can be saved before codes are ready and completed from the side-code editor; Roblox batch creation and controls are available in the TUI queue.
- `.apexpublish` import/export and a Windows direct Chrome handoff request are available through typed commands. The handoff requires the running desktop app and a recently verified extension connection.
- The TUI draft detail uses `h` for a standard handoff and uppercase `H` when the user explicitly approves saved decal-code snapshots for changed or unavailable source liveries. The CLI equivalent is `posts handoff <ID> [--reviewed]`; `.apexpublish` remains the manual fallback.
- The Windows named-pipe client is covered by a disposable local protocol test. A live handoff still needs a run with the updated desktop app and the configured Chrome extension.

## User workflows

Typical command-line usage (the API key is never an argument):

```powershell
apex-studio-cli roblox credentials status
apex-studio-cli roblox credentials set --account-id 123456789
apex-studio-cli roblox liveries --account-id 123456789
apex-studio-cli roblox batch start --account-id 123456789 --livery <FOLDER_ID> --yes
apex-studio-cli roblox queue --json
apex-studio-cli posts list
apex-studio-cli posts import .\post.apexpublish
apex-studio-cli posts handoff <POST_ID>
apex-studio-cli codes export .\livery-codes.md --post <POST_ID> --post <POST_ID>
```

- The Roblox credential set command opens a hidden prompt after the command runs. The local desktop app must be running to execute queued Roblox transfers.
- Running Apex Studio CLI from PowerShell requires either the install folder on `PATH` or an explicit executable path, for example `& "$env:LOCALAPPDATA\Apex Studio\apex-studio-cli.exe" roblox queue`.

- Browse eligible livery folders across projects, filter/search and inspect slot, decal-code, moderation, and account-specific upload status.
- Export the selected liveries' usable decal IDs as one Markdown file. Include only the final selected decal result for the matching account; never emit image IDs, first-pass IDs, stale-account IDs, or rejected decals as usable codes.
- Configure the Roblox account ID and API key using a hidden interactive prompt. Validate the key's owner, expiry, and asset read/write scopes before replacing Credential Manager contents. Do not accept API keys as command-line arguments or include them in JSON or logs.
- Preview and create multi-livery upload batches, review existing/reused codes and double-upload pass count, then deliberately confirm before network activity. Show persisted queue status and support pause, resume, cancel pending, retry failed, and needs-review inspection.
- Create/list/show/edit global ERLCX post drafts through typed JSON input; add preview PNGs from explicit paths; inspect stale livery sources; export `.apexpublish` without overwrite. Do not publish or submit automatically.
- Use the existing Chrome assistant for browser preparation through the running desktop bridge; keep `.apexpublish` export/import as a manual fallback. The assistant does not submit or publish the final post.

## Architecture and safety

- Move reusable Roblox credential, transport, and queue-worker behavior behind shared application services instead of duplicating HTTP or queue logic in the CLI. Keep the current Windows Credential Manager service target and persisted queue schema compatible with Apex Studio. Credential access and HTTP/response validation are shared; queue-worker extraction remains open.
- CLI and desktop may open the same workspace concurrently. Queue task claims remain exclusive and durable. CLI startup must not call crash-recovery logic that pauses a healthy desktop queue. Any shared queue runner uses persisted leases/state transitions; ambiguous POST results remain `needs_review` and are never blindly repeated.
- Keep rate-limit response handling, account pinning, one/two-pass preference, and final decal-ID selection consistent with the existing desktop worker. No key rotation or rate-limit bypass.
- Separate secret entry from Clap argument parsing. Avoid shell-history exposure, echo, debug output, JSON, backups, and logs. Validate new credentials before replacing the existing secret; changing credentials pauses that account's queued batches.
- Batch previews summarize project/livery/texture counts and existing IDs. Network uploads require explicit user confirmation. Automated tests use local mocks only; live uploads require separately selected user textures.
- ERLCX drafts use revision checks and `PublishingInput` validation. Saved posts are immutable snapshots; refresh IDs only on request, and require explicit review for changed or missing source liveries.
- Browser handoff must use the registered extension and one-time, short-lived tokens. The CLI must not accept arbitrary extension messages, expose workspace paths, or launch the final Submit/Publish action.

## Incremental implementation

1. ~~Add typed shared application wrappers and CLI commands for global post drafts, livery summaries, queue snapshots, and Markdown code output.~~ Initial read-only and draft workflows are implemented; account-verified code filtering from Roblox upload records remains part of the queue/credential work below.
2. ~~Extract credential validation/access from Tauri-only code into a shared service.~~ Credential validation and vault access now live in the shared application crate; upload-worker extraction and CLI execution remain.
3. ~~Add hidden credential configuration and double-upload preference commands.~~ Add security tests that prove no secret appears in stdout/stderr or process arguments and failed validation leaves previous credentials intact.
4. Add batch preview/confirm/start and queue controls with offline mock transport tests, including retries, throttling, interrupted operations, account changes, and pass-one/pass-two behavior.
5. ~~Add ERLCX draft commands, PNG media import, source-staleness review, `.apexpublish` import/export and the supported Chrome bridge request.~~ Typed and TUI draft flows, bundle round-trips, explicit stale-snapshot review, and the Windows request to the active desktop bridge are implemented; a live Chrome handoff remains an external verification step.
6. ~~Add searchable TUI views and selection workflows for Roblox and ERLCX.~~ Searchable draft/queue views, ERLCX metadata/media/livery/code editing, source-linked livery selection, reviewed multi-livery batch creation, and selected-batch pause/resume/retry/cancel controls are implemented.
7. Run debug/release checks, application and desktop adapter checks, and all mock/disposable workspace tests. Do not perform a live Roblox upload or ERLCX submission as part of routine verification.

## Exit gate

The CLI can inspect/create/recover durable multi-livery upload batches and global ERLCX drafts without bypassing the shared storage invariants. Secret configuration is hidden and validated; queue snapshots and markdown export contain only the correct account's final decal IDs. Both TUI and JSON commands support the reviewed offline workflows. `.apexpublish` round-trips safely and Chrome handoff stops before submission.

## Deferred

Live Roblox upload verification, live ERLCX submission, publishing/scheduling, organization/group upload identities, and Team Sync operations are not automated by this phase.
