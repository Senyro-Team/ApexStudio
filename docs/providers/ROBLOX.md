# Roblox Open Cloud Assets

Research checked 2026-10-20 against the official [Assets Usage Guide](https://create.roblox.com/docs/cloud/guides/usage-assets), [Assets API reference](https://create.roblox.com/docs/cloud/reference/features/assets), and [Open Cloud scopes](https://create.roblox.com/docs/cloud/reference/scopes).

## Implemented contract

- Create: `POST https://apis.roblox.com/assets/v1/assets`, multipart fields `request` (JSON) and `fileContent` (PNG bytes).
- Request JSON: `assetType` (`Image` or `Decal`), `displayName`, neutral description, and `creationContext.creator.userId` or `groupId`. Creator IDs remain decimal strings. The operation response uses enum values such as `ASSET_TYPE_DECAL`; the adapter normalizes these only for classification.
- Authentication: user-created Open Cloud API key in `x-api-key`; API keys need the relevant Assets read/write operation permissions. The documented OAuth scopes are `asset:write` and `asset:read`, but OAuth app setup is not implemented. The key is held in the OS credential store and never serialized into profiles.
- Create may initially return only the operation path. `GET /assets/v1/operations/{operationId}` is polled using the provider-returned relative `operations/{id}` path. The adapter records `done`, `response.assetId`, `response.assetType`, and `response.moderationResult.moderationState` when present.
- Metadata read: `GET /assets/v1/assets/{assetId}`.
- Only the asset ID returned with its actual `assetType` is classified. The reviewed API response does not document a related Image/Decal ID mapping. The other kind therefore remains unavailable; no arithmetic or second upload is performed.

## Limits and policy difference

The Roblox usage guide allows a single request up to 20 MB and says Decal/Image dimensions must be smaller than 8000×8000. The local Apex PNG policy remains inclusive at 20,000,000 bytes and 8000 pixels per side. Apex does not resize files; `provider_image_warning` reports the stricter provider edge separately. The exact `displayName` maximum and a stable price/quota table were not found in the reviewed endpoint guide and are not guessed here; the provider remains authoritative for validation, throttling and quota rejection.

## Security behavior

The live client accepts only the official HTTPS API origin, uses TLS verification, a 10-second connect timeout and 45-second request timeout, disables redirects, caps JSON responses at 1 MiB, sends neutral multipart filename `asset.png`, and never includes the local source filename or Commission name. Operation IDs are validated relative path segments. An upload transport error maps to `transport_unknown_outcome`, which is not retryable. Read failures and provider errors use sanitized fixed messages and at most a sanitized request ID; response bodies and API keys are not logged.

The simulator is currently a test-only loopback HTTP fixture; it cannot be selected by the production client. No real account credential or live upload was used in development or tests.

## Known gaps

The adapter ports exist, but profile management is not yet connected to the desktop or CLI. Deterministic HTTP coverage currently includes a successful operation poll and contract checks; the full fixture matrix for POST, 401/403, 429/Retry-After, quota, 5xx, timeout, redirect and oversized/malformed responses remains open. Provider rate-limit retry timing and cost details remain provider-owned rather than guessed.
