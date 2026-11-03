import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { loadWallpaper, saveWallpaper } from "./wallpaper";
import { invoke } from "@tauri-apps/api/core";
import {
  assetBoard,
  readWorkspace,
  renameWorkspace,
  listProjects,
  vehicleCatalogue,
  listAssets,
  saveProject,
  statisticsHistory,
  statisticsProfile,
  statisticsSales,
  type Project,
} from "./bridge";

vi.mock("./bridge", () => ({
  listPosts: vi.fn().mockResolvedValue([]),
  chromeSettings: vi
    .fn()
    .mockResolvedValue({ chromePath: "", profile: "Default", extensionId: "" }),
  browserStatus: vi.fn().mockResolvedValue({
    connected: false,
    state: "",
    step: "",
    message: "",
    postId: null,
  }),
  robloxSettings: vi
    .fn()
    .mockResolvedValue({ configured: false, accountId: null }),
  setEditLock: vi.fn().mockResolvedValue(undefined),
  desktopAvailable: true,
  assetBoard: vi.fn(),
  readWorkspace: vi.fn(),
  renameWorkspace: vi.fn(),
  listProjects: vi.fn(),
  vehicleCatalogue: vi.fn(),
  listAssets: vi.fn(),
  saveProject: vi.fn(),
  projectSyncSettings: vi.fn().mockResolvedValue(null),
  setProjectSync: vi.fn().mockResolvedValue(null),
  syncProjects: vi.fn().mockResolvedValue({uploaded: 0, downloaded: 0, errors: []}),
  archiveProject: vi.fn(),
  statisticsProfile: vi.fn().mockResolvedValue(null),
  statisticsHistory: vi.fn().mockResolvedValue([]),
  statisticsSales: vi.fn().mockResolvedValue(null),
  statisticsBrowserStatus: vi.fn(),
  startStatisticsBrowser: vi.fn(),
  setStatisticsProfile: vi.fn(),
  refreshStatistics: vi.fn(),
  importAssets: vi.fn(),
  saveAssetCopy: vi.fn(),
  errorMessage: (error: unknown) =>
    error instanceof Error ? error.message : "Workspace error",
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("./wallpaper", () => ({
  loadWallpaper: vi.fn().mockResolvedValue(null),
  saveWallpaper: vi.fn().mockResolvedValue(undefined),
}));
it("opens the global ERLCX area with one page heading", async () => {
  render(<App />);
  await screen.findByText("My workspace");
  await userEvent.click(screen.getByRole("button", { name: "ERLCX" }));
  await screen.findByRole("button", { name: "New draft" });
  expect(
    screen.getAllByRole("heading", { name: "ERLCX", level: 1 }),
  ).toHaveLength(1);
});
const info = {
  name: "My workspace",
  root: "C:/local/workspace",
  schemaVersion: 1,
  managedFileCount: 0,
};
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(loadWallpaper).mockResolvedValue(null);
  vi.mocked(saveWallpaper).mockResolvedValue(undefined);
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true,
    value: vi.fn(() => "blob:wallpaper-test"),
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true,
    value: vi.fn(),
  });
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [],
    folders: [],
    placements: [],
  });
  vi.mocked(readWorkspace).mockResolvedValue(info);
  vi.mocked(listProjects).mockResolvedValue([]);
  vi.mocked(listAssets).mockResolvedValue([]);
  vi.mocked(vehicleCatalogue).mockResolvedValue({
    capturedAt: "2026-10-22",
    source: "https://example.com",
    completeness: "Unverified",
    vehicles: [],
  });
});

