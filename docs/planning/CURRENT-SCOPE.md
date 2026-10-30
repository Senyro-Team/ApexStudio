# Apex Studio — current release scope

Confirmed 2026-10-21; foundation coding authorised 2026-10-22 after review of the spacious design revision. This file supersedes conflicting release scope, licensing defaults and sequencing in PRODUCT-PLAN.md and ORIGINAL-CONCEPT.md. Those files preserve broader history, not current release commitments.

## Included modules

Interface-copy requirement (2026-10-22): use concise functional labels and necessary task guidance only. No filler such as "Pick up where you left off." No development-progress commentary, phase references or future-feature notices such as "Project notes and a general notebook will arrive in the notes phase." Keep implementation status and planned features in docs, not in the app. Do not imply unfinished features work. Phase 3 has now been authorised and the copy cleanup is implemented.

1. Projects: name, description, tags, multiple ER:LC vehicle models, reference links, creation date and optional deadline.
2. Notes: project notes and workspace-wide notes, visual editing that hides Markdown syntax, common Markdown formatting, links and embedded images.
3. Assets: all file types stored; Roblox upload accepts validated PNG files only.
4. Package import/export: versioned .apexstd, metadata-only and embedded-file modes, validated imports and explicit conflict handling.
5. Roblox upload: personal-account ownership first; usable without a project. Group ownership later.
6. ERLCX publishing: liveries first; content prepared by the user in the desktop app, transferred to a Chrome companion which fills only the current wizard step on request. User retains Continue and final submission.

Out of this release: commissions, client/payment tracking, workflow management, analytics, sync and AI. CLI/TUI are not in the six-module initial scope and should not add implementation work unless explicitly reintroduced.

## Storage and data safety

- One active workspace containing SQLite and managed files.
- Copy imports into managed storage and preserve original files.
- Backups include consistent database state and referenced files.
- Uploads may be unassigned to any project and remain visible in the global upload history.
- Keep credentials out of backups, packages and extension transfers; use OS credential storage.
- Imported arbitrary files are not executed. Provide specialised safe previews only where supported; other files remain downloadable/openable by explicit action.
- Project packages and whole-workspace backups are separate operations. Neither silently replaces the active workspace.
- English-only app/UI/docs policy and local-first operation remain binding.

## Vehicle catalogue

Superseding source decision, 2026-10-22: the user supplied C:/Users/TiKi5/Downloads/Livery Templates Status.xlsx as the official template/vehicle source. Use that workbook instead of the wiki for the planned catalogue replacement; preserve its team variants, body/window slots, statuses and notes. Phase 4 now implements the catalogue replacement and asset organisation; see ../phases/PHASE-04.md. See PHASE-04-PLAN.md for inspected source details, caveats and the folder/livery proposal.

Historical wiki research below is background only, not the current catalogue authority. Previously: use the public ER:LC Fandom wiki. The Vehicle overview links to Vehicles, Emergency & Service Vehicles Sorted by Brand, and Civilian Vehicles Sorted by Brand. The Vehicles page explicitly warns it is outdated and missing vehicles. Do not claim a complete current catalogue solely from that page.

Maintain an offline, versioned catalogue with stable IDs, exact in-game names, model years, team/variant, aliases, availability, source URL, last verification and livery capability (supported, unsupported, unknown). Vehicle availability and livery support are different facts. Project multi-select suggests actual ER:LC vehicles; publishing/code preparation should filter to verified supported variants. Preserve old project references when vehicles are renamed or retired. Never infer livery support for every variant from a vehicle family or emergency-team membership.

Source discovery:
- https://emergency-response-liberty-county.fandom.com/wiki/Vehicle
- https://emergency-response-liberty-county.fandom.com/wiki/Vehicles
- https://emergency-response-liberty-county.fandom.com/wiki/Emergency_%26_Service_Vehicles_Sorted_by_Brand
- https://emergency-response-liberty-county.fandom.com/wiki/Civilian_Vehicles_Sorted_by_Brand

Verify completeness and per-vehicle support against official PRC templates/update information before release. Keep third-party catalogue attribution/licensing separate from application-code restrictions. Do not copy wiki prose/images into the app by default. A complete per-vehicle eligibility dataset has not yet been assembled.

