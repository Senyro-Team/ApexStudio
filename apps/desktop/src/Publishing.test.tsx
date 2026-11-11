import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  syncUploadLibraryImages,
  syncCreateInviteCampaign,
  syncListInviteCampaigns,
  syncServiceAvailability,
  type MediaLibraryItem,
  type SyncServiceAvailability,
  type InviteCampaign,
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
  syncUploadLibraryImages: vi.fn(),
  syncCreateInviteCampaign: vi.fn(),
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
  vi.mocked(syncUploadLibraryImages).mockResolvedValue({ images: [], errors: [] });
  vi.mocked(syncCreateInviteCampaign).mockResolvedValue({
    id: "campaign-1",
    title: "Campaign",
    category: "livery",
    notes: "",
    channelId: "12345678901234567",
    code: "abc123",
    inviteUrl: "https://discord.gg/abc123",
    createdAt: "2026-11-10T00:00:00Z",
    expiresAt: null,
    maxUses: 0,
    totalUses: 0,
    isActive: true,
    trackedJoins: 0,
  });
});
afterEach(() => {
  cleanup();
  window.localStorage.clear();
  vi.restoreAllMocks();
});
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

it("loads local composer images while project assets are still loading", async () => {
  const user = userEvent.setup();
  const localImage: MediaLibraryItem = {
    id: "local-banner",
    hash: "e".repeat(64),
    name: "Local banner",
    altText: "Reusable local banner",
    imageUrl: "https://images.example/banner.png",
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
  vi.mocked(mediaLibrary).mockResolvedValue([localImage]);
  vi.mocked(listAssets).mockImplementation(() => new Promise(() => {}));

  render(<Publishing project={project} catalogue={null} onEditingChange={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  const librarySelect = await screen.findByRole("combobox", { name: "Choose an image from Image Library" });
  expect(Array.from((librarySelect as HTMLSelectElement).options).map((option) => option.textContent)).toContain("Local banner");
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

it("creates and copies manual post Markdown while optional services are disconnected", async () => {
  const user = userEvent.setup();
  const clipboard = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(<Publishing project={project} catalogue={catalogue} onEditingChange={() => {}} />);

  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  await user.click(screen.getByRole("button", { name: "Add text" }));
  await user.type(screen.getByLabelText("Markdown text"), "**Latest release**");
  await user.click(screen.getByRole("button", { name: "Add image URL" }));
  fireEvent.change(screen.getByLabelText("Alt text"), { target: { value: "Release [banner]" } });
  await user.type(screen.getByLabelText("Public image URL"), "https://images.example.com/release_(1).png");
  await user.type(screen.getByLabelText("Click-through URL (optional)"), "https://erlcx.com/groups/123");
  await user.click(screen.getByRole("button", { name: "Add invite link" }));
  await user.clear(screen.getByLabelText("Link label"));
  fireEvent.change(screen.getByLabelText("Link label"), { target: { value: "Join [Discord]" } });
  await user.type(screen.getByLabelText("Invite URL"), "https://discord.gg/example");
  await user.click(screen.getByRole("button", { name: "Copy Markdown" }));

  await waitFor(() => expect(clipboard).toHaveBeenCalledOnce());
  expect(clipboard.mock.calls[0][0]).toBe([
    "**Latest release**",
    "[![Release \\[banner\\]](<https://images.example.com/release_(1).png>)](<https://erlcx.com/groups/123>)",
    "[Join \\[Discord\\]](<https://discord.gg/example>)",
  ].join("\n\n"));
  expect(syncImageLibrary).not.toHaveBeenCalled();
  expect(syncCreateInviteCampaign).not.toHaveBeenCalled();
});

it("does not copy empty or unsafe manual invite links", async () => {
  const user = userEvent.setup();
  const clipboard = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(<Publishing project={project} catalogue={catalogue} onEditingChange={() => {}} />);

  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  await user.click(screen.getByRole("button", { name: "Add invite link" }));
  await user.click(screen.getByRole("button", { name: "Copy Markdown" }));
  expect(await screen.findByText("Add a public image URL or an invite link before copying.")).toBeTruthy();

  await user.type(screen.getByLabelText("Invite URL"), "javascript:alert(1)");
  await user.click(screen.getByRole("button", { name: "Copy Markdown" }));
  expect(clipboard).not.toHaveBeenCalled();
});

it("reuses an uploaded image URL when only its Markdown destination changes", async () => {
  const user = userEvent.setup();
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  const image: MediaLibraryItem = {
    id: "local-banner",
    hash: "f".repeat(64),
    name: "Banner.png",
    altText: "Studio banner",
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
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(mediaLibrary).mockResolvedValue([image]);
  vi.mocked(syncServiceAvailability).mockResolvedValue({
    projectSync: true,
    imageHosting: true,
    inviteTracking: false,
  });
  vi.mocked(syncUploadLibraryImages).mockResolvedValue({
    images: [{
      id: "hosted-banner",
      name: image.name,
      clientId: image.id,
      imageUrl: "https://images.example/banner.png",
      hash: image.hash!,
      altText: image.altText,
      defaultLinkUrl: null,
      ownerScope: "personal",
      createdAt: image.createdAt,
      reused: false,
    }],
    errors: [],
  });

  render(<Publishing project={project} catalogue={null} onEditingChange={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  await user.selectOptions(await screen.findByRole("combobox", { name: "Choose an image from Image Library" }), image.id);
  await user.click(screen.getByRole("button", { name: "Add library image" }));
  await user.click(screen.getByRole("button", { name: "Upload missing images" }));
  const imageUrl = await screen.findByLabelText("Public image URL") as HTMLInputElement;
  await waitFor(() => expect(imageUrl.value).toBe("https://images.example/banner.png"));
  expect(syncUploadLibraryImages).toHaveBeenCalledTimes(1);
  expect(confirm).toHaveBeenCalledOnce();

  await user.type(screen.getByLabelText("Click-through URL (optional)"), "https://erlcx.com/groups/123");
  expect(imageUrl.value).toBe("https://images.example/banner.png");
  expect(syncUploadLibraryImages).toHaveBeenCalledTimes(1);
});

it("reuses the same invite idempotency key after an ambiguous failure and remount", async () => {
  const user = userEvent.setup();
  const campaign: InviteCampaign = {
    id: "campaign-2",
    title: "Emergency campaign",
    category: "livery",
    notes: "",
    channelId: "12345678901234567",
    code: "invite2",
    inviteUrl: "https://discord.gg/invite2",
    createdAt: "2026-11-10T00:00:00Z",
    expiresAt: null,
    maxUses: 0,
    totalUses: 0,
    isActive: true,
    trackedJoins: 0,
  };
  vi.spyOn(window, "confirm").mockReturnValue(true);
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(syncServiceAvailability).mockResolvedValue({
    projectSync: true,
    imageHosting: false,
    inviteTracking: true,
  });
  vi.mocked(syncCreateInviteCampaign)
    .mockRejectedValueOnce(new Error("Network timeout"))
    .mockResolvedValueOnce(campaign);

  const renderComposer = () => render(
    <Publishing project={project} catalogue={null} onEditingChange={() => {}} />,
  );
  const fillInviteForm = async () => {
    await user.click(await screen.findByRole("button", { name: "Markdown" }));
    await user.type(await screen.findByLabelText("Campaign title"), "Emergency campaign");
    await user.type(screen.getByLabelText("Discord channel ID"), "12345678901234567");
    await user.click(screen.getByRole("button", { name: "Create invite and add link" }));
  };

  const first = renderComposer();
  await fillInviteForm();
  await screen.findByText("Network timeout");
  const firstRequestId = vi.mocked(syncCreateInviteCampaign).mock.calls[0][2];
  const storedRequests = window.localStorage.getItem("apex-studio.pending-invite-requests.v1") ?? "";
  expect(storedRequests).toContain(firstRequestId);
  expect(storedRequests).not.toContain("Emergency campaign");
  first.unmount();

  renderComposer();
  await fillInviteForm();
  await screen.findByText("Tracked invite created and added to the post.");
  expect(syncCreateInviteCampaign).toHaveBeenCalledTimes(2);
  expect(vi.mocked(syncCreateInviteCampaign).mock.calls[1][2]).toBe(firstRequestId);
  expect(window.localStorage.getItem("apex-studio.pending-invite-requests.v1")).toBe("[]");
});

it("shows tracked campaign loading failures and allows a manual retry", async () => {
  const user = userEvent.setup();
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(syncServiceAvailability).mockResolvedValue({
    projectSync: true,
    imageHosting: false,
    inviteTracking: true,
  });
  vi.mocked(syncListInviteCampaigns)
    .mockRejectedValueOnce(new Error("Server temporarily unavailable"))
    .mockResolvedValueOnce([]);

  render(<Publishing project={project} catalogue={null} onEditingChange={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Markdown" }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "Could not load tracked campaigns: Server temporarily unavailable",
  );

  await user.click(screen.getByRole("button", { name: "Refresh campaigns" }));
  expect(await screen.findByText("No tracked campaigns yet.")).toBeTruthy();
  expect(syncListInviteCampaigns).toHaveBeenCalledTimes(2);
});

it("keeps manual Markdown available when optional server services are unreachable", async () => {
  const user = userEvent.setup();
  const localImage: MediaLibraryItem = {
    id: "offline-banner",
    hash: "9".repeat(64),
    name: "Offline banner.png",
    altText: "Offline banner",
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
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(mediaLibrary).mockResolvedValue([localImage]);
  vi.mocked(syncServiceAvailability).mockResolvedValue({
    projectSync: true,
    imageHosting: true,
    inviteTracking: true,
    imageHostingReachable: false,
    inviteTrackingReachable: false,
  });

  render(<Publishing project={project} catalogue={null} onEditingChange={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Markdown" }));

  await user.selectOptions(
    await screen.findByRole("combobox", { name: "Choose an image from Image Library" }),
    localImage.id,
  );
  await user.click(screen.getByRole("button", { name: "Add library image" }));
  const publicUrl = await screen.findByLabelText("Public image URL");
  await user.type(publicUrl, "https://images.example/offline-banner.png");

  expect(screen.getByRole("button", { name: "Add invite link" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Upload missing images" })).toBeNull();
  expect(screen.queryByText("Tracked Discord invites")).toBeNull();
  expect(syncListInviteCampaigns).not.toHaveBeenCalled();
  expect(syncCreateInviteCampaign).not.toHaveBeenCalled();
});
