# ERLCX — bulk statistics investigation

## Signed-out follow-up — 2026-10-21

Verified with Login visible in Chrome and `/users/me` returning 401. The site loaded this public request successfully:

```http
GET https://api.erlcx.com/users/roblox/2919579206/bundles?page=1&limit=6&sortBy=latest
```

The response contains the same metric fields: views, likes, copies, favourites, ratingCount and ratingAverage. Page 1 returned six posts; clicking Next produced page 2 with two posts, total 8 and totalPages 2. The previously observed `/bundles/me` returned 401 while signed out.

A separate ordinary HTTP request using PowerShell Invoke-RestMethod, with no browser session or credentials supplied, also returned page 1, total 8, six items and first-item views 91. This confirms that browser authentication is unnecessary for this observed public endpoint, and provides evidence that a native desktop HTTP client can retrieve it.

Across both public pages: 1,761 views, 44 likes, 74 copies and 53 favourites. These are sums of public-post counters at collection time, not unique users or full signed-in-account totals. The first post still returned 91 views; this observation is not a guarantee about all server-side effects.

Public results contain 8 posts versus 10 in the earlier authenticated list. The previously seen unlisted post did not appear in the public first page or second page. Keep public-profile and owner-session scopes distinct; the exact status of every omitted owner entry was not inspected.

### Updated architecture recommendation

Implement public-profile analytics through an opt-in Rust HTTP adapter using the observed public endpoint, with local snapshots and manual Refresh first. The Chrome extension is not technically required to obtain these public metrics. Retain file import as an independent fallback. Owner-only data remains a separate optional future integration; never move browser cookies into the desktop app.

The endpoint is still undocumented in the material reviewed. Public technical access does not establish a supported API contract, rate limits or approval for scheduled collection. Confirm those before releasing scheduled integration. Handle schema changes, pagination changes, timeouts and 429 responses gracefully. Store scope `public-profile` and profile ID with every run. A refresh is complete only when all pages validate and IDs are deduplicated; preserve partial-run evidence without replacing a complete total.

The earlier sections below describe the initial signed-in investigation; their public-access and page-2 unknowns are superseded by this follow-up for this profile and observed limit.

Observed 2026-10-21 in Chrome on the user's signed-in profile:
https://erlcx.com/profile/2919579206

## Verified result

Chrome DevTools Protocol network capture of a profile reload showed this successful browser request:

```http
GET https://api.erlcx.com/bundles/me?page=1&limit=6
```

The JSON response contains page, limit, total, totalPages and items. Observed values: page 1, limit 6, total 10, totalPages 2. Each item contains bundleId, title, views, likes, copies, favourites, ratingCount and ratingAverage, alongside broader publishing metadata. Reading the list response does not require navigating to each item page.

Sanitised example, restricted to useful metric fields:

```json
{
  "page": 1,
  "limit": 6,
  "total": 10,
  "totalPages": 2,
  "items": [{
    "bundleId": 580732245,
    "title": "[APX] NYPD Christmas 2019 Livery",
    "views": 91,
    "likes": 5,
    "copies": 7,
    "favourites": 4,
    "ratingCount": 3,
    "ratingAverage": 5
  }]
}
```

The example is shortened to one item and is a historical observation, not live seed data.

Other first-page observations:

| Post | Views | Likes | Copies | Favourites |
| --- | ---: | ---: | ---: | ---: |
| Chicago Fire Department Firehouse 42 | 119 | 3 | 3 | 3 |
| Silas Security Pack | 208 | 4 | 7 | 8 |
| RCPD Silver Pack | 328 | 6 | 6 | 9 |
| Rivercity Addon Pack | 381 | 8 | 14 | 12 |

This is not a complete-account total. Only the first page's metrics were inspected.

## Important distinctions

- `/me` scopes this observation to the currently signed-in account. Public/unauthenticated access and other-user list endpoints were not established.
- Authentication headers/cookies were not exported. The desktop must not copy browser session secrets.
- The list can contain unlisted posts and broader metadata. Persist only selected metrics, IDs, titles and account scope; never archive full browser responses by default.
- Rating averages, favourites, likes and copies are different metrics. Copies do not establish unique downloads or sales; views do not establish unique visitors.
- A zero rating average with ratingCount 0 means no ratings. Display “No ratings”. Missing counters are null, not zero.
- The profile displayed “Total Uploads 8” and an “Uploads 10” tab. Do not derive statistics from these inconsistent summary labels.
- The browser also prefetched item routes with Next.js `_rsc` parameters while showing the profile. Apex should consume list data and avoid deliberately visiting/prefetching detail routes. No claim is made that the site's own navigation has no side effects.
- This is an observed internal endpoint, not a documented public integration contract. No rate limit, stable API promise, authentication mechanism, or permission for scheduled access was established.

## Collection design

Start with local import of a metrics-only JSON file. For an authorised browser connector, collect account-scoped list responses using the existing browser session without exposing credentials. Keep this behind a connector feature flag until the access method is clarified with ERLCX.

Pagination should follow the observed page/limit response contract, validate finite bounds, de-duplicate by account and bundle ID, and record incomplete runs. Page 2 was not captured in this investigation; verify pagination in an integration test before claiming complete collection. Do not increase the observed limit without testing/documentation.

Store append-only snapshots with collectedAt in UTC, source/version, account scope and nullable counters. Deduplicate reimports by import ID and account/post/time identity. Counter decreases are possible; keep them and label negative changes rather than replacing them with zero. Missing posts are not automatically deleted or marked zero. Errors preserve the last successful snapshot and show freshness.

Calculate interval changes only between comparable samples; do not pretend to know events before the first snapshot or exact event times between samples. Display rating count beside average. Partial collection must not replace complete account totals.

On 401/403 request browser sign-in or stop the connector; on 429 honour Retry-After where provided; on network errors use bounded backoff. Do not repeatedly load item pages as fallback. No background polling until provider permissions and limits are resolved.

## Provider terms review

The [published ERLCX terms](https://erlcx.com/terms), accessed 2026-10-21, prohibit unauthorised internal access, interference, moderation circumvention and artificial engagement. They do not provide a documented developer API contract or explicit approval for this integration. This is an engineering observation, not a legal determination. Preserve the original concept's requirement to clarify permitted integration methods before releasing continuous API access or automated submission.

## Follow-up verification

1. Confirm permitted use of the account list endpoint with ERLCX and obtain any supported integration documentation.
2. Verify page 2 and account/group attribution without exposing credentials.
3. Inspect a signed-out public profile separately if public-account analytics is needed.
4. Verify whether creator dashboard metrics offer a more complete supported export.
5. Establish snapshot retention, freshness intervals and user-driven versus scheduled collection.

No posts were published, edited, liked, copied or deleted during the investigation. The upload wizard was opened and Liveries selected for field inspection; no draft was saved.
