# Apex Studio — implementation plan

> Historical plan: CURRENT-SCOPE.md records the user's reduced six-module release and licensing requirements. Conflicting commission, analytics, CLI/TUI and MIT assumptions below are not implementation instructions. Coding has since been authorised; current implemented scope and evidence are recorded in docs/phases/PHASE-01.md and PHASE-02.md.

Status: planning baseline, 2026-10-21. No milestone is claimed implemented.

## Confirmed decisions

- Start fresh. Preserve the existing working-tree deletions; do not restore the old implementation or generated dependencies.
- Plan the complete product before implementation.
- Windows first; Tauri 2, React, TypeScript, Rust, SQLite, Clap and Ratatui remain the architectural baseline.
- English (en-US) throughout UI, CLI, installer, documentation, errors, templates, and source identifiers. User-authored content may use any language.
- Local-first, no Apex account, no telemetry by default, optional external connectors.
- Chrome extension fills the current ERLCX step on explicit request. The user clicks Continue and Publish.
- Public ERLCX post metrics were verified without authentication on both pages. Use a separate optional Rust public-profile adapter; the extension is needed for form assistance, not public analytics. Provider support/rate limits remain to be clarified.
- Commission and project are separate entities. One commission may contain many projects; standalone projects are supported.
- Original concept is preserved in ORIGINAL-CONCEPT.md. This plan clarifies delivery order and integration boundaries.

## Product boundaries

The desktop app owns local records, files, preparation, and analytics history. The shared Rust core owns validation and business rules. The CLI and TUI invoke the same services. The extension assists a signed-in browser session; it does not become an independent project database. Optional sync and AI are separately enabled capabilities.

Proposed code layout:

```text
apps/desktop/                 React UI and narrow Tauri command bridge
apps/extension/               Chrome Manifest V3, TypeScript, ERLCX adapters
crates/apex-domain/           Types, invariants, identifiers, error contract
crates/apex-core/             Use cases, repository/provider interfaces
crates/apex-storage/          SQLite migrations, repositories, blob store
crates/apex-package/          .apexstd reader/writer and import transactions
crates/apex-images/           PNG validation, classification and hashing
crates/apex-roblox/           Open Cloud adapter and upload operations
crates/apex-cli/              Clap commands and Ratatui interface
contracts/                   Versioned JSON schemas and fixtures
docs/                        Architecture, decisions, release evidence
```

No SQL, unrestricted filesystem access, provider credentials, or business rules in React. External integrations implement ports behind feature flags. The app must remain usable with every connector disabled.

## Data and persistence

Use UUIDv7 technical IDs, separate configurable commission numbers, UTC ISO 8601 timestamps, integer money amounts with explicit currency, and string provider IDs. Never derive Image IDs from Decal IDs. Persist migrations and schema versions; back up before upgrades.

Primary entities:

| Entity | Required responsibilities |
| --- | --- |
| Workspace/settings | Locale policy, studio identity, commission numbering, ID preference, paths, connector options |
| Client/organisation | Private contact records, references from commissions |
| Commission | All customer, requirements, workflow, assignment, revisions, price, payment and licence fields from the concept |
| Project | Optional commission ID, title, description, state, priority, dates, requirements, design type, Figma URL, licence overrides |
| Vehicle/template/slot | Vehicle catalogue, aliases, template version, allowed sides and numbered slots |
| Asset/version | Original file, hashes, dimensions, side/slot, version lineage, project, independent typed Roblox IDs |
| Note/attachment | Markdown source, local relative links, attachments and project references |
| Upload job/attempt | Account reference, file hash, idempotency evidence, state, lease, operation reference, result and redacted errors |
| Publication draft | Explicitly selected public metadata, category-specific code rows, media references, target author/group |
| External post link | Provider, account scope, bundle ID, related project(s), title and last successful import |
| Metric snapshot | Account scope, bundle ID, observedAt, nullable counters/ratings, source and import identity |
| Activity | Local user actions, recoverable archive operations and connector outcomes; no secrets |

SQLite is the source of truth. Large files live in a content-addressed local blob store. Use atomic file writes and database transactions; staged imports must recover cleanly after interruption. Never silently delete source files. Archived projects remain recoverable; permanent removal requires explicit confirmation.

## Desktop experience

Navigation: Dashboard, Projects, Commissions, Asset Library, Notes, Uploads, Analytics, Publish, Settings. Dashboard uses real local data and clear empty states. Dark-first tokens with an accessible light theme; keyboard navigation, visible focus, readable contrast, responsive panels and reduced-motion support.