const project: Project = {
  id: "test-project",
  name: "Harbor project",
  description: "",
  tags: [],
  vehicles: [],
  referenceLinks: [],
  deadline: null,
  createdAt: "2026-10-22T12:00:00Z",
  updatedAt: "2026-10-22T12:00:00Z",
  archived: false,
  assetCount: 0,
};
it("creates a project and opens the asset-first detail with no sample files", async () => {
  vi.mocked(saveProject).mockResolvedValue(project);
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("My workspace");
  await user.click(screen.getByRole("button", { name: /^New project$/ }));
  await user.type(screen.getByLabelText("Project name"), "Harbor project");
  await user.click(screen.getByRole("button", { name: "Save project" }));
  await screen.findByRole("heading", { name: "Harbor project" });
  expect(screen.getByText("Add your first files")).toBeTruthy();
  expect(saveProject).toHaveBeenCalledWith(
    null,
    expect.objectContaining({ name: "Harbor project" }),
  );
});
it("edits only writable project fields and retains the creation metadata", async () => {
  vi.mocked(listProjects).mockResolvedValue([project]);
  vi.mocked(saveProject).mockResolvedValue({
    ...project,
    name: "Updated project",
  });
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("Harbor project");
  await user.click(screen.getByRole("button", { name: /Harbor project/ }));
  await user.click(screen.getByRole("button", { name: "Project actions" }));
  await user.click(screen.getByRole("button", { name: "Edit project" }));
  await user.clear(screen.getByLabelText("Project name"));
  await user.type(screen.getByLabelText("Project name"), "Updated project");
  expect(
    (screen.getByRole("button", { name: "Settings" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  await user.click(screen.getByRole("button", { name: "Save project" }));
  await screen.findByRole("heading", { name: "Updated project" });
  const fields = vi.mocked(saveProject).mock.calls[0][1];
  expect(fields).not.toHaveProperty("id");
  expect(fields).not.toHaveProperty("createdAt");
});

it("lets a new project opt into the selected team's sync destination", async () => {
  localStorage.setItem("apex-studio.team-sync.server-url", "https://sync.example.test");
  localStorage.setItem("apex-studio.team-sync.active-team", "team-id");
  vi.mocked(saveProject).mockResolvedValue(project);
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("My workspace");
  await user.click(screen.getByRole("button", {name: /^New project$/}));
  const toggle = screen.getByRole("checkbox", {name: "Synchronize with team server"});
  expect(toggle).toHaveProperty("checked", false);
  await user.click(toggle);
  await user.type(screen.getByLabelText("Project name"), "Harbor project");
  await user.click(screen.getByRole("button", {name: "Save project"}));
  await screen.findByRole("heading", {name: "Harbor project"});
  expect(saveProject).toHaveBeenCalledWith(null, expect.objectContaining({name: "Harbor project"}), {
    enabled: true, serverUrl: "https://sync.example.test", teamId: "team-id",
  });
});
afterEach(cleanup);

describe("workspace shell", () => {
  it("saves a background image, restores its glass mode, and removes it", async () => {
    let savedImage: Blob | null = null;
    vi.mocked(loadWallpaper).mockImplementation(async () => savedImage);
    vi.mocked(saveWallpaper).mockImplementation(async (image) => { savedImage = image; });
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const image = new File(["image bytes"], "background.png", { type: "image/png" });
    expect(screen.getByRole("button", { name: /Upload background image/ })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Background image file"), { target: { files: [image] } });
    await waitFor(() => expect(document.documentElement.dataset.wallpaper).toBe("true"));
    expect(savedImage).toBe(image);
    expect(document.documentElement.style.getPropertyValue("--wallpaper-image")).toContain("blob:wallpaper-test");
    view.unmount();
    render(<App />);
    await waitFor(() => expect(document.documentElement.dataset.wallpaper).toBe("true"));
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Remove background image" }));
    await waitFor(() => expect(document.documentElement.dataset.wallpaper).toBeUndefined());
    expect(savedImage).toBeNull();
  });
  it("saves the selected theme and updates the app logo and desktop icon", async () => {
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Ocean" }));
    expect(document.documentElement.dataset.theme).toBe("ocean");
    expect(localStorage.getItem("apex-studio-theme")).toBe("ocean");
    expect(
      screen
        .getByRole("button", { name: "Ocean" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      (document.querySelector(".brand-mark") as HTMLElement).style
        .backgroundColor,
    ).toBe("rgb(99, 185, 255)");
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_app_theme_icon", {
        color: "#63b9ff",
      }),
    );
    view.unmount();
    render(<App />);
    expect(document.documentElement.dataset.theme).toBe("ocean");
  });
  it("previews separate colors and saves edits to the active preset", async () => {
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.change(screen.getByLabelText("Button color"), {
      target: { value: "#2277cc" },
    });
    fireEvent.change(screen.getByLabelText("Line color"), {
      target: { value: "#bb66dd" },
    });
    fireEvent.change(screen.getByLabelText("Text color"), {
      target: { value: "#f0dc9a" },
    });
    fireEvent.change(screen.getByLabelText("Logo color"), {
      target: { value: "#23ff98" },
    });
    expect(
      document.documentElement.style.getPropertyValue("--accent-strong"),
    ).toBe("#2277cc");
    expect(document.documentElement.style.getPropertyValue("--line")).toBe(
      "#bb66dd",
    );
    expect(document.documentElement.style.getPropertyValue("--text")).toBe(
      "#f0dc9a",
    );
    expect(
      (document.querySelector(".brand-mark") as HTMLElement).style
        .backgroundColor,
    ).toBe("rgb(35, 255, 152)");
    expect(localStorage.getItem("apex-studio-theme-overrides")).toBeNull();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_app_theme_icon", {
        color: "#23ff98",
      }),
    );
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(
      JSON.parse(localStorage.getItem("apex-studio-theme-overrides") ?? "{}")
        .violet.colors,
    ).toEqual({
      button: "#2277cc",
      line: "#bb66dd",
      text: "#f0dc9a",
      logo: "#23ff98",
    });
    view.unmount();
    render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(document.documentElement.style.getPropertyValue("--line")).toBe(
      "#bb66dd",
    );
    fireEvent.change(screen.getByLabelText("Line color"), {
      target: { value: "#444444" },
    });
    await user.click(screen.getByRole("button", { name: "Discard preview" }));
    expect(document.documentElement.style.getPropertyValue("--line")).toBe(
      "#bb66dd",
    );
    await user.click(screen.getByRole("button", { name: "Restore preset" }));
    expect(document.documentElement.style.getPropertyValue("--line")).toBe(
      "#2a2c33",
    );
    expect(
      JSON.parse(localStorage.getItem("apex-studio-theme-overrides") ?? "{}"),
    ).toEqual({});
  });
  it("creates and restores a black custom theme, then removes it", async () => {
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.type(
      screen.getByLabelText("Name", { selector: "#custom-theme-name" }),
      "My Crimson",
    );
    fireEvent.change(screen.getByLabelText("Accent color"), {
      target: { value: "#dd4477" },
    });
    await user.click(screen.getByRole("button", { name: "Create theme" }));
    expect(document.documentElement.dataset.themeMode).toBe("black");
    expect(
      (document.querySelector(".brand-mark") as HTMLElement).style
        .backgroundColor,
    ).toBe("rgb(221, 68, 119)");
    expect(
      screen
        .getByRole("button", { name: "My Crimson" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      JSON.parse(localStorage.getItem("apex-studio-custom-themes") ?? "[]"),
    ).toEqual([
      expect.objectContaining({
        name: "My Crimson",
        color: "#dd4477",
        mode: "black",
      }),
    ]);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_app_theme_icon", {
        color: "#dd4477",
      }),
    );
    fireEvent.change(screen.getByLabelText("Text color"), {
      target: { value: "#efe4aa" },
    });
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    view.unmount();
    render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(
      screen
        .getByRole("button", { name: "My Crimson" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(document.documentElement.style.getPropertyValue("--text")).toBe(
      "#efe4aa",
    );
    await user.click(screen.getByRole("button", { name: "Delete My Crimson" }));
    expect(document.documentElement.dataset.theme).toBe("violet");
    expect(
      JSON.parse(localStorage.getItem("apex-studio-custom-themes") ?? "[]"),
    ).toEqual([]);
  });
  it("shows empty local records and no mockup projects", async () => {
    render(<App />);
    await screen.findByText("My workspace");
    expect(screen.getByText("No projects yet")).toBeTruthy();
    expect(screen.queryByText("NYPD Christmas 2019")).toBeNull();
  });
  it("edits dashboard widgets on a grid only after confirmation and keeps the layout", async () => {
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Robux revenue" }));
    await user.click(screen.getByRole("gridcell", { name: "Row 1, column 4: Empty" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(localStorage.getItem("apex-studio-dashboard-widgets")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Robux revenue" }));
    await user.click(screen.getByRole("gridcell", { name: "Row 1, column 4: Empty" }));
    await user.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(screen.getByRole("heading", { name: "Robux revenue" })).toBeTruthy();
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").robux).toEqual({ column: 3, row: 0, width: 1, height: 1 });
    view.unmount();
    render(<App />);
    await screen.findByText("My workspace");
    expect(screen.getByRole("heading", { name: "Robux revenue" })).toBeTruthy();
  });
  it("moves and removes widgets in the live dashboard editor", async () => {
    render(<App />);
    await screen.findByText("My workspace");
    await userEvent.click(screen.getByRole("button", { name: "Edit dashboard" }));
    const transfer = {
      data: "",
      effectAllowed: "move",
      dropEffect: "move",
      setData(_type: string, value: string) { this.data = value; },
      getData() { return this.data; },
    };
    fireEvent.dragStart(screen.getByRole("gridcell", { name: /Row 1, column 1: Live clock/ }), { dataTransfer: transfer });
    fireEvent.dragOver(screen.getByRole("gridcell", { name: "Row 1, column 4: Empty" }), { dataTransfer: transfer });
    fireEvent.drop(screen.getByRole("gridcell", { name: "Row 1, column 4: Empty" }), { dataTransfer: transfer });
    expect(screen.getByRole("gridcell", { name: /Row 1, column 4: Live clock/ })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Remove Views" }));
    expect(screen.getByRole("gridcell", { name: "Row 2, column 1: Empty" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Robux revenue" }));
    await userEvent.click(screen.getByRole("gridcell", { name: "Row 2, column 1: Empty" }));
    expect(screen.getByRole("gridcell", { name: /Row 2, column 1: Robux revenue/ })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Drag to resize Robux revenue" }));
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("gridcell", { name: /Robux revenue, 2 by 1/ })).toBeTruthy();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("gridcell", { name: /Robux revenue, 2 by 2/ })).toBeTruthy();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("gridcell", { name: /Robux revenue, 1 by 2/ })).toBeTruthy();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.queryByLabelText("Robux revenue width")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    const saved = JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}");
    expect(saved.clock).toEqual({ column: 3, row: 0, width: 1, height: 1 });
    expect(saved.views).toBeUndefined();
    expect(saved.robux).toEqual({ column: 0, row: 1, width: 2, height: 2 });
    expect(saved.likes).not.toEqual({ column: 1, row: 1, width: 1, height: 1 });
  });
  it("loads layouts saved before widget resizing was added", async () => {
    localStorage.setItem("apex-studio-dashboard-widgets", JSON.stringify({ clock: 0, date: 1 }));
    render(<App />);
    await screen.findByText("My workspace");
    await userEvent.click(screen.getByRole("button", { name: "Edit dashboard" }));
    expect(screen.getByRole("gridcell", { name: /Row 1, column 1: Live clock, 1 by 1/ })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Drag to resize Live clock" }));
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").clock).toEqual({ column: 0, row: 0, width: 1, height: 2 });
  });
  it("resizes a widget by dragging its corner and previews the size before saving", async () => {
    render(<App />);
    await screen.findByText("My workspace");
    await userEvent.click(screen.getByRole("button", { name: "Edit dashboard" }));
    const grid = screen.getByRole("grid", { name: "Dashboard layout" });
    Object.defineProperty(grid, "clientWidth", { configurable: true, value: 842 });
    const handle = screen.getByRole("button", { name: "Drag to resize Live clock" });
    vi.stubGlobal("PointerEvent", MouseEvent);
    try {
      fireEvent.pointerDown(handle, { clientX: 100, clientY: 100 });
      fireEvent.pointerMove(handle, { clientX: 314, clientY: 314 });
      fireEvent.pointerUp(handle);
    } finally {
      vi.unstubAllGlobals();
    }
    expect(screen.getByRole("gridcell", { name: /Live clock, 2 by 2/ })).toBeTruthy();
    expect(localStorage.getItem("apex-studio-dashboard-widgets")).toBeNull();
  });
  it("previews and saves a round analog clock design", async () => {
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Live clock" }));
    await user.click(screen.getByRole("button", { name: "Live clock design: Round clock" }));
    const clockCell = screen.getByRole("gridcell", { name: /Live clock, 1 by 1/ });
    expect(within(clockCell).getByRole("img", { name: /Round clock/ })).toBeTruthy();
    expect(localStorage.getItem("apex-studio-dashboard-widgets")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").clock.design).toBe("analog");
    view.unmount();
    render(<App />);
    await screen.findByText("My workspace");
    expect(screen.getByRole("img", { name: /Round clock/ })).toBeTruthy();
  });
  it("offers equally many digital and round clock looks and saves the chosen dial", async () => {
    const user = userEvent.setup();
    const view = render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Live clock" }));
    const designs = screen.getByRole("group", { name: "Live clock designs" });
    expect(within(designs).getAllByRole("button")).toHaveLength(8);
    await user.click(within(designs).getByRole("button", { name: "Live clock design: Bold digital" }));
    const clockCell = screen.getByRole("gridcell", { name: /Live clock, 1 by 1/ });
    expect(clockCell.querySelector(".dashboard-digital-bold")).toBeTruthy();
    await user.click(within(designs).getByRole("button", { name: "Live clock design: Roman dial" }));
    expect(within(clockCell).getByRole("img", { name: /Roman dial/ })).toBeTruthy();
    expect(within(clockCell).getByText("XII")).toBeTruthy();
    await user.click(within(designs).getByRole("button", { name: "Live clock design: Glow dial" }));
    expect(within(clockCell).getByRole("img", { name: /Glow dial/ })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").clock.design).toBe("accent");
    view.unmount();
    render(<App />);
    await screen.findByText("My workspace");
    expect(screen.getByRole("img", { name: /Glow dial/ })).toBeTruthy();
  });
  it("previews and saves calendar and split date designs", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Date" }));
    await user.click(screen.getByRole("button", { name: "Date design: Calendar" }));
    const dateCell = screen.getByRole("gridcell", { name: /Date, 1 by 1/ });
    expect(within(dateCell).getByRole("img", { name: /Calendar date/ })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").date.design).toBe("calendar");
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Date" }));
    await user.click(screen.getByRole("button", { name: "Date design: Split date" }));
    expect(screen.getByRole("gridcell", { name: /Date, 1 by 1/ }).querySelector(".dashboard-split-date")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").date.design).toBe("split");
  });
  it("applies a selected design when adding a new widget", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await user.click(screen.getByRole("button", { name: "Robux revenue" }));
    await user.click(screen.getByRole("button", { name: "Robux revenue design: Glass" }));
    await user.click(screen.getByRole("gridcell", { name: "Row 1, column 4: Empty" }));
    const widget = screen.getByRole("gridcell", { name: /Robux revenue, 1 by 1/ });
    expect(widget.querySelector(".design-glass")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(JSON.parse(localStorage.getItem("apex-studio-dashboard-widgets") ?? "{}").robux.design).toBe("glass");
  });
  it("shows saved post and sales figures in dashboard widgets", async () => {
    vi.mocked(statisticsProfile).mockResolvedValue("123");
    vi.mocked(statisticsHistory).mockResolvedValue([{
      collectedAt: "2026-10-30T12:00:00Z",
      posts: [{ bundleId: 42, title: "Latest livery", views: 120, likes: 7, copies: 3, favourites: 0, ratingCount: 0, ratingAverage: null }],
    }]);
    vi.mocked(statisticsSales).mockResolvedValue({
      collectedAt: "2026-10-30T12:00:00Z",
      data: { sales: 5, allTimeSales: 17, netRobux: 42, grossRobux: 60, series: [] },
    });
    render(<App />);
    await screen.findByText("My workspace");
    await screen.findAllByText("Latest livery");
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(screen.getByRole("button", { name: "Edit dashboard" }));
    await userEvent.click(screen.getByRole("button", { name: "Robux revenue" }));
    await userEvent.click(screen.getByRole("gridcell", { name: "Row 1, column 4: Empty" }));
    await userEvent.click(screen.getByRole("button", { name: "All-time sales" }));
    await userEvent.click(screen.getByRole("gridcell", { name: "Row 2, column 4: Empty" }));
    await userEvent.click(screen.getByRole("button", { name: "Confirm dashboard" }));
    expect(screen.getByText("R$ 42")).toBeTruthy();
    expect(screen.getByText("17")).toBeTruthy();
    expect(screen.getAllByText("120").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Latest livery").length).toBeGreaterThan(0);
  });
  it("saves a workspace name and uses the confirmed response in the shell", async () => {
    vi.mocked(renameWorkspace).mockResolvedValue({
      ...info,
      name: "Apex Designs",
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.clear(screen.getByLabelText("Workspace name"));
    await user.type(screen.getByLabelText("Workspace name"), "Apex Designs");
    await user.click(screen.getByRole("button", { name: "Save name" }));
    await screen.findByText("Workspace name saved.");
    expect(renameWorkspace).toHaveBeenCalledWith("Apex Designs");
    expect(screen.getByText("Apex Designs")).toBeTruthy();
  });
  it("keeps the entered name and presents a recoverable save error", async () => {
    vi.mocked(renameWorkspace).mockRejectedValue(
      new Error("The workspace is read-only."),
    );
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("My workspace");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Save name" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "read-only",
    );
    expect(
      (screen.getByLabelText("Workspace name") as HTMLInputElement).value,
    ).toBe("My workspace");
    expect(screen.queryByText("Workspace name saved.")).toBeNull();
  });
});
