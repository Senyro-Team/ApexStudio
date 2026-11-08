# CLI Phase 3 — Packages, backups, and workspace settings

Status: Complete. Verification passed on 2026-11-08.

## User workflows

- Export a project to `.apexstd`, embedding managed files by default and optionally omitting notes or all file content.
- Inspect a project package or workspace backup before importing/restoring it. Show archive type, title, file/note counts, and stored byte count.
- Import a project package as a new project after explicit confirmation of the preview. Never merge into or overwrite an existing project.
- Create a whole-workspace `.apexbackup` containing a consistent SQLite snapshot and the referenced managed files.
- Restore a workspace backup only after showing what will be replaced and requiring `--yes` or a typed interactive confirmation. Use storage's automatic before-restore backup and show its path after completion.
- Show workspace name, path, schema version, and managed-file count; rename the workspace through the shared validation service.

## Safety and architecture

- Add wrappers to `apex-application`; CLI code must not inspect or write the SQLite database directly.
- Reuse `preview_package`, `export_project`, `import_package`, `backup`, and `restore_backup`. `.apexstd` and `.apexbackup` bytes and validation remain owned by `apex-storage`.
- Check the 2 GiB selected-archive limit used by the desktop flow. Reject wrong archive kinds before asking for confirmation.
- Export and backup use create-new semantics. Never overwrite a destination.
- Restore confirmation identifies the current workspace and the selected backup. Invalid or mismatched archives do not change the active workspace. Successful restore reports the automatic safety-backup path.
- Do not include device credentials, browser tokens, or API keys in archives or JSON output. Preserve current storage policy.
- Only expose settings that already have a shared backend contract. Phase 3's supported mutable workspace setting is the workspace name; per-window theme and future integration credentials remain in their existing owners.

## Incremental implementation

1. Add typed package/backup operations to `ApplicationServices` and test wrappers against temporary workspaces.
2. Add `packages export|inspect|import`, `backups create|inspect|restore`, and `workspace rename` commands with stable JSON results and safe confirmations.
3. Add smoke coverage for project round-trip (including assets, notes, versions and image hashes), backup/restore (database plus files), wrong-kind archives, corrupted input, no-overwrite destinations, and safety-backup creation.
4. Verify CLI operations against a reopened disposable workspace, then run the full workspace tests and a PTY session.

## Exit gate

Disposable projects and whole-workspace backups round-trip with database records and file hashes intact. Invalid archives and destination collisions leave existing data unchanged. Restore creates and reports a pre-restore backup. JSON and terminal output agree on archive preview details; workspace rename survives reopen.

## Verification record

- `cargo fmt -p apex-cli -p apex-application --check` passed.
- `cargo test -p apex-cli -p apex-application` passed: 15 tests, including 8 command-level integration tests over generated workspaces and files.
- `cargo clippy -p apex-cli -p apex-application --all-targets -- -D warnings -A clippy::unnecessary_sort_by` passed. The allowance is for an existing unrelated storage lint.
- `cargo check -p apex-cli -p apex-application -p apex-desktop --features apex-desktop/custom-protocol` passed.
- The TUI opened and exited in a Windows PTY using a newly generated disposable workspace.
- Package tests covered metadata-only and embedded exports, contents preview, import confirmation, backup restore of database and managed files, safety-backup path, invalid/wrong-kind/oversized archives, and no-overwrite behavior.
- The installer was not rebuilt for this phase because a running `target/release/apex-desktop.exe` prevented Tauri from replacing that executable in the previous installer attempt. The existing installer configuration still includes the release CLI component; the CLI release binary is built independently.

## Deferred

Integration secrets, Roblox and ERLCX preferences, Team Sync settings, app theme settings, package conflict/merge flows, and selective backup restore are handled by later feature phases or are intentionally unsupported by the current shared settings model.
