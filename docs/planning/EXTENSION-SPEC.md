# Apex Studio Chrome companion — specification

Current scope and distribution decisions are tracked in CURRENT-SCOPE.md. No Web Store release is requested; plan an unpacked build first rather than claiming ordinary Windows .crx installation.

## Implementation addendum — 2026-10-22

The development build now uses a version-1 `.apexpublish` ZIP and Manifest V3 assistant with only activeTab/scripting. Imported public metadata and selected PNG blobs persist in IndexedDB until Clear draft; this supersedes earlier storage proposals. Distribution is a manually installed unpacked ZIP. No browser installation or settings changes are performed by the build.

Fresh signed-in DOM inspection reconfirmed Categories, Details and Media. Current public upload-page JavaScript confirmed Codes placeholders, dynamic row controls, portalled colour popovers and opacity bounds 0–100. Local fixtures exercise these controls. Fresh full Media/Codes filling in Chrome remains unverified because the research connector could not attach local images. Historical successful wizard inspection below does not substitute for this implementation check. No post or draft was submitted. See ../phases/PHASE-05.md and ../../apps/erlcx-extension/README.md.

## Media step verification — 2026-10-21

Follow-up: the user manually selected the two authorised images. The tool then reached Codes, Monetization and Submit. This supersedes the access blocker described below. No draft was saved, agreement accepted or post published.

Resumed after user sign-in. Selected Liveries and entered temporary sample title/description, without saving a draft or publishing. Media requires 2–10 images, maximum 20 MB each. UI lists PNG, JPG and GIF. The first image is the main thumbnail; drag-to-reorder is offered. Optional YouTube link uses input id youtubeUrl.

The user explicitly authorised attaching Banner_Small.png and Banner_Small2.png for inspection. File selection failed because the ChatGPT Chrome extension requires “Allow access to file URLs”; no banner files were attached by the tool. Codes, Monetization and Submit remain unverified pending manual file selection or enabling that access. The user retains final submission. Research sample must not be saved/published.

## Codes, Monetization and Submit verification — 2026-10-21

Codes requires at least one entry per selected category. Add Livery Code creates a row with:

- Vehicle-name text input, placeholder `Vehicle name (e.g., Crown Victoria)`. It is free text in the observed ERLCX form; Apex's verified catalogue should supply the prepared name.
- Primary colour, default #FFFFFF; optional secondary colour, initially #000000 when enabled, with removal control.
- Opacity numeric input, default 100 percent. Exact accepted bounds were not tested.
- Side-name input, placeholder `Side (e.g. Left, Right)`, and Decal ID text input.
- Add Side and Remove side; livery-level Duplicate, Delete, Move Up and Move Down controls. Reordering was disabled for the single observed livery.

Inspected adding/removing a second side and enabling secondary colour. Used the deliberately temporary vehicle name `Apex Studio inspection - not for publishing`, side Left and sample numeric value 123456789 solely to advance. The sample is not a verified Roblox asset ID and must never become product seed data. Reaching the next step proves only client-side acceptance, not provider validation or correctness.

Monetization offers optional Roblox Game Pass link or ID, input id robloxGamepassUrl. UI states blank means free. Left blank; paid-path dependent fields, permissions and validation were not tested. The extension should not enter paid settings or agree to terms implicitly.

Submit displays:

- An unchecked guidelines agreement, id guidelines; the Publish Post button stays disabled without it.
- Publish immediately, described as going live after moderator approval.
- Schedule for later with a local date/time input and Clear schedule. UI recommends at least 12 hours ahead for review; treat this as site guidance, not an Apex guarantee. Scheduling was inspected and cleared without submission.
- Back, Save Draft and Publish Post controls. Neither Save Draft nor Publish Post was invoked.

Extension contract additions: livery name/vehicle reference, ordered side/Decal ID pairs, primary/optional secondary colour, opacity, optional YouTube URL, ordered preview files. Validate confirmed limits in the app; inspect/verify colour controls, numeric bounds and all actual saved values during implementation tests. Do not treat the inspection as end-to-end autofill verification.

Retain final agreement and publish action for the user. Basic free-livery path is inspected; multi-livery limits, multi-category paths, paid-post requirements and server validation remain unverified.

Evidence: erlcx-final-review.png shows the review screen with guidelines unchecked and Publish Post disabled. Browser tab retained for handoff; it contains temporary unsaved sample content and the user's selected preview images. Do not save or submit it.

Status: planned; no extension implementation or Chrome installation yet.

2026-10-21 update: signed-out public metrics were verified at `/users/roblox/2919579206/bundles?page=1&limit=6&sortBy=latest`, including page 2 and a separate HTTP request without browser credentials. Public analytics can use a Rust adapter independently of this extension. The extension's initial scope is publishing assistance; owner-session metrics are optional later work. See ERLCX-RESEARCH.md for evidence and limitations.

