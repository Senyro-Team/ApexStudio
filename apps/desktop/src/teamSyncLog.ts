import type { SyncEntry, SyncReport } from "./bridge";

export interface TeamSyncLogItem {
  at: string;
  source: "manual" | "automatic";
  summary: string;
  entries: SyncEntry[];
  error: string;
}

const MAX_ITEMS = 50;
const key = (serverUrl: string, teamId: string) =>
  `apex-studio.team-sync.log:${serverUrl.trim().replace(/\/$/, "")}:${teamId}`;

export function syncSummary(report: SyncReport) {
  if (report.errors.length) return `${report.errors.length} project error${report.errors.length === 1 ? "" : "s"} · ${report.uploaded} uploaded · ${report.downloaded} downloaded`;
  const skipped = report.entries?.filter((entry) => entry.status === "skipped").length ?? 0;
  if (report.uploaded || report.downloaded) {
    const completed = `${report.uploaded} uploaded · ${report.downloaded} downloaded · ${report.upToDate ?? 0} already current`;
    return skipped ? `${completed} · ${skipped} need attention` : completed;
  }
  if (skipped) {
    return `${skipped} project${skipped === 1 ? "" : "s"} need attention · ${report.remoteProjects ?? 0} team project${report.remoteProjects === 1 ? "" : "s"} available`;
  }
  if (report.sharedLocal === 0 && report.remoteProjects === 0) {
    return "No shared projects yet. Enable “Synchronize with team server” for a project, then choose Sync projects now.";
  }
  if ((report.remoteProjects ?? 0) > 0 && report.checked === 0) {
    return `${report.remoteProjects} team project${report.remoteProjects === 1 ? "" : "s"} found, but none were synchronized. See the sync log for details.`;
  }
  if (report.checked !== undefined) return `${report.upToDate ?? 0} of ${report.checked} projects already up to date.`;
  return `${report.uploaded} uploaded · ${report.downloaded} downloaded`;
}

export function readTeamSyncLog(serverUrl: string, teamId: string): TeamSyncLogItem[] {
  if (!serverUrl || !teamId) return [];
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key(serverUrl, teamId)) ?? "[]");
    return Array.isArray(value)
      ? value.filter((item): item is TeamSyncLogItem =>
          item !== null && typeof item === "object" && typeof item.at === "string" && typeof item.summary === "string",
        ).slice(0, MAX_ITEMS)
      : [];
  } catch {
    return [];
  }
}

function writeTeamSyncLog(serverUrl: string, teamId: string, item: TeamSyncLogItem) {
  if (!serverUrl || !teamId) return;
  try {
    const previous = readTeamSyncLog(serverUrl, teamId);
    localStorage.setItem(key(serverUrl, teamId), JSON.stringify([item, ...previous].slice(0, MAX_ITEMS)));
    window.dispatchEvent(new CustomEvent("apex-team-sync-log", { detail: { serverUrl, teamId } }));
  } catch {
    // Sync still works when local log storage is unavailable.
  }
}

export function recordTeamSyncReport(serverUrl: string, teamId: string, source: "manual", report: SyncReport) {
  writeTeamSyncLog(serverUrl, teamId, {
    at: new Date().toISOString(), source, summary: syncSummary(report), entries: report.entries ?? [], error: "",
  });
}

export function recordTeamSyncError(serverUrl: string, teamId: string, source: "manual", error: string) {
  writeTeamSyncLog(serverUrl, teamId, {
    at: new Date().toISOString(), source, summary: "Synchronization failed", entries: [], error,
  });
}
