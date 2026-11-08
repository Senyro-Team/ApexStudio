# Group Revenue

Group Revenue keeps Roblox group transactions in the local workspace. It does not send sales records to Team Sync.

## CSV import

The importer accepts Roblox's current `Date and Time` / `Location` headers and the older `Sale Date and Time` / `Sale Location` names. Transaction `Id` is optional. Buyer User IDs and raw CSV rows are never persisted. Robux values such as `700.0` are accepted when their fractional part is zero; non-zero fractional Robux are rejected rather than rounded.

When `Id` is present, Apex uses it as the idempotency key. Otherwise it hashes the canonical sale fields and an occurrence number. This avoids storing buyer identifiers, but indistinguishable rows cannot always be matched perfectly across sources.

## Game Pass selection

Catalog items from the Roblox refresh and unknown pass IDs observed in sales appear in the selection list. New catalog items start unselected. Historical imports remain stored when a pass is unselected, but selected-pass analytics, comparisons and the sales table include only selected IDs. `Revenue` and buyer `Price` are separate; held and released revenue are shown separately, and cancelled rows do not contribute to totals.

## Browser refresh

Refresh is explicit and uses the configured Chromium profile and the installed Apex extension. The extension makes GET requests to Roblox for group experiences, each experience's Game Pass catalog, group transactions and the optional group revenue summary. It never reads or serializes browser cookies. The native bridge accepts only a one-time, expiring refresh token and sanitized batches; buyer identifiers and raw responses are discarded. No request is sent to Team Sync.

Roblox currently marks the group transaction feed and group revenue summary Experimental and the creator Game Pass listing Beta. The transaction response schema does not guarantee that buyer price and group revenue are both available for every record. Apex imports a transaction only when it can identify the experience, Game Pass, timestamp, status, Revenue and Price. Other returned transactions are counted as requiring CSV review. If a Roblox response is unauthorized, rate-limited, malformed or unsupported, the last successful catalog, coverage and summary remain intact. CSV import remains available for backfill.

The displayed coverage is derived from dates in the pages returned by the API; a successful response does not prove that Roblox exposed the group's entire historical period. The group revenue summary is shown separately and is never added to selected Game Pass totals. Refresh is manual; there are no background Roblox API requests.

Game Pass IDs and API sales are deduplicated using transaction IDs when available. Exact field matches are used across CSV/API sources when no ID exists. The workspace database migration is schema 18. Workspace backups include the catalog and selection. Project-only packages do not include workspace-wide revenue records.

## Roblox references

- [Group APIs](https://create.roblox.com/docs/cloud/reference/features/groups) — group experiences, experimental transaction feed, and revenue summary.
- [Game Pass APIs](https://create.roblox.com/docs/cloud/reference/features/game-passes) — Beta creator catalog.
- [Game Pass permissions](https://create.roblox.com/docs/cloud/reference/scopes) — `game-pass:read` scope for Open Cloud alternatives.
- [Sales CSV fields](https://create.roblox.com/docs/production/analytics/analytics-dashboard#sales-data) — export cadence and Revenue/Price meanings.
