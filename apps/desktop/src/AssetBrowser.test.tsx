import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AssetBrowser from "./AssetBrowser";
import {
  assetBoard,
  prepareLivery,
  commitLivery,
  saveFolder,
  type Project,
  type Catalogue,
} from "./bridge";
vi.mock("./bridge", () => ({
  assetBoard: vi.fn(),
  prepareLivery: vi.fn(),
  commitLivery: vi.fn(),
  saveFolder: vi.fn(),
  cancelLivery: vi.fn().mockResolvedValue(undefined),
  assetVersions: vi.fn(),
  moveAsset: vi.fn(),
  restoreAssetVersion: vi.fn(),
  setFolderCover: vi.fn(),
  importAssets: vi.fn(),
  saveAssetCopy: vi.fn(),
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : "Failed"),
}));
const project: Project = {
  id: "project",
  name: "Design",
  description: "",
  tags: [],
  vehicles: [],
  referenceLinks: [],
  deadline: null,
  createdAt: "",
  updatedAt: "",
  archived: false,
  assetCount: 0,
};
const catalogue: Catalogue = {
  capturedAt: "2026-10-22",
  source: "Official",
  completeness: "Snapshot",
  vehicles: [
    {
      id: "car",
      name: "Vehicle",
      team: "Law",
      source: "Official",
      liverySupport: "supported",
      bodyStatus: "Completed",
      windowStatus: "N/A",
      notes: [],
      slots: [
        { key: "Left 1", kind: "body", status: "Completed" },
        { key: "Right 1", kind: "body", status: "Completed" },
      ],
    },
  ],
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [],
    folders: [],
    placements: [],
  });
});
afterEach(cleanup);
it("requires correction of duplicate slots and keeps the reviewed assignments in native-file order", async () => {
  const user = userEvent.setup();
  vi.mocked(prepareLivery).mockResolvedValue({
    token: "draft",
    files: [
      { index: 0, name: "Left1_7.png", suggestedSlot: "Left 1" },
      { index: 1, name: "Lft1_9.png", suggestedSlot: "Left 1" },
    ],
  });
  vi.mocked(commitLivery).mockResolvedValue("livery");
  render(
    <AssetBrowser
      project={project}
      catalogue={catalogue}
      onBusy={() => {}}
      onChange={() => {}}
    />,
  );
  await user.click(
    await screen.findByRole("button", { name: "Import livery" }),
  );
  await user.type(screen.getByLabelText("Livery name"), "Patrol");
  await user.selectOptions(screen.getByLabelText("Livery vehicle"), "car");
  expect(screen.getByRole("button", { name: "Confirm import" })).toHaveProperty(
    "disabled",
    true,
  );
  await user.selectOptions(
    screen.getByLabelText("Slot for Lft1_9.png"),
    "Right 1",
  );
  await user.click(screen.getByRole("button", { name: "Confirm import" }));
  await screen.findByText("Livery imported.");
  expect(commitLivery).toHaveBeenCalledWith(
    "draft",
    null,
    null,
    "Patrol",
    "car",
    [
      { index: 0, slot: "Left 1" },
      { index: 1, slot: "Right 1" },
    ],
    null,
  );
});
it("saves a new folder with its selected project and parent", async () => {
  const user = userEvent.setup();
  vi.mocked(saveFolder).mockResolvedValue({
    id: "folder",
    projectId: "project",
    parentId: null,
    name: "References",
    kind: "folder",
    vehicleId: null,
    coverHash: null,
    preview: null,
  });
  render(
    <AssetBrowser
      project={project}
      catalogue={catalogue}
      onBusy={() => {}}
      onChange={() => {}}
    />,
  );
  await user.click(await screen.findByRole("button", { name: "New folder" }));
  await user.type(screen.getByLabelText("Folder name"), "References");
  await user.click(screen.getByRole("button", { name: "Save folder" }));
  expect(saveFolder).toHaveBeenCalledWith("project", null, null, "References");
});
