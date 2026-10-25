# Import multiple livery folders

Implemented 2026-10-25.

## Use

In project Assets, open the arrow beside Import livery and choose Import livery folders. Select multiple folders in the native folder picker (Ctrl/Shift selection). Each selected folder becomes a separate livery under the current asset folder.

The review shows one livery at a time. Folder names become editable livery names. Choose a vehicle, correct filename-derived side/number assignments, select a cover and optionally Add images from elsewhere. Choices persist when switching between folders. Vehicle suggestions appear after typing. Mark unwanted or invalid entries Skip this folder, then choose Import N liveries.

Each successful livery is committed independently. If another fails, its message remains with that row; correct it and retry only the remaining liveries. Successful rows are not imported again. Cancel/Done releases unimported staged copies; original PC folders are never changed. Closing after a partial import retains the successfully imported liveries.

Only PNGs directly inside each selected folder are staged. Non-PNG files, links and nested folders are skipped with a visible warning; folders are never recursively flattened together. Empty or invalid folders show individual errors and require correction or an explicit skip. Limits: 50 selected folders, 100 PNG files per folder, 20 MiB and 8000 × 8000 inclusive per PNG, 2 GiB for initial batch staging. Duplicate names within the batch and invalid/duplicate side slots block import. Existing destination-name conflicts surface per livery and can be fixed by renaming before retry.

This action imports local folders into the managed project. Roblox upload remains an explicit action on an imported livery; folder selection itself never sends files to Roblox or ERLCX.

## Verification

- Native adapter: six tests pass, including folder staging/import separation with identical filenames, filename aliases, source preservation, empty/invalid PNGs and batch byte limits.
- Frontend/extension: 39 tests pass, including independent per-folder assignments/covers, partial failures with retry of only failed items, skipping empty folders, and cleanup of unimported drafts.
- Production frontend/native builds, Rust formatting and workspace Clippy with warnings denied pass. Impeccable detector reports no findings for the new review.
- Native folder-picker and review verification uses temporary local sample folders and cancels before importing into the user's active project. No external upload is initiated.

No database migration or additional frontend filesystem permission is required; native staged tokens keep absolute filesystem paths out of the frontend.

Windows launch note: when started from the packaged Codex process, Windows can redirect this debug executable into Codex’s app-data context. A desktop-shell launch through Windows Explorer reopened the existing Tiki Workspace correctly. No workspace data was moved, merged or restored. Keep this distinction when verifying later builds.
