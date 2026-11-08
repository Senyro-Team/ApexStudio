# Roblox group revenue analytics

Status: local CSV prototype implemented, 2026-11-08. Experimental API and scheduled sync remain proposed and require the implementation gates below.

## Implemented prototype — 2026-11-08

The first local CSV vertical slice is implemented in Apex Studio's Group Revenue screen. It accepts Roblox Creator Hub sales CSV files, associates them with a user-configured group, filters to Game Pass rows for configured experience IDs, previews before import, stores rows and ID-to-label mappings in the workspace database, and shows released/held/cancelled totals. Re-importing a file is idempotent through a fingerprint built from available sale fields. Buyer User ID and raw CSV rows are discarded; no group cookie is requested, stored, or transmitted. Imported transaction rows remain local to the workspace.

The screen compares CSV aggregate counts and Revenue with the saved ERLCX 30-day series only for dates present in both sources. It labels the sources, retrieval/import times, range, unknown coverage, and aggregate-only limitation. It does not identify transaction-level ERLCX misses or change either source's data.

The Roblox experimental group API is still deferred. The implementation does not collect a Roblox session cookie. Fingerprints omit buyer identity; because Roblox's documented CSV has no unique transaction ID, identical sales cannot always be distinguished across overlapping or altered exports. User-selected CSV completeness and group association cannot be independently verified by Apex. Existing manual CSV import remains the only active group revenue ingestion path until the implementation gates below are completed.

## Goal

Analyze purchases and Robux revenue for selected Roblox group-owned Game Passes, compare platform sales with ERLCX statistics, and make missed ERLCX purchases visible without counting the same sale twice.

Keep Roblox group revenue separate from the existing ERLCX creator sales snapshot. They have different account scopes, coverage and freshness. A combined display must show the source, collection time, time range and whether the data is complete.

## Recommendation

Use Roblox's experimental group revenue endpoints as an opt-in data source, with the experimental status shown in the UI and every result labelled with source, retrieval time and freshness. The Creator Hub catalog lists `GET /v1/groups/{groupId}/revenue/summary/{timeFrame}` as experimental and cookie-authenticated; it also lists `GET /v2/groups/{groupId}/transactions` as cookie-authenticated. This is enough to justify a prototype, but the endpoint documentation does not establish stable schema, rate limits, or whether each response carries Game Pass-level detail. First verify response shape and coverage against the official CSV export, which includes asset IDs and row-level sales fields and is updated every 48 hours. See [Roblox Groups API reference](https://create.roblox.com/docs/cloud/reference/features/groups) and [Roblox Analytics sales data](https://create.roblox.com/docs/production/analytics/analytics-dashboard#sales-data).

