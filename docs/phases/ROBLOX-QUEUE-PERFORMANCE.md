# Faster imports and persistent Roblox queue

Implemented 2026-10-27. This supersedes the single-file upload workflow in PHASE-05-ROBLOX.md.

## Workflow

- Open **Roblox** in the main sidebar. Filter across projects, search, select liveries and review **Upload missing/changed** or **Re-upload selected**. Livery upload shortcuts preselect the relevant project and folder.
- Review shows the account, assigned and unique texture counts, saved IDs reused, shared pending work and newly created decal count. Covers and unassigned files are excluded. Starting a batch is an explicit separate action.
- New texture/account pairs upload twice with identical PNG bytes. Pass two starts after pass one finishes processing, including when moderation is still reviewing. Only a validated, matching-owner **Decal** ID from pass two replaces the selected result. Both resulting IDs and individual POST attempts are retained; no remote asset is deleted. This workaround does not guarantee higher resolution.
- Existing saved IDs remain legacy results and are reused. Explicit re-upload retains the previous selected ID until the new second pass succeeds. Retry failed resumes the failed pass; it does not repeat a completed first pass. An unknown POST result stays **Needs review**. Check Creator Dashboard before cancelling its pending batch and explicitly choosing a new upload.
- The project **ERLCX** tab prepares posts using saved selected decal IDs. Missing IDs link to the Roblox queue. Existing drafts remain saved snapshots; **Refresh Roblox IDs** is explicit and does not automatically save the changed draft.

## Execution and recovery

Native code owns execution. Schema 7 persists batches, immutable texture/account/side snapshots, deduplicated jobs, two pass records, POST attempts and image-validation metadata. Overlapping batches share an active account/hash job. Queue reads use a consistent SQLite read transaction and frontend snapshots carry monotonic revisions. Events coalesce at 500 ms; upload events never rebuild an asset board.

Upload transfer slots start at five and can increase by one per clean 30-second interval up to ten only with observed server budget headroom. Throttling halves the limit, with a floor of one. Processing GETs have their own two-request pool and use 2/4/8/15-second backoff, so waiting for processing never occupies a transfer slot. One HTTP client is reused. Owner/endpoint budget reservations respect remaining/reset headers, numeric or HTTP-date Retry-After and jittered exponential fallback. Keys are never rotated to evade limits. Roblox documents owner-wide key budgets and possible additional limits: https://github.com/Roblox/creator-docs/blob/main/content/en-us/cloud/reference/rate-limits.md . Other applications' usage can still produce 429s.

Credential changes pause and drain requests while blocking new leases. Jobs remain pinned to their account. Keys stay in Windows Credential Manager and never enter SQLite, backups, packages or publishing bundles. Headers are sensitive; response bodies and keys are not logged.

Navigation leaves uploads running. Closing the window with unfinished batches hides it to the tray; Open Apex Studio and double-click reopen it. Tray Pause/Resume controls native batches. Explicit Exit stops scheduling, pauses persisted work and waits briefly for in-flight requests. An interrupted POST with no persisted operation requires review; known operations can be polled safely after explicit Resume. Unsaved edits and active file operations still block closing/exiting. Restart and restored backups pause unfinished batches; restore discards cached validation trust.

## Import pipeline

Native selection returns filenames and slot suggestions before asynchronous validation. Preparation status includes per-file errors. A four-worker executor copies and hashes each source once into workspace staging. Unique content shares an opaque backend-owned prepared handle. Successful validation is cached by SHA-256 and validator version. Decoding and thumbnail pixel generation share the decoded image. Small thumbnail encoding and caching run separately; files can become usable before every preview is written.

A weighted semaphore limits decoded image work to four concurrent workers and 1 GiB of reserved image memory. Large images wait for enough budget. Commit syncs staged file contents and promotes them by same-volume hard link, then uses one short FULL-durability SQLite transaction per livery. There is no second full image copy/hash/decode during commit. Original source files remain untouched. Regular asset listings return lightweight preview URLs; visible previews are served through a native hash-only protocol. Project/folder permission and upload queries are targeted. Asset placements/maps are memoized, lists are paged and thumbnails load only near the viewport.

PNG limits remain inclusive 8000 × 8000 and 20 MiB, with bounded decoder allocations. Browser flags cannot mark a texture validated. Thumbnails are disposable; managed originals and recoverable versions remain authoritative.