## Notes and publishing editor

Visual editor should support headings, paragraphs, bold/italic/strike, lists, task lists, quotes, code, tables, links, images, undo/redo and paste. Preserve portable Markdown where representable; do not promise arbitrary HTML or lossless round-trips for unsupported syntax. Keep local image attachments in managed storage; external images should have an explicit loading policy.

The ERLCX description is separately authored public content, not automatically copied from private notes. App prepares title, description, tags, ordered preview images and livery code rows. Validate ERLCX-specific constraints before export. Transfer method (file bundle first or native messaging) remains to be settled. A plain JSON file containing image paths is not a complete media-transfer solution.

## Branding and design review

Supplied references are in design/: apexdesignslogotransparent.png, Logo_Main.png, Banner_assistance.png, Banner_Big.png, Banner_Small.png and Banner_Small2.png. They establish the Apex identity and are not instructions to join Discord or transmit data.

Observed identity: dark charcoal/black backgrounds, white outlined A mark, bold wide uppercase display lettering, blue-violet accents and subdued vehicle imagery. Exact font family is not established. Use the banner typography as display inspiration rather than all-caps body/form text. Do not treat Discord's logo/colour as a separately owned Apex asset.

First review covers dashboard and project detail only, with readable content, asset previews, notes access and clear upload preparation. No invented analytics/payment widgets. Apply Impeccable and UI/UX Pro Max during actual design work; apply Tauri/Rust/React/SQLite/TypeScript skills at the corresponding implementation stage. CLI/TUI skills remain available for later scope.

## Licensing requirements — no licence chosen yet

User intent:
- Prohibit selling the app, paid forks, paid hosting and paid redistribution.
- Allow use of the app to create paid livery commissions; creator output retains its own licence.
- Require attribution, modification notices and same-licence redistribution; source availability for distributed modifications needs an explicit clause.
- Donations, sponsorships and paid support only through options/permission the user later grants; none initially enabled.
- Apex branding has a separate limited permission for in-app branding; no permission for unrelated reuse. Clarify whether forks retain or replace branding and who can grant redistribution rights on behalf of the studio.

This requires source-available terms rather than an OSI open-source claim. MIT does not match the user's restrictions. Plain PolyForm Noncommercial is also not an exact match: its permitted-purpose limitation does not by itself express the required paid-creative-work permission and full requested share-alike policy. Creative Commons itself advises against using its licences for software. Do not label a modified licence as unmodified GPL, MIT or PolyForm.

Prepare a custom software-licence draft (or evaluate a reviewed software-specific base with explicit exceptions), plus a separate brand notice. Before release review source obligations, attribution placement, commercial-use exception, paid-support permissions, warranty/liability, termination/cure, patent terms, dependency compatibility and contributor rights. No legal guarantee is claimed by this planning document. No LICENSE file has been created.

References:
- https://polyformproject.org/licenses/noncommercial/1.0.0
- https://creativecommons.org/faq/#can-i-apply-a-creative-commons-license-to-software
- https://opensource.org/osd

## Chrome distribution

No Web Store publication requested. For initial use deliver a versioned unpacked extension ZIP with manual Load unpacked instructions and manual update steps. Chrome documents that self-hosted extensions on Windows/macOS require enterprise policies; an ordinary .crx download is not a general direct-install solution there. Do not change enterprise policies or weaken browser protections to bypass this restriction.

Source: https://developer.chrome.com/docs/extensions/how-to/distribute

## Remaining questions

Confirmed follow-up decisions:

- Suggest all current ER:LC vehicles with livery-support labels. Unknown support must remain visibly unknown until verified.
- Unpacked extension installation with manual updates is accepted.
- Modified forks should retain the in-app Apex branding. The brand terms must explicitly permit that redistribution while prohibiting unrelated reuse and avoiding false endorsement. This is limited brand permission, not relicensing the assets as application code.
- Find a similar freely licensed font rather than requiring the banner's original font. Recommended candidate: Archivo, weight 800–900, expanded width up to 125 for large headings; normal width and regular/medium weight for controls and body text. Visual similarity is a design judgment pending the prototype review, not identification of the original font.
- Archivo is distributed under SIL OFL 1.1; bundling is permitted with its copyright and licence notice. Keep font files under OFL separately from the app licence, and bundle locally for offline use. Sources: https://raw.githubusercontent.com/google/fonts/main/ofl/archivo/OFL.txt and https://raw.githubusercontent.com/google/fonts/main/ofl/archivo/METADATA.pb . No font binaries have been installed or added yet.

