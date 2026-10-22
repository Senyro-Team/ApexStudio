import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AssetBrowser from "./AssetBrowser";
import {
  assetBoard,
  prepareLivery,
  commitLivery,
  saveFolder,
  copyAssetItem,
  moveAsset,
  assetVersions,
  assignAssetSlot,
  type Project,
  type Catalogue,
} from "./bridge";
vi.mock("./bridge", () => ({
  assetBoard: vi.fn(),
  copyAssetItem: vi.fn(),
  assignAssetSlot: vi.fn(),
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
  await user.type(screen.getByLabelText("Livery vehicle"), "Veh");
  await user.click(screen.getByRole("option", { name: /Vehicle/ }));
  expect(screen.getByRole("button", { name: "Confirm import" })).toHaveProperty(
    "disabled",
    true,
  );
  await user.selectOptions(
    screen.getByLabelText("Side for Lft1_9.png"),
    "Right",
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

it("shows no vehicle suggestions before typing and appends images without losing assignments", async () => {
  const user = userEvent.setup();
  const first = { index: 0, name: "texture.png", suggestedSlot: null };
  vi.mocked(prepareLivery)
    .mockResolvedValueOnce({ token: "draft", files: [first] })
    .mockResolvedValueOnce({
      token: "draft",
      files: [first, { index: 1, name: "cover.png", suggestedSlot: null }],
    });
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
  await user.click(screen.getByLabelText("Livery vehicle"));
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(screen.queryByLabelText("Find vehicle")).toBeNull();
  await user.selectOptions(
    screen.getByLabelText("Side for texture.png"),
    "Window Back",
  );
  await user.click(screen.getByRole("button", { name: "Add images" }));
  await screen.findByLabelText("Side for cover.png");
  expect(prepareLivery).toHaveBeenLastCalledWith("project", "draft");
  expect(screen.getByLabelText("Side for texture.png")).toHaveProperty(
    "value",
    "Window Back",
  );
  expect(
    screen.getByLabelText("Livery cover").querySelectorAll("option"),
  ).toHaveLength(3);
});
it("copies and cuts a folder with keyboard shortcuts, then pastes into the open folder", async () => {
  const user = userEvent.setup();
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [],
    placements: [],
    folders: [
      {
        id: "source",
        name: "Sources",
        projectId: "project",
        kind: "folder",
        parentId: null,
        vehicleId: null,
        coverHash: null,
        preview: null,
      },
      {
        id: "target",
        name: "Destination",
        projectId: "project",
        kind: "folder",
        parentId: null,
        vehicleId: null,
        coverHash: null,
        preview: null,
      },
    ],
  });
  render(
    <AssetBrowser
      project={project}
      catalogue={catalogue}
      onBusy={() => {}}
      onChange={() => {}}
    />,
  );
  await user.click(
    await screen.findByRole("button", { name: /Sources Folder/ }),
  );
  await user.keyboard("{Control>}c{/Control}");
  await user.dblClick(
    screen.getByRole("button", { name: /Destination Folder/ }),
  );
  await user.click(screen.getByRole("button", { name: "Paste" }));
  expect(copyAssetItem).toHaveBeenCalledWith(
    "project",
    "source",
    true,
    "target",
  );
  await screen.findByText("Copied.");
});
it("allows changing an existing livery file to any side", async () => {
  const user = userEvent.setup();
  vi.mocked(assetVersions).mockResolvedValue([]);
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [
      {
        id: "file",
        name: "texture.png",
        projectId: "project",
        hash: "hash",
        size: 100,
        importedAt: "",
        preview: null,
      },
    ],
    placements: [{ assetId: "file", folderId: "livery", slot: "Left 1" }],
    folders: [
      {
        id: "livery",
        name: "Patrol",
        projectId: "project",
        kind: "livery",
        parentId: null,
        vehicleId: "car",
        coverHash: null,
        preview: null,
      },
    ],
  });
  render(
    <AssetBrowser
      project={project}
      catalogue={catalogue}
      onBusy={() => {}}
      onChange={() => {}}
    />,
  );
  await user.dblClick(
    await screen.findByRole("button", { name: /Patrol Livery/ }),
  );
  await user.click(screen.getByRole("button", { name: /texture.png/ }));
  await user.selectOptions(
    screen.getByLabelText("Side for texture.png"),
    "Window Front",
  );
  expect(assignAssetSlot).toHaveBeenCalledWith("file", "Window Front 1");
});

it("drops a dragged file into a folder without treating it as a copy", async () => {
  const user = userEvent.setup();
  vi.mocked(assetVersions).mockResolvedValue([]);
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [
      {
        id: "file",
        name: "texture.png",
        projectId: "project",
        hash: "hash",
        size: 100,
        importedAt: "",
        preview: null,
      },
    ],
    placements: [{ assetId: "file", folderId: null, slot: null }],
    folders: [
      {
        id: "target",
        name: "Destination",
        projectId: "project",
        kind: "folder",
        parentId: null,
        vehicleId: null,
        coverHash: null,
        preview: null,
      },
    ],
  });
  render(
    <AssetBrowser
      project={project}
      catalogue={catalogue}
      onBusy={() => {}}
      onChange={() => {}}
    />,
  );
  const folder = await screen.findByRole("button", {
    name: /Destination Folder/,
  });
  const data = new Map<string, string>();
  const dataTransfer = {
    setData: (key: string, value: string) => data.set(key, value),
    getData: (key: string) => data.get(key) ?? "",
    effectAllowed: "",
  };
  fireEvent.dragStart(screen.getByRole("button", { name: /texture.png/ }), {
    dataTransfer,
  });
  fireEvent.drop(folder, { dataTransfer });
  await screen.findByText("Moved.");
  expect(moveAsset).toHaveBeenCalledWith("file", "target");
  expect(copyAssetItem).not.toHaveBeenCalled();
});
