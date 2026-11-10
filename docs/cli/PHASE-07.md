# Phase 7 — MCP contract and release hardening

Status: In progress.

## Implemented in this slice

- Added `apex-studio-cli completions <shell>` using Clap's generated completion scripts. Completions run without workspace discovery, database access, or migration.
- Added a disposable-workspace smoke test for PowerShell completion output.
- Added the v1 JSON/error and shared-service contract documentation, including a machine-readable error-envelope schema and a rule that MCP tools call application services rather than scraping terminal output.
- Added schemas for workspace/project/note/asset-board/ERLCX-draft/Roblox-queue/ERLCX-statistics/Team Sync/Group Revenue/trash records and documented their command mappings.
- Updated the Inno installer to add the selected CLI installation directory to the current user's `PATH`, avoid duplicate entries, remove only that exact entry if the CLI component is unselected, and clean it on uninstall.
- Added the installed CLI directory to the current Windows user's `PATH`, replaced its executable with the optimized build, and verified direct command invocation and live PowerShell tab completion.
- Added the CLI executable to signed updater resources and the release workflow build, so app updates refresh the command-line binary too.
- Verified Inno first install with and without the CLI component, component removal on upgrade, and uninstall; unrelated `PATH` entries remained byte-for-byte unchanged.
- Verified the updated release CLI in a Windows PTY against a new disposable workspace and exited cleanly.
- Added an offline, machine-checked schema suite that validates all schema documents, representative serialized workspace/service results, and ad hoc CLI operation reports. The existing CI `cargo test --locked` runs this suite.
- Added a report schema for command-specific JSON wrappers, including synchronization runs, revenue CSV imports, and status/configuration results; the contract tests validate these representative shapes.
- Added direct-output contracts for the official vehicle catalogue search, asset versions, and nullable creator-sales snapshots.
- Latest verification: root `cargo test --locked` passed 139 tests (30 application, 29 CLI unit, 17 CLI smoke, 5 JSON-contract, 7 core, 51 storage); `cargo check -p apex-desktop --all-targets --locked`, `cargo fmt --all --check`, `git diff --check`, and targeted CLI Clippy passed. Clippy explicitly allows two pre-existing warnings in Group Revenue storage and the TUI worker enum; no new warning was reported.

## Remaining acceptance work

- Produce and verify a signed updater artifact through the GitHub release workflow; signing credentials are available only to Actions.
- Finish mapping and checking the less common direct-model CLI outputs against the corresponding typed schemas.
- Finish the broad Windows TTY, disposable-workspace, and application regression matrix; verify the MCP service boundary remains fully typed and credential-free.
- Commit the completed phase only after all remaining acceptance checks pass.
