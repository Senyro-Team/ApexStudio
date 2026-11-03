import { useEffect } from "react";
import { desktopAvailable, syncProjects } from "./bridge";

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
let inFlight: Promise<unknown> | null = null;
export async function synchronizeTeamProjects() {
  const { serverUrl, teamId } = savedSyncDestination();
  if (!desktopAvailable || !serverUrl || !teamId || inFlight) return;
  inFlight = syncProjects(serverUrl, teamId);
  try {
    const report = await inFlight;
    window.dispatchEvent(
      new CustomEvent("apex-projects-synced", { detail: report }),
    );
    return report;
  } finally {
    inFlight = null;
  }
}
export default function useTeamProjectSync(paused: boolean) {
  useEffect(() => {
    if (!desktopAvailable || paused) return;
    let active = true;
    const run = () => {
      if (!active) return;
      void synchronizeTeamProjects().catch((reason: unknown) => {
        const message =
          typeof reason === "string"
            ? reason
            : "Project sync could not finish. Check Settings → Team sync.";
        window.dispatchEvent(
          new CustomEvent("apex-project-sync-error", { detail: message }),
        );
      });
    };
    // A short delay coalesces local save/import events and StrictMode effect replay.
    const start = window.setTimeout(run, 1000);
    const timer = window.setInterval(run, 30_000);
    window.addEventListener("focus", run);
    window.addEventListener("apex-project-sync-request", run);
    return () => {
      active = false;
      clearTimeout(start);
      clearInterval(timer);
      window.removeEventListener("focus", run);
      window.removeEventListener("apex-project-sync-request", run);
    };
  }, [paused]);
}
