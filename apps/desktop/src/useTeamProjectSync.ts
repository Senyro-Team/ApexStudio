import { useEffect, useSyncExternalStore } from "react";
import { desktopAvailable, syncProjects, type SyncReport } from "./bridge";
import { autoSyncEnabled, subscribeAutoSync } from "./teamSyncPreferences";
import { recordTeamSyncError, recordTeamSyncReport } from "./teamSyncLog";

export const AUTO_SYNC_DEBOUNCE_MS = 250;
export const AUTO_SYNC_POLL_MS = 2000;

export function savedSyncDestination() {
  try {
    return {
      serverUrl: localStorage.getItem("apex-studio.team-sync.server-url") ?? "",
      teamId: localStorage.getItem("apex-studio.team-sync.active-team") ?? "",
    };
  } catch {
    return { serverUrl: "", teamId: "" };
  }
}
let inFlight: Promise<SyncReport> | null = null;
export async function synchronizeTeamProjects() {
  const { serverUrl, teamId } = savedSyncDestination();
  if (!desktopAvailable || !serverUrl || !teamId) return;
  if (inFlight) return inFlight;
  inFlight = syncProjects(serverUrl, teamId);
  try {
    const report = await inFlight;
    recordTeamSyncReport(serverUrl, teamId, "automatic", report);
    window.dispatchEvent(
      new CustomEvent("apex-projects-synced", { detail: report }),
    );
    return report;
  } catch (reason) {
    recordTeamSyncError(serverUrl, teamId, "automatic", typeof reason === "string" ? reason : "Project sync could not finish.");
    throw reason;
  } finally {
    inFlight = null;
  }
}
export default function useTeamProjectSync(paused: boolean) {
  const enabled = useSyncExternalStore(
    subscribeAutoSync,
    autoSyncEnabled,
    () => true,
  );
  useEffect(() => {
    if (!desktopAvailable || paused || !enabled) return;
    let active = true;
    let running = false;
    let pending = false;
    let failures = 0;
    let timer: ReturnType<typeof setTimeout>;
    const schedule = (delay: number) => {
      clearTimeout(timer);
      if (active) timer = setTimeout(() => void run(), delay);
    };
    const run = async () => {
      if (!active || !autoSyncEnabled()) return;
      if (running) {
        pending = true;
        return;
      }
      running = true;
      try {
        await synchronizeTeamProjects();
        failures = 0;
      } catch (reason: unknown) {
        failures = Math.min(failures + 1, 4);
        const message =
          typeof reason === "string"
            ? reason
            : "Project sync could not finish. Check Settings → Team sync.";
        window.dispatchEvent(
          new CustomEvent("apex-project-sync-error", { detail: message }),
        );
      } finally {
        running = false;
        // Start the next poll after completion; slow transfers never build a request queue.
        const delay = pending
          ? AUTO_SYNC_DEBOUNCE_MS
          : AUTO_SYNC_POLL_MS * 2 ** failures;
        pending = false;
        schedule(delay);
      }
    };
    const request = () => {
      if (running) pending = true;
      else schedule(AUTO_SYNC_DEBOUNCE_MS);
    };
    // Coalesce saves, focus changes, and StrictMode's effect replay.
    schedule(AUTO_SYNC_DEBOUNCE_MS);
    window.addEventListener("focus", request);
    window.addEventListener("apex-project-sync-request", request);
    window.addEventListener("online", request);
    return () => {
      active = false;
      clearTimeout(timer);
      window.removeEventListener("focus", request);
      window.removeEventListener("apex-project-sync-request", request);
      window.removeEventListener("online", request);
    };
  }, [paused, enabled]);
}
