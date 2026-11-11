# Optional Server and Post Preparation Workflow

## Product contract

Apex Studio remains a local-first desktop application. Project management, notes, project assets, overlay composition, draft authoring, and Markdown generation work without an account or server connection. The user can complete the same post-preparation workflow manually by importing local images, pasting public image URLs and invite URLs, ordering the content, previewing it, saving the draft, and copying the resulting Markdown.

Server-backed services are optional conveniences. A disconnected, unavailable, or unconfigured service must not prevent local editing or export. Project content and image bytes transfer only after the user selects the visible manual sync or upload action; linking a project alone does not transfer data. Uploading a file to a public image service requires an explicit action and clear confirmation that the resulting link is public.

## App information architecture

- Add **Server** to main navigation. It contains the existing Team Sync connection and independently configured capabilities for Image Hosting and Invite Tracking. Each service displays its own connection and permission state. Missing services are simply unavailable; there is no server requirement for other app modules.
- Keep app appearance, workspace, backups, Chrome, and Roblox configuration in **Settings**. Team selection and shared-service setup live under **Server**.
- Add an **Image Library** view under **Assets** for user-owned reusable post media. Project assets remain project-scoped; users may add a project asset to the library without moving or altering its source.
- Extend **ERLCX** with a post composer organized into content, livery codes, media, and final Markdown. Keep the composer capable of manual URLs and local-only preview. Add ExecLab as a separate target adapter later, reusing the same content model while respecting its own dimensions and Markdown rules.

## Authentication and service boundaries

Use the existing Sync Server account and team membership as the one app-to-server authentication system. Current Sync Server sessions are opaque database-backed bearer tokens, so do not copy them into Image Provider or Invite Tracker databases or expose their service credentials to the desktop app.

The authenticated server API should authorize each operation and act as a gateway to optional service APIs using server-held, narrowly scoped credentials. Each service keeps its own database, file storage, rate limits, and operational logs. Public service endpoints must use HTTPS because the gateway sends service credentials; HTTP is restricted to loopback, private IP addresses, and local service hostnames for deployments on a trusted private network. Enforce team membership and role permissions at the gateway; never infer a user's team ownership from a client-supplied ID. Project sync, image hosting, and invite tracking are separate capabilities and can be enabled independently.

Image-library ownership is **personal by default**. A user explicitly marks an item for team sharing. This controls who can organize and reuse its library metadata; it does not make an uploaded provider URL private. Public media and private project files are different data classes and must be presented as such.

The app stores server sessions in the platform credential store. The Discord bot token, OAuth client secret, provider API keys, and database credentials remain on the server. Services should accept only gateway-authenticated requests, not arbitrary client bearer tokens.

## Reusable media and versioning

Represent library media with a stable local ID, content hash, managed-file reference, original name, alt text, tags/folder, ownership scope, and optional composition metadata. A composition records its template and editable text/layer settings so a text-specific preview can be regenerated.

Track each remote provider object separately from the local image: provider, remote object ID, public URL, content hash, upload state, and last checked time. Reuse an existing remote mapping when the bytes and provider match. Changing a Markdown link destination changes only the Markdown block and never triggers an image upload. Changing overlay text or pixels creates a new content hash and a new media version; retain the previous version while a saved post references it.

Saved posts are immutable content snapshots unless the user edits them. Changes to a source image, template, library item, or remote URL do not silently rewrite existing drafts. Backups preserve library metadata, referenced local blobs, versions, and remote mappings; a restored mapping may be marked unchecked until the provider is reachable.

## Post workflow

1. Create or open a platform-specific post draft and select liveries from any project.
2. Choose local files, project assets, or Image Library items. Optionally apply a user-selected overlay template and export one or many previews at the target dimensions.
3. Review the exact media to publish. If Image Hosting is configured, explicitly upload selected local media in batches; otherwise paste URLs from an external host. The app shows which remote URL maps to which local image and reuses a URL for identical bytes.
4. Optionally create a tracked Discord invite campaign with useful context such as project and post title. Without Invite Tracking, paste a link created elsewhere. Creating an invite is an explicit remote side effect and must not be repeated automatically after an ambiguous timeout.
5. Arrange reusable Markdown blocks: plain image, linked image, text, and invite link. Let a user reuse a banner's image while changing its destination. Provide a target-specific preview, save a draft snapshot, and copy the complete Markdown with intentional spacing.
6. Continue through the existing ERLCX extension workflow. The extension remains responsible for the ERLCX form; media and Markdown preparation do not silently submit or publish a post. ExecLab integration uses a separate target adapter when implemented.