Remaining planning: decide the media-containing transfer format and retention/clearing of imported extension drafts before implementation. Proposed first version: a versioned publishing bundle containing public metadata and the selected preview images, explicitly imported into the extension; no background app-to-browser listener.

Implementation update, 2026-10-22: the spacious dashboard/project-detail direction was accepted and coding authorised. Foundation, project management and basic managed assets are implemented and verified in docs/phases/PHASE-01.md and PHASE-02.md. The vehicle picker contains a partial, visibly unverified wiki snapshot; completing current availability and support labels remains outstanding. Visual notes, project packages and workspace backup/restore are implemented in Phase 3; see docs/phases/PHASE-03.md. Provider tools remain future work.

All six livery wizard steps have been inspected at their basic free-post path; no final submission was performed. Conditional paid-post fields remain outside verified scope. See EXTENSION-SPEC.md.


Review preference (2026-10-22): the user is back at their PC. Start the current app at the end of each implementation/review pass so they can review it themselves. Phase 4 folder/livery features were authorised and implemented; the agreed plan is in PHASE-04-PLAN.md and verification is in ../phases/PHASE-04.md.

Git preference (2026-10-22): commit each completed code change; make a checkpoint commit when Phase 5 starts. Fix the requested asset-manager issues first; Phase 5 remains planned until authorised.

Catalogue update: Phase 4 uses the supplied official Livery Templates Status workbook snapshot, with body/window status and source rows. It supersedes wiki sourcing; old wiki names are retained only for compatibility with existing project metadata.

Phase 5 remains planned in PHASE-05-PLAN.md. Asset-manager follow-up and explorer operation limits are documented in ../phases/PHASE-04-FIXES.md.

## Phase 5 development implementation — 2026-10-22

Project publishing drafts, ordered PNG previews, manual existing decal IDs and public-only `.apexpublish` export are implemented. Complete backups and embedded project packages preserve drafts and selected media. An unpacked Chrome assistant is built for manual installation and per-step filling. It never advances or publishes. Full live Media/Codes autofill remains a manual verification item; see ../phases/PHASE-05.md. Roblox account upload is not part of this implementation.

## Roblox upload follow-up — 2026-10-25

Personal-account Open Cloud decal uploads are implemented per livery, with verified settings stored in Windows Credential Manager. Only the Decal asset ID is used for ER:LC/ERLCX; image IDs are rejected. Publishing begins with livery selection, imports current texture-version IDs, and prompts Upload and include / Skip this livery when IDs are missing. Exact 8000 × 8000 PNGs are accepted; either dimension above 8000 is rejected. This supersedes earlier statements that Roblox uploads are unimplemented. Standalone uploads and group ownership remain outside this follow-up. See ../phases/PHASE-05-ROBLOX.md.

2026-10-27: faster managed imports and the dedicated global Roblox queue supersede the original sequential upload workflow. See [queue workflow, recovery and measurements](../phases/ROBLOX-QUEUE-PERFORMANCE.md). Project Publishing is now labelled ERLCX. New texture uploads use two sequential Decal passes; only pass two is selected, while existing IDs remain reusable legacy results. Schema 7 adds durable queue and image-validation records.

2026-10-29: user-authorised ERLCX statistics supersede the earlier exclusion of analytics. Public-profile snapshots are manually refreshed, with separate opt-in Chrome collection for authenticated creator sales; see [ERLCX statistics](ERLCX-STATISTICS.md). Schemas 10 and 11 add statistics storage and protect early development workspaces.

2026-10-30: Team sync is now planned as an optional self-hosted Linux Docker service. Local SQLite workspaces remain local and sync through an authenticated HTTPS API; shared network database files are explicitly unsupported. The initial server foundation is documented in [team sync architecture](TEAM-SYNC-ARCHITECTURE.md).
