import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ArchiveImport, ProjectExport } from "./Packages";
import { applyArchive, exportProject, inspectArchive } from "./bridge";
vi.mock("./bridge", () => ({
  desktopAvailable: true,
  applyArchive: vi.fn(),
  inspectArchive: vi.fn(),
  exportProject: vi.fn(),
  createBackup: vi.fn(),
  errorMessage: (e: unknown) => String(e),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it("requires preview confirmation before importing and uses the opaque selection token", async () => {
  const user = userEvent.setup();
  const complete = vi.fn();
  vi.mocked(inspectArchive).mockResolvedValue({
    token: "selected",
    preview: {
      title: "Harbor",
      assets: 3,
      notes: 1,
      bytes: 1024,
      kind: "project",
    },
  });
  vi.mocked(applyArchive).mockResolvedValue("new-id");
  render(<ArchiveImport onComplete={complete} />);
  await user.click(screen.getByRole("button", { name: "Import project" }));
  await screen.findByText("Harbor");
  expect(applyArchive).not.toHaveBeenCalled();
  await user.click(
    screen.getByRole("button", { name: "Import as new project" }),
  );
  await screen.findByText("Project imported as a new project.");
  expect(applyArchive).toHaveBeenCalledWith("selected", false);
  expect(complete).toHaveBeenCalledOnce();
});
it("excludes private notes by default when exporting a project", async () => {
  const user = userEvent.setup();
  vi.mocked(exportProject).mockResolvedValue(true);
  render(<ProjectExport id="project" />);
  await user.click(screen.getByRole("button", { name: "Export project" }));
  expect(
    screen.getByRole("checkbox", { name: "Include project notes" }),
  ).toHaveProperty("checked", false);
  await user.click(screen.getByRole("button", { name: "Choose destination" }));
  await screen.findByText("Project exported.");
  expect(exportProject).toHaveBeenCalledWith("project", true, false);
});
