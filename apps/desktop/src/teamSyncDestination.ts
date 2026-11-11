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
