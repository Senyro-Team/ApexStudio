# Phase 2 — projects and managed assets

2026-10-22. User authorised continuing Phase 2. This slice implements local project management and the first usable asset flow, preserving the spacious reviewed layout.

## Implemented

- Create and edit projects with name, description, tags, multiple vehicle names, reference links and optional deadline. UUID identity and automatic UTC creation date are preserved on edit.
- Search across names, descriptions, tags and vehicles. Separate active and archived views; archive/restore preserves records and files. No permanent deletion.
- Dashboard recent projects and deadline shortcuts using saved data. Project detail defaults to Assets; Notes and Publishing explicitly identify future work.
- Native multi-file selection, streaming managed copies, SHA-256 deduplication, per-project duplicate detection and per-file failure feedback. Imported originals remain untouched.
- PNG thumbnails for bounded supported images, generic file cards for other formats, file search, selection-only inspector, and native Save a copy that refuses to overwrite existing files.
- Global asset library with project links; no independent asset import outside projects yet.
- Version 2 SQLite migration with projects/assets, foreign keys and indexes. Version 1 workspace settings and managed files survive migration.
- Shared Rust metadata validation; narrow asynchronous IPC. Native file pickers live in Rust, with no raw frontend path-import API or frontend filesystem/SQL permissions.
- Unsaved form indicator, discard confirmation on Back, and section-navigation lock during editing or file operations. Save explicitly before closing the native window; close-time draft recovery is not implemented.

## Vehicle catalogue limitation

`catalogue/vehicles.json` contains 146 deduplicated vehicle-name suggestions captured from the public wiki Vehicles page on 2026-10-22, with source links and attribution. The page explicitly warns it is outdated. Every support label remains unknown; no capability was inferred from vehicle type or team. The picker displays this limitation.

This does **not** fulfil the final all-current-vehicles/verified-livery-support catalogue requirement. Verify against current official templates and other wiki lists before publishing preparation; maintain stable catalogue IDs and rename/retirement handling when extending this snapshot. No runtime network request is needed for the current picker.

## Validation

- `npm run build`: TypeScript and production bundle passed.
- `npm test`: five frontend interaction tests passed, including create and edit flows, writable-fields-only IPC, and editing navigation protection.
- `cargo test --locked`: eight core/storage tests passed. Coverage includes invalid metadata, deduplication, originals preserved, partial import failures, reopen persistence, archive/restore, copy overwrite refusal, version 1 migration and newer-schema refusal.
- `cargo check --locked -p apex-desktop --features custom-protocol`: passed.
- `cargo clippy --locked --workspace --all-targets --features custom-protocol -- -D warnings`: passed.
- Rust formatting and Impeccable detector passed (no UI detector findings).
- `cargo build --locked -p apex-desktop --features custom-protocol`: passed, embedding the current production frontend.
- Native Windows smoke test: created `Phase 2 verification`, imported a PNG and text file through the real system file picker, confirmed thumbnail/generic cards, closed and relaunched the executable, and reopened the project with both files present. Source SHA-256 hashes were unchanged. Archived the test project afterward, preserving its files. Screenshot: `phase-02-native.png`.
- No remote CI run, signed installer or cross-platform native verification is claimed. Browser preview intentionally cannot save local workspace records.

## Remaining boundaries

Visual notes, package/backup/restore, Roblox uploads and ERLCX extension remain future slices. Asset versions, side/slot classification, bulk organisation, standalone library imports and removal are not included yet. Thumbnail limits (8 MiB source and 16 million pixels) affect previews only; other files still import as assets. Lists currently load whole collections; pagination is future scaling work.

The workspace is local and unencrypted. Do not manually edit the database or delete managed blobs. Consistent whole-workspace backup and orphan-file reconciliation remain required before release. Code/branding licence finalisation is still outstanding. No existing Git deletions were restored, no staging or commit was performed, and no provider post or upload was submitted.
