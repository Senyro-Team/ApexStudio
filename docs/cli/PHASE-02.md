# CLI Phase 2 — Projects, notes, and assets

Status: Complete. Verification passed on 2026-11-08.

## User workflows

- Manage active and archived projects: create, edit, search, inspect, archive, restore, and permanently delete an archived project after an explicit confirmation.
- Browse project and general notes in the TUI. Create, show, edit from Markdown files, export Markdown with embedded images, archive, restore, and delete notes through scriptable commands with revision checks.
- Browse the project asset tree: regular folders, vehicle-specific livery folders, files, slots, versions, previews, and trash in the TUI. Search and navigate with keyboard selection. Reimport into an existing livery to update matching slots while retaining versions.
- Import arbitrary files by path into a project or folder. Storage owns the copy; the source file must remain unchanged. PNG validation applies only to workflows that already require PNGs (covers and livery textures).
- Move/copy assets and folders, set or clear slots, inspect/restore file versions, export a managed asset without overwriting an existing destination, and use the project trash with recover/confirm-before-purge behavior.
- Provide scriptable equivalents with structured JSON output so later MCP tooling does not parse TUI text.

## Architecture and safety

- Add typed operations to `apex-application`; the CLI invokes those services and storage remains the owner of validation, revision conflicts, managed-file copies, history, and garbage collection.
- No direct SQLite access from CLI code and no second data model. Desktop and CLI continue sharing one workspace concurrently.
- Markdown editing uses a temporary file and the configured editor (`VISUAL`, then `EDITOR`). Save only after a successful editor exit; preserve note images through the existing `apex-image:<hash>` references and import local PNG references into managed storage.
- File paths are supplied explicitly; do not recurse outside folders supplied by the user. Import previews report imported/skipped/failed files. Export uses create-new semantics and will not overwrite.
- Destructive actions show project/item identity and affected-file counts, then require an explicit `--yes` or interactive confirmation. Permanently deleted content follows storage's existing blob-reference and version rules.

## Incremental implementation

1. Extend the shared application service with project lifecycle, note, asset-board, folder, placement, version, import/export, and trash methods; add typed, disposable-workspace service tests.
2. Add project create/edit/archive/restore/delete commands and a searchable TUI list/detail browser.
3. Add note list/show/create/edit/archive/restore/delete commands and TUI list/detail browser flows with Markdown round-trip, links, and embedded PNGs.
4. Add asset tree/list/import/move/copy/slot/version/export commands and TUI browser; add trash list/restore/purge flows.
5. Add smoke tests using generated nested folders, arbitrary files, valid/invalid PNGs, Markdown notes, conflicts, versions, and trash. Reopen each disposable workspace to verify persistence and check source hashes are unchanged.
6. Review command help/errors and a real PTY session at narrow, standard, and wide dimensions; verify shared desktop/storage operations against the same throwaway workspace.

## Exit gate

A disposable project can be created and edited, notes survive Markdown/image round-trip, arbitrary assets survive import/move/copy/export/reopen with originals unchanged, livery/slot/version state remains valid, and trash recovery/purge works without removing referenced content. TUI and JSON commands use the same application services. No real workspace or user asset is used as fixture data.

## Verification record

- `cargo fmt -p apex-cli -p apex-application --check` passed.
- `cargo test -p apex-cli -p apex-application` passed: 14 tests, including disposable CLI workflows and concurrent clients.
- `cargo clippy -p apex-cli -p apex-application --all-targets -- -D warnings -A clippy::unnecessary_sort_by` passed. The allowance is for an existing unrelated storage lint.
- `cargo build -p apex-cli --release` passed.
- Opened and navigated the TUI in a real Windows PTY against a generated disposable workspace, then exited cleanly.
- `npm run build:windows-installer` completed its Inno Setup packaging successfully before the final existing-livery CLI option was added. The final rerun compiled the current Rust targets but Tauri could not replace `target/release/apex-desktop.exe` because that executable was running. Close Apex Studio and rerun the installer script to produce a package containing the final CLI build.
- `git diff --check` passed.

## Deferred

Project-package/workspace backup flows, Roblox/ERLCX publishing, statistics, Team Sync, credentials, and MCP transport are later roadmap phases. Their menu entries remain absent until those workflows are implemented.
