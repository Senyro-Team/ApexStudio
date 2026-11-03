import { useEffect, useState, useSyncExternalStore } from "react";
import { autoSyncEnabled, subscribeAutoSync } from "./teamSyncPreferences";
import {
  desktopAvailable,
  projectSyncSettings,
  setProjectSync,
  syncProjects,
  resolveProjectSync,
  type ProjectSyncSettings,
} from "./bridge";
import { savedSyncDestination } from "./useTeamProjectSync";

export function syncErrorText(reason: unknown) {
  return typeof reason === "string"
    ? reason
    : reason instanceof Error
      ? reason.message
      : "Sync could not finish. Try again.";
}
export default function ProjectSyncStatus({
  projectId,
}: {
  projectId: string;
}) {
  const [binding, setBinding] = useState<ProjectSyncSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [resolution, setResolution] = useState<"server" | "local" | null>(null);
  const automaticSync = useSyncExternalStore(
    subscribeAutoSync,
    autoSyncEnabled,
    () => true,
  );
  useEffect(() => {
    if (!desktopAvailable) {
      setLoading(false);
      return;
    }
    let active = true;
    const refresh = () =>
      void projectSyncSettings(projectId)
        .then((b) => {
          if (active) setBinding(b);
        })
        .catch((e: unknown) => {
          if (active) setError(syncErrorText(e));
        })
        .finally(() => {
          if (active) setLoading(false);
        });
    refresh();
    window.addEventListener("apex-projects-synced", refresh);
    return () => {
      active = false;
      window.removeEventListener("apex-projects-synced", refresh);
    };
  }, [projectId]);
  const destination = savedSyncDestination();
  async function toggle(enabled: boolean) {
    setBusy(true);
    setError("");
    try {
      setBinding(
        await setProjectSync(projectId, {
          enabled,
          serverUrl: binding?.origin ?? destination.serverUrl,
          teamId: binding?.teamId ?? destination.teamId,
        }),
      );
      window.dispatchEvent(new Event("apex-project-sync-request"));
    } catch (e) {
      setError(syncErrorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function syncNow() {
    if (!binding) return;
    setBusy(true);
    setError("");
    try {
      const report = await syncProjects(binding.origin, binding.teamId);
      setBinding(await projectSyncSettings(projectId));
      if (report.errors.length) setError(report.errors.join("\n"));
      window.dispatchEvent(
        new CustomEvent("apex-projects-synced", { detail: report }),
      );
    } catch (e) {
      setError(syncErrorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function resolve() {
    if (!resolution) return;
    setBusy(true);
    setError("");
    try {
      await resolveProjectSync(projectId, resolution === "server");
      setResolution(null);
      setBinding(await projectSyncSettings(projectId));
      window.dispatchEvent(new Event("apex-projects-synced"));
    } catch (e) {
      setError(syncErrorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      className="project-sync-options"
      aria-label="Project synchronization"
    >
      <h3>Synchronization</h3>
      <label className="check-label">
        <input
          type="checkbox"
          checked={binding?.enabled ?? false}
          disabled={
            !desktopAvailable ||
            loading ||
            busy ||
            (!binding && !(destination.serverUrl && destination.teamId))
          }
          onChange={(e) => void toggle(e.target.checked)}
        />{" "}
        Synchronize with team server
      </label>
      <p className="field-hint">
        {binding?.enabled
          ? binding.message
          : "Local only on this device. Existing server copies are kept when you turn sync off."}
      </p>
      {binding?.enabled && !automaticSync && (
        <p className="field-hint">
          Automatic sync is off. Use Sync now to send and receive changes.
        </p>
      )}
      {binding?.origin && (
        <p className="field-hint">
          {binding.origin}
          {binding.syncedAt
            ? ` · Last synced ${new Date(binding.syncedAt).toLocaleString()}`
            : ""}
        </p>
      )}
      {!binding && (
        <p className="field-hint">
          Choose your server and team in Settings → Team sync.
        </p>
      )}
      {binding?.enabled && (
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={() => void syncNow()}
        >
          {busy ? "Synchronizing…" : "Sync now"}
        </button>
      )}
      {binding?.enabled && binding.message.startsWith("Conflict:") && (
        <div className="button-row">
          <button
            className="secondary"
            disabled={busy}
            onClick={() => setResolution("local")}
          >
            Use this device's version
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => setResolution("server")}
          >
            Use server version
          </button>
        </div>
      )}
      {resolution && (
        <div
          className="confirm-panel"
          role="alertdialog"
          aria-label="Resolve project sync conflict"
        >
          <p>
            {resolution === "server"
              ? "Replace this project's local contents with the server version? A workspace backup will be saved first."
              : "Replace the shared project with this device's version? The previous version stays in the server's revision history."}
          </p>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void resolve()}
          >
            Confirm version
          </button>
          <button
            className="text-button"
            disabled={busy}
            onClick={() => setResolution(null)}
          >
            Cancel
          </button>
        </div>
      )}
      {error && (
        <p role="alert" className="inline-error pre-wrap">
          {error}
        </p>
      )}
    </section>
  );
}
