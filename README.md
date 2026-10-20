# Apex Studio

Apex Studio is a local-first toolkit for Roblox / ER:LC livery workflows. The companion phase package is a specification, not evidence of completed features.

## Current implementation

Phases 1 and 2 have working foundations, Phase 3 package export/validation is in progress, and Phase 4 local image processing has started. Phases 5–8 now have partial implementation with phase-by-phase reports; none is claimed as fully accepted. The Rust workspace provides validated domain values and shared application services, while the CLI and Tauri desktop shell use the same SQLite-backed project and commission repositories. Project descriptions/tags, commission records, revision history, a local outbox, search, reversible project archiving, and SQLite backup are implemented. Commission records preserve the broader workflow, design, Roblox identifier, hash, payment, and licensing metadata in the versioned SQLite schema and JSON payload.

The current desktop view supports project creation/edit/archive and commission creation/listing; the Tauri bridge also offers filename slot suggestions. The CLI supports project and commission create/list plus project update/archive, `.apexstd` export, import dry-run validation, and local image inspect/import. Phase 5 adds a Roblox Open Cloud adapter and OS keyring port, but account setup is not wired to the UI/CLI; no credentials or live upload are used in tests. Phase 6 adds a durable queue intent/lease core with unknown-outcome protection, but no worker/retry UI. Phase 7 adds separate Image/Decal domain types and persistent preferences, but no identifier CRUD/export or UI/CLI workflow. Phase 8 adds the desktop shell navigation and honest empty states for future work areas. See `docs/phases/PHASE-05.md` through `PHASE-08.md` for acceptance evidence and remaining work. It stores data under the operating system local application-data directory in `Apex Studio/apex-studio.sqlite3` and `Apex Studio/blobs` (on Windows, under `%LOCALAPPDATA%`). There is no account, server, or network dependency for local workflows.

Phase 2 is not yet fully accepted. Commission editing/deletion are exposed as Rust/Tauri commands but do not yet have desktop/CLI workflows; create forms expose only part of the full metadata model; migration-upgrade/rollback testing and a visible desktop E2E run remain open. Phase 3 currently exports metadata and selected caller-supplied files, validates and stages imports, and offers CLI dry-run. A database/file-store import commit and consistent database snapshot are still open. Phase 4 currently provides a local PNG core, parser, hashes, and content-addressed blob store with CLI entry points. Desktop image import, batch processing, and SQLite asset records remain open; see `docs/phases/PHASE-04.md`. Phases 5–8 are partial and independently documented in `docs/phases/PHASE-05.md` to `PHASE-08.md`.

The frontend invokes only explicitly registered commands. It has no direct filesystem, shell, or SQL access. No provider client or telemetry is initialized by default.

## Requirements

- Windows 10/11 with Microsoft C++ Build Tools and WebView2 Runtime for the desktop shell
- Rust 1.96.0 (`rustup` installs the pinned toolchain from `rust-toolchain.toml`)
- Node.js 22–24 and pnpm 12.8.1

## Start

```powershell
pnpm install --frozen-lockfile
pnpm dev
```

For the desktop-independent command line:

```powershell
cargo run --locked -p apex-cli -- --help
cargo run --locked -p apex-cli -- project create "Mein Projekt"
cargo run --locked -p apex-cli -- project list --json
cargo run --locked -p apex-cli -- commission create <PROJECT_UUID> "Auftragsname"
cargo run --locked -p apex-cli -- commission list <PROJECT_UUID> --json
cargo run --locked -p apex-cli -- project export <PROJECT_UUID> "Kunde.apexstd" --profile client-share
cargo run --locked -p apex-cli -- project import "Kunde.apexstd" --dry-run
```

## Workspace map

- `crates/apex-domain`: validated domain values and stable error contract
- `crates/apex-core`: application services and repository ports
- `crates/apex-storage`: SQLite and development adapters
- `crates/apex-images`: local PNG validation, filename parser, hashes, and blob storage
- `crates/apex-package`, `apex-roblox`: bounded extension crates for later phases
- `crates/apex-cli`: standalone CLI, independent of Tauri and WebView
- `apps/desktop`: React UI and the narrow Tauri bridge
- `docs/phases/PHASE-01.md`: Phase 1 scope and validation evidence
- `docs/phases/PHASE-02.md`: Phase 2 implementation report and remaining acceptance work
- `docs/phases/PHASE-03.md`: Phase 3 package implementation report and remaining acceptance work
- `docs/phases/PHASE-04.md`: Phase 4 image implementation report and remaining acceptance work
- `docs/phases/PHASE-05.md` to `docs/phases/PHASE-08.md`: implementation reports and per-phase acceptance status
- `docs/providers/ROBLOX.md`: Roblox Open Cloud endpoints, limits, and security behavior
- `docs/design/BRAND.md`: brand observations, working design tokens, asset rights and accessibility gaps
- `docs/adr/ADR-0002-upload-and-identifier-schema-v2.md`: database and identifier contract decision
- `docs/packages/FORMAT.md`: `.apexstd` v1 contract and import/export limits

## Toolchain and reproducibility

Rust is pinned in `rust-toolchain.toml`; Rust dependencies are resolved in `Cargo.lock`. Node dependencies use the `pnpm@12.8.1` package-manager pin and `pnpm-lock.yaml`. Dependency versions are recorded in manifests and lockfiles, not floating `latest` tags.