The supplied overlay-builder HTML is a behavior and format reference only. Build a customizable, brand-neutral editor with user-owned templates and optional logos; do not import its embedded Apex Designs overlay or make Apex branding mandatory. Support the supplied ERLCX and ExecLab aspect ratios and batch preview/export.

## Rollout sequence

1. **Server module and local-first structure:** move Team Sync out of general Settings into **Server**; preserve the existing local workspace and project sync behavior. Implement status for each independently configured service without displaying unimplemented capabilities as functional.
2. **Local media library and composer:** add reusable user-owned media, manual image/invite URLs, Markdown blocks, draft snapshots, and image-version references. Build and review a neutral customizable overlay editor with batch export and project asset import/export.
3. **Shared authentication gateway:** add capability discovery and authorization to Sync Server. Require explicit feature enablement and role checks. Keep backing service stores separate.
4. **Image hosting integration:** add scoped gateway upload/list/health operations, stable provider mappings, batch progress, retries with duplicate prevention, and explicit-public-upload review.
5. **Invite tracking integration:** scoped campaign create/list/status operations and post metadata are implemented in the desktop publishing Markdown step. Campaign creation asks before the remote action, uses a persistent idempotency key across ambiguous retries, and automatically inserts the returned invite link. Manual links remain available.
6. **Team library sharing and release review:** add explicit personal/team visibility, permission checks, backup/restore, offline and unavailable-service behavior, then add ExecLab target support.

## Implementation checkpoints

- The Server navigation entry now contains the existing Team Sync surface and capability discovery. Image Hosting and Invite Tracking are independently optional gateway services.
- The local Image Library stores personal records in the workspace and can reference a managed PNG, an existing project PNG, or a manually entered HTTPS URL. Adding an image does not upload it. Items can be organized into flat, user-named folders and filtered with tags. Team sharing and provider mappings are supported. Workspace backups include the SQLite database and managed files.
- Workspace-backup validation recognizes the current media library, overlay-template, render-version, reusable Markdown, and post-media tables. A restore roundtrip regression confirms library items, templates, Markdown presets, project assets, notes, and managed images survive together.
- The ERLCX composer loads project and local Image Library media independently of optional Server requests. Team availability, campaigns, and shared images arrive in the background; late responses merge with rather than replace local items. This keeps local preview and Markdown editing usable while the team server is slow or unreachable.
- Team project transfers are initiated only by the visible **Sync projects now** action in Server. Project links are configuration only; save, focus, network reconnection, navigation and timers do not upload or download project content.
- Local Image Library records and project-asset manifests load independently in both Assets → Image Library and the ERLCX composer. A slow project-asset query cannot delay local media editing or selection; a project-image load failure leaves the local library usable and reports the missing project-image source.
- The Sync Server now has a team-authenticated Image Hosting gateway. It checks membership for reads, write-capable roles for uploads/sharing, proxies with a server-held key, and sends the asserted Apex user/team scope to the separately deployed Image Provider. Local library uploads require a separate confirmation, default to Personal, and can be explicitly team-shared. Identical normalized PNG bytes are reused within personal/team scope. The Image Provider owns its own service data and public image blobs.
- The Image Library can select up to ten local images for one explicit public upload action. Batches are limited to 48 MB before the Sync Server's 50 MB request cap; each image still follows the provider's 40 MB per-file limit. The gateway forwards transient per-file client IDs so normalized uploads can be matched back to their local records without relying on hashes that may change during PNG conversion. The provider echoes those IDs only in the response and does not store or log them. Personal remains the default scope, partial failures remain selected for retry, and service image deduplication continues by normalized content hash.
- ERLCX drafts support ordered text, image, and invite Markdown blocks. The composer shows rendered images and links plus a safe formatted text preview and expandable Markdown source. Named reusable blocks are stored locally per workspace and can be inserted into other drafts; deleting a saved block leaves existing post snapshots unchanged. Image URL/link edits do not start uploads. The composer can copy the assembled Markdown. Local-only images remain non-copyable until given a public URL. A regression test exercises manual text, linked-image, and Discord-invite Markdown creation and copying with Image Hosting and Invite Tracking unavailable, confirming no optional-service request is made.
- From the Markdown step, the composer can upload local image-library items referenced by image blocks directly to personal Image Hosting and insert returned URLs into the current unsaved draft. The action is explicit, confirmed, limited to ten and does not require leaving the composer; missing/unavailable hosting retains manual URL entry. The draft still requires the user's normal Save action.
- The first local Overlay Studio is integrated into the preview step. It supports saved workspace templates, text/image/shape layers, direct canvas dragging plus precise numeric positioning, local and managed project/library source images, 16:9 ERLCX and 16:10 ExecLab canvases, and batched PNG output directly into a draft. Generated previews also enter the local Image Library as content-addressed versions grouped by source image and target format; identical output bytes reuse their prior version, while changed pixels create a retained new version. Version metadata records the editable text and layer settings without embedding template image bytes. Templates and generated images are workspace-local; public uploads still require the Image Library confirmation. Portable `.apexoverlay.json` templates can be imported as new workspace entries and exported without replacing existing templates. The format is versioned JSON, capped at 25 MB and 50 layers, with embedded PNG/JPEG/WebP layer images capped at 2 MB each. Windows visual review in the running debug app confirmed a managed project image and text layer render on the ERLCX canvas, with the layer controls stacked below the canvas at the tested 1280 px window width. A separate visual review confirmed the ExecLab canvas renders at a 16:10 ratio (888 × 556 px in the tested window). These reviews used an unsaved test draft and did not export previews or save changes. A frontend regression test verifies a two-image ExecLab batch yields two 1920 × 1200 outputs with matching source hashes and target metadata; storage tests verify content-addressed versions and preserved post snapshots. Interactive visual QA of batch export and portable-template import remains outstanding; the import unit test confirms importing a same-named template creates a separate workspace entry and preserves the original.
- The Overlay Studio editor uses an auto-fitting grid with a 520 px minimum column, so the layer panel moves below the canvas whenever both columns do not fit in the remaining workspace area.
- The storage boundary verifies generated PNG dimensions against the selected target format. Regression coverage confirms identical pixels reuse a content version, changed pixels create the next version, saved post snapshots retain their original hash, and backups restore overlay versions with their files.
- Invite Tracking campaign creation and listing are available when configured on the Sync Server and Invite Tracker. Campaigns are team-scoped; aggregate tracked joins and lifetime invite uses are shown. The Discord channel ID is entered in the post composer. Existing manual links remain available while the service is disabled. Pending invite retries persist a team/draft-scoped request UUID and payload fingerprint locally (not the payload or credentials), so a timeout followed by an app restart can safely reuse the server idempotency key. The Tracker source at `D:\ChatGPT\Inviter Bot` is outside this repository and has no Git metadata, so its schema, route, deployment configuration, and migration must be deployed separately from this repository's commit.

