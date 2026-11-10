# Apex Studio CLI Roadmap

Status: Phases 1–6 are complete; Phase 7 (MCP contract and release hardening) is in progress.

## Product shape

The CLI is a Windows-first, portable Rust client for the same local workspace and application services as Apex Studio. Starting `apex-studio` without a subcommand opens a keyboard-first full-screen TUI inspired by the supplied terminal references: framed panels, searchable/selectable lists, visible focus, contextual key help and clear progress. Named subcommands provide scriptable operations and JSON output for future MCP integration.

The CLI must support concurrent use with the desktop app. Both clients use the existing SQLite workspace and managed file store; no private CLI copy, parallel schema, or custom second workspace lock is introduced. SQLite WAL, transactions and busy handling remain authoritative. Tests must open the same disposable workspace from separate `Workspace` instances/processes and prove reads and writes remain consistent. Schema migration behavior under concurrent startup is an explicit Phase 1 gate.

Workspace initialization uses a short-lived OS file lock around database setup and migrations. The lock is released immediately after initialization; ordinary desktop and CLI operations remain concurrent through SQLite WAL and busy handling.

Application rules belong in shared Rust services, with Tauri and the CLI acting as adapters. The CLI must not reimplement validation or storage invariants. Existing credentials stay in the operating system credential store; secrets never appear in command output, JSON, backups, or logs. Destructive operations, uploads, sync and publish handoff require clear target summaries and the appropriate explicit confirmation.

The first release is shipped through the Windows installer; the CLI crate itself must stay portable. The current Group Revenue work continues independently. CLI integration for that module starts only after its data and service contracts are stable.

## Capability coverage

The roadmap targets nearly all user-facing desktop capabilities:

| Desktop capability | CLI coverage |
| --- | --- |
| Projects, search, archive and restore | TUI and commands |
| Project/general notes and Markdown | TUI editor/preview and commands; retain image/link references |
| Assets, livery folders, slots, versions and trash | TUI browser/search and file commands |
| `.apexstd` import/export and workspace backups | Path-based commands with inspect/confirm steps |
| Roblox decal queue and codes | Selectable batches, progress, recovery and Markdown code output |
| ERLCX drafts and Chrome assistant | Draft preparation and handoff; browser-only wizard remains in Chrome |
| ERLCX statistics and Roblox Group Revenue | Summaries, history, CSV import, and terminal charts/tables |
| Team Sync | Account, invitation/member, project sync and shared-code operations |
| Settings and credentials | Supported configuration, with secrets in OS credential storage |

Window layout, drag-and-drop gestures, native file-picker dialogs, and browser-only interactions are not literal TUI replicas. The CLI provides equivalent keyboard/path-based workflows or launches the existing browser/desktop integration where required.

## Sequential phases and gates

### Phase 1 — Shared foundation and project explorer

- Add a portable Rust CLI package and a shared application-service boundary that can be called by both the TUI and future MCP adapters.
- Establish workspace path resolution compatible with the desktop app, explicit `--workspace` override, Clap help/errors/exit codes, and `--json` output for supported commands.
- Build the Ratatui/Crossterm TUI shell with responsive framed layout, menu navigation, search, selection, focus indication and help. Initial functional surface: project list, filtering, project details, and a scriptable project list.
- Include the CLI executable as a selectable feature of the Windows Inno Setup installer. Installation/update/uninstall must preserve existing desktop preferences and data.
- Test with temporary dummy workspaces. Verify independent CLI processes and the desktop adapter can read/write the same workspace concurrently; exercise normal, narrow, resized, color-disabled, and non-interactive terminals. Do not use the real workspace as fixture data.

**Exit gate:** Release and debug CLI build; `--help`, JSON project listing and TUI navigation work; shared-workspace/concurrent-startup tests pass; installer stages and installs the CLI component without altering desktop-only installs.

### Phase 2 — Projects, notes and asset explorer (complete)

- Complete project create/edit/archive/restore and search/filter.
- Add project/general notes, rendered preview and Markdown editing that preserves supported Markdown content.
- Add asset/folder/livery browsing, import, rename/move/copy, slot assignment, versions, cover previews, trash restore/purge and safe file export.
- Reuse core/storage validation and preserve originals on import.

