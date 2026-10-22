# Phase 3 — notes, project packages and workspace backups

2026-10-22. Authorised by the user, including removal of filler copy and in-app development commentary. Preserve the approved spacious dashboard/project layout. Screenshots are delivered in chat for remote review.

## Implemented

- Project and general notes in a visual Tiptap editor: headings, bold/italic/strike, links, bullets, numbered/check lists, quotes, code blocks, dividers, tables, local PNG images and undo/redo. Formatting syntax is hidden; structured JSON is authoritative and a Markdown representation is saved alongside it.
- Explicit Save note, unsaved indicator, discard confirmation, navigation protection, conflict detection using optimistic revisions, and archive/restore. Archived projects/notes are read-only.
- Local image picker copies original PNG bytes into managed storage; note references use content hashes. External images are removed on HTML paste and rejected by backend validation. No remote image fetching. Images are limited to 8 MiB and 4096×4096 pixels.
- `.apexstd` project packages: metadata-only or embedded files, with an explicit opt-in for project notes. General notes and other projects are excluded. Imports show a preview and create a new project with new IDs; there is no overwrite/merge path. Metadata-only exports contain project details without assets or notes.
- ZIP archives contain a versioned manifest and SHA-256 blobs. Validation rejects traversal/unexpected names, duplicates, symlinks, undeclared/missing files, bad hashes/sizes, unsupported versions, oversized manifests and excessive entries. Visible imported records commit in one SQLite transaction. Orphan blobs after interrupted imports remain a documented cleanup task.
- Whole-workspace `.apexbackup` files include a consistent SQLite online backup and all registered managed files. Restore validates integrity, schema, foreign keys and blob presence, saves a complete before-restore backup under the workspace `backups/` directory, then restores through SQLite's backup API. Existing blobs are preserved; thumbnails are derived and regenerated where needed.
- Native file pickers stay in Rust. Archive selection is copied to an immutable temporary file and represented in the UI by an opaque token; frontend callers cannot supply import/restore paths.
- Schema 3 migration adds notes and image references while retaining prior workspace/project/asset records. Editor code is loaded on demand.
- Removed filler/taglines and phase/future-feature notices. Unimplemented upload/publishing entry points are hidden. Documentation retains the implementation status. Necessary validation, empty-state and task instructions remain.

## Validation

- `npm run build`: passed; editor is a separate lazy-loaded chunk. Vite reports a >500 kB editor-chunk advisory (about 169 kB compressed); no build failure.
- `npm test`: nine interaction tests passed, covering shell/project behavior, note save/failure/discard behavior, archive-import confirmation and notes excluded by default from exports.
- `cargo test --locked`: thirteen tests passed. Includes migration/reopen behavior, note conflicts, remote-image/script-link rejection, project packages with managed note images, metadata-only exclusion, restoration into a different workspace, safety-backup creation, traversal and corrupted-file rejection before records are inserted.
- Clippy across the workspace, all targets and custom-protocol, with warnings denied: passed. Rust formatting passed. Impeccable detector reported no findings.
- Embedded native Windows debug build verified. Native smoke-test details and screenshot filenames are recorded below after visual verification.

## Format and recovery limits

Archive version 1; schema version 3. Imports are limited to 2 GiB total decompressed content, 512 MiB per entry, 10,000 blobs, 1,000 notes and an 8 MiB manifest. Exports reject selections beyond supported limits rather than producing knowingly unimportable archives. A workspace database must fit the per-entry limit. Backups from other schema versions currently require the matching app version; automatic backup-version conversion is not implemented.

Notes are limited to 1 MB per representation, 10,000 nodes and nesting depth 32. Markdown is an interchange representation, not a guarantee of arbitrary HTML or lossless round-trips for every Markdown dialect. Task-list nesting and tables use the editor's supported structure. Raw Markdown file import/export and non-PNG note images are not exposed as separate tools yet. Note archive is reversible; permanent deletion is not provided.

Packages preserve content and use new local identities and creation timestamps. Importing repeated packages intentionally creates separate projects; explicit confirmation calls this out. Secrets are not stored by this app and no credentials are included. Archives are unencrypted. Restore preserves the previous complete workspace as an archive; no automatic retention deletion is configured. Do not treat copying a live SQLite file by itself as a backup.

No provider upload/post was performed. Roblox/ERLCX, complete verified vehicle capabilities, scheduled backups, package merging and signed installers remain outside this slice. Tests do not constitute exhaustive crash/power-loss testing or native cross-platform verification.

## Native verification

Created a general note through the real native editor, typed multiple paragraphs, applied Heading 2 through the style control, and saved. Rebuilt/reopened the app and verified title, paragraphs and heading persisted. The explicitly named `Phase 3 verification` note remains in the workspace as test evidence; it is not seeded application content.

The native backup file picker saved `.tools/phase3-smoke.apexbackup`. Independently inspected its ZIP manifest, every blob size/hash, database checksum, SQLite quick_check, schema 3 and the persisted note. Restore was exercised against isolated temporary workspaces by Rust tests; the user's active workspace was not replaced. Package file-dialog end-to-end testing and every editor formatting combination have not all been manually exercised.

Visual review removed a duplicate Notes heading. Native testing also caught the missing window-destroy capability required by Tauri's close-request listener; the main-window-only capability is now explicit. No filesystem, SQL, shell or network frontend permission was added.

Screenshots: `phase-03-dashboard.png`, `phase-03-notes.png`, `phase-03-backups.png`, captured from the native Windows build and posted inline in chat.

## Implementation references

- https://tiptap.dev/docs/editor/getting-started/install/react
- https://tiptap.dev/docs/editor/markdown/getting-started/installation
- https://docs.rs/rusqlite/latest/rusqlite/backup/index.html

Tiptap dependencies retain their own MIT notices; ZIP and SQLite dependencies retain their respective licences. Application source/brand licence finalisation remains outstanding.
