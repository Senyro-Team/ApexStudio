# ADR 0002: Persisted upload intent and explicit identifier kinds

- Status: accepted for local implementation; phase acceptance remains partial
- Date: 2026-10-20

## Context

Phases 5–7 need a durable upload intent, immutable creator association, safe lease recovery, separate Image/Decal IDs, and preferences. The original schema version 1 had an underspecified `upload_jobs` table and raw identifier rows.

## Decision

Schema version 2 adds a new `upload_jobs_v2` journal rather than silently reinterpreting legacy rows. Jobs retain creator profile ID, asset version reference, upload type, SHA-256, immutable snapshot JSON, state, operation ID, revision and lease. Append-only `upload_events` records transitions. Expired `uploading` leases transition to `unknown_remote_outcome`, never back to the queue. Global and project identifier preference tables store `image|decal` and `numeric|rbxassetid_uri`.

Rust domain types represent Image and Decal separately, keep external identifiers as decimal strings, track status and source, and select the effective kind using action > project > global. Missing preferred values are errors; no alternate kind is substituted.

## Consequences and gaps

Version 1 data is preserved as-is and not migrated into guessed ID kinds. Phase 7 still needs asset identifier CRUD/persistence, package import/export compatibility, and CLI/UI bindings. The queue currently implements intent, claim lease, operation persistence, unknown outcome and history, but not worker orchestration, retry/backoff, pause/cancel, moderation completion, deduplication or JSONL/CSV export. Those omissions prevent full Phase 6 acceptance.
