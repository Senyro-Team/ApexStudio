# CLI Phase 1 — Shared foundation and project explorer

Status: Complete on `codex/apex-cli-phase-1`.

## Delivered

- Added a portable `apex-cli` Rust package with Ratatui/Crossterm terminal UI and Clap commands.
- Added `apex-application` as a shared service boundary. Project list/detail/save/archive operations used by the desktop project adapter and CLI now share these service methods.
- The CLI defaults to the Apex Studio workspace under the OS local application-data directory and accepts `--workspace <PATH>` for disposable or alternate workspaces.
- Interactive mode has framed Apex styling, a menu, project search, project detail view, refresh, responsive narrow/standard/wide layouts, visible key help, `NO_COLOR`, and non-TTY rejection with a usable command hint.
- Scriptable commands include `projects list`, `projects show`, and `workspace`; `--json` returns structured data and stable error codes.
- Added an optional **Interactive command-line interface** component and Start Menu shortcut to the Windows Inno Setup installer. The installer build script stages the CLI executable.
- Workspace initialization now uses a short-lived cross-process OS file lock around initialization/migrations only. The lock is released before normal operations; SQLite WAL remains the concurrent-use mechanism.

## Dummy-workspace verification

All workspace records used for CLI verification were generated in temporary directories and deleted with their test fixtures.

- `cargo fmt --all --check` passed in an isolated checkout of the complete CLI patch.
- `cargo test --locked` passed: 63 tests across core, storage, application services and CLI, including JSON list/detail/filter/error outputs, TUI state/layout, parallel first-start processes, and separate CLI readers while a storage client writes.
- `cargo clippy --locked -p apex-cli -p apex-application --all-targets -- -D warnings` passed.
- `cargo check --locked -p apex-desktop --lib` passed with the shared application-service adapter.
- `cargo build --release --locked -p apex-cli` passed.
- A real PTY run opened the TUI, navigated into Projects, returned to the menu, exited, and restored the terminal.
- Inno Setup compiled the installer script with the new optional component and staged CLI executable. The component syntax and file/shortcut wiring were verified; this was a packaging smoke build with placeholder desktop/extension payloads, not a distributable installer.

The full workspace Clippy invocation still reports two pre-existing warnings in desktop files outside this phase (`browser_bridge.rs` and `installer_resources.rs`). Targeted CLI/application Clippy is clean. The shared checkout's separate, in-progress Group Revenue changes currently fail compilation due to missing `rusqlite::OptionalExtension` imports; they were left untouched. CLI verification was performed in an isolated checkout without those unrelated uncommitted edits.

## Current commands

```text
apex-studio-cli
apex-studio-cli projects list
apex-studio-cli projects list --search EMS --json
apex-studio-cli projects show <PROJECT_ID> --json
apex-studio-cli workspace --json
```

Use `/` to search in the TUI, Enter to open a project, `r` to refresh, Escape to go back, and `q` to return or exit.

## Next phase

Phase 2 adds project editing and the project/general notes and asset explorer workflows. See [the full sequential roadmap](ROADMAP.md).
