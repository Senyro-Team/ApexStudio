import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Publishing from "./Publishing";
import {
  robloxLivery,
  publishingDraft,
  assetBoard,
  savePublishing,
  exportPublishing,
  type PublishingData,
  type Project,
} from "./bridge";
vi.mock("./bridge", () => ({
  robloxLivery: vi.fn(),
  uploadRobloxTexture: vi.fn(),
  publishingDraft: vi.fn(),
  publishingPreviews: vi.fn().mockResolvedValue([]),
  assetBoard: vi.fn(),
  savePublishing: vi.fn(),
  exportPublishing: vi.fn(),
  pickPublishingImages: vi.fn(),
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : "Failed"),
}));
const project: Project = {
  id: "p",
  name: "Studio",
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
const empty: PublishingData = {
  title: "",
  description: "",
  tags: [],
  youtubeUrl: null,
  media: [],
  liveries: [],
};
beforeEach(() => {
  vi.clearAllMocks();
  HTMLDialogElement.prototype.showModal = vi.fn(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.mocked(publishingDraft).mockResolvedValue({
    revision: 0,
    data: structuredClone(empty),
  });
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [],
    folders: [],
    placements: [],
  });
});
afterEach(cleanup);
it("saves incomplete work and keeps export unavailable until required fields exist", async () => {
  const user = userEvent.setup();
  vi.mocked(savePublishing).mockImplementation(async (_, revision, data) => ({
    revision: revision + 1,
    data,
  }));
  render(
    <Publishing
      project={project}
      catalogue={null}
      onEditingChange={() => {}}
    />,
  );
  await user.click(await screen.findByRole("button", { name: "Details" }));
  await user.type(await screen.findByLabelText("Post title"), "Patrol");
  await user.click(screen.getByRole("button", { name: "Save draft" }));
  await screen.findByText("Publishing draft saved.");
  expect(savePublishing).toHaveBeenCalledWith(
    "p",
    0,
    expect.objectContaining({ title: "Patrol" }),
  );
  await user.click(screen.getByRole("button", { name: "Review" }));
  expect(
    screen.getByRole("button", { name: "Export for ERLCX extension" }),
  ).toHaveProperty("disabled", true);
  expect(exportPublishing).not.toHaveBeenCalled();
});
it("exports a complete saved draft but blocks exporting unsaved edits", async () => {
  const user = userEvent.setup();
  vi.mocked(publishingDraft).mockResolvedValue({
    revision: 1,
    data: {
      ...empty,
      title: "Patrol",
      description: "Description",
      media: [
        { hash: "a".repeat(64), name: "one.png" },
        { hash: "b".repeat(64), name: "two.png" },
      ],
      liveries: [
        {
          vehicle: "Vehicle",
          primaryColor: "#FFFFFF",
          secondaryColor: null,
          opacity: 100,
          sides: [{ side: "Left 1", decalId: "12345" }],
        },
      ],
    },
  });
  vi.mocked(exportPublishing).mockResolvedValue(true);
  render(
    <Publishing
      project={project}
      catalogue={null}
      onEditingChange={() => {}}
    />,
  );
  await user.click(await screen.findByRole("button", { name: "Details" }));
  await screen.findByLabelText("Post title");
  await user.click(screen.getByRole("button", { name: "Review" }));
  await user.click(
    screen.getByRole("button", { name: "Export for ERLCX extension" }),
  );
  await screen.findByText("Publishing bundle exported.");
  expect(exportPublishing).toHaveBeenCalledWith("p");
  await user.click(screen.getByRole("button", { name: "Details" }));
  await user.type(screen.getByLabelText("Post title"), " edit");
  await user.click(screen.getByRole("button", { name: "Review" }));
  expect(
    screen.getByRole("button", { name: "Export for ERLCX extension" }),
  ).toHaveProperty("disabled", true);
});

it("loads decal IDs from a selected livery instead of requiring manual code entry", async () => {
  const user = userEvent.setup();
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [],
    placements: [],
    folders: [
      {
        id: "f",
        projectId: "p",
        parentId: null,
        name: "Patrol livery",
        kind: "livery",
        vehicleId: "v",
        coverHash: null,
        preview: null,
      },
    ],
  });
  vi.mocked(robloxLivery).mockResolvedValue({
    folderId: "f",
    name: "Patrol livery",
    vehicle: "Vehicle",
    accountId: "123",
    textures: [
      {
        assetId: "a",
        name: "Left1.png",
        side: "Left 1",
        hash: "a".repeat(64),
        upload: {
          accountId: "123",
          hash: "a".repeat(64),
          assetId: "456",
          state: "uploaded",
          operation: "operations/test",
          moderation: "approved",
          message: "",
        },
      },
    ],
  });
  vi.mocked(savePublishing).mockImplementation(async (_, revision, data) => ({
    revision: revision + 1,
    data,
  }));
  render(
    <Publishing
      project={project}
      catalogue={null}
      onEditingChange={() => {}}
    />,
  );
  await user.selectOptions(await screen.findByLabelText("Livery source"), "f");
  await user.click(screen.getByRole("button", { name: "Include livery" }));
  expect(await screen.findByLabelText("Decal ID for row 1")).toHaveProperty(
    "value",
    "456",
  );
  expect(screen.getByLabelText("Decal ID for row 1")).toHaveProperty(
    "disabled",
    true,
  );
  await user.click(screen.getByRole("button", { name: "Save draft" }));
  await screen.findByText("Publishing draft saved.");
  expect(savePublishing).toHaveBeenCalledWith(
    "p",
    0,
    expect.objectContaining({
      liveries: [
        expect.objectContaining({
          source: { folderId: "f", accountId: "123", hashes: ["a".repeat(64)] },
          sides: [{ side: "Left 1", decalId: "456" }],
        }),
      ],
    }),
  );
});
it("asks before uploading a missing livery and lets the user skip just that livery", async () => {
  const user = userEvent.setup();
  vi.mocked(assetBoard).mockResolvedValue({
    assets: [],
    placements: [],
    folders: [
      {
        id: "f",
        projectId: "p",
        parentId: null,
        name: "Patrol livery",
        kind: "livery",
        vehicleId: "v",
        coverHash: null,
        preview: null,
      },
    ],
  });
  vi.mocked(robloxLivery).mockResolvedValue({
    folderId: "f",
    name: "Patrol livery",
    vehicle: "Vehicle",
    accountId: "123",
    textures: [
      {
        assetId: "a",
        name: "Left1.png",
        side: "Left 1",
        hash: "a".repeat(64),
        upload: null,
      },
    ],
  });
  render(
    <Publishing
      project={project}
      catalogue={null}
      onEditingChange={() => {}}
    />,
  );
  await user.selectOptions(await screen.findByLabelText("Livery source"), "f");
  await user.click(screen.getByRole("button", { name: "Include livery" }));
  await screen.findByText("This livery needs Roblox decal IDs.");
  await user.click(screen.getByRole("button", { name: "Skip livery" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("No liveries selected.")).toBeTruthy();
  expect(savePublishing).not.toHaveBeenCalled();
});
