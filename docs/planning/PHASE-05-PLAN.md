# Phase 5 — Publishing preparation and ERLCX assistance

Status: planned, not started. The user requested asset-manager fixes before authorising Phase 5. Make a Git checkpoint commit when Phase 5 starts, then commit each completed code change.

## Planned deliverables

- Prepare a public livery post within the app: title, description, tags, ordered preview images, optional YouTube URL, and vehicle/side/decal-ID rows with colour and opacity settings.
- Save editable publishing drafts locally. Let users review exactly what will be transferred; exclude general/project notes and unrelated assets.
- Export a versioned publishing bundle containing the selected public metadata and preview media. Validate filenames, sizes, hashes and schema. No background local web server is needed.
- Build a Manifest V3 Chrome extension distributed as an unpacked ZIP with manual installation and updates. Import the bundle, review it, fill only the current ERLCX livery wizard step on an explicit request, and report skipped or unsupported fields.
- Keep Continue, Save Draft, guidelines agreement, scheduling and final publishing under user control. No implicit paid settings. Provide Clear Draft and document temporary-data retention.
- Verify supported steps with local fixtures, then inspect actual ERLCX form behavior without publishing. Recheck provider constraints before implementation; earlier inspection notes are evidence, not a guarantee that the current site is unchanged.

The first supported category is liveries. Actual Roblox account uploads are a separate later integration; existing decal IDs can be entered for this handoff. Group uploads, payments, workflows, analytics and other ERLCX categories are outside this phase. Resolve the final bundle/retention details as part of Phase 5 planning, before implementing the extension.

Authoritative scope: CURRENT-SCOPE.md. Wizard inspection and adapter constraints: EXTENSION-SPEC.md.
