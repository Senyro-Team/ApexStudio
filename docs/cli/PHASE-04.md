# CLI Phase 4 — Roblox uploads and ERLCX publishing

Status: In progress.

## Implemented so far

- Shared application wrappers expose global ERLCX post drafts, source status, preview PNG import, `.apexpublish` export, Roblox livery summaries, and durable queue snapshots.
- `posts list|create|show|save|sources|add-images|export` provides JSON-editable global draft workflows. Saving uses the draft revision for conflict detection; adding media imports and pins PNGs through the storage service.
- `codes export <PATH> --post <ID>...` combines up to 50 saved post snapshots into one escaped Markdown file, requires `--reviewed` for stale/missing source liveries, and refuses to overwrite files.
- `roblox liveries --account-id <ID>` and `roblox queue` expose read-only account-specific livery coverage and persisted queue state. These commands do not start network transfers or perform startup recovery.
- `roblox batch start` creates an explicitly confirmed durable batch; pause/resume/cancel/retry controls operate on the shared queue. The existing desktop worker handles network requests while the desktop app is running, using credentials configured in Settings.
- `roblox double-upload --enable|--disable` changes the persisted preference used by future batches.
- `roblox credentials status|set|clear` uses the same Windows Credential Manager entry as the desktop app. `set` reads the API key in a hidden terminal prompt, validates account ownership, expiration and asset scopes, and stores only after successful validation; queued batches are paused before replacing or clearing credentials.
- The TUI includes searchable ERLCX draft and Roblox queue views, with post snapshot details, batch status, texture pass state, and both decal IDs.
- Disposable integration coverage checks multi-post Markdown output, escaping, no-overwrite, livery listing, queue creation/pause, and the double-upload preference without any live Roblox requests. Local HTTP mocks exercise Roblox credential introspection, and a Clap test ensures API keys are rejected as command-line arguments; no real credentials are read by tests. TUI rendering coverage includes the new screens at compact and standard widths.

## Current limitations

- An upload worker that can run without the desktop app has not been implemented in the CLI yet. A CLI-created batch is durable, but it waits for the existing desktop worker.
- Drafts can be edited through typed JSON files; the new TUI draft and queue views are currently read-only.
- `.apexpublish` export is available; a CLI importer and direct Chrome bridge handoff are not yet available.
- The planned Chrome path is confirmed: request a handoff through the running desktop bridge; retain `.apexpublish` as the manual fallback.

## User workflows

Typical command-line usage (the API key is never an argument):

```powershell
apex-studio-cli roblox credentials status
apex-studio-cli roblox credentials set --account-id 123456789
apex-studio-cli roblox liveries --account-id 123456789
apex-studio-cli roblox batch start --account-id 123456789 --livery <FOLDER_ID> --yes
apex-studio-cli roblox queue --json
apex-studio-cli posts list
apex-studio-cli codes export .\livery-codes.md --post <POST_ID> --post <POST_ID>
```

- The Roblox credential set command opens a hidden prompt after the command runs. The local desktop app must be running to execute queued Roblox transfers.
- Running Apex Studio CLI from PowerShell requires either the install folder on `PATH` or an explicit executable path, for example `& "$env:LOCALAPPDATA\Apex Studio\apex-studio-cli.exe" roblox queue`.

- Browse eligible livery folders across projects, filter/search and inspect slot, decal-code, moderation, and account-specific upload status.
- Export the selected liveries' usable decal IDs as one Markdown file. Include only the final selected decal result for the matching account; never emit image IDs, first-pass IDs, stale-account IDs, or rejected decals as usable codes.
- Configure the Roblox account ID and API key using a hidden interactive prompt. Validate the key's owner, expiry, and asset read/write scopes before replacing Credential Manager contents. Do not accept API keys as command-line arguments or include them in JSON or logs.
- Preview and create multi-livery upload batches, review existing/reused codes and double-upload pass count, then deliberately confirm before network activity. Show persisted queue status and support pause, resume, cancel pending, retry failed, and needs-review inspection.
- Create/list/show/edit global ERLCX post drafts through typed JSON input; add preview PNGs from explicit paths; inspect stale livery sources; export `.apexpublish` without overwrite. Do not publish or submit automatically.
- Use the existing Chrome assistant for browser preparation when the desktop bridge is available; keep `.apexpublish` export as a manual fallback.

## Architecture and safety

- Move reusable Roblox credential, transport, and queue-worker behavior behind shared application services instead of duplicating HTTP or queue logic in the CLI. Keep the current Windows Credential Manager service target and persisted queue schema compatible with Apex Studio.
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
5. Add ERLCX draft commands, PNG media import, source-staleness review, `.apexpublish` import/export and the supported Chrome bridge request.
6. ~~Add searchable TUI views and selection workflows for Roblox and ERLCX.~~ Read-only searchable draft/queue views are implemented; TUI selection workflows and confirmation actions remain for future iteration.
7. Run debug/release checks, application and desktop adapter checks, and all mock/disposable workspace tests. Do not perform a live Roblox upload or ERLCX submission as part of routine verification.

## Exit gate

The CLI can inspect/create/recover durable multi-livery upload batches and global ERLCX drafts without bypassing the shared storage invariants. Secret configuration is hidden and validated; queue snapshots and markdown export contain only the correct account's final decal IDs. Both TUI and JSON commands support the reviewed offline workflows. `.apexpublish` round-trips safely and Chrome handoff stops before submission.

## Deferred

Live Roblox upload verification, live ERLCX submission, publishing/scheduling, organization/group upload identities, and Team Sync operations are not automated by this phase.
