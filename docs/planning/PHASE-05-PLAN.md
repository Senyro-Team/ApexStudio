# Phase 5 — Publishing preparation and ERLCX assistance

Status: development implementation delivered 2026-10-22; checkpoint c90fde7 followed the asset drag/layout fix a21f85f. Full live Media/Codes autofill remains a manual verification item. See ../phases/PHASE-05.md for evidence and limitations.

## Planned deliverables

- Prepare a public livery post within the app: title, description, tags, ordered preview images, optional YouTube URL, and vehicle/side/decal-ID rows with colour and opacity settings.
- Save editable publishing drafts locally. Let users review exactly what will be transferred; exclude general/project notes and unrelated assets.
- Export a versioned publishing bundle containing the selected public metadata and preview media. Validate filenames, sizes, hashes and schema. No background local web server is needed.
- Build a Manifest V3 Chrome extension distributed as an unpacked ZIP with manual installation and updates. Import the bundle, review it, fill only the current ERLCX livery wizard step on an explicit request, and report skipped or unsupported fields.
- Keep Continue, Save Draft, guidelines agreement, scheduling and final publishing under user control. No implicit paid settings. Provide Clear Draft and document temporary-data retention.
- Verify supported steps with local fixtures, then inspect actual ERLCX form behavior without publishing. Recheck provider constraints before implementation; earlier inspection notes are evidence, not a guarantee that the current site is unchanged.

The first supported category is liveries. Actual Roblox account uploads are a separate later integration; existing decal IDs can be entered for this handoff. Group uploads, payments, workflows, analytics and other ERLCX categories are outside this phase. Resolve the final bundle/retention details as part of Phase 5 planning, before implementing the extension.

Authoritative scope: CURRENT-SCOPE.md. Wizard inspection and adapter constraints: EXTENSION-SPEC.md.

## Implementation decisions

Use a versioned .apexpublish ZIP with public.json and explicitly selected preview media. No local listener or browser credentials. Store editable project publishing drafts and preview references locally with revision checks; include them in complete workspace backups. Extension imported bundles stay in IndexedDB until Clear draft (including across browser restart); explicitly state this retention in the extension and documentation. This avoids Chrome session storage quota limits for image bundles. No draft contents are sent until the user invokes Fill current step. The extension is delivered as a ZIP for manual unpacked installation; do not install it automatically.
