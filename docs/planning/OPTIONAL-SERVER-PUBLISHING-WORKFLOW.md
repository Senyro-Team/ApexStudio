# Optional Server and Post Preparation Workflow

## Product contract

Apex Studio remains a local-first desktop application. Project management, notes, project assets, overlay composition, draft authoring, and Markdown generation work without an account or server connection. The user can complete the same post-preparation workflow manually by importing local images, pasting public image URLs and invite URLs, ordering the content, previewing it, saving the draft, and copying the resulting Markdown.

Server-backed services are optional conveniences. A disconnected, unavailable, or unconfigured service must not prevent local editing or export. The app must never upload project content in the background. Uploading a file to a public image service requires an explicit action and clear confirmation that the resulting link is public.

## App information architecture

- Add **Server** to main navigation. It contains the existing Team Sync connection and independently configured capabilities for Image Hosting and Invite Tracking. Each service displays its own connection and permission state. Missing services are simply unavailable; there is no server requirement for other app modules.
- Keep app appearance, workspace, backups, Chrome, and Roblox configuration in **Settings**. Team selection and shared-service setup live under **Server**.
- Add an **Image Library** view under **Assets** for user-owned reusable post media. Project assets remain project-scoped; users may add a project asset to the library without moving or altering its source.
- Extend **ERLCX** with a post composer organized into content, livery codes, media, and final Markdown. Keep the composer capable of manual URLs and local-only preview. Add ExecLab as a separate target adapter later, reusing the same content model while respecting its own dimensions and Markdown rules.

## Authentication and service boundaries

Use the existing Sync Server account and team membership as the one app-to-server authentication system. Current Sync Server sessions are opaque database-backed bearer tokens, so do not copy them into Image Provider or Invite Tracker databases or expose their service credentials to the desktop app.

The authenticated server API should authorize each operation and act as a gateway to optional service APIs using server-held, narrowly scoped credentials. Each service keeps its own database, file storage, rate limits, and operational logs. Enforce team membership and role permissions at the gateway; never infer a user's team ownership from a client-supplied ID. Project sync, image hosting, and invite tracking are separate capabilities and can be enabled independently.

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
5. **Invite tracking integration:** add scoped campaign create/list/status operations and post metadata. Show aggregate attribution limits and make ambiguous creates recoverable rather than retrying blindly.
6. **Team library sharing and release review:** add explicit personal/team visibility, permission checks, backup/restore, offline and unavailable-service behavior, then add ExecLab target support.

## Implementation checkpoints

- The Server navigation entry now contains the existing Team Sync surface. Image Hosting and Invite Tracking are documented as future optional capabilities and are not presented as working integrations.
- The local Image Library stores personal records in the workspace and can reference a managed PNG, an existing project PNG, or a manually entered HTTPS URL. Adding an image does not upload it. Explicit team sharing, folders/tags, remote provider mappings, and backup packaging still need implementation.
- ERLCX drafts now support ordered text, image, and invite Markdown blocks. The draft snapshot retains references to local library image blobs, and the composer can copy the assembled Markdown. Local-only images remain non-copyable until given a public URL. Server upload automation and the reusable overlay editor remain outstanding.

## Acceptance criteria

- A clean install with no server URL, account, or service secrets can manage projects and assets, create/edit drafts and overlay previews, paste manual URLs, save, and copy Markdown.
- Turning off or losing any one server capability leaves other app functions and server capabilities usable.
- No image bytes or project data leave the device except through a visible, user-triggered action.
- Public image uploads are explicit; identical bytes reuse a valid provider mapping; link-only edits do not re-upload; pixel/text changes create a new version without rewriting existing posts.
- Invite creation is explicitly confirmed and duplicate-resistant. Manual invite links remain supported.
- Team sharing is opt-in, team membership and role are checked on the server, and personal items stay personal by default.
- Backups/restores preserve local media referenced by the library and drafts without including service secrets.
- The overlay editor does not depend on Apex-branded artwork and exports the selected target dimensions in a batch.
