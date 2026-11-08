import { check, type Update } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";

type CheckState = "checking" | "available" | "current" | "error";

export default function UpdaterSettings({
  enabled,
  visible,
}: {
  enabled: boolean;
  visible: boolean;
}) {
  const [state, setState] = useState<CheckState>("checking");
  const [update, setUpdate] = useState<Update | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const didAutoCheck = useRef(false);

  const checkForUpdate = useCallback(async () => {
    if (!enabled || busy) return;
    setBusy(true);
    setMessage("");
    try {
      update?.close();
      const available = await check();
      setUpdate(available);
      setState(available ? "available" : "current");
    } catch (error) {
      setState("error");
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [busy, enabled, update]);

  useEffect(() => {
    if (enabled && !didAutoCheck.current) {
      didAutoCheck.current = true;
      void checkForUpdate();
    }
    // Check once when the app starts; manual checks use the same callback.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  const installUpdate = async () => {
    if (!update || busy) return;
    setBusy(true);
    setMessage("Downloading update…");
    let downloaded = 0;
    let total = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          setProgress(total > 0 ? 0 : null);
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setProgress(
            total > 0
              ? Math.min(100, Math.round((downloaded / total) * 100))
              : null,
          );
        } else if (event.event === "Finished") {
          setProgress(100);
        }
      });
      setMessage(
        "Update installed. Apex Studio will close to finish the update.",
      );
      setUpdate(null);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  if (!visible) return null;

  return (
    <section
      className="updater-settings"
      id="settings-updates"
      aria-labelledby="updates-heading"
    >
      <div>
        <h2 id="updates-heading">Updates</h2>
        <p>
          Automatic update checks run when Apex Studio starts. Update packages
          are verified with Apex Studio’s signing key.
        </p>
      </div>
      {state === "available" && update ? (
        <div className="updater-available">
          <p>
            <strong>Apex Studio {update.version} is available.</strong>
          </p>
          {update.body && <p className="field-hint">{update.body}</p>}
          <button
            className="primary"
            type="button"
            onClick={() => void installUpdate()}
            disabled={busy}
          >
            {busy ? "Installing update…" : "Download and install"}
          </button>
        </div>
      ) : (
        <p role="status" className="field-hint">
          {busy || state === "checking"
            ? "Checking for updates…"
            : state === "current"
              ? "Apex Studio is up to date."
              : message || "Could not check for updates."}
        </p>
      )}
      {progress !== null && (
        <progress
          className="updater-progress"
          max={100}
          value={progress}
          aria-label="Update download progress"
        />
      )}
      {state === "error" && (
        <button
          className="secondary"
          type="button"
          onClick={() => void checkForUpdate()}
          disabled={busy}
        >
          Check again
        </button>
      )}
      {state !== "error" && state !== "available" && (
        <button
          className="secondary"
          type="button"
          onClick={() => void checkForUpdate()}
          disabled={busy}
        >
          Check for updates
        </button>
      )}
    </section>
  );
}
