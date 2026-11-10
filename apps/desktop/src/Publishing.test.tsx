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
  listAssets,
  mediaLibrary,
  syncImageLibrary,
  syncListInviteCampaigns,
  syncServiceAvailability,
  type MediaLibraryItem,
  type SyncServiceAvailability,
  type PublishingData,
  type Project,
  type Catalogue,
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
  listAssets: vi.fn(),
  mediaLibrary: vi.fn(),
  syncImageLibrary: vi.fn(),
  syncListInviteCampaigns: vi.fn(),
  syncServiceAvailability: vi.fn(),
  listMarkdownPresets: vi.fn().mockResolvedValue([]),
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
const catalogue: Catalogue = {
  capturedAt: "2026-11-04",
  source: "Official",
  completeness: "Snapshot",
  vehicles: [{
    id: "truck",
    name: "Truckatron",
    team: "Fire",
    source: "Official",
    liverySupport: "supported",
    bodyStatus: null,
    windowStatus: null,
    notes: [],
    slots: [],
  }],
};
beforeEach(() => {
  vi.clearAllMocks();
  window.localStorage.clear();
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
  vi.mocked(listAssets).mockResolvedValue([]);
  vi.mocked(mediaLibrary).mockResolvedValue([]);
  vi.mocked(syncServiceAvailability).mockResolvedValue({
    projectSync: true,
    imageHosting: false,
    inviteTracking: false,
  });
  vi.mocked(syncListInviteCampaigns).mockResolvedValue([]);
  vi.mocked(syncImageLibrary).mockResolvedValue({ images: [], errors: [] });
});
afterEach(cleanup);
it("uses an app menu for vehicle suggestions", async () => {
  const user = userEvent.setup();
  render(<Publishing project={project} catalogue={catalogue} onEditingChange={() => {}} />);
  await user.click(await screen.findByText("Use existing decal IDs manually"));
  await user.click(screen.getByRole("button", { name: "Add manual livery" }));
  const input = await screen.findByLabelText("Publishing vehicle") as HTMLInputElement;
  await user.type(input, "Truck");
  expect(screen.getByRole("listbox")).toBeTruthy();
  await user.click(screen.getByRole("option", { name: "Truckatron" }));
  expect(input.value).toBe("Truckatron");
  expect(screen.queryByRole("listbox")).toBeNull();
});
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
  const codes = screen.getByText("View livery codes (1)");
  expect(codes.closest("details")?.open).toBe(false);
  await user.click(codes);
  const vehicleCodes = screen.getByText("Vehicle · 1 texture");
  expect(vehicleCodes.closest("details")?.open).toBe(false);
  await user.click(vehicleCodes);
  expect(screen.getByText("Left 1 — 12345")).toBeTruthy();
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

it("shows local library images in the composer without waiting for optional server services", async () => {
  const user = userEvent.setup();
  const localImage: MediaLibraryItem = {
    id: "local-banner",
    hash: "b".repeat(64),
    name: "Local banner",
    altText: "Reusable local banner",
    imageUrl: "",
    defaultLinkUrl: null,
    folder: "",
    tags: [],
    overlayVersionGroup: null,
    overlayVersion: null,
    overlayTarget: null,
    ownerScope: "personal",
    teamId: null,
    providerImageId: null,
    createdAt: "2026-11-10T00:00:00Z",
    updatedAt: "2026-11-10T00:00:00Z",
    preview: null,
  };
  let finishAvailability!: (value: SyncServiceAvailability) => void;
  let finishSharedImages!: (value: { images: []; errors: [] }) => void;
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(mediaLibrary).mockResolvedValue([localImage]);
  vi.mocked(syncServiceAvailability).mockImplementation(() => new Promise((resolve) => {
    finishAvailability = resolve;
  }));
  vi.mocked(syncImageLibrary).mockImplementation(() => new Promise((resolve) => {
    finishSharedImages = resolve;
  }));

  render(<Publishing project={project} catalogue={null} onEditingChange={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  const librarySelect = await screen.findByRole("combobox", { name: "Choose an image from Image Library" });
  expect(Array.from((librarySelect as HTMLSelectElement).options).map((option) => option.textContent)).toContain("Local banner");
  expect(syncServiceAvailability).toHaveBeenCalledWith("https://team.example", "team-1");
  expect(syncImageLibrary).toHaveBeenCalledWith("https://team.example", "team-1");

  finishAvailability({ projectSync: true, imageHosting: true, inviteTracking: true });
  finishSharedImages({ images: [], errors: [] });
  await vi.waitFor(() => expect(screen.getByText("Local banner")).toBeTruthy());
});

it("keeps a fast team-library response when the local media query finishes later", async () => {
  const user = userEvent.setup();
  const localImage: MediaLibraryItem = {
    id: "local-banner",
    hash: "c".repeat(64),
    name: "Local banner",
    altText: "",
    imageUrl: "",
    defaultLinkUrl: null,
    folder: "",
    tags: [],
    overlayVersionGroup: null,
    overlayVersion: null,
    overlayTarget: null,
    ownerScope: "personal",
    teamId: null,
    providerImageId: null,
    createdAt: "2026-11-10T00:00:00Z",
    updatedAt: "2026-11-10T00:00:00Z",
    preview: null,
  };
  let finishLocal!: (rows: MediaLibraryItem[]) => void;
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(mediaLibrary).mockImplementation(() => new Promise((resolve) => {
    finishLocal = resolve;
  }));
  vi.mocked(syncImageLibrary).mockResolvedValue({
    images: [{
      id: "shared-banner",
      name: "Shared banner",
      clientId: null,
      imageUrl: "https://images.example/shared.png",
      hash: "d".repeat(64),
      altText: "",
      defaultLinkUrl: null,
      ownerScope: "team",
      createdAt: "2026-11-10T00:00:00Z",
      reused: true,
    }],
    errors: [],
  });

  render(<Publishing project={project} catalogue={null} onEditingChange={() => {}} />);
  await vi.waitFor(() => expect(syncImageLibrary).toHaveBeenCalled());
  finishLocal([localImage]);
  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  const librarySelect = await screen.findByRole("combobox", { name: "Choose an image from Image Library" });
  const availableImages = Array.from((librarySelect as HTMLSelectElement).options).map((option) => option.textContent);
  expect(availableImages).toContain("Local banner");
  expect(availableImages).toContain("Shared banner");
});
