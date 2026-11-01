import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Roblox from "./Roblox";
import {
  robloxLivery,
  robloxLiveries,
  robloxSettings,
  uploadSnapshot,
  previewUploadBatch,
  startUploadBatch,
  controlUploadBatch,
  type UploadSnapshot,
} from "./bridge";
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("./bridge", () => ({
  robloxLiveries: vi.fn(),
  robloxSettings: vi.fn(),
  robloxLivery: vi.fn(),
  uploadSnapshot: vi.fn(),
  previewUploadBatch: vi.fn(),
  startUploadBatch: vi.fn(),
  controlUploadBatch: vi.fn(),
  errorMessage: (e: unknown) => String(e),
}));
const empty: UploadSnapshot = {
  revision: 0,
  queue: { jobs: [], items: [], batches: [] },
  concurrency: 5,
  activeTransfers: 0,
  nextAllowedAt: 0,
};
beforeEach(() => {
  vi.clearAllMocks();
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  vi.mocked(robloxLiveries).mockResolvedValue([
    {
      id: "a",
      name: "EMS",
      projectId: "p",
      projectName: "FDNY",
      vehicleId: "v",
      textures: 3,
      uploaded: 1,
    },
    {
      id: "b",
      name: "Patrol",
      projectId: "q",
      projectName: "Police",
      vehicleId: "v",
      textures: 2,
      uploaded: 0,
    },
  ]);
  vi.mocked(robloxSettings).mockResolvedValue({
    configured: true,
    accountId: "123",
  });
  vi.mocked(uploadSnapshot).mockResolvedValue(structuredClone(empty));
  vi.mocked(previewUploadBatch).mockResolvedValue({
    accountId: "123",
    liveries: 1,
    textures: 3,
    uniqueTextures: 3,
    reused: 1,
    queued: 0,
    creates: 4,
  });
  vi.mocked(startUploadBatch).mockResolvedValue("batch");
  vi.mocked(controlUploadBatch).mockResolvedValue();
});
afterEach(cleanup);
it("preselects project shortcuts and requires review before starting", async () => {
  const user = userEvent.setup();
  render(
    <Roblox
      focus={{ projectId: "p", folderIds: ["a"] }}
      onSettings={() => {}}
    />,
  );
  await screen.findByText("EMS");
  expect(screen.queryByText("Patrol")).toBeNull();
  expect(startUploadBatch).not.toHaveBeenCalled();
  await user.click(
    screen.getByRole("button", { name: "Upload missing/changed" }),
  );
  const dialog = await screen.findByRole("dialog");
  expect(previewUploadBatch).toHaveBeenCalledWith(["a"], false);
  expect(within(dialog).getByText(/4 new Roblox decals/)).toBeTruthy();
  await user.click(within(dialog).getByRole("button", { name: "Start batch" }));
  await vi.waitFor(() =>
    expect(startUploadBatch).toHaveBeenCalledWith(
      expect.any(String),
      ["a"],
      false,
    ),
  );
});
it("selects only filtered liveries and supports explicit re-upload", async () => {
  const user = userEvent.setup();
  render(<Roblox focus={{}} onSettings={() => {}} />);
  await screen.findByText("EMS");
  await user.type(screen.getByRole("searchbox"), "Patrol");
  await user.click(
    screen.getByRole("checkbox", { name: "Select all filtered liveries" }),
  );
  await user.click(screen.getByRole("button", { name: "Re-upload selected" }));
  await screen.findByRole("dialog");
  expect(previewUploadBatch).toHaveBeenCalledWith(["b"], true);
  expect(startUploadBatch).not.toHaveBeenCalled();
});
it("resumes paused native queues without changing their account", async () => {
  const user = userEvent.setup();
  vi.mocked(uploadSnapshot).mockResolvedValue({
    ...empty,
    queue: {
      ...empty.queue,
      batches: [
        {
          id: "batch",
          accountId: "123",
          state: "paused",
          createdAt: "2026-10-27T12:00:00Z",
        },
      ],
    },
  });
  render(<Roblox focus={{}} onSettings={() => {}} />);
  await screen.findByText("EMS");
  await user.click(screen.getByRole("button", { name: "Resume all" }));
  expect(controlUploadBatch).toHaveBeenCalledWith(null, "resume");
  expect(startUploadBatch).not.toHaveBeenCalled();
});

