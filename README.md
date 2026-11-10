# Apex Studio

Local-first desktop workspace for ER:LC creators. Manage projects, notes, assets and livery publishing drafts in a spacious desktop interface. Core workspace features work offline; Roblox uploads require configured credentials. No telemetry is collected.

## Run

Requirements: Node.js 22.12+ (24 LTS recommended), Rust stable, and the Tauri platform prerequisites. Windows native builds need Microsoft C++ Build Tools with the Desktop development with C++ workload, Windows SDK, and WebView2. See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run desktop
```

For a frontend-only browser preview, use `npm run dev`. It clearly identifies browser mode and does **not** emulate SQLite persistence with browser storage. Foundation checks include `npm run build`, `npm test`, `npm run build:extension`, `cargo fmt --all --check`, `cargo fmt --manifest-path apps/sync-server/Cargo.toml --all --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo check --manifest-path apps/sync-server/Cargo.toml --locked`. The Windows job also checks the desktop Rust target. If Rust was installed without updating PATH, add the existing `.cargo/bin` directory to your shell PATH.

For a standalone development build with the frontend embedded:

```powershell
npm run build
cargo build --locked -p apex-desktop --features custom-protocol
.\target\debug\apex-desktop.exe
```

The Rust workspace also contains `apex-studio-cli`. Run `cargo run -p apex-cli` for the interactive terminal interface, or use `cargo run -p apex-cli -- projects list --json` for scriptable output. The CLI opens the same local workspace as the desktop app; pass `--workspace <PATH>` to use a disposable or alternate workspace. The Windows installer offers the CLI as a selectable component and adds its install folder to the current user's `PATH`. After installation, open a new PowerShell window and run `apex-studio-cli`; shell completions are available with `apex-studio-cli completions powershell`.

The native Windows build and project/file persistence across restart have been verified. Current check results are available in the repository's GitHub Actions workflow.

## Windows installer

Install Inno Setup 6, then run:

```powershell
npm run build:windows-installer
```

The installer is written to `target/release/bundle/inno/`. It includes the ERLCX Chrome extension package, offers Roblox/Group Revenue and ERLCX setup choices, and installs per-user. Release signing is not configured yet.

## Current behaviour

- Dashboard with recent projects, deadlines and project creation; searchable active/archived projects with edit, archive and restore.
- Project names, descriptions, tags, multiple vehicle suggestions, reference links, automatic creation dates and optional deadlines.
- Assets-first project detail, file search, PNG thumbnails, a selection-only inspector and native Save a copy.
- Native multi-file import accepts any file type, copies into managed storage and preserves originals. Repeated imports with the same project/name/bytes are skipped; shared bytes are deduplicated.
- Workspace asset library links files back to their owning projects.
- One workspace created under the OS local app-data directory for `studio.apexdesigns.apexstudio`, in `workspace/`.
- Transactional SQLite migrations through schema version 17, WAL, foreign keys, bounded busy timeout and durable writes.
- Rename the workspace in Settings; it persists across desktop restarts.
- Shared Rust core and SQLite adapter; narrow Tauri commands, no frontend filesystem/SQL/network access. The external-link opener is limited to the Apex Studio Discord invite.
- Official supplied template catalogue with team variants, body/window statuses and slot information. Snapshot dated October 7, 2026; unspecified statuses stay unspecified.
- Nested asset folders, livery import review, filename slot suggestions, covers and recoverable texture versions.
- Visual general/project notes with local PNG images, formatting, tables, explicit saving and revision conflict protection.
- Versioned .apexstd export/import with content validation and import-as-copy confirmation; notes excluded unless selected.
- Whole-workspace .apexbackup export and validated restore with an automatic before-restore backup.
- Roblox Group Revenue view under Statistics with local Creator Hub sales CSV import, configured group/experience/Game Pass filters, duplicate-safe imports and released/held/cancelled aggregates. Buyer IDs and raw CSV rows are discarded; no Roblox session cookie or experimental group API is used. See [scope and limitations](docs/planning/ROBLOX-GROUP-REVENUE.md).
- Local brand/font assets, concise English copy, keyboard focus and responsive shell.

Prepare project publishing drafts, select and order PNG previews, use saved Roblox decal IDs or enter existing IDs manually, and export a public-only `.apexpublish` bundle. The Chrome assistant fills the current ERLCX livery step on request. Build it with `npm run build:extension`; see [manual installation and privacy](apps/erlcx-extension/README.md). Live Media/Codes autofill still needs manual Chrome verification. Personal-account Roblox decal uploads are available from livery folders after configuring a key in Settings.

## Architecture

`crates/apex-core`: shared contracts, validation and use cases. `crates/apex-storage`: SQLite and immutable file storage. `apps/desktop`: React shell and Tauri adapter. Managed files are published only after their contents are written; a crash before database insertion can leave an unreferenced blob, never a recorded partial file. Reconciliation/cleanup is future work. Do not manually delete managed blobs.

Schema upgrades are transactional; newer schemas are rejected. The app never opens the earlier implementation's data location automatically. Use the built-in backup/restore tools for database and files together; copying a live SQLite file alone is not a supported backup.

## Rights and assets

Original application source code is licensed under [Apache License 2.0](LICENSE). Apex Studio name, logo, application icons and other brand artwork are excluded from that licence and governed by [BRANDING.md](BRANDING.md). In particular, forks may keep the Apex attribution while identifying modified builds as unofficial, or remove the Apex Marks and rebrand. The Apache licence does not require use of the Apex Marks.

The software is provided “AS IS”, without warranties, under the terms and limitations in the licence. Nothing in the licence excludes liability where applicable law does not permit it. Archivo remains under its own SIL OFL 1.1 licence in `apps/desktop/public/fonts/OFL.txt`; third-party assets and dependencies remain under their own licences and notices.

Current scope: `docs/planning/CURRENT-SCOPE.md`. Validation and limitations: [Phase 4](docs/phases/PHASE-04.md), [Phase 3](docs/phases/PHASE-03.md), [Phase 2](docs/phases/PHASE-02.md), [Phase 1](docs/phases/PHASE-01.md).

Asset-manager follow-up: see [changes and verification](docs/phases/PHASE-04-FIXES.md). Phase 5 development build: [features, verification and limitations](docs/phases/PHASE-05.md).

Roblox workflow: [setup, uploads, publishing handoff and verification](docs/phases/PHASE-05-ROBLOX.md).

Multiple-folder livery imports: [workflow and limits](docs/phases/LIVERY-FOLDER-IMPORT.md).
