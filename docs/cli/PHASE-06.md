# Phase 6 — Team Sync

Status: In progress.

## Implemented

- Team Sync protocol and OS credential handling live in `apex-application`; the Tauri command layer delegates to it.
- CLI commands cover server check, owner setup, sign-in, invite acceptance/creation, member listing/removal, project previews and bindings, linking/unlinking, project synchronization, conflict resolution, shared decal-code refresh, and disconnect.
- Passwords, owner setup keys, and invite acceptance codes are read without terminal echo. Passwords are not supported as CLI arguments. Session tokens remain in Windows Credential Manager and are not serialized to CLI output.
- Project sync reuses the desktop implementation for snapshots, chunked files, team code sharing, collision checks, recovery and conflict resolution. Desktop editing-state checks are supplied as a callback so a remote snapshot is not applied while an upload edit is active.
- Sync and member-removal commands print a target summary and require explicit confirmation; JSON mode requires `--yes` for those actions.

## Verification to date

- `cargo test -p apex-application -p apex-cli`: passed with dummy workspaces; includes application tests for two-device sync, conflict preservation, large-file chunk transfer, API identity validation against a loopback mock, and CLI command/password-argument parsing.
- `cargo check -p apex-cli -p apex-desktop`: passed after the shared-service adapter change.

## Remaining before Phase 6 acceptance

- Add Team Sync to the framed TUI with searchable projects/members and keyboard workflows for linking, syncing and conflict review.
- Add mock-server coverage for owner setup, invitation lifecycle, member permissions, preview summaries, shared code retrieval, offline behavior and retry handling.
- Verify the TUI/commands on Windows, including a disposable workspace and a desktop/CLI shared-workspace run. Do not use a destructive live team operation as an automated test.
