# Dashboard and project detail — design review

2026-10-22 implementation update: user accepted the simpler revision as a better direction, asked how to continue, then explicitly authorised the first coding phase. Spacious-v2 guides the shell. Foundation status and native verification blocker are recorded in docs/phases/PHASE-01.md. Earlier pending/rejected status notes below describe historical review rounds.

## Revision 2 — spacious direction

The user rejected the initial concepts as crowded and insufficiently organised, requesting intuitive navigation, more space and less content at once. The user approved proceeding with a simpler design exploration, not application coding.

New concepts: dashboard-spacious-v2.png and project-spacious-v2.png under .impeccable/mocks/. Pending review; neither is approved. Dashboard leads with recent projects, then deadlines, one New project action and a quiet sidebar. Module access is consolidated under navigation rather than repeated overview panels. Uploads is a proposed hub for Roblox and ERLCX; packages remain contextual project actions. Project detail uses Assets/Notes/Publishing/Details tabs, a six-file thumbnail grid and no persistent inspector. Selection opens details as needed.

Keep this composition's low information density in the next iteration; do not reintroduce all module panels to satisfy the earlier overview request. Future statistics remain out of this release. Consistent shell and generous grouping gaps matter more than filling unused space.

Image limitations: generated dashboard dates say 2024 although project examples are for 2026; all are sample content. Project PNG thumbnails depict complete vehicles instead of actual flat livery texture files; do not imply the app renders textures onto 3D models. PSD/PDF previews are illustrative and not evidence of implemented preview support. Actual import previews must display file pixels or a generic file-type icon when unsupported. Exact font rendering and logo fidelity need implementation-time verification.

The original concepts below are retained as rejected exploration, not implementation instructions.

2026-10-22. Status: three generated static concepts awaiting user review; no design approved and no app code written. Built-in image generation was used; exact prompts and approval flags are beside images under .impeccable/mocks/.

## Confirmed hierarchy

Dashboard gives a complete overview of every active module, recent-project access and upcoming deadlines. Future statistics may join later, but initial screens must not show fake analytics or disabled statistics scaffolding. Project detail leads with assets/livery files and puts visual notes one click away.

## Concepts

1. dashboard-gallery.png: visual recent-project cards above module panels, deadlines to the right. Stronger visual recognition, but consumes more space and pushes module coverage down.
2. project-assets.png: grouped file table plus selected-file inspector. Tabs give access to Notes, Publishing and Details. Bulk actions belong to the selected files; inspector actions belong to the inspected file.
3. dashboard-overview.png: compact module strip, recent-project list, right-hand deadlines/activity, lower notes/packages. Recommended dashboard starting point because every active module is visible and the structure can accommodate future statistics without redesigning navigation.

Shared visual direction is the supplied Apex branding: charcoal ground, white mark, blue-violet accents, expanded heavy display headings and readable regular-width body. Archivo is a proposed type family; generated glyphs are only a visual approximation.

## Corrections required before a final design

- Use one consistent Apex Studio application identity and workspace label in both screens; generated concepts currently differ in logo placement, scale and header composition.
- Reduce oversized dashboard headline/logo to give working content more room.
- All names, counts, previews, vehicle variants, dates and identifiers in these images are illustrative, not verified catalogue or actual user records. Gallery image's generated backup timestamp has an incorrect year; replace with real runtime freshness or 'No backup yet'.
- Distinguish Image ID and Decal ID, never a generic ambiguous Roblox ID. Unuploaded files have no IDs; disable Copy ID only when unavailable. The generated selected-row ID versus disabled inspector copy state is inconsistent.
- Files such as PSD sources/previews need 'Not applicable' rather than an implication they can all be uploaded as livery textures. Do not suggest automatic PSD-to-PNG conversion.
- Enforce .apexstd terminology in package copy; the generated overview calls it ZIP generically and implies a separate asset-package capability that is not established.
- Use one global Upload PNG entry independent of projects; project-level uploads may preselect the current project but must show scope.
- Clarify app publishing bundle export versus actual ERLCX publication. 'Ready-to-publish' means prepared content, never validation or submission success.
- Livery support needs supported/unsupported/unknown text labels, with unknown preserved. Catalogue values in generated mockups are not evidence.
- Avoid giant decorative empty-state icons; compact action guidance when modules have no records.

## Behaviour and smaller-window requirements

At smaller desktop widths, collapse the sidebar, move the inspector into a drawer and put deadlines below recent projects. Preserve file names and selection; secondary columns can be hidden rather than shrinking everything. Keyboard users must reach all actions, row selection and inspector without dragging. Reordering images also needs buttons. Use explicit progress, errors and recovery feedback. Notes must show local save state and retain edits across tab changes. Empty project starts with Import assets and Create note, not sample records.

## Approval requested

Choose gallery or overview dashboard (or specify a combination); review whether the asset table/inspector fits the project-detail task. Approval selects a direction for refinement; it does not authorise app coding automatically. Next deliverable is a consistent two-screen revision based on feedback.