**Exit gate:** Dummy project with nested folders, text/image/PNG and arbitrary file types survives import, edit, version restore, trash recovery and app/CLI reopen with originals unchanged.

### Phase 3 — Packages, backups and local settings (complete)

- Add `.apexstd` preview/import/export and whole-workspace backup/restore with the same validation and pre-restore safety backup behavior as desktop.
- Add local workspace and supported application settings; show explicit confirmations and recovery details.

**Exit gate:** Round-trip disposable projects and backups, verify contents and file hashes, test conflicts and interrupted/invalid archives without changing the active real workspace.

### Phase 4 — Roblox and ERLCX (complete)

- Add Roblox account configuration through the OS credential store, livery selection, queue controls, double-upload preference, progress/retry/recovery, and Markdown-formatted code export.
- Add global ERLCX draft management, preview/livery selection, `.apexpublish` export and the existing Chrome Native Messaging handoff.
- Keep final ERLCX submission and Roblox uploads deliberate; the CLI must never auto-publish or bypass browser confirmation.

**Exit gate:** Offline/mock transport tests cover selection, durable queue transitions, credential absence/expiry and retry states. Live upload remains a separate explicit test with user-chosen assets.

### Phase 5 — Statistics and Group Revenue (complete)

- Add public ERLCX profile snapshots, authenticated browser refresh handoff, imported sales summaries and group-revenue CSV import after the current Group Revenue service/data contract stabilizes.
- Display labeled text charts, totals, date ranges, source freshness and limitations without conflating Roblox and ERLCX metrics.

**Exit gate:** Synthetic history and CSV fixtures validate calculations, duplicate-safe imports, empty/partial periods, and the distinction between gross, held, released and cancelled amounts.

See [Phase 5 acceptance record](PHASE-05.md#acceptance-record) for the Windows test counts, release build, PTY review and the desktop test-harness limitation.

### Phase 6 — Team Sync (in progress)

- **Complete:** the shared `apex-application::team_sync` service owns API identity/status, owner setup, Credential Manager sessions, invitation acceptance/creation, member listing/removal, project bindings, chunked project transfer, conflict resolution, streamed livery previews and shared Roblox decal codes. The desktop Tauri module is now an adapter to those services. Loopback mocks cover authentication and team workflows; the interactive TUI shows project/member state, links/unlinks projects, syncs, and reviews/resolves conflicts.
- CLI commands cover `sync check`, `setup`, `status`, `sign-in`, `accept-invitation`, `invite`, `members`, `remove-member`, `projects`, `bindings`, `link`, `unlink`, `run`, `resolve`, `codes` and `disconnect`. Sync and destructive actions show their target and require confirmation; passwords and bootstrap/invitation acceptance secrets use non-echoing prompts rather than command-line arguments or JSON output.
- Windows verification passed with a fresh disposable TUI workspace, application/CLI tests, and `cargo check -p apex-desktop --all-targets`; no real team data was modified during automated tests.
- Keep credentials in the OS credential store; show account/team/target summaries before sync or member removal.

**Exit gate:** Local mock-server cases cover invite expiry, role access, offline state, project conflict, retry and shared-code reuse. No live destructive team action is part of automated verification.

### Phase 7 — MCP-ready service contract and release hardening

- Complete a stable typed application-service API and documented JSON schemas/error codes so a later MCP server can invoke the same services without scraping TUI text.
- Add shell completions and command documentation, finalize installer component defaults and PATH/start-menu behavior, and verify upgrade/uninstall behavior.
- Run the complete Windows, TTY, dummy-workspace, installer and regression matrix. Document unsupported desktop-only interactions.

**Exit gate:** Installer-installed CLI works in PowerShell and a real terminal; all supported capabilities have CLI/TUI/JSON tests; MCP can consume the shared service layer without depending on terminal rendering.

## Working rules

- Complete and verify one phase before beginning the next. Keep each phase's changes scoped and documented.
- Use only disposable generated workspaces/files for routine verification. Preserve the real workspace and user files.
- After each phase, run relevant tests/builds and manually inspect at least one PTY session; capture issues and fix them before advancing.
- Keep app-facing copy functional and concise. Planned or incomplete CLI features belong in documentation, not as fake active menu items.
- Do not alter the in-progress Group Revenue implementation as part of CLI groundwork.
