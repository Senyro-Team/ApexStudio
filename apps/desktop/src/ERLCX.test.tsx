import { afterEach, beforeEach, it, expect, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ERLCX from "./ERLCX";
import * as bridge from "./bridge";
vi.mock("./bridge", () => ({
  listPosts: vi
    .fn()
    .mockResolvedValue([
      {
        id: "post",
        title: "Test draft",
        updatedAt: "2026-10-28T12:00:00Z",
        liveries: 0,
        revision: 1,
      },
    ]),
  createPost: vi.fn().mockResolvedValue("new"),
  listProjects: vi.fn().mockResolvedValue([
    { id: "p", name: "Project One" },
    { id: "q", name: "Project Two" },
  ]),
  vehicleCatalogue: vi.fn().mockResolvedValue({ vehicles: [] }),
  postDraft: vi.fn(),
  postSources: vi.fn().mockResolvedValue([]),
  releasePostImages: vi.fn().mockResolvedValue(undefined),
  pinPostImages: vi.fn().mockResolvedValue(undefined),
  browserStatus: vi
    .fn()
    .mockResolvedValue({
      connected: true,
      state: "",
      step: "",
      message: "",
      postId: null,
    }),
  assetBoard: vi
    .fn()
    .mockResolvedValue({ assets: [], folders: [], placements: [] }),
  publishingPreviews: vi.fn().mockResolvedValue([]),
  robloxLiveries: vi.fn().mockResolvedValue([
    {
      id: "l1",
      name: "Livery One",
      projectId: "p",
      projectName: "Project One",
      vehicleId: "car",
      textures: 1,
      uploaded: 0,
    },
    {
      id: "l2",
      name: "Livery Two",
      projectId: "q",
      projectName: "Project Two",
      vehicleId: "car",
      textures: 1,
      uploaded: 0,
    },
  ]),
  robloxLivery: vi.fn(),
  savePost: vi.fn(),
  exportPost: vi.fn(),
  prepareBrowser: vi.fn(),
  listMarkdownPresets: vi.fn().mockResolvedValue([]),
  mediaLibrary: vi.fn().mockResolvedValue([]),
  listAssets: vi.fn().mockResolvedValue([]),
  syncServiceAvailability: vi.fn().mockResolvedValue({
    projectSync: true,
    imageHosting: false,
    inviteTracking: false,
  }),
  syncListInviteCampaigns: vi.fn().mockResolvedValue([]),
  syncImageLibrary: vi.fn().mockResolvedValue({ images: [], errors: [] }),
  errorMessage: (e: Error) => e.message,
}));
const empty = {
  title: "Demo title",
  description: "Demo text",
  tags: [],
  youtubeUrl: null,
  media: [
    { hash: "a".repeat(64), name: "one.png" },
    { hash: "b".repeat(64), name: "two.png" },
  ],
  liveries: [],
};
beforeEach(() => {
  vi.clearAllMocks();
  window.localStorage.clear();
  vi.mocked(bridge.postDraft).mockResolvedValue({
    data: structuredClone(empty),
    revision: 1,
  });
  vi.mocked(bridge.savePost).mockImplementation(
    async (_id, revision, data) => ({ revision: revision + 1, data }),
  );
  vi.mocked(bridge.robloxLivery).mockImplementation(async (id) => ({
    folderId: id,
    accountId: "123",
    name: id,
    vehicle: "Car",
    textures: [
      {
        assetId: "asset",
        name: "Left.png",
        hash: "c".repeat(64),
        side: "Left 1",
        upload: null,
      },
    ],
  }));
});
afterEach(() => {
  cleanup();
  window.localStorage.clear();
});
it("retries loading liveries after a workspace error without discarding the draft", async () => {
  vi.mocked(bridge.robloxLiveries).mockRejectedValueOnce(
    new Error("locking protocol"),
  );
  render(<ERLCX onBusy={() => {}} onOpenRoblox={() => {}} />);
  await userEvent.click(
    await screen.findByRole("button", { name: /Test draft/ }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Retry loading liveries" }),
  );
  expect(await screen.findByText("Livery One")).toBeTruthy();
  await userEvent.click(screen.getByRole("button", { name: "Details" }));
  expect(
    ((await screen.findByLabelText("Post title")) as HTMLInputElement).value,
  ).toBe("Demo title");
});
it("selects liveries across projects, saves missing IDs and blocks browser/export", async () => {
  render(<ERLCX onBusy={() => {}} onOpenRoblox={() => {}} />);
  await userEvent.click(
    await screen.findByRole("button", { name: /Test draft/ }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Select all filtered" }),
  );
  await waitFor(() =>
    expect(
      within(screen.getByRole("region", { name: "Select liveries" }))
        .getAllByRole("checkbox")
        .every((e) => (e as HTMLInputElement).checked),
    ).toBe(true),
  );
  await userEvent.click(screen.getByRole("button", { name: "Save draft" }));
  expect(bridge.savePost).toHaveBeenCalledWith(
    "post",
    1,
    expect.objectContaining({
      liveries: expect.arrayContaining([
        expect.objectContaining({
          source: expect.objectContaining({ folderId: "l1" }),
        }),
        expect.objectContaining({
          source: expect.objectContaining({ folderId: "l2" }),
        }),
      ]),
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Review" }));
  expect(
    (
      screen.getByRole("button", {
        name: "Prepare on ERLCX",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(
    (
      screen.getByRole("button", {
        name: "Export for ERLCX extension",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(bridge.prepareBrowser).not.toHaveBeenCalled();
});
it("requires explicit review of a missing source without rewriting saved IDs", async () => {
  const data = {
    ...structuredClone(empty),
    liveries: [
      {
        vehicle: "Saved Car",
        primaryColor: "#FFFFFF",
        secondaryColor: null,
        opacity: 100,
        source: {
          folderId: "deleted",
          accountId: "123",
          hashes: ["c".repeat(64)],
        },
        sides: [{ side: "Frontplate 1", decalId: "456" }],
      },
    ],
  };
  vi.mocked(bridge.postDraft).mockResolvedValue({ data, revision: 1 });
  vi.mocked(bridge.postSources).mockResolvedValue([
    { folderId: "deleted", vehicle: "Saved Car", state: "missing" },
  ]);
  render(<ERLCX onBusy={() => {}} onOpenRoblox={() => {}} />);
  await userEvent.click(
    await screen.findByRole("button", { name: /Test draft/ }),
  );
  await userEvent.click(await screen.findByRole("button", { name: "Review" }));
  const prepare = await screen.findByRole("button", {
    name: "Prepare on ERLCX",
  });
  expect((prepare as HTMLButtonElement).disabled).toBe(true);
  await userEvent.click(screen.getByRole("checkbox", { name: /I reviewed/ }));
  expect((prepare as HTMLButtonElement).disabled).toBe(false);
  expect(bridge.savePost).not.toHaveBeenCalled();
  expect(screen.getByText("Frontplate 1 — 456")).toBeTruthy();
});