it("deletes a cancelled upload batch after confirmation", async () => {
  const user = userEvent.setup();
  const cancelled: UploadSnapshot = {
    ...empty,
    revision: 1,
    queue: {
      batches: [{ id: "batch", accountId: "123", state: "cancelled", createdAt: "2026-10-27T12:00:00Z" }],
      jobs: [{ id: "job", accountId: "123", hash: "hash", name: "Left.png", state: "cancelled", pass: 1, nextAt: 0, message: "", operation: null, firstId: null, secondId: null, moderation: "unknown", generation: 1 }],
      items: [{ batchId: "batch", jobId: "job", folderId: "a", side: "Left 1" }],
    },
  };
  vi.mocked(uploadSnapshot).mockResolvedValueOnce(cancelled).mockResolvedValue({ ...empty, revision: 2 });
  render(<Roblox focus={{}} onSettings={() => {}} />);
  await screen.findByText("EMS");
  await user.click(screen.getByText(/0\/1 textures · cancelled/));
  await user.click(screen.getByRole("button", { name: "Delete batch" }));
  const confirmation = screen.getByRole("group", { name: "Confirm batch deletion" });
  expect(controlUploadBatch).not.toHaveBeenCalled();
  await user.click(within(confirmation).getByRole("button", { name: "Delete batch" }));
  await vi.waitFor(() => expect(controlUploadBatch).toHaveBeenCalledWith("batch", "delete"));
  expect(await screen.findByText("No upload batches.")).toBeTruthy();
});

it("copies several selected liveries with fresh saved decal IDs without uploading", async () => {
  const user = userEvent.setup();
  const clipboard = vi
    .spyOn(navigator.clipboard, "writeText")
    .mockResolvedValue();
  vi.mocked(robloxLivery).mockImplementation(async (id) => ({
    folderId: id,
    accountId: "123",
    name: id === "a" ? "EMS" : "Patrol",
    vehicle: "Vehicle",
    textures: [
      {
        assetId: id,
        hash: id,
        name: "Left.png",
        side: "Left 1",
        upload: {
          accountId: "123",
          hash: id,
          state: "uploaded",
          assetId: id === "a" ? "222" : "444",
          operation: null,
          moderation: "reviewing",
          message: "",
        },
      },
    ],
  }));
  render(<Roblox focus={{}} onSettings={() => {}} />);
  await screen.findByText("EMS");
  await user.click(
    screen.getByRole("checkbox", { name: "Select all filtered liveries" }),
  );
  await user.click(
    screen.getByRole("button", { name: "Copy codes (Markdown)" }),
  );
  await vi.waitFor(() => expect(clipboard).toHaveBeenCalledOnce());
  const copied = clipboard.mock.calls[0][0];
  expect(copied).toContain("## FDNY — EMS");
  expect(copied).toContain("## Police — Patrol");
  expect(copied).toContain("`222`");
  expect(copied).toContain("`444`");
  expect(startUploadBatch).not.toHaveBeenCalled();
});
it("does not replace the clipboard when no usable saved IDs exist", async () => {
  const user = userEvent.setup();
  const clipboard = vi
    .spyOn(navigator.clipboard, "writeText")
    .mockResolvedValue();
  vi.mocked(robloxLivery).mockResolvedValue({
    folderId: "a",
    accountId: "123",
    name: "EMS",
    vehicle: "Vehicle",
    textures: [],
  });
  render(<Roblox focus={{ folderIds: ["a"] }} onSettings={() => {}} />);
  await screen.findByText("EMS");
  await user.click(
    screen.getByRole("button", { name: "Copy codes (Markdown)" }),
  );
  await screen.findByRole("alert");
  expect(clipboard).not.toHaveBeenCalled();
});