Project detail contains Overview, Assets, Notes, Commission, Activity and Publishing. Commission detail exposes full metadata in sections, with private client/payment fields visibly identified. Asset tables support filtering, side ambiguity resolution, history, typed ID copying and CSV export. Upload queues show actionable failures, attempts and unknown outcomes. Analytics shows freshness and gaps, never fabricated zeroes.

## Delivery sequence and acceptance gates

Each slice updates documentation, includes meaningful tests, and preserves prior data. A slice is complete only after its visible workflow and persistence are exercised.

| Slice | Deliverable | Acceptance gate |
| --- | --- | --- |
| 0 — Plan | Architecture, provider research, contracts, decision register | Requirements mapped; assumptions distinguished from observations |
| 1 — Foundation | Rust workspace, Tauri shell, React, SQLite, settings, errors, CI | Clean checkout builds; core tests pass; app opens and persists a setting offline |
| 2 — Projects/commissions | Full entity models, client/org records, CRUD, search, filters, archives | Multiple projects per commission; standalone project; restart persistence; validation and migration tests |
| 3 — Notes/packages | Markdown editor, preview, images, attachments, .apexstd export and import | Round-trip, privacy selection, conflict resolution, traversal/size rejection, interrupted import recovery |
| 4 — Assets | Templates, aliases, slots, files, versions, typed IDs, CSV | Ambiguous classification is reviewed; malformed CSV handled; private fields excluded from public export |
| 5 — Publishing preparation/extension | Versioned publishing JSON, Chrome package, current-step assistance | Local fixture tests for every supported step; no Continue/Save Draft/Publish clicks; real-site verification on disposable data |
| 6 — Analytics | Snapshot imports, trends, extension-assisted collection where permitted | Complete pagination, account isolation, nullable metrics, duplicate import handling, stale-state and partial-failure display |
| 7 — Roblox uploads | Accounts, PNG validation/hashes, Open Cloud adapter, durable queue | Verified current API constraints; mocked failure/restart tests; explicit live test with authorised account; no inferred IDs |
| 8 — CLI/TUI | Project, commission, assets, packages, queue, settings and analytics commands | Same core validation as desktop; JSON output; documented exit codes; keyboard TUI smoke test |
| 9 — Distribution | Windows installer, associations, optional CLI PATH, signing/update strategy | Install/upgrade/uninstall checks; user data preserved; release checksums and notices |
| 10 — Optional sync | Self-hostable service, membership, roles, change log, conflict UI | Offline reconciliation, access isolation, revocation and backup/restore tests |
| 11 — Optional AI | User-provided credentials, selected-data preview, explainable insights | No silent transfer; deterministic statistics remain available; uncertainty and insufficient data visible |

Bring publishing preparation and analytics imports forward from the original roadmap because they are now explicit priorities. Continuous ERLCX API integration remains conditional on permitted access; local files and user-driven workflows can proceed independently.

## Portable formats

`.apexstd` is a versioned ZIP containing manifest, project data, optional commission data, Markdown, asset mappings, checksums and optional files. Full and metadata-only modes. Default exports exclude client contacts, payments, internal notes and secrets. Export preview lists all included private sections; inclusion is explicit.

Reject absolute/traversal paths, symlinks, duplicate paths, excessive expanded size/file counts, unsupported versions and checksum mismatches. Stage imports before commit. On UUID conflict offer cancel, explicit update, or import-as-copy with remapped internal references. Never overwrite silently.

Use a separate `.apex-publish.json` contract for extension handoff and `.apex-metrics.json` for snapshots. Publishing files contain only fields deliberately selected for ERLCX. Initial handoff uses files, avoiding a local HTTP listener or broad native integration. Native messaging can be evaluated after standalone workflows are stable.

## Upload rules

Supported sides: Left, Right, Front, Back, Rear, Top, WindowLeft, WindowRight, WindowBack, WindowTop, FrontPlate. User-defined aliases and numbered slots are supported; ambiguous matches require review.

Validate PNG decoding, dimensions and configurable size (initially 20 MB). Verify Roblox's current constraints, including the exact 8000-pixel boundary, against official documentation before implementing. Preserve SHA-256 original bytes, canonical decoded-pixel SHA-256 with a documented normalisation algorithm, and versioned pHash. Uploaded names use APXSTD-{HASH} with a documented length policy.

Queue states: prepared, queued, uploading, polling, succeeded, failed, cancelled, unknown. An ambiguous request outcome must be reconciled before retrying creation. Store returned provider operations and typed identifiers; do not fabricate missing values. OS credential storage holds secrets; SQLite stores references only. Hashes are matching evidence, not proof of ownership.

## Testing and release discipline

