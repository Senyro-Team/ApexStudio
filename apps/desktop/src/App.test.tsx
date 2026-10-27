import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
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
const info = {
  name: "My workspace",
  root: "C:/local/workspace",
  schemaVersion: 1,
  managedFileCount: 0,
};
beforeEach(() => {
  vi.clearAllMocks();
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
