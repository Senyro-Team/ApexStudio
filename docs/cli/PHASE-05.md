# CLI Phase 5 — Statistics and Group Revenue

Status: In progress. Planning recorded on 2026-11-10; public statistics, Group Revenue commands, CSV import, interactive Group Revenue workflows, authenticated creator-sales bridge refresh, and detailed statistics panels implemented on 2026-11-10.

## Implemented baseline

- `statistics profile [--set ID]`, `statistics summary`, `statistics history`, `statistics refresh`, and `statistics sales` are available in both human-readable and `--json` output.
- Statistics refresh uses the same typed application service as the desktop app. It requests only ERLCX's paginated public profile bundles endpoint, validates the entire result, and saves a local snapshot. No post detail pages are opened.
- `revenue groups` and `revenue show <group_id>` expose locally stored group settings, imported sales, and discovered passes as text or JSON.
- `revenue configure` creates or updates a group and its experience IDs; it preserves previously discovered pass mappings and selected passes. `revenue select-pass`, `revenue deselect-pass`, and `revenue remove` manage selections and remove group data, with destructive removal requiring `--yes` in non-interactive use.
- `revenue import-csv` reads Creator Hub exports without changing them, supports multi-file preview, bounded UTF-8 CSV parsing, quoted fields, header aliases, UTC normalization, safe Robux values, and transaction-ID deduplication. It reports ignored rows and possible duplicates before import; writing requires interactive `YES` or explicit `--yes`. Buyer IDs and raw rows are never saved or printed.
- The TUI has Statistics and Group Revenue pages. Statistics shows coverage-aware totals and a compact view-history chart. Group Revenue supports searching, add/edit forms, pass selection, CSV file selection and preview, and confirmed group removal.
- `statistics refresh-sales` and the Statistics TUI `s` action request authenticated creator sales through the running desktop's existing Chrome bridge. Requests use a typed allowlist; the CLI does not read cookies or scrape pages. Refresh runs in the background in the TUI and waits for completion before exit.
- The Statistics TUI has Overview, Posts, Changes and Sales panels; `w/m/y/a` filters history to 7/30/90 days or all available history. Posts are ranked by views, Changes compares matching posts across the two most recent snapshots in range, and Sales charts daily net Robux from the latest authenticated snapshot.
- `statistics summary --json` includes coverage-aware totals, aggregate deltas from the previous snapshot and the top ten posts by views. Signed deltas are strings so large values remain lossless in JSON clients.
- `statistics compare --range 7d|30d|90d|all [--json]` uses a shared application-layer comparison contract for ERLCX creator sales and selected Roblox passes. The report restricts daily figures to overlapping UTC dates, labels Roblox `price`/`revenue` separately from ERLCX `netRobux`, and reports CSV/partial dates as observed rows rather than treating missing days as zero. ERLCX gross Robux is identified as the full snapshot window because the source exposes no per-day gross series.
- TUI CSV imports use a preview screen before confirmation, do not touch source files, and show skipped rows and duplicate estimates. The pass list combines discovered passes, imported sale IDs, and manually mapped IDs.
- Public profile refresh runs in a background thread in the TUI. A disposable Windows terminal review confirmed both pages render, the empty Group Revenue state is understandable, and the terminal is restored after exit.
- Dummy-workspace smoke coverage verifies saved statistics/revenue JSON output. The profile HTTP test uses a local mock server and confirms the only requested path is `/users/roblox/{id}/bundles`.

## Remaining in Phase 5

- A final acceptance matrix across CLI, TUI, JSON and dummy workspaces, including the stable JSON contracts expected by future MCP clients.

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
