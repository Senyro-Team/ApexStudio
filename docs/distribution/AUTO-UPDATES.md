# Automatic updates

Apex Studio checks for updates when the desktop app starts. Users can also check and install updates in **Settings → Updates**. Update payloads are verified with the Tauri updater public key embedded in `apps/desktop/src-tauri/tauri.conf.json`.

## Windows publishing flow

Inno Setup remains the first-install download. When a GitHub Release is published, `.github/workflows/updater-assets.yml` builds a signed Tauri NSIS updater bundle, creates `latest.json`, and uploads both along with the signature to that release. The NSIS bundle includes the Chrome bridge and ERLCX extension resources needed by the installed app.

The updater feed is the public `latest.json` asset on the latest GitHub Release. Releases published before the updater was added do not contain this asset. Therefore, the already published `v0.1.0` app cannot update itself; users need a later updater-enabled app installation before in-app updates can be received.

## Signing key

The updater private key must stay outside the repository. The matching public key is safe to commit and is embedded in the Tauri configuration. GitHub Actions reads the private key from the repository Actions secret `TAURI_SIGNING_PRIVATE_KEY`; the secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is optional when the private key is password-protected.

Keep a protected backup of the private key. Losing it prevents signing updates trusted by already installed versions. Do not rotate it without a migration plan.

The current GitHub Actions workflow builds Windows x64 updates. Other platforms need their own signed artifacts and platform entries in `latest.json` before enabling their updater builds.
