import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import useTeamProjectSync, {
  synchronizeTeamProjects,
} from "./useTeamProjectSync";
import { syncProjects } from "./bridge";
import { setAutoSyncEnabled } from "./teamSyncPreferences";

vi.mock("./bridge", () => ({ desktopAvailable: true, syncProjects: vi.fn() }));
const report = { uploaded: 0, downloaded: 0, errors: [] };
beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  localStorage.setItem(
    "apex-studio.team-sync.server-url",
    "https://sync.example.test",
  );
  localStorage.setItem("apex-studio.team-sync.active-team", "team");
  vi.mocked(syncProjects).mockReset().mockResolvedValue(report);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
const advance = async (ms: number) => {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
};

it("debounces saved changes and checks for remote changes two seconds after completion", async () => {
  renderHook(() => useTeamProjectSync(false));
  await advance(249);
  expect(syncProjects).not.toHaveBeenCalled();
  await advance(1);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  await advance(1999);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  await advance(1);
  expect(syncProjects).toHaveBeenCalledTimes(2);
  act(() => {
    window.dispatchEvent(new Event("apex-project-sync-request"));
  });
  await advance(200);
  act(() => {
    window.dispatchEvent(new Event("apex-project-sync-request"));
  });
  await advance(249);
  expect(syncProjects).toHaveBeenCalledTimes(2);
  await advance(1);
  expect(syncProjects).toHaveBeenCalledTimes(3);
});

it("persists the auto-sync switch immediately while keeping manual sync available", async () => {
  setAutoSyncEnabled(false);
  renderHook(() => useTeamProjectSync(false));
  await advance(10_000);
  expect(syncProjects).not.toHaveBeenCalled();
  act(() => {
    setAutoSyncEnabled(true);
  });
  await advance(250);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  act(() => {
    setAutoSyncEnabled(false);
  });
  await advance(10_000);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  await act(async () => {
    await synchronizeTeamProjects();
  });
  expect(syncProjects).toHaveBeenCalledTimes(2);
});

it("pauses for unsaved edits and starts promptly when editing finishes", async () => {
  const { rerender } = renderHook(({ paused }) => useTeamProjectSync(paused), {
    initialProps: { paused: true },
  });
  await advance(5000);
  expect(syncProjects).not.toHaveBeenCalled();
  rerender({ paused: false });
  await advance(250);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  rerender({ paused: true });
  await advance(5000);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  rerender({ paused: false });
  await advance(250);
  expect(syncProjects).toHaveBeenCalledTimes(2);
});

it("coalesces requests during a slow transfer without queuing overlapping runs", async () => {
  let finish!: (value: typeof report) => void;
  vi.mocked(syncProjects).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  renderHook(() => useTeamProjectSync(false));
  await advance(250);
  act(() => {
    window.dispatchEvent(new Event("apex-project-sync-request"));
    window.dispatchEvent(new Event("focus"));
  });
  await advance(20_000);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  await act(async () => {
    finish(report);
  });
  await advance(249);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  await advance(1);
  expect(syncProjects).toHaveBeenCalledTimes(2);
});

it("backs off on connection failures and retries promptly when connectivity returns", async () => {
  vi.mocked(syncProjects).mockRejectedValueOnce("Offline");
  renderHook(() => useTeamProjectSync(false));
  await advance(250);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  await advance(3999);
  expect(syncProjects).toHaveBeenCalledTimes(1);
  act(() => {
    window.dispatchEvent(new Event("online"));
  });
  await advance(250);
  expect(syncProjects).toHaveBeenCalledTimes(2);
});
