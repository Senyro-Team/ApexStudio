# Apex Studio

Local-first desktop workspace for ER:LC creators. Manage projects, notes, assets and livery publishing drafts in a spacious desktop interface. Core workspace features work offline; Roblox uploads require configured credentials. No telemetry is collected.

## Run

Requirements: Node.js 22.12+ (24 LTS recommended), Rust stable, and the Tauri platform prerequisites. Windows native builds need Microsoft C++ Build Tools with the Desktop development with C++ workload, Windows SDK, and WebView2. See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run desktop
```

For a frontend-only browser preview, use `npm run dev`. It clearly identifies browser mode and does **not** emulate SQLite persistence with browser storage. `npm run build`, `npm test`, `cargo test --locked`, and `cargo fmt --all --check` provide foundation checks. If Rust was installed without updating PATH, add the existing `.cargo/bin` directory to your shell PATH.

For a standalone development build with the frontend embedded:

```powershell
npm run build
cargo build --locked -p apex-desktop --features custom-protocol
.\target\debug\apex-desktop.exe
```

The native Windows build, 29 Rust tests, 35 frontend/extension tests, and project/file persistence across restart have been verified. This is a debug executable, not a signed release installer.

## Current behaviour

- Dashboard with recent projects, deadlines and project creation; searchable active/archived projects with edit, archive and restore.
- Project names, descriptions, tags, multiple vehicle suggestions, reference links, automatic creation dates and optional deadlines.
- Assets-first project detail, file search, PNG thumbnails, a selection-only inspector and native Save a copy.
- Native multi-file import accepts any file type, copies into managed storage and preserves originals. Repeated imports with the same project/name/bytes are skipped; shared bytes are deduplicated.
- Workspace asset library links files back to their owning projects.
- One workspace created under the OS local app-data directory for `studio.apexdesigns.apexstudio`, in `workspace/`.
- Transactional SQLite migration from earlier versions to version 6, WAL, foreign keys, bounded busy timeout and durable writes.
- Rename the workspace in Settings; it persists across desktop restarts.
- Shared Rust core and SQLite adapter; narrow Tauri commands, no frontend filesystem/SQL/network permissions.
- Official supplied template catalogue with team variants, body/window statuses and slot information. Snapshot dated October 7, 2026; unspecified statuses stay unspecified.
- Nested asset folders, livery import review, filename slot suggestions, covers and recoverable texture versions.
- Visual general/project notes with local PNG images, formatting, tables, explicit saving and revision conflict protection.
- Versioned .apexstd export/import with content validation and import-as-copy confirmation; notes excluded unless selected.
- Whole-workspace .apexbackup export and validated restore with an automatic before-restore backup.
- Local brand/font assets, concise English copy, keyboard focus and responsive shell.

Prepare project publishing drafts, select and order PNG previews, use saved Roblox decal IDs or enter existing IDs manually, and export a public-only `.apexpublish` bundle. The Chrome assistant fills the current ERLCX livery step on request. Build it with `npm run build:extension`; see [manual installation and privacy](apps/erlcx-extension/README.md). Live Media/Codes autofill still needs manual Chrome verification. Personal-account Roblox decal uploads are available from livery folders after configuring a key in Settings. No signed installer is produced.

## Architecture

`crates/apex-core`: shared contracts, validation and use cases. `crates/apex-storage`: SQLite and immutable file storage. `apps/desktop`: React shell and Tauri adapter. Managed files are published only after their contents are written; a crash before database insertion can leave an unreferenced blob, never a recorded partial file. Reconciliation/cleanup is future work. Do not manually delete managed blobs.

Schema upgrades are transactional; newer schemas are rejected. The app never opens the earlier implementation's data location automatically. Use the built-in backup/restore tools for database and files together; copying a live SQLite file alone is not a supported backup.

## Rights and assets

Application licence is still being finalised; no MIT/open-source grant is implied. Apex branding is supplied for in-app use only. Archivo remains under its own SIL OFL 1.1 licence in `apps/desktop/public/fonts/OFL.txt`. Third-party dependencies retain their licences. Do not publish a release before code and brand terms are finalised.

Current scope: `docs/planning/CURRENT-SCOPE.md`. Validation and limitations: [Phase 4](docs/phases/PHASE-04.md), [Phase 3](docs/phases/PHASE-03.md), [Phase 2](docs/phases/PHASE-02.md), [Phase 1](docs/phases/PHASE-01.md).

Asset-manager follow-up: see [changes and verification](docs/phases/PHASE-04-FIXES.md). Phase 5 development build: [features, verification and limitations](docs/phases/PHASE-05.md).

Roblox workflow: [setup, uploads, publishing handoff and verification](docs/phases/PHASE-05-ROBLOX.md).
