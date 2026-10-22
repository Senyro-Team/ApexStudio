# Phase 4 — organised assets and livery imports

Implemented 2026-10-22 after user authorisation. Scope follows docs/planning/PHASE-04-PLAN.md and the confirmed filename/folder/version decisions.

## Features

- Nested project folders with breadcrumbs, scoped search, create, rename and explicit move controls. Moves affect logical organisation only. Cycle and cross-project checks protect hierarchy/ownership. No permanent deletion was added.
- Special Livery folders hold one official catalogue vehicle/team variant, a user-defined name, cover and slot assignments. Multiple liveries for the same vehicle are supported. A livery cannot contain another folder/livery.
- Import livery selects up to 100 PNG files through a native picker, copies them into a temporary snapshot, then shows a review screen before committing. Review includes vehicle search, body/window status, source notes, per-file slot correction, duplicate/unsupported-slot feedback and optional cover selection. Cancelling leaves no project/folder/asset records.
- Filename parsing is case-insensitive, uses token boundaries and accepts joined side-number forms and aliases. Left/Lft/Lf, Right/Rgt/Rght/Rt, Front/Frt/Frnt, Back/Rear/Bk and Top/Roof are supported. Window/Windows/Win/Wnd prefixes select window slots. The first following number is used; numbers before the side and later suffix numbers are not substituted. Ambiguous or unnumbered names remain unassigned.
- Updating an existing livery explicitly replaces matching assigned slots while retaining old texture versions. The inspector lists versions and can restore previous bytes. Unassigned incoming files are new entries; matching filenames alone do not overwrite assets.
- PNG covers can be chosen from reviewed textures or separately through Set cover. Covers are metadata, not automatically added to a texture slot. Regular folders also support covers.
- Managed file copies preserve original bytes and sources. Plain file imports accept all regular file types. Livery imports validate PNG decoding with local import limits of 20 MiB and 8000×8000; these are local limits, not claims about current provider upload requirements.
- Global asset links navigate to the owning project folder and open file details.

## Official catalogue

`catalogue/official-vehicles.json` is derived from the user-supplied Livery Templates Status.xlsx, last updated October 7, 2026. It contains 120 distinct team/name entries from the union of the body/window sheets. This is not a claim of 120 distinct vehicle models. Exact statuses, supported slot columns, vehicle comments and source-row coordinates are retained. Blank status is unspecified; In Progress is not automatically unsupported. The body/window Chevlon Camion naming discrepancy is kept separate rather than guessed.

`scripts/extract_catalogue.py` reads only Vehicle Templates and Window Templates and leaves the workbook unchanged. Source SHA-256 is recorded and verified. Community-report/user/contact data is not bundled. Confirm source distribution/attribution terms before public release. The old wiki names remain only for compatibility when editing/importing existing project metadata; UI suggestions use the official source.

## Persistence and portability

- Schema 4 adds folders, placements and versions and removes the old project/name/hash uniqueness constraint so identical texture bytes can be represented in multiple liveries without collisions. Physical bytes remain deduplicated.
- Existing workspaces receive a complete before-schema-4 backup before the asset migration. Existing records retain identities. The live workspace migration and safety archive were verified.
- Project package version 2 includes folders, covers, assignments and all retained texture versions when files are included. Version-1 packages still import as flat assets. Metadata-only exports preserve folder/livery metadata but omit files, covers, placements, notes and versions. Imports use new IDs and reconstruct parent relationships transactionally.
- Archive validation checks IDs, ownership references, hierarchy/cycles, cover/version blob references, duplicate slots and size limits as well as existing path/hash validation.
- Backups retain consistent SQLite plus managed bytes. Schema-3 backups can be validated and migrated in staging before restore; schema-4 backups restore with folder/version state. Before-restore snapshots remain enabled. Newer schemas are refused.

## Note-editor fixes

- Contextual table controls now render directly below the formatting toolbar, before the editor content. The old position after the editor's minimum-height region caused the gap in the supplied screenshot. Controls preserve selection on mouse-down and use consistent sizing/spacing.
- Enter in a heading splits into a paragraph using one ProseMirror transaction. Other block types retain their normal Enter behavior. An initial chained-command implementation exposed a position-mapping error in the regression test and was replaced before delivery.

## Verification

- `cargo test --locked`: 18 tests passed. New tests exercise aliases/first-number parsing, folder cycles and cross-project rejection, two liveries for one vehicle, version recovery, atomic rejection of duplicate-slot batches, structured package round-trips with covers/history, schema-3 migration/safety backup and backup restore. Existing storage/note/archive tests also pass.
- `npm test`: 12 tests passed. New cases cover duplicate-slot correction before commit, native-file assignment order, folder creation and real Enter-key dispatch producing a paragraph after a heading.
- TypeScript/production bundle build passed. Clippy for workspace/all targets/custom-protocol with warnings denied passed. Rust formatting passed. Impeccable detector returned no findings.
- Native Windows: saved the user's previously unsaved verification note/table before rebuilding; opened and migrated the real workspace; created a verification project and Liveries folder; imported LEFT 1 7 and RGT 1 9 images, selected an official vehicle and cover, and confirmed Left 1 / Right 1 assignments.
- Native package export wrote `.tools/phase4-native.apexstd`. Independently inspected its version-2 manifest: two folders, two assets, two placements, two version records and a cover reference. Native import preview and confirmation created a separate project with remapped folders/assets. The two named Phase 4 verification projects are test evidence, not seeded app content.
- Live SQLite quick_check returned ok; foreign_key_check returned no rows; every managed-file hash matched. The before-schema-4 archive exists.
- Native note verification: the table controls appeared beside the toolbar; Add row increased the table from three to four rows; Enter after the heading showed Paragraph. Undid only the temporary test edits, saved, and compared the note document against the pre-migration snapshot: exact structural equality, preserving the user's table.
- Final app is launched for local review. Native version restore and schema-3 backup restore are covered by automated tests; not every formatting combination, alias or failure/power-loss scenario has been manually exercised.

## Boundaries

No provider upload/post, extension, Windows file association/registry change, signed installer, CSV tools or permanent deletion. Current preview caching is bounded to PNGs up to 8 MiB and 16 million pixels; larger valid imported textures are retained but may show generic previews. Folder moves and imports are local; a cover is not automatically published. Version history is included with embedded project exports; selective version-history export is not yet exposed. Lists currently load whole collections. Automatic orphan-blob cleanup and simultaneous multi-process edit coordination remain future work.

Screenshots: phase-04-table-controls.png documents the repaired note controls; phase-04-livery-folders.png shows the imported livery folder and cover after restart. Native project/livery and package flows were visually inspected directly. No staging or commit was performed; unrelated historical deletions remain untouched.
