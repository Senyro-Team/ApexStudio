# Phase 7 — MCP contract and release hardening

Status: Complete on `codex/apex-cli-phase-1`.

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
- Rechecked the installed `apex-studio-cli` in a fresh Windows PTY: the boxed navigation renders all modules, the selection hints are visible, and `q` exits cleanly with terminal state restored.
- Verified the optimized CLI build and ran `workspace --json` against a fresh disposable workspace (schema version 18, zero managed files). Frontend build/typecheck, all 99 frontend tests, and the ERLCX extension build passed. Vite reports the existing Notes chunk above its 500 kB advisory threshold.
- Added an offline, machine-checked schema suite that validates all schema documents, representative serialized workspace/service results, and ad hoc CLI operation reports. The existing CI `cargo test --locked` runs this suite.
- Added a report schema for command-specific JSON wrappers, including synchronization runs, revenue CSV imports, and status/configuration results; the contract tests validate these representative shapes.
- Added direct-output contracts for the official vehicle catalogue search, asset versions, and nullable creator-sales snapshots.
- Verified GitHub Actions updater run `42499606466` completed successfully on ancestor commit `32ff8067`; the v0.1.1 release contains the signed NSIS updater, `.sig`, and `latest.json` assets.
- Final verification: root `cargo test --locked` passed 139 tests (30 application, 29 CLI unit, 17 CLI smoke, 5 JSON-contract, 7 core, 51 storage); `cargo check -p apex-desktop --all-targets --locked`, `cargo fmt --all --check`, `git diff --check`, and targeted CLI Clippy passed. Frontend build/typecheck, 99 frontend tests, extension build, optimized CLI build, and a Windows PTY review passed. The installed command and PowerShell completion work directly.

## Separate workspace note

Group Revenue's strict `apex-storage` Clippy check now passes: the experience IDs use the direct stable sort, and fallible test setup/operations return errors instead of unwrapping. A full-workspace strict Clippy run then reaches separate warnings in `apps/cli/src/main.rs`; those are outside this Group Revenue fix. GitHub's latest successful Foundation run predates commit `be0ec2a`; the signed updater run and uploaded v0.1.1 assets were verified separately.
