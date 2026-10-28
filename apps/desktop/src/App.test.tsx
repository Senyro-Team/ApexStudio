import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { invoke } from "@tauri-apps/api/core";
import {
  assetBoard,
  readWorkspace,
  renameWorkspace,
  listProjects,
  vehicleCatalogue,
  listAssets,
  saveProject,
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
  archiveProject: vi.fn(),
  importAssets: vi.fn(),
  saveAssetCopy: vi.fn(),
  errorMessage: (error: unknown) =>
    error instanceof Error ? error.message : "Workspace error",
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
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
afterEach(cleanup);

describe("workspace shell", () => {
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
    expect(screen.getByText("No projects")).toBeTruthy();
    expect(screen.queryByText("NYPD Christmas 2019")).toBeNull();
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
