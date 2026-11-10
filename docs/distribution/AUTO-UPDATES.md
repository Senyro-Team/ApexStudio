# Automatic updates

Apex Studio checks for updates when the desktop app starts. Users can also check and install updates in **Settings → Updates**. Update payloads are verified with the Tauri updater public key embedded in `apps/desktop/src-tauri/tauri.conf.json`.

## Windows publishing flow

Inno Setup remains the first-install download. Its CLI component adds the selected install directory to the current user's `PATH`; deselecting the component on an Inno maintenance install or uninstall removes only that exact path entry. Other user `PATH` entries are preserved. Open a new terminal after installation so it inherits the updated environment.

When a GitHub Release is published, `.github/workflows/updater-assets.yml` builds a signed Tauri NSIS updater bundle, creates `latest.json`, and uploads both along with the signature to that release. The NSIS bundle includes the Chrome bridge, ERLCX extension, and current CLI executable. An existing CLI install is therefore updated with the desktop app; the install folder remains on `PATH` when the CLI component was selected in the first-install installer.

The updater feed is the public `latest.json` asset on the latest GitHub Release. The already published `v0.1.0` app predates the updater and cannot update itself; `v0.1.1` is the first updater-enabled installation and can receive subsequent signed updates.

## Signing key

The updater private key must stay outside the repository. The matching public key is safe to commit and is embedded in the Tauri configuration. GitHub Actions reads the private key from the repository Actions secret `TAURI_SIGNING_PRIVATE_KEY`; the secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is optional when the private key is password-protected.

Keep a protected backup of the private key. Losing it prevents signing updates trusted by already installed versions. Do not rotate it without a migration plan.

The current GitHub Actions workflow builds Windows x64 updates. Other platforms need their own signed artifacts and platform entries in `latest.json` before enabling their updater builds.
