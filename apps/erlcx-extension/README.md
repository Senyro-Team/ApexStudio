# Apex Studio ERLCX Assistant

## Install and update

Build with `npm run build:extension`. Keep `dist/erlcx-extension` in a permanent location or extract the versioned ZIP into your existing extension folder. In Chrome or Opera GX, open the extensions page, enable Developer mode, choose Load unpacked and select that folder. For updates keep the same folder and choose Reload, preserving the installed extension ID. No Web Store account is required.

Installation is manual; the build does not install an extension or change browser settings. Extension source code is covered by the repository's [Apache-2.0 licence](../../LICENSE). Apex Studio marks are excluded from that code licence and governed by the [brand policy](../../BRANDING.md): a modified fork that retains the marks must clearly identify itself as unofficial; forks may remove the marks and rebrand.

## Use

Upload textures in the global Roblox tab after configuring Settings → Roblox. In the global ERLCX area, create or open a draft, select up to twenty liveries across projects, fill Details and choose 2–10 PNG previews. Combine previews from your PC and assets from any project. Save and review. Missing IDs prevent browser preparation but do not prevent saving; Refresh Roblox IDs is explicit.

Build the Windows installer with `npm run build:windows-installer`; it includes `apex-chrome-bridge.exe` beside `apex-desktop.exe`. For a standalone build, use `cargo build --release --locked -p apex-desktop --features custom-protocol` and keep both executables together. In Settings → ERLCX · Browser enter the executable for Chrome, Opera GX or another Chromium browser, its user data folder, the profile folder name (Default or Profile 1) and the installed Apex extension ID. Save, then Check connection. Opera GX uses the Chrome native-messaging host registry location and its own profile root (commonly under `%APPDATA%\Opera Software\Opera GX Stable`); saving Opera settings requests Windows elevation to register that host. Local paths stay outside workspaces, backups and Git.

Prepare on ERLCX opens a new upload tab and fills the free livery workflow. Each step is verified before Continue, including preview upload completion. The assistant stops at Submit: agreement, scheduling, Save Draft and Publish remain manual. Login, CAPTCHA, conflicting fields, unknown layouts and validation problems pause it. Resume/Cancel are available in the app and on the assistant page. Only one handoff runs at a time. App restart or workspace restore requires a new handoff; worker restart requires explicit Resume.

The manual fallback remains: Export for ERLCX extension, import the .apexpublish bundle in the popup, then Fill current step and Continue yourself. Existing text is retained unless Replace is selected. Images are never removed or re-uploaded blindly. The popup cannot change the bundle during an active automatic handoff.

Undo filled fields is best-effort on the same page: it restores only fields still matching the values Apex wrote. It retains subsequent user edits. Added rows remain for manual review; uploaded images must be removed manually. Reloading the page clears page-side undo state.

## Privacy and retention

The imported public bundle and its selected images are kept in this extension's IndexedDB across browser restarts until Clear draft. Importing a new bundle replaces that stored draft. Chrome clearing ordinary browsing data does not necessarily remove extension storage. Clear draft removes the extension's stored bundle and attempts to clear staged/undo data on the current ERLCX tab; it does not erase fields already written to ERLCX. Reload other previously filled ERLCX tabs to clear their in-memory undo state.

Permissions are activeTab, scripting, nativeMessaging and website access limited to https://erlcx.com/*. There are no cookies, browser history, debugger, telemetry, remote scripts or account credentials. The standalone bridge serves only the app-approved immutable bundle over sequential chunks below Chrome's 1 MiB limit. An expiring one-time token starts the transfer; later requests require its session. The helper cannot open arbitrary files or the workspace database. The content adapter checks the HTTPS ERLCX origin and /upload path. Preview images may reach ERLCX as soon as the media input changes, before publication.

The publishing ZIP is a strict version-1 ZIP_STORED format: public.json plus 2-10 PNG files named by SHA-256. Imports reject unexpected paths/fields, duplicates, unsupported codecs, excessive sizes, invalid dimensions and mismatched hashes. The format contains no project notes, unrelated assets or filesystem paths.

Chrome protocol reference: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging . Implementation and verification: ../../docs/planning/GLOBAL-ERLCX-IMPLEMENTATION.md.
