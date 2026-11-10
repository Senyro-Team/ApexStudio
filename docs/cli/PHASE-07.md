# Phase 7 — MCP contract and release hardening

Status: In progress.

## Implemented in this slice

- Added `apex-studio-cli completions <shell>` using Clap's generated completion scripts. Completions run without workspace discovery, database access, or migration.
- Added a disposable-workspace smoke test for PowerShell completion output.
- Added the v1 JSON/error and shared-service contract documentation, including a machine-readable error-envelope schema and a rule that MCP tools call application services rather than scraping terminal output.
- Added schemas for workspace/project/note/asset-board/ERLCX-draft/Roblox-queue/ERLCX-statistics records and documented their command mappings.
- Updated the Inno installer to add the selected CLI installation directory to the current user's `PATH`, avoid duplicate entries, remove only that exact entry if the CLI component is unselected, and clean it on uninstall.
- Added the installed CLI directory to the current Windows user's `PATH`, replaced its executable with the optimized build, and verified direct command invocation and live PowerShell tab completion.
- Added the CLI executable to signed updater resources and the release workflow build, so app updates refresh the command-line binary too.
- Verified Inno first install with and without the CLI component, component removal on upgrade, and uninstall; unrelated `PATH` entries remained byte-for-byte unchanged.
- Verified the updated release CLI in a Windows PTY against a new disposable workspace and exited cleanly.

## Remaining acceptance work

- Produce and verify a signed updater artifact through the GitHub release workflow; signing credentials are available only to Actions.
- Add schemas for the remaining report-specific Team Sync and Group Revenue JSON forms before external consumers rely on those CLI transport shapes.
- Finish the broad Windows TTY, disposable-workspace, and application regression matrix; verify the MCP service boundary remains fully typed and credential-free.
- Commit the completed phase only after all remaining acceptance checks pass.
