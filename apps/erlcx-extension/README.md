# Apex Studio ERLCX Assistant

## Install and update

Build with `npm run build:extension` from the repository root. Extract `dist/apex-erlcx-extension-0.1.0.zip` into a permanent folder. In Chrome, open Extensions, enable Developer mode, choose Load unpacked and select that folder. No Web Store account is required. For updates, replace the folder contents with the new build, then choose Reload on the extension card. Refresh an existing ERLCX upload tab after updates.

Installation is manual; the build does not install an extension or change browser settings. Keep the Apex Studio branding and bundled brand notice with this assistant.

## Use

Upload assigned livery textures using Apex Studio’s Upload to Roblox action after configuring Settings → Roblox. In the project’s Publishing tab, select the liveries to import their saved decal IDs, fill Details, choose 2-10 separate PNG previews, review and save the draft, then Export for ERLCX extension. Manual existing decal IDs remain available. Open ERLCX's upload wizard and import the bundle in this extension. Review its public-content preview. Invoke Fill current step for each wizard page, inspect the report and click Continue yourself.

Existing text is preserved unless Replace existing text fields is explicitly checked. Existing images are never removed automatically. Category assistance supports the livery-only wizard. The extension does not change Post as, paid settings, agreements, scheduling, Save Draft, Continue or Publish. Unknown layouts stop with a review message. Dynamic Codes controls and portalled colour pickers are fixture-tested and checked against the current public upload-page script. Fresh live browser filling of Codes/Media is still pending because the research connector cannot attach images. Check every field before continuing.

Undo filled fields is best-effort on the same page: it restores only fields still matching the values Apex wrote. It retains subsequent user edits. Added rows remain for manual review; uploaded images must be removed manually. Reloading the page clears page-side undo state.

## Privacy and retention

The imported public bundle and its selected images are kept in this extension's IndexedDB across browser restarts until Clear draft. Importing a new bundle replaces that stored draft. Chrome clearing ordinary browsing data does not necessarily remove extension storage. Clear draft removes the extension's stored bundle and attempts to clear staged/undo data on the current ERLCX tab; it does not erase fields already written to ERLCX. Reload other previously filled ERLCX tabs to clear their in-memory undo state.

The assistant declares only activeTab and scripting. It has no background worker, host permissions, cookies, history, debugger, network interception, telemetry, external scripts or account credentials. No data is transferred from the imported bundle until the user invokes Fill current step. The content adapter checks the HTTPS ERLCX origin and /upload path. ERLCX may upload images immediately when its media input changes; this is why filling is always explicit.

The publishing ZIP is a strict version-1 ZIP_STORED format: public.json plus 2-10 PNG files named by SHA-256. Imports reject unexpected paths/fields, duplicates, unsupported codecs, excessive sizes, invalid dimensions and mismatched hashes. The format contains no project notes, unrelated assets or filesystem paths.

Chrome references: https://developer.chrome.com/docs/extensions/develop/concepts/activeTab and https://developer.chrome.com/docs/extensions/develop/concepts/storage-and-cookies . Current implementation status and provider limits are documented in ../../docs/phases/PHASE-05.md.