## Measurement

Machine: Windows, 16 GiB RAM, 12 logical processors. Approved EMS batch: 195 PNGs, 15 livery folders, 117 distinct contents, approximately 69.4 MiB compressed. Measurements use isolated copied inputs and temporary workspaces, never the active Tiki Workspace. Manual assignment/review time and build time are excluded.

Baseline at commit 91b069b, existing sequential preparation/import path:

| Build | Preparation | Commit including repeated image work | Total |
|---|---:|---:|---:|
| Old debug | 164.40 s | 816.16 s | 980.56 s |
| Old release | 11.83 s | 29.64 s | 41.47 s |

Baseline runs were diagnostic and overlapped development/build activity; they are not controlled speedup claims. New dev builds optimize image/png/sha2 dependencies; compare release builds for delivery performance.

| New release run | Manifest | Prepare/validate | Commit | Files usable | Thumbnail tail | Peak working set |
|---|---:|---:|---:|---:|---:|---:|
| First, empty app caches | 1.358 s | 7.672 s | 0.610 s | 9.639 s | 0.031 s | 446.6 MiB |
| 2 | 0.033 s | 7.878 s | 0.566 s | 8.477 s | 0.030 s | 446.6 MiB |
| 3 | 0.042 s | 7.341 s | 0.518 s | 7.901 s | 0.036 s | 447.1 MiB |

Median files-usable time: **8.477 seconds**, within the 120-second target. First manifest: **1.358 seconds**, within two seconds. All runs used fresh workspace/validation/thumbnail caches. OS file cache was not flushed. Four workers and at most 1,055,347,752 reserved decoded bytes (below 1 GiB) were observed. Each run decoded exactly 117 unique PNGs and asserted that commit did not increment the decode count. Thumbnail completion was measured separately after files became usable; some thumbnail encoding can already finish during validation.

New debug run: manifest 1.509 s; preparation 14.590 s; commit 0.703 s; usable 16.801 s; thumbnail tail 0.043 s; peak working set 448.1 MiB. This build includes optimized image dependencies, not fully unoptimized codecs.

Reproduce from the repository root:

```powershell
cargo build --release --locked -p apex-storage --example import_bench
./scripts/benchmark-import.ps1 -Source '<approved EMS folder>' -Runs 3
```

The script first copies its input into a new temporary test directory and samples process memory every 100 ms. The benchmark itself opens another temporary workspace. Renderer long tasks over 50 ms are instrumented locally and visible under Settings → Interface performance; no telemetry is sent.

## Verification

Automated checks cover PNG corruption and bounds, original preservation, prepared-handle deduplication, livery atomicity/partial failures, schema migration, backups and cache invalidation; queue deduplication/account isolation, sequential passes and only-second-ID selection, legacy reuse, failure preserving a previous ID, second-pass-only retry, cancellation of shared work, unknown-request recovery and paused restore. Local HTTP fixtures exercise multipart Decal creation, exact PNG bytes on both passes, strict owner/type parsing, lost POST responses, 401 and 429 handling. Adaptive budget tests cover reset/Retry-After, HTTP dates and the ten-transfer ceiling.

Live Roblox uploads and image-quality comparison remain a separate explicit test using user-chosen textures. No live upload, deletion, ERLCX draft save or post submission is part of this verification.

Final native review used the Explorer-launched optimized executable in **Tiki Workspace**, with the existing 195-file FDNY EMS project intact. Verified sidebar Roblox selection, saved-ID counts, the account/count/two-pass review dialog (cancelled before Start), the ERLCX project tab and rendered livery texture previews. One buffered renderer task exceeded 50 ms during this navigation/preview session: **55.0 ms** total and maximum. This is a navigation/preview measurement, not a claim of a measured worst case for every import/upload workload. Idle window close exited normally. The tray is created successfully at native startup; active-upload tray interactions remain part of the explicit live-livery review.

Final required checks: 46 Rust tests across core/storage/native adapters; 41 desktop frontend tests; TypeScript/Vite production build; workspace Clippy with warnings denied; Rust formatting; optimized Windows native build. The existing lazy notes-editor chunk still produces Vite's size advisory.

After all runs, SHA-256 comparisons confirmed that all 195 approved source PNGs still match the untouched isolated input copy.
