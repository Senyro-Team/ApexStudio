# Phase 4 — asset organisation and vehicle-specific liveries

Phase 4 implementation authorised and completed 2026-10-22; see ../phases/PHASE-04.md for implemented scope and verification. This document preserves the agreed planning baseline. This plan replaces the broad historical Phase 4 list in PRODUCT-PLAN.md for discussion; the decisions below are confirmed. The existing Phase 3 app is to be started at the end of each implementation/review pass so the user can review it locally. Screenshots remain useful when requested.

## Proposed scope

1. Complete the outstanding native package-dialog walkthrough before changing portable asset metadata.
2. Replace wiki-based suggestions with a versioned catalogue derived from the user-supplied official Livery Templates Status workbook. Keep vehicle/team variants, body and window slots, exact statuses, notes and source provenance.
3. Nested project asset folders: create, rename, navigate with breadcrumbs, move files/folders and search. Prevent cycles and cross-project ownership errors. Folder operations change logical organisation, not source files or content-addressed blobs.
4. A special Livery folder type holding one selected vehicle variant and a user-authored livery name, multiple texture assets and a separate optional cover image. Multiple liveries of the same vehicle must be representable.
5. An Import livery action alongside Import files. Select multiple PNG textures, choose the catalogue vehicle/team variant, name the livery, review suggested slots and choose an optional cover, then import as one logical batch into its livery folder. Nothing is uploaded to Roblox or ERLCX by this local import action.
6. Body/window slot assignments based on the supplied catalogue. Suggest from filenames/aliases, preserve ambiguity, and let the user correct assignments. Never silently guess between slots such as Left 1 / Left 2 or body / window. Missing, duplicate and unavailable slots receive task-specific feedback. Template refresh status is not itself a hard import ban.
7. Cover-image cards for liveries, file counts, clear folder/livery visual distinction, folder-scoped searches and ordinary navigation. The cover is managed separately from upload textures so it is never silently uploaded as a texture. Existing imported images may be selected, or a separate cover copied into managed storage.
8. Preserve prior texture versions when replacing a slot, if confirmed by the user. Use explicit replacement/revision actions rather than silently replacing files based only on a matching filename. Source files and previous managed bytes remain intact.
9. Extend packages and backups to include folder hierarchy, livery metadata, covers, slots and any included version history. Retain import support for existing version-1 projects as flat assets. Snapshot before schema migration, and support safe reading/migration of Phase 3 backups rather than stranding existing backups.

Excluded unless separately requested: actual provider uploads, extension automation, commissions/payments, analytics, a full template editing/downloading service, CSV tools, and a general workflow module. Public Roblox IDs can be introduced with upload integration; they need not expand this asset-organisation slice.

## Folder/livery structure — proposal

Example logical hierarchy:

    Project assets
      References/
      Source files/
      Liveries/
        Highway Patrol — Falcon Interceptor Utility 2024  [Livery]
          Left1.png    [Body / Left 1]
          Right1.png   [Body / Right 1]
          WindowBack1.png [Window / Back 1]

A regular folder groups assets. A Livery folder adds a canonical vehicle/team reference, a user-controlled display name, cover and typed texture slots. Do not use the visible folder name as identity. Renaming or moving a folder must not break its assets, note-image references, package relations or future publishing links.

Recommended default: the importer creates a separate livery folder per import inside the current regular folder (or a chosen Liveries folder). Subsequent imports explicitly choose New livery or Update existing livery; the latter retains prior versions if approved. Regular folders can nest; a livery should not contain another livery in the first implementation. Manual Move controls are required even if drag-and-drop is also offered.

Do not implement permanent deletion implicitly. Decide reversible folder removal/archival semantics before adding a destructive action. Existing project archives and original files must remain recoverable.

## Supplied vehicle source — inspected

