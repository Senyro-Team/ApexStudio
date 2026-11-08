# Apex Studio v0.1.1

This update adds the first in-app update flow and broadens the ERLCX publishing handoff.

## Highlights

- ERLCX handoff now supports Chromium browsers, including Chrome, Microsoft Edge and Opera GX. Browser user-data folders and profile names can be configured separately.
- Apex Studio checks for signed updates at startup. Users can review and install them in **Settings → Updates**.
- Team synchronization now limits concurrent blob uploads and total blob storage, and cleans up expired upload sessions and orphaned blobs.
- The Windows installer remains per-user and includes the Chrome bridge and ERLCX extension package.

## Upgrade

Run the Windows installer to install or update Apex Studio. Version `0.1.0` predates the in-app updater and must be upgraded manually to `0.1.1`; later versions can be delivered through the signed updater.

The ERLCX browser extension still requires its manual browser setup. Roblox Group Revenue continues to use local CSV imports.
