# Asset explorer refinement

Mode: Operate. Preserve the accepted dark Archivo/Apex visual world.

Task path: identify current folder, find an item or import files, then act on the selected item. Previously three unrelated command rows gave creation, folder settings and clipboard actions the same emphasis. The revised structure keeps breadcrumbs first, search and creation together, then a stable-height metadata/selection strip. Folder settings live in a labelled overflow menu beside the path. Copy/Cut/Open appear only for a selection; Paste appears only with clipboard content. This avoids a toolbar full of unavailable commands while retaining keyboard and single-pointer alternatives to dragging.

Creation controls form a 10px group separated from search by generous free space. Selection commands are quieter text controls, adjacent to the selected name. At narrower widths the command bar stacks search above creation, with selection actions wrapping below rather than causing horizontal overflow. The strip keeps a reserved height so beginning a drag does not move the file grid. Folder-drop outlines show valid destinations. Original source files remain untouched.

Windows integration: Tauri dragDropEnabled=false is required for frontend HTML5 drag/drop. Internal transfers use captured pointer gestures, an 8px threshold, hit-tested folder targets, and a drag label. This avoids dependence on WebView HTML dragover timing; the trace of a short gesture showed dragenter then dragend with no usable drop. Cancel on Escape/pointercancel; suppress only the trailing drag click. HTML handlers remain a fallback. Folder cycles and livery nesting rules remain enforced in storage. Testing must include a real native gesture, not only synthetic DOM drop events.

Verification (2026-10-22): real Windows pointer drag moved empty Folder 2 from Folder 1 to project root, then a tile-to-tile drag moved it back. Original hierarchy restored. 18 frontend tests pass, including pointer threshold, Ctrl-copy, rejected targets, and contextual toolbar behavior. Production/native builds pass. Existing Rust storage move/copy validation remains unchanged.

Folder batch import (2026-10-25): a compact arrow beside Import livery exposes Import livery folders without another full-width toolbar command. The review uses a folder list and one active livery editor, retaining each folder’s vehicle, side and cover choices. The footer reports imported/remaining counts and provides one batch action. See ../phases/LIVERY-FOLDER-IMPORT.md.