- Source: C:/Users/TiKi5/Downloads/Livery Templates Status.xlsx, supplied by user as the official source.
- SHA-256: A1942D9B77758E2B226EAD9AA889BF8AF142C995C9F8EAE362587399FCEAF407.
- Welcome!C22 states Last Updated: October 7 2026. The local workbook is a snapshot; do not describe it as automatically live.
- Vehicle Templates has 119 populated vehicle rows (107 distinct displayed names), grouped into Law Enforcement, Fire Department, Department of Transportation and Civilian Jobs. The rows include repeated names in different groups and 15 starred rows without a status. Counts are rows, not claims about 119 distinct vehicles.
- Body statuses: 86 Completed, 14 In Progress, 4 Not in Drive, 15 blank. Window statuses: 39 Completed, 55 N/A, 9 Model Issue, 1 Not in Game, 15 blank.
- Body and window template slots have distinct per-slot values. Preserve exact status values. A blank is unspecified; N/A is distinct from missing/blocked/in progress.
- The New column describes remodel/interior status per its cell comment; it is not a generic livery-support boolean. Welcome describes usable existing templates for In Progress entries; do not translate In Progress into unsupported.
- Preserve vehicle-specific comments as source notes; do not convert future-update comments into claims that updates have already shipped.
- Naming discrepancy requiring reconciliation: Vehicle Templates!A60 is Chevlon Camion 2018, whereas Window Templates!A60 is Chevlon Camion PPV 2018. Do not fuzzy-merge these silently or join rows solely by position.
- The supplied source supersedes the wiki as the catalogue authority. It covers the template/livery domain; absence must not be presented as proof that a vehicle does not exist anywhere in ER:LC. Preserve existing project vehicle references during replacement/mapping.
- Community Reports/import contain reporter information and issue submissions. Do not bundle those sheets, usernames or contact details into the application catalogue. Only selected official template data and relevant source notes are needed. Retain the original workbook unchanged; distribution permission/attribution for derived data should be recorded before release.

## Package icons — planning only

Windows Explorer displays the icon registered for the file type. An Apex installer can register .apexstd as an Apex Studio project package and supply a custom .ico. The icon is an association resource on the machine, not an icon embedded in every ZIP package. Double-click should open Apex's import preview, never silently import or replace a workspace. A separate .apexbackup icon and restore-preview association may be useful.

Treat icon artwork and association handling as separate tasks. No registry/default-app change is authorised by this planning discussion. Installer association/activation testing belongs with distribution; the current standalone debug executable is not installed. Preserve user default-app choices and test uninstall/upgrade cleanup.

Sources:
- https://learn.microsoft.com/en-us/windows/win32/shell/how-to-assign-a-custom-icon-to-a-file-type
- https://v2.tauri.app/reference/config/

## Confirmed decisions — 2026-10-22

- One special folder per vehicle-specific livery. Several liveries may reference the same vehicle.
- Suggest body/window slots from filenames, with review and correction before import.
- Keep previous texture files as recoverable versions when revising an existing livery.

These three choices and Phase 4 implementation are authorised by the user. Cover is optional and separate from upload textures. Canonical vehicle selection and an editable livery/folder display name remain distinct proposed fields.

### Filename recognition

User-defined rule: filenames typically follow Name Side Number ExtraNumber. Use the **first number after the recognised side** as the slot number; never substitute a later extra number or a number appearing before the side.

Recognise side names case-insensitively and support spelling aliases, including Left / LEFT / Lft and Back / Rear. Canonical sides: Right, Left, Front, Back, Top, Window Right, Window Left, Window Back, Window Top, Window Front. Inspect the supplied workbook headers for supported numbered slots for each vehicle/team variant.

Implementation proposals for reviewable recognition:
- Normalise spaces, underscores and hyphens and support joined side/number forms, such as Left1. Prefer the complete window-side phrase over its body-side substring: Window Left 2 means Window / Left 2, not Body / Left 2.
- Match known aliases at token boundaries so vehicle/livery names containing side-like substrings do not silently become slot assignments. Additional aliases such as Rgt, Frt and WinLeft need explicit tests; unknown or ambiguous abbreviations remain unassigned rather than guessed.
- Examples: Patrol LEFT 2 7.png → Body / Left 2; Patrol Lft2_7.png → Body / Left 2; Unit 2024 Rear 1 9.png → Body / Back 1; Patrol Window Left 2 7.png → Window / Left 2.
- Missing numbers, multiple competing side matches, duplicate assignments and slots absent from the selected vehicle's catalogue require review. Do not silently default an unnumbered side to slot 1. Preserve files even when they remain unassigned.
- Template status is a separate property from slot recognition. Preserve source warnings without treating an In Progress refresh as proof that a slot is unusable.

## Acceptance criteria

- Existing Phase 3 projects, notes, assets and backups remain usable after migration.
- Create nested folders, move/rename them and restart; relations and covers survive.
- Import a multi-image livery, inspect/correct ambiguous slots, cancel without visible partial records, and recover originals after a per-file failure.
- Import two liveries for the same vehicle without a collision or silent overwrite.
- Package export/import into another workspace preserves structure, vehicle/team, cover, slot mappings and selected versions; old packages still import.
- Backup and restore recover the same structure and original bytes; migration creates a safety snapshot.
- UI review follows the existing spacious layout with functional copy only. Launch the current app at the end of each pass for user review.
