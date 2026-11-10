# CLI Phase 5 — Statistics and Group Revenue

Status: In progress. Planning recorded on 2026-11-10.

## User workflows

- Set and inspect the ERLCX profile ID from CLI settings without opening any individual post page.
- Refresh public profile post metrics from the same ERLCX profile API used by the desktop app. Store a local snapshot on each successful refresh and preserve historic snapshots.
- Inspect latest totals (views, likes, copies, favourites, rating count/average), per-post ranking, change between snapshots, collection time, and missing metric coverage.
- Read ERLCX creator sales snapshots captured through the existing authenticated Chrome/desktop integration. The CLI must not scrape creator-dashboard pages itself or store browser cookies.
- List and configure Roblox groups, experience IDs, pass mappings, selected passes, imported sales and API refresh coverage using the existing Group Revenue storage contract.
- Import Roblox CSV sales with a preview, source-file identity, skipped-row count and duplicate count before explicit confirmation. Re-imports remain safe through the storage service's current deduplication rules.
- Compare Roblox group sales with ERLCX creator sales only over explicitly overlapping UTC dates. Label Roblox `price`/`revenue` and ERLCX `grossRobux`/`netRobux` separately and show coverage gaps rather than presenting them as equivalent accounting values.
- Provide compact dashboard summaries and detailed searchable tables/charts in the TUI, plus stable JSON output for MCP clients.

## Architecture and safety

- Extend `apex-application` with typed methods for statistics profile/history/sales and Roblox Group Revenue data/configuration/import. CLI code must not query SQLite directly.
- Move public ERLCX page-fetching and response validation behind the shared application layer so desktop and CLI call one implementation. Preserve bounds on page count, response bytes, duplicate post IDs and malformed metrics.
- Reuse existing `StatisticsSnapshot`, `SalesSnapshot`, `RobloxRevenueGroup`, `RobloxGroupSale`, and storage import APIs. Add only domain fields or migrations when the current schema cannot represent a required value.
- CSV parsing must handle UTF-8 BOM, quoted commas/quotes, CRLF, header aliases, malformed rows, invalid dates and safe integer amounts. Do not silently convert malformed sales to zero. Preview before import and preserve original CSV files.
- Keep browser authentication in the current desktop/Chrome bridge. Browser refresh requests must be typed and allowlisted; do not expose cookies, arbitrary page evaluation or workspace paths to the extension.
- TUI charts use labeled text/Unicode bars with plain-text fallbacks under `NO_COLOR`; always display metric, period, units, source and collection time.

## Implementation sequence

1. Add shared read/configuration wrappers and isolated service tests for statistics snapshots, latest sales, group configuration, import results and duplicate handling.
2. Implement typed JSON commands for statistics profile/history/refresh and Roblox Group Revenue groups/data/import/remove; secrets and browser session data must not enter command args or output.
3. Add robust CSV preview/import and synthetic fixtures for header variants, quoted fields, dates, currency formats, invalid rows, duplicate transaction IDs and repeat imports.
4. Add TUI navigation for Statistics and Group Revenue: focused summaries, time range selection, searchable post/sales tables, CSV preview confirmation, and explicit freshness/coverage states.
5. Route authenticated creator-dashboard refresh through the existing typed desktop/Chrome bridge; report when the desktop app or signed-in browser session is unavailable.
6. Run dummy-workspace and mock-HTTP tests, CLI/application/desktop checks, optimized Windows build, and a PTY review at 40/80/120 columns. Do not use real browser credentials or submit any post.

## Exit gate

- Synthetic profile history renders correct totals, deltas, ranking and stale/partial states; API refresh is tested against bounded local HTTP fixtures and persists only complete, validated snapshots.
- Synthetic creator sales imports and group CSV imports are duplicate-safe and preserve price/revenue, hold state and UTC coverage distinctly.
- TUI and JSON output agree on the same saved data, and authenticated refresh requests use only the existing desktop bridge.

## Deferred

Live ERLCX creator-dashboard refresh with the user's browser session, live Roblox API refresh and any real CSV import require a separately selected explicit test. No sale or publish action is automated.
