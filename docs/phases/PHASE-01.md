# Phase 1 — foundation

2026-10-22. Fresh implementation authorised by the user following the spacious mockup revision. Scope is foundation, not the project-management vertical slice. Native verification completed after the user installed the C++ workload.

## Implemented

- Cargo workspace: core validation/use-case contracts, SQLite storage adapter, and Tauri 2 desktop adapter.
- React/TypeScript shell: dashboard, Projects, Notes, Assets, Uploads and Settings; responsive sidebar, keyboard labels, focus and reduced-motion support.
- Real empty states; browser preview explicitly disconnected from native storage. No localStorage fake persistence, mock projects, provider requests, credentials or telemetry.
- Local Archivo font/OFL, supplied Apex logo and generated application icons.
- Workspace creation under app-local data, transactional schema v1, WAL/foreign keys/busy timeout, newer-schema refusal, persisted workspace name command and UI.
- Managed-file foundation streams arbitrary regular files into immutable SHA-256 blobs; originals preserved and duplicate bytes deduplicated. No UI import yet. Crash recovery may need orphan-blob reconciliation in a later phase.
- Frontend tests and Rust storage tests; locked dependencies and CI checks defined. CI has not been run remotely.

## Validation

- `npm run build`: passed (TypeScript and production bundle).
- `npm test`: three interaction tests passed, including name-save result and failed-save feedback. These mock the IPC boundary; they do not establish native persistence.
- `npm audit`: zero known vulnerabilities after updating the initial test-tool dependencies.
- `cargo test --locked`: all four core/storage tests passed, including settings reopen, file preservation/deduplication, and newer-schema refusal. The first native compile exposed unsupported unsigned SQLite conversions, corrected with checked i64/u64 conversions.
- `cargo check --locked -p apex-desktop`: passed. Core/storage Clippy with warnings denied passed. Rust formatting passed.
- `cargo build --locked -p apex-desktop --features custom-protocol`: passed. A distinct library name avoids the binary/library Windows PDB collision. The custom-protocol feature embeds the production frontend so the executable does not need Vite.
- Native debug executable launched; the real Settings form saved 'Apex Designs', displayed success and updated the header. Closed the window and reopened the executable; the header still displayed 'Apex Designs'. Screenshot: phase-01-native.png. The local workspace remains available; no project/sample data was inserted.
- No signed installer or release build produced. This is a verified development foundation, not a feature-complete app.
- Browser preview inspected at desktop (1280×900) and narrow viewport; navigation and layout checked. Fixed icon-only navigation accessible labels after inspection.
- Impeccable detector returned no findings for the shell files.

## Machine setup and blocker

Rust 1.99.1, rustfmt and clippy were installed using the official rustup installer without changing PATH. Node 26.12.0 was already installed. Add `.cargo/bin` to a terminal PATH to use cargo normally.

Initial blocker (resolved): automatic approval review rejected the official Build Tools install command with “blocked by policy”; no alternate route was attempted. The user installed the Desktop development with C++ workload. Visual Studio 18 BuildTools is now detected and both compilation and native restart persistence were verified. WebView2 was already installed. Platform prerequisites: https://v2.tauri.app/start/prerequisites/ .

To build a self-contained development executable: `npm run build`, then `cargo build --locked -p apex-desktop --features custom-protocol`. Launch `target/debug/apex-desktop.exe`. For hot reload use `npm run desktop`. The OS-reported local-data path is displayed in Settings; when launched by packaged host tools Windows may redirect it into the host's local package cache. Do not hardcode this machine's reported path.

## Later work

Projects/notes/asset UI, vehicle catalogue, backup/restore, packages, Roblox and extension work are not implemented. The settings page clearly labels backup as future work. Workspace path selection and multi-workspace switching are not part of this initial fixed-workspace foundation.

No user database was migrated from the old implementation. Existing unrelated deleted files remain deleted; fresh implementations deliberately replace matching source paths. Old history includes tracked node_modules files; new ignore rules prevent new dependency additions but do not untrack legacy entries automatically. No commits or staging changes were made.
