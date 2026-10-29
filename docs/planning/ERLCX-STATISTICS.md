# ERLCX statistics implementation

## Contracts and migration

Workspace schemas 10 and 11 hold profile and sales snapshots; schema 11 also repairs early development workspaces that had schema 10 without the sales table. The profile setting is a numeric Roblox user ID. Each complete run is stored atomically with UTC collection time, scope, account ID, and only validated post IDs, titles, counters and rating metadata. Backups use the existing database backup path. Apex does not navigate to or directly request post detail pages, nor does it persist cookies or buyer identities. ERLCX itself may prefetch links when rendering its creator dashboard. Only explicit refresh contacts ERLCX. Partial pages and failed requests cannot replace the last complete snapshot.

The public list endpoint uses the observed `page=1..N`, `limit=6`, and `sortBy=latest` contract. Pagination must be bounded and consistent, duplicates rejected, null values preserved, and unexpected responses reported. Public metrics are not equivalent to owner totals: unlisted content can be absent. Snapshot trends compare observations rather than asserting event times or unique users.

## Authenticated dashboard research

On 2026-10-28 the signed-in Creator Dashboard displayed sales and earnings and made requests to `/users/me/monetization/earnings?days=30&limit=1`, `/bundles/me/stats`, and `/bundles/me/analytics?days=30`. Earnings exposes a summary and time series as well as individual transactions with buyer identifiers. The user-initiated Chrome bridge transfers only aggregate counts, Robux totals and daily series; it discards transaction records in the page context. It checks the authenticated `profile.sub` against the selected ID. A short-lived one-use token and session bind the transfer to the current app request; restored workspaces invalidate pending handoffs. The Chrome session remains in Chrome. Aggregated sales snapshots are included in workspace backups, which are not encrypted by Apex Studio.

Sales use the last 30 days selected by ERLCX plus its all-time purchase counter. Public profile figures can omit unlisted posts, so they are presented separately from creator-dashboard sales. Historical charts compare saved observations; they do not reconstruct earlier activity. The endpoint is undocumented and may change, so failures leave the last saved result intact.

## Native verification

On 2026-10-29 the release app was tested in Tiki Workspace with the owner's selected Roblox ID. A public refresh loaded all available pages and created a second observation, enabling the history chart. After verifying the Chrome connection, the creator-dashboard transfer produced the same aggregate sales and net revenue shown by ERLCX. No individual post page was deliberately opened. A development workspace that had been migrated to the original schema 10 without the later sales table was repaired by schema 11; an automated migration and backup/restore test covers this case.