- Rust unit tests for invariants; SQLite migration/integration tests and package adversarial fixtures.
- Frontend tests for critical validation, private-export selection and error recovery; desktop smoke tests for persistence and navigation.
- Extension DOM fixtures and integration tests for rerenders, duplicate labels, unsupported layouts, existing values, category rows and missing files.
- Provider mocks for expiry, rate limits, timeouts, schema drift, pagination and partial results. Live tests are opt-in and never part of ordinary CI.
- Locked dependencies, formatting, linting, type checking and reproducible release builds. Keep generated dependencies/build outputs out of Git.
- Windows first; core/CLI CI on Linux and macOS as practical. Do not claim native support until exercised.
- Release notes state implemented scope and known limitations. No calendar commitments until the first vertical slice is measured.

## Decisions still to settle

Recommended defaults below permit planning without inventing user choices:

1. Code licence: MIT; branding remains separately licensed. Confirm before public release.
2. Branding: create a temporary text wordmark/tokens until authorised logo/font files are supplied.
3. Database encryption: rely initially on OS access protections and OS secret storage; document that SQLite/exports are not encrypted. Consider encrypted archives separately.
4. Backup: manual backup/restore in alpha; later configurable local scheduled backups with retention.
5. ERLCX scope: first support liveries end-to-end, then uniforms, ELS, map templates and UGC using separately verified adapters. “Every form” means the supported ERLCX publishing wizard, not arbitrary sites.
6. Analytics: manual snapshot import and collection first; scheduled polling only after permission/authentication/rate limits are established.
7. Full metadata UI, package import commit and reliable upload reconciliation are release gates, not deferred hidden gaps.

## Build entry point

After the planning review, implement Slice 1 from a clean authored structure, then complete one vertical project/commission workflow before expanding. Existing Git history remains untouched; the old implementation is not restored. Rust tools are currently absent from the shell PATH; check installed toolchain locations before choosing installation steps. Node/npm are available.

References: [Tauri 2 architecture and start](https://v2.tauri.app/start/), [Chrome activeTab](https://developer.chrome.com/docs/extensions/develop/concepts/activeTab). Provider-specific observations are in ERLCX-RESEARCH.md and EXTENSION-SPEC.md.

## Before implementation — decision checklist

Only decisions affecting the first data model or vertical slice need resolution now. Branding polish, update hosting, sync deployment and AI model/provider choices can wait for their slices.

| Topic | Proposed starting decision | Why it matters |
| --- | --- | --- |
| First usable slice | Create/edit a commission with two projects, notes and assets; restart and recover records; export/import a private-data-controlled package | Establishes a measurable alpha instead of an empty navigation shell |
| Commission numbering | Unique per workspace; configurable display pattern; transactional counter allocation; UUID is authoritative | Prevents collisions and later changes from breaking references |
| Workflow | Separate configurable commission/project states with stable internal keys; completion/archive timestamps independent | A delivered project and unpaid commission may coexist |
| Money | Tracking only; configurable currency and integer minor units; manual payment records; no payment processing/accounting claims | Avoids accidental financial automation and floating-point rounding |
| File ownership | Copy imported source files into managed storage by default, preserve originals; offer explicit linked files later | Defines portability, deletion and missing-file behaviour |
| Workspaces | One active workspace at a time, distinct directory/database; configurable location; no concurrent cloud-drive database sync | Isolates studio data and provides a clear backup boundary |
| Backup/restore | Consistent SQLite-and-blob backup; preview restore and back up current state before replacement | Protects the complete workspace, not just database rows |
| Privacy | Client/payment/internal data excluded from publishing exports; explicit selection for project packages | Prevents private commission details entering public descriptions |
| Analytics attribution | Track profile scope and post ID; keep uploader user/group metadata; allow manual project links and exclude duplicate posts from totals | Group and personal portfolios can overlap |
| Analytics frequency | Manual refresh initially; display observed time, scope and completeness | Avoids promising unavailable historical data or an unverified polling contract |
| Extension scope | Liveries first; fill the current step only; reviewed overwrite and explicit media selection | All category-specific form schemas are not yet verified |
| Design | A small reviewed dashboard + commission/project detail prototype before broad UI implementation | Resolves information density and daily workflow early |
| Licence | MIT proposed for code; separately identified brand assets and third-party notices | Needed before public distribution; not required to prototype privately |

Technical preflight: locate/install Rust and Windows C++ prerequisites, select supported/pinned runtime versions, initialise clean authored manifests and ignore rules, define migration/error contracts, and make a reproducible build before feature work. Keep the old tracked deletions intact unless a specific path is deliberately replaced by fresh implementation.

Before provider work: verify current Roblox upload dimensions, supported types, account/group ownership, key scopes, quotas, operation semantics and identifier availability against official documentation. For ERLCX, clarify supported access/rate limits and inspect the remaining publishing steps. Neither provider's unfinished adapter should block the local project-management slice.
