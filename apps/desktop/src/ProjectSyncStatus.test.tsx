import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ProjectSyncStatus from "./ProjectSyncStatus";
import {
  projectSyncSettings,
  setProjectSync,
  resolveProjectSync,
  type ProjectSyncSettings,
} from "./bridge";
vi.mock("./bridge", () => ({
  desktopAvailable: true,
  projectSyncSettings: vi.fn(),
  setProjectSync: vi.fn(),
  resolveProjectSync: vi.fn(),
  syncProjects: vi
    .fn()
    .mockResolvedValue({ uploaded: 0, downloaded: 0, errors: [] }),
}));
const binding: ProjectSyncSettings = {
  projectId: "project",
  origin: "https://sync.example.test",
  serverInstance: "instance",
  teamId: "team",
  enabled: true,
  localHash: "hash",
  revision: 2,
  cursor: 2,
  message: "Synced",
  syncedAt: null,
};
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(projectSyncSettings).mockResolvedValue(binding);
  vi.mocked(setProjectSync).mockResolvedValue({ ...binding, enabled: false });
  vi.mocked(resolveProjectSync).mockResolvedValue(undefined);
});
afterEach(cleanup);
it("disables project synchronization without removing the local or remote project", async () => {
  const user = userEvent.setup();
  render(<ProjectSyncStatus projectId="project" />);
  await screen.findByText("Synced");
  await user.click(
    screen.getByRole("checkbox", { name: "Synchronize with team server" }),
  );
  expect(setProjectSync).toHaveBeenCalledWith("project", {
    enabled: false,
    serverUrl: binding.origin,
    teamId: binding.teamId,
  });
  await screen.findByText(/Existing server copies are kept/);
});
it("requires a deliberate version choice before resolving a conflict", async () => {
  vi.mocked(projectSyncSettings).mockResolvedValue({
    ...binding,
    message: "Conflict: both devices changed.",
  });
  const user = userEvent.setup();
  render(<ProjectSyncStatus projectId="project" />);
  await user.click(
    await screen.findByRole("button", { name: "Use server version" }),
  );
  expect(resolveProjectSync).not.toHaveBeenCalled();
  expect(
    screen.getByRole("alertdialog", { name: "Resolve project sync conflict" }),
  ).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Confirm version" }));
  expect(resolveProjectSync).toHaveBeenCalledWith("project", true);
});
