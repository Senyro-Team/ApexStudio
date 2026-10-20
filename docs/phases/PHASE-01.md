# Phase 01 – Repository, Toolchain und modularer Core

## Scope and status

This phase establishes the repository layout and a minimal real project workflow. Product and data contracts derive from the supplied Phase 00/01 packet dated 2026-10-19. The package is a plan; no pre-existing application behavior or test results are assumed.

Implemented source scope: Cargo workspace; Rust domain and application service; temporary in-memory project repository; standalone CLI; Tauri 2 + React + TypeScript shell with a narrow command bridge; versioned DTO fields; minimal capabilities; pinned toolchain/package manager; CI definitions.

## Data and API contracts

- Project IDs are UUIDv7 strings; timestamps are UTC RFC 3339; revision starts at 1.
- Local calendar dates are represented separately from UTC instants; file references use normalized relative paths.
- Schema, IPC, event, and hash algorithm versions have separate Rust newtypes.
- Project names are trimmed and limited to 120 Unicode scalar values; control characters and empty names are rejected in Rust.
- Roblox IDs have a dedicated string value type and are never represented as JavaScript numbers.
- `CreateProject` and `ListProjects` share `apex-core` use cases across CLI and Tauri.
- The in-memory repository is temporary and is recreated for each process. Nothing in this phase claims durable persistence.
- Tauri exposes only `create_project` and `list_projects`; frontend calls use the typed `ProjectDto` and error shapes.
- Version fields are distinct for schema, IPC, event, and hashing contracts. The common event envelope is reserved for later event-producing workflows.

## Toolchain recorded in repository

- Rust toolchain: 1.96.0, edition 2024 (minimum declared Rust version: 1.90)
- Node.js: 24 (local machine reported 24.x)
- pnpm: 12.8.1
- React 19.3.1, TypeScript 7.0.2, Vite 8.3.4
- Tauri JS API/CLI: 2.12.4 / 2.12.3
- Tauri Rust crate: 2.12.4; tauri-build: 2.7.3

Frontend package versions were queried from the npm registry on 2026-10-19 and are pinned exactly. Rust crate constraints are pinned by `Cargo.lock` once generated.

## Validation record

Verified on 2026-10-19 with Rust 1.96.0 / Node 24.19.0 / pnpm 12.8.1:

- `pnpm install --frozen-lockfile` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo test --workspace --locked` — passed; 6 behavior tests passed, remaining placeholder crates had no tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- `pnpm typecheck` and `pnpm build` — passed.
- CLI smoke: `apex --help`, `project create`, and `project list --json` all exited successfully. A fresh `project list` returned `[]`, confirming per-process temporary storage.

An initial workspace test exposed a missing Windows icon resource. A simple source SVG and generated platform icons were added; the subsequent full workspace test passed. A first TypeScript run also caught the CSS module declaration gap; adding the Vite declaration made the typecheck pass. These were corrected before accepting Phase 1 as tested. A visible Tauri window/E2E interaction was not run in this environment. DTO drift generation/checking remains open.

## Known limitations / Phase 02 handoff

- Project data disappears when CLI exits or desktop closes. Phase 2 must replace the temporary adapter with SQLite without duplicating the application use cases.
- The CLI has no persistent cross-command storage yet; `project list --json` returns the current process's repository contents (empty in a fresh invocation).
- No `UnitOfWork`, BlobStore, SecretStore, production provider, or outbox implementation exists; ports are scaffolding only.
- No generated DTO drift check is configured yet; Rust DTOs and `apps/desktop/src/types.ts` are manually paired.
- Desktop bundle packaging is disabled pending branding-rights review and distribution phase.
- No end-to-end UI or adapter contract tests were added or run.

## Next phase entry conditions

Keep the use-case service as the single source for project creation/listing. Introduce schema/migrations, persistence and restart/transaction behavior in `apex-storage`; preserve the explicit temporary adapter only for isolated development if still useful. Add DTO generation or a checked schema-generation workflow before growing bridge contracts.
