# Asset-manager follow-up — 2026-10-22

Phase 5 remains planned. See ../planning/PHASE-05-PLAN.md. The completed Phase 4 baseline was committed before these changes (606bbad). Commit completed code changes and create another checkpoint when Phase 5 is authorised and starts.

## Changes

- Livery import has a single Vehicle search field. No list appears until typing; at most eight matching vehicle/team suggestions appear, with keyboard navigation. The duplicate Find vehicle field is removed.
- Every reviewed texture has independent side and number controls. All ten canonical sides are available, including window sides; the number is editable from 1 to 100. Filename parsing still suggests the first number after the side. Duplicate slots are rejected. Manual assignments are metadata, not a guarantee of vehicle support; the official catalogue remains the support reference.
- Add images reopens the native PNG picker within the same review, retains all prior assignments and cover selection, and includes appended images in the cover picker. An extra cover from another directory can remain unassigned as an ordinary file.
- Existing livery files expose the same editable slot controls in File details. Occupied slots must first be cleared rather than silently overwriting another file.
- Folder covers fill their thumbnail area using cover cropping; texture previews retain their aspect ratio.
- Thumbnail decoding now shares the livery bounds (20 MiB, 8000 pixels per side, 256 MiB decoder allocation limit). Previously it silently rejected PNGs above 8 MiB or 16 million pixels, despite accepting them as textures. Missing cache entries regenerate when assets/folders are read.
- Disabled buttons use neutral dark gray rather than a faded active colour.
- Select a file/folder, then Copy or Cut and Paste into the current directory; Ctrl+C/X/V also work within the asset browser. Double-click folders, use Enter on a focused folder, or choose Open folder. Drag an item onto a folder to move it; hold Ctrl when dropping to copy. Drop on All assets to move to the project root.
- Copies receive independent IDs while sharing immutable stored bytes. Folder copies preserve their hierarchy, livery metadata, covers, slots and recoverable file history. Duplicate copy names get a suffix. Single-file copies are unassigned; moves between folders clear the old slot. Dropping into the same directory preserves slots.
- Moving/copying a folder into itself or its descendants is rejected. Folder nesting is allowed in regular folders; livery folders remain the leaf containers for vehicle-specific files. Archived projects remain read-only.

Clipboard scope is one selected item within the current project and current asset-browser session. It does not read or overwrite the Windows clipboard, move original source files, or support cross-project paste. No OS shell access or broad filesystem permissions were added. SQLite schema remains version 4; existing packages/backups remain compatible.

## Verification

Automated checks: 20 Rust tests, 16 frontend tests, TypeScript/Vite production build, Rust formatting and Clippy with warnings denied. Regression coverage includes manual side reassignment, append-and-retain import review, typing-only suggestions, keyboard copy/paste, drag-to-move, history/cover-preserving tree copies, same-directory slot preservation, cycle/cross-project rejection, and regeneration of a 4100 x 4000 PNG preview after cache removal. Impeccable detector returned no findings. The existing large lazy notes-chunk advisory remains a build warning.

Native verification: opened the existing seven-file verification project, confirmed all five texture previews in the Bullhorn BH15 livery and full-tile covers, opened a temporary import with the prior local test texture, searched and selected a vehicle, appended Banner_Small2.png from a separate directory, selected Window Top despite a Left filename suggestion, and confirmed the new banner appeared in the cover picker. Cancelled the temporary review without importing or modifying existing livery records. The initial cache regeneration is slower in the debug executable; subsequent opens reuse thumbnails. Reviewed disabled gray controls and grouped the explorer actions after detecting inherited full-width spacing. The rebuilt app is left open for user review.