## Acceptance criteria

- A clean install with no server URL, account, or service secrets can manage projects and assets, create/edit drafts and overlay previews, paste manual URLs, save, and copy Markdown.
- Turning off or losing any one server capability leaves other app functions and server capabilities usable.
- No image bytes or project data leave the device except through a visible, user-triggered action.
- Public image uploads are explicit; identical bytes reuse a valid provider mapping; link-only edits do not re-upload; pixel/text changes create a new version without rewriting existing posts.
- A selected batch of up to ten images is uploaded in one gateway request, enforces the proxy-size budget before sending, maps each successful URL to the correct local image after normalization, and retains failed selections for retry.
- Invite creation is explicitly confirmed and duplicate-resistant. Manual invite links remain supported.
- A team member can list only campaigns attached to their selected team, and read-only roles cannot create campaigns.
- Team sharing is opt-in, team membership and role are checked on the server, and personal items stay personal by default.
- Backups/restores preserve local media referenced by the library and drafts without including service secrets.
- The overlay editor does not depend on Apex-branded artwork and exports the selected target dimensions in a batch.
- Sync Server service endpoint configuration rejects cleartext HTTP to public hosts while retaining private-network and Docker service-name endpoints. Tests cover local/private IPv4 and IPv6, public hosts, and unspecified addresses.

## Current verification evidence (2026-11-11)

- Desktop publishing, Image Library, and Overlay Studio regression suites pass: 14 tests across three files. The desktop TypeScript/Vite production build succeeds; Vite reports the existing large Notes chunk warning.
- Sync Server formatting, four unit tests, and strict Clippy (`--all-targets -- -D warnings`) pass. Clippy required replacing an explicit upload-count range check and allowing `expect` only in tests that parse fixed known-valid URLs.
- Invite Tracker attribution tests pass (5), workspace type checks pass, and its Next.js production build succeeds. The Image Provider source passes `node --check`; its package has no automated test script.
- These are source-level checks. Live calls through the deployed gateway to both optional services have not been re-run in this review. Interactive batch-export and portable-template import review also remain outstanding.
