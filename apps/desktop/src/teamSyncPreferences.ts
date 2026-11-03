export const AUTO_SYNC_KEY = "apex-studio.team-sync.auto-sync";
const CHANGED_EVENT = "apex-auto-sync-changed";

export function autoSyncEnabled() {
  try {
    // Preserve automatic synchronization for devices configured before this option existed.
    return localStorage.getItem(AUTO_SYNC_KEY) !== "false";
  } catch {
    return true;
  }
}

export function setAutoSyncEnabled(enabled: boolean) {
  localStorage.setItem(AUTO_SYNC_KEY, String(enabled));
  window.dispatchEvent(new Event(CHANGED_EVENT));
}

export function subscribeAutoSync(listener: () => void) {
  const storageChanged = (event: StorageEvent) => {
    if (event.key === AUTO_SYNC_KEY || event.key === null) listener();
  };
  window.addEventListener(CHANGED_EVENT, listener);
  window.addEventListener("storage", storageChanged);
  return () => {
    window.removeEventListener(CHANGED_EVENT, listener);
    window.removeEventListener("storage", storageChanged);
  };
}
