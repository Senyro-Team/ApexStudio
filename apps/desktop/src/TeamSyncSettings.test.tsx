import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import TeamSyncSettings from "./TeamSyncSettings";
import {
  syncConnectionStatus,
  syncListMembers,
  syncProjects,
  type SyncConnectionStatus,
} from "./bridge";
import { autoSyncEnabled } from "./teamSyncPreferences";
vi.mock("./bridge", () => ({
  desktopAvailable: true,
  syncConnectionStatus: vi.fn(),
  syncProjects: vi.fn(),
  syncAcceptInvitation: vi.fn(),
  syncCheckServer: vi.fn(),
  syncCreateInvitation: vi.fn(),
  syncDisconnect: vi.fn(),
  syncInitialSetup: vi.fn(),
  syncSignIn: vi.fn(),
  syncListMembers: vi.fn(),
}));
const connection: SyncConnectionStatus = {
  server: {
    product: "Apex Sync Server",
    origin: "https://sync.example.test",
    apiVersion: "v1",
    serverVersion: "fixture",
    serverInstanceId: "fixture",
  },
  user: {
    id: "user",
    email: "designer@example.test",
    displayName: "Designer",
    teams: [{ id: "team", name: "Design team", role: "designer" }],
  },
  sessionExpired: false,
  savedSessionForDifferentServer: false,
};
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  localStorage.setItem(
    "apex-studio.team-sync.server-url",
    connection.server.origin,
  );
  localStorage.setItem("apex-studio.team-sync.active-team", "team");
  vi.mocked(syncConnectionStatus).mockResolvedValue(connection);
  vi.mocked(syncProjects).mockResolvedValue({
    uploaded: 1,
    downloaded: 0,
    errors: [],
  });
  vi.mocked(syncListMembers).mockResolvedValue([
    { id: "owner", email: "owner@example.test", displayName: "Studio Owner", role: "owner" },
    { id: "user", email: "designer@example.test", displayName: "Designer", role: "designer" },
  ]);
});
afterEach(cleanup);

it("restores an available team when the saved team selection is missing", async () => {
  localStorage.removeItem("apex-studio.team-sync.active-team");
  render(<TeamSyncSettings onBusy={vi.fn()} />);
  await screen.findByRole("checkbox", {name: "Automatically synchronize projects"});
  expect(localStorage.getItem("apex-studio.team-sync.active-team")).toBe("team");
});

it("keeps the auto-sync choice when Settings is reopened and allows manual synchronization", async () => {
  const user = userEvent.setup();
  const first = render(<TeamSyncSettings onBusy={vi.fn()} />);
  const toggle = await screen.findByRole("checkbox", {
    name: "Automatically synchronize projects",
  });
  expect(toggle).toHaveProperty("checked", true);
  await user.click(toggle);
  expect(autoSyncEnabled()).toBe(false);
  first.unmount();
  render(<TeamSyncSettings onBusy={vi.fn()} />);
  expect(
    await screen.findByRole("checkbox", {
      name: "Automatically synchronize projects",
    }),
  ).toHaveProperty("checked", false);
  await user.click(screen.getByRole("button", { name: "Sync projects now" }));
  await screen.findAllByText("1 uploaded · 0 downloaded · 0 already current");
  expect(syncProjects).toHaveBeenCalledWith(connection.server.origin, "team");
});

it("lists the selected team's members and can refresh them", async () => {
  const user = userEvent.setup();
  render(<TeamSyncSettings onBusy={vi.fn()} />);
  expect(await screen.findByText("Studio Owner")).toBeTruthy();
  const memberList = document.querySelector(".team-sync-member-list");
  expect(memberList).toBeTruthy();
  expect(within(memberList as HTMLElement).getByText("owner@example.test")).toBeTruthy();
  expect(within(memberList as HTMLElement).getByText("designer@example.test")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  expect(syncListMembers).toHaveBeenLastCalledWith(connection.server.origin, "team");
});

it("explains a zero transfer and records the manual sync", async () => {
  vi.mocked(syncProjects).mockResolvedValue({
    uploaded: 0, downloaded: 0, checked: 4, upToDate: 4,
    sharedLocal: 4, remoteProjects: 4, errors: [], entries: [],
  });
  render(<TeamSyncSettings onBusy={vi.fn()} />);
  await userEvent.click(await screen.findByRole("button", { name: "Sync projects now" }));
  await screen.findAllByText("4 of 4 projects already up to date.");
  expect(screen.getByLabelText("Synchronization log").textContent).toContain("Manual");
});
