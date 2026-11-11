import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import MediaLibrary from "./MediaLibrary";
import {
  listAssets,
  mediaLibrary,
  syncImageLibrary,
  syncUploadLibraryImages,
  type MediaLibraryItem,
} from "./bridge";

vi.mock("./bridge", () => ({
  addMediaLibraryAsset: vi.fn(),
  addMediaLibraryUrl: vi.fn(),
  deleteMediaLibraryItem: vi.fn(),
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "Failed",
  listAssets: vi.fn(),
  mediaLibrary: vi.fn(),
  pickMediaLibraryImage: vi.fn(),
  updateMediaLibraryItem: vi.fn(),
  syncImageLibrary: vi.fn(),
  syncUploadLibraryImages: vi.fn(),
  syncSetLibraryImageScope: vi.fn(),
}));

const localItem: MediaLibraryItem = {
  id: "local-banner",
  hash: "a".repeat(64),
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

beforeEach(() => {
  vi.clearAllMocks();
  window.localStorage.setItem("apex-studio.team-sync.server-url", "https://team.example");
  window.localStorage.setItem("apex-studio.team-sync.active-team", "team-1");
  vi.mocked(mediaLibrary).mockResolvedValue([localItem]);
  vi.mocked(listAssets).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

it("keeps local library editing available while shared images are still loading", async () => {
  const user = userEvent.setup();
  let finishRemote!: (result: { images: []; errors: [] }) => void;
  vi.mocked(syncImageLibrary).mockImplementation(() => new Promise((resolve) => {
    finishRemote = resolve;
  }));
  render(<MediaLibrary onBusy={vi.fn()} />);

  expect(await screen.findByText("Local banner")).toBeTruthy();
  expect(syncImageLibrary).toHaveBeenCalledWith("https://team.example", "team-1");
  await user.click(screen.getByRole("button", { name: "Add image URL" }));
  expect((screen.getByLabelText("Public image URL") as HTMLInputElement).disabled).toBe(false);

  finishRemote({ images: [], errors: [] });
});

it("loads and edits local library items while project assets are still loading", async () => {
  const user = userEvent.setup();
  vi.mocked(listAssets).mockImplementation(() => new Promise(() => {}));
  render(<MediaLibrary onBusy={vi.fn()} />);

  expect(await screen.findByText("Local banner")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Add image URL" }));
  expect((screen.getByLabelText("Public image URL") as HTMLInputElement).disabled).toBe(false);
});

it("sends no public upload when the user declines the confirmation", async () => {
  const user = userEvent.setup();
  vi.spyOn(window, "confirm").mockReturnValue(false);
  vi.mocked(syncImageLibrary).mockResolvedValue({ images: [], errors: [] });
  render(<MediaLibrary onBusy={vi.fn()} />);

  expect(await screen.findByText("Local banner")).toBeTruthy();
  expect((screen.getByLabelText("For uploads") as HTMLSelectElement).value).toBe("personal");
  await user.click(screen.getByLabelText("Select for upload"));
  await user.click(screen.getByRole("button", { name: "Upload 1" }));

  expect(window.confirm).toHaveBeenCalledOnce();
  expect(syncUploadLibraryImages).not.toHaveBeenCalled();
});