An “experimental” warning is useful for setting expectations, but it does not protect the Roblox account credential or make the numbers complete. The group endpoints require a Roblox session cookie, not an Open Cloud API key. If polling runs on the Apex Sync Server, that cookie would be stored on the server and could grant account access beyond read-only revenue. The Open Cloud documentation treats API keys as passwords and recommends least privilege; the session-cookie path deserves at least as much care. Keep the CSV import as a fallback and validation source. See [Open Cloud API key guidance](https://create.roblox.com/docs/cloud/auth/api-keys).

The current ERLCX bridge is already explicitly user-triggered and transfers only aggregate earnings, counts and daily series. It intentionally drops individual transaction rows and buyer IDs. Reuse that interaction pattern for user-initiated refreshes where supported, but keep ERLCX aggregates distinct from transaction-level group sales. See [ERLCX statistics](ERLCX-STATISTICS.md).

Proposed rollout: begin with a member-triggered refresh using the experimental revenue summary, clearly marked as experimental; add the transaction endpoint only after verifying it returns useful, deduplicable sale rows. Then offer optional one-to-two-hour server polling for teams that explicitly enable it and accept that dependency. Store the cookie only in the self-hosted server's secret store, never in project-sync records, client logs, backups shared with team members, or source code; provide a disconnect/revoke path and surface authentication, rate-limit and schema failures without replacing the last good snapshot. If secret storage or revoke handling is not in place, keep refresh manual in the signed-in member's browser instead of sending the cookie to the server.

## First vertical slice

1. Configure a numeric Roblox group ID and the relevant group-owned experiences.
2. Maintain a mapping from Roblox Game Pass asset ID to an ERLCX pass/product identifier and display name. IDs are authoritative; names are labels only. Support multiple Roblox IDs mapping to one ERLCX item where appropriate, and flag unknown IDs for review.
3. Add a member-triggered fetch of the experimental group revenue summary, and inspect the transaction endpoint before depending on it. Provide CSV import as a validation and recovery path. Validate response fields, dates, currency and group/experience context; preview unexpected or incomplete results.
4. Where transaction rows are available, filter to `Game Pass` rows for configured group experiences. Keep held, released and cancelled revenue distinct. Preserve the platform's reported revenue and price; do not derive net earnings from list price.
5. Deduplicate imports using a stable transaction fingerprint from available sale fields. Re-importing the same CSV must not increase totals. If Roblox supplies no unique transaction ID, document and test collision handling rather than pretending a composed key is guaranteed unique.
6. Display Roblox sales and ERLCX counts/earnings side by side for matching date ranges. With today's ERLCX aggregate-only figures, show totals and the difference; do not label individual Roblox rows as missing from ERLCX. If ERLCX later provides per-pass IDs or transaction rows, show unmatched Roblox transactions and unresolved ERLCX-only differences.
7. Keep imports and snapshots local to the workspace initially. Do not sync transaction-level buyer identities. Drop the CSV's Buyer User ID during parsing; persist only fields needed for analytics and reconciliation.

## Data and calculations

Store each imported sale with source, group ID, universe ID, pass asset ID, sale timestamp (UTC), sale location, hold status, price, revenue and import batch ID. Do not retain buyer ID, raw CSV row, or email download link. Store the original file only if a separate user-facing audit/backup requirement is decided; default is parse, preview, then discard it.

Use revenue as Roblox's stated amount received, and price as the buyer-facing listing price. Do not add held/cancelled revenue into released Robux. Any totals should state whether they include held amounts. Do not silently equate ERLCX's current aggregate `netRobux` and `grossRobux` with CSV `Revenue` and `Price` until their definitions and scopes are confirmed against example data.

Reconciliation compares transaction counts and totals by pass and date window. ERLCX currently exposes an aggregate 30-day series and an all-time purchase counter, not a per-pass purchase ledger, so it cannot currently be matched purchase-by-purchase. If ERLCX later exposes per-pass IDs or a transaction export, add exact-ID matching as a separate adapter. Before that, report the delta and possible missing count without claiming which exact sales ERLCX missed.

## Refresh options

| Option | Freshness | Credential / contract | Proposed use |
| --- | --- | --- | --- |
| Manual Roblox CSV import | Roblox says updated every 48 hours | Official dashboard download; no Apex-held Roblox secret | Fallback, transaction validation and historical import |
| Member-triggered experimental API refresh | Potentially fresher; coverage and schema unverified | Roblox session cookie stays in the member's browser | Recommended prototype starting point |
| Opt-in Sync Server polling every 1–2 hours | Potentially frequent; coverage and rate limits unverified | Requires storing a Roblox session cookie on the self-hosted server | Later option, only with explicit opt-in, secure secret storage and revoke path |

Manual CSV import should not require the team sync server. A member may later share opt-in aggregate summaries with their team; raw transaction records remain private by default.

## UI notes

- Add a Group Revenue section under Statistics with group selector, last import time, import source and coverage status.
- Show total sales, released Robux, held Robux, cancelled rows and unmatched Game Pass IDs.
- Let users review/map unknown pass IDs and see the ERLCX comparison range.
- Show a clear delay note for CSV data: Roblox says the export updates every 48 hours. Label API observations as experimental rather than live or guaranteed complete.
- Preserve the existing creator-dashboard snapshot and its manual Chrome refresh flow.

## Decisions before implementation

- Which group's sales and which group-owned experiences are in scope?
- Does ERLCX expose any Game Pass IDs or per-pass metrics in its dashboard/export, beyond the current aggregate purchase and earnings figures?
- Should group members share only aggregate totals, or also mapped per-pass totals? Raw sales rows stay local by default.
- Should an ERLCX mismatch be treated as a correction overlay in charts, or simply shown as a reconciliation report? Proposed default: show both source values and the difference; never rewrite either source record.

## Implementation gates

Before release, compare the experimental summary and transaction endpoint against the official CSV for several time windows, including held/cancelled and Game Pass sales. Record observed schema and limits, deduplicate safely, label all charts experimental, and keep CSV import available. Before enabling scheduled polling, require explicit opt-in and secure server-side secret storage, log redaction, revocation, bounded retry/backoff and preservation of the last good snapshot. Before ERLCX automation, confirm its supported access and rate limits. Build import validation, duplicate-import, partial-month, held/cancelled, unknown-pass, privacy and backup/restore coverage before relying on imported data.
