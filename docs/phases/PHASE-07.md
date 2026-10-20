# Phase 07 – Getrennte Image-/Decal-IDs

**Status: Teilabnahme; Phase nicht vollständig abgenommen.** Checked 2026-10-20. Identifier contracts and persisted preferences are in schema version 2 / [ADR 0002](../adr/ADR-0002-upload-and-identifier-schema-v2.md).

## Implemented

Domain types keep `image` and `decal` separate, represent decimal identifiers as strings, track source/status, and reject mismatched value/status combinations. Selection order is explicit action, project override, global preference; default is Image/numeric. Selecting an absent preferred kind returns an error rather than substituting the other kind. SQLite persists global and project preference rows. The Roblox operation classifier assigns only the ID paired with the returned asset type; the other kind remains `unavailable`.

Roblox's reviewed [Assets API response documentation](https://create.roblox.com/docs/cloud/reference/features/assets) documents an asset ID and asset type but no related Image/Decal ID pair in that operation response. No inferred relation or second upload is used.

## Verification and acceptance

| Criterion | Result |
|---|---|
| Both kinds and resolution status in views/exports | Domain model only; no views, asset identifier CRUD or export binding. Open |
| Preference matrix shared by desktop/CLI | Core precedence helper and persistent DB preference tests; UI/CLI bindings absent. Partial |
| Missing kind never becomes false ID | Domain test passes. Pass |
| IDs above JS safe integer survive | Rust type uses decimal string; roundtrip is tested in model/adapter. Full DB/UI/package roundtrip not yet tested. Partial |
| Values and provenance survive export/import/restart | Preference restart tested; identifier persistence/export/import not implemented. Open |

Phase 7 is not fully accepted. No official related-ID resolver could be identified in the current official response contract, so that bounded provider feature is unavailable and shown as such by the model. `cargo test --workspace --locked` passed (39 tests total); preference persistence and safe-selection tests passed, while identifier CRUD and package roundtrip remain unimplemented.