## Confirmed interaction

The user imports a publishing file prepared by Apex Studio, opens ERLCX's upload wizard, and invokes “Fill current step”. The extension reports fields filled, skipped and requiring attention. The user reviews and clicks Continue. Repeat per step. Never click Continue, Save Draft, Submit or Publish automatically.

Scope is ERLCX publishing forms, with category-specific support verified independently. Do not advertise arbitrary-site autofill or claim all ERLCX categories are supported from one livery test.

## Observed wizard

Initial navigation to https://erlcx.com/upload displayed Categories, Details, Media, Codes and Submit. Selecting Liveries changed this to six steps by adding Monetization before Submit.

Verified on the Details step:

| Field | Observed constraint/selector |
| --- | --- |
| Title | input id `title`; required; minimum 5 and maximum 50 characters |
| Description | textarea; required; maximum 2000 characters; Write/Preview editor |
| Tags | optional input; Enter adds a tag; up to 10 tags |

Categories observed: Liveries, Uniforms, ELS, Map Templates, UGC. “Post as” offered the user account and an authorised group. Never change attribution silently.

Media, Codes, Monetization and Submit internals remain unverified. Inspection did not enter invented publishing content or save a draft. Exact selectors, code-row schemas, media limits and conditional fields must be captured before implementing those adapters. The extension must stop with an explicit unsupported-step message if a layout is unknown.

## Permissions and isolation

Use Chrome Manifest V3, bundled TypeScript output and a local popup/side panel. Prefer `activeTab` plus `scripting` for user-invoked current-tab form assistance; narrowly validate HTTPS origin `https://erlcx.com` and upload path at execution time. Avoid `<all_urls>`, cookies, history, debugger and webRequest permissions in the shipped autofill extension. Use session storage for temporary draft data and expose Clear Draft.

Chrome documents that activeTab grants temporary current-tab access following a user gesture: https://developer.chrome.com/docs/extensions/develop/concepts/activeTab . Cross-origin API access for analytics is a separate capability and may need additional narrowly scoped permissions. Do not claim activeTab alone grants API-origin access.

No generic page-to-extension command bridge, remote executable code or automatic project transmission. Validate all imported files and message payloads against versioned schemas. Display the public-data preview before applying fields. Local privacy-sensitive records never enter the publishing contract unless explicitly selected and supported.

## Publishing contract v1 (proposed)

Envelope: schemaVersion, exportId, generatedAt, projectId and targetOrigin. Payload: categories, author selection, title, descriptionMarkdown, tags, media references and category-specific code rows. Separate optional monetization preferences from ordinary content; never infer acceptance of agreements or enable a commercial setting.

Media references contain relative names, roles and hashes, not arbitrary filesystem paths the extension reads silently. Initial implementation asks the user to select referenced media files in the extension, matches them by hash and previews attachment selection. Implement programmatic attachment only after verifying browser/site support; otherwise provide a clear manual step. Do not include huge binary data in JSON or pretend a filename reference uploads a file.

Metrics export is a separate contract containing schemaVersion, importId, account scope, collectedAt, completeness and an allowlisted array of post metrics. Never include cookies, request headers, full descriptions, moderation data or draftData.

## Form adapter behaviour

1. Validate target origin/path and import schema/limits.
2. Detect the current step using stable semantic labels and verified controls.
3. Build an application preview and detect existing content/conflicts.
4. Match fields unambiguously; preserve non-empty values by default. Explicit overwrite is a separate user action.
5. Apply fields with React-compatible setters and input/change events; read back values after rerender.
6. Add dynamic tag/code rows through verified controls. Avoid repeated duplicates; verify select/multi-select values and category state.
7. Report exact successes, skips and errors. Never present partial fill as completion.
8. Stop at commercial choices, agreements, authentication, CAPTCHA and final submission for user action.

“Fill current step” must be idempotent for the same export. Failed fields should be retryable without duplicating successful rows. Capture original field values for a best-effort undo within the active page; explain when the site has changed a field and undo is unsafe. Never clear an entire form to recover.

## Implementation and acceptance

First build the exporter, validation schemas, extension shell and a synthetic six-step local fixture. Then implement and verify Categories and Details against the observed form, followed by actual Media/Codes/Monetization layouts using disposable user-provided publishing data. Add other categories only after their schemas and conditional steps are verified.

Tests must cover malicious/oversized imports, wrong origin, unknown step, ambiguous labels, existing values, React rerenders, missing files, repeated invocation, dynamic rows, account/group mismatch and partial failure. Assert that no submission/navigation buttons are invoked and no background requests transmit draft data.

Release as an unpacked development build for user verification first, then prepare Chrome Web Store packaging/privacy disclosures. No installation or browser permission grant is implied by writing the source code. Analytics collection remains separately gated by the provider-access decision documented in ERLCX-RESEARCH.md.
