import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import TeamSyncSettings from "./TeamSyncSettings";
import {
  syncConnectionStatus,
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
  await screen.findByText("1 uploaded · 0 downloaded");
  expect(syncProjects).toHaveBeenCalledWith(connection.server.origin, "team");
});
