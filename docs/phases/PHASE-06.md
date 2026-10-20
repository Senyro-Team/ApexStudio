# Phase 06 – Persistente Uploadqueue

**Status: Teilabnahme; Phase nicht vollständig abgenommen.** Checked 2026-10-20. Schema decision: [ADR 0002](../adr/ADR-0002-upload-and-identifier-schema-v2.md).

## Implemented

Schema version 2 adds durable upload intents and append-only transition events. `enqueue_upload` persists an immutable snapshot, creator ID, asset version reference, upload type, file hash and `queued` state. SQLite immediate transactions claim a single job with a lease; a second worker cannot claim it. The operation ID is atomically recorded before the lease is released. A transport-unknown outcome has its own persistent state. Expired `uploading` leases become `unknown_remote_outcome`, never queued for a blind POST. History survives reopen.

## Verification and acceptance

| Criterion | Result |
|---|---|
| Restart/fault safety and no blind duplicate POST | Persist/reopen unknown outcome covered; process kill fault matrix not run. Partial |
| Parallel worker claim safety | Competing claim test covers serialized SQLite claim. Partial |
| Full asset/hash/creator/operation/time trace | Intent preserves creator/hash/snapshot and operation; requested/accepted/completed timestamps and full asset relation not implemented. Open |
| Pause/cancel/partial failure UI and recovery | Not implemented. Open |
| Simulator-only retries | No automatic retry worker exists; safe unknown handling is covered. Partial |

Executed: `cargo test --workspace --locked` passed (39 tests total, including 10 storage tests). It covers v1→v2 migration, worker claim/lease conflict, operation persistence, restart recovery and unknown-outcome snapshot. Queue snapshot fields are allowlisted to prevent arbitrary credentials or paths from entering the journal. Retry/backoff, provider and creator rate limiting, moderation transitions, duplicate handling, pause/resume/cancel/retry/reconcile APIs, and JSONL/CSV redacted exports remain open. Phase 6 is not fully accepted.
