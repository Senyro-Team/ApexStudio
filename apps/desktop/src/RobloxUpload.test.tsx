import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import RobloxUpload, {
  publishingLivery,
  readyForPublishing,
} from "./RobloxUpload";
import RobloxSettings from "./RobloxSettings";
import {
  robloxLivery,
  uploadRobloxTexture,
  robloxSettings,
  saveRobloxSettings,
  type LiveryUploads,
} from "./bridge";
vi.mock("./bridge", () => ({
  robloxLivery: vi.fn(),
  uploadRobloxTexture: vi.fn(),
  robloxSettings: vi.fn(),
  saveRobloxSettings: vi.fn(),
  testRobloxConnection: vi.fn(),
  clearRobloxSettings: vi.fn(),
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : "Failed"),
}));
const plan: LiveryUploads = {
  folderId: "f",
  name: "Patrol",
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
};
beforeEach(() => {
  vi.clearAllMocks();
  HTMLDialogElement.prototype.showModal = vi.fn(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.mocked(robloxLivery).mockResolvedValue(structuredClone(plan));
  vi.mocked(robloxSettings).mockResolvedValue({
    configured: false,
    accountId: null,
  });
});
afterEach(cleanup);
it("does not upload until requested, then includes the returned decal ID with texture provenance", async () => {
  const user = userEvent.setup(),
    done = vi.fn();
  const uploaded = structuredClone(plan);
  uploaded.textures[0].upload = {
    accountId: "123",
    hash: plan.textures[0].hash,
    state: "uploaded",
    assetId: "456",
    operation: "operations/test",
    moderation: "reviewing",
    message: "",
  };
  vi.mocked(uploadRobloxTexture).mockImplementation(async () => {
    vi.mocked(robloxLivery).mockResolvedValue(uploaded);
    return uploaded.textures[0].upload!;
  });
  render(
    <RobloxUpload
      folderId="f"
      include
      onDone={done}
      onClose={() => {}}
      onBusy={() => {}}
    />,
  );
  await screen.findByText("Left 1");
  expect(uploadRobloxTexture).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Upload and include" }));
  await vi.waitFor(() => expect(done).toHaveBeenCalledWith(uploaded));
  expect(publishingLivery(uploaded).sides).toEqual([
    { side: "Left 1", decalId: "456" },
  ]);
  uploaded.textures[0].upload!.moderation = "rejected";
  expect(readyForPublishing(uploaded)).toBe(false);
});
it("skips a missing livery without uploading or including it", async () => {
  const user = userEvent.setup(),
    close = vi.fn(),
    done = vi.fn();
  render(
    <RobloxUpload
      folderId="f"
      include
      onDone={done}
      onClose={close}
      onBusy={() => {}}
    />,
  );
  await screen.findByText("Left 1");
  await user.click(screen.getByRole("button", { name: "Skip this livery" }));
  expect(close).toHaveBeenCalledOnce();
  expect(done).not.toHaveBeenCalled();
  expect(uploadRobloxTexture).not.toHaveBeenCalled();
});
it("keeps an API key hidden and clears the field after a verified secure save", async () => {
  const user = userEvent.setup();
  vi.mocked(saveRobloxSettings).mockResolvedValue();
  render(<RobloxSettings onBusy={() => {}} />);
  await user.type(await screen.findByLabelText("Account ID"), "123");
  await user.type(screen.getByLabelText("API key"), "dummy-key");
  expect(screen.getByLabelText("API key")).toHaveProperty("type", "password");
  vi.mocked(robloxSettings).mockResolvedValue({
    configured: true,
    accountId: "123",
  });
  await user.click(screen.getByRole("button", { name: "Verify and save" }));
  await screen.findByText("Roblox connection verified and saved.");
  expect(saveRobloxSettings).toHaveBeenCalledWith("123", "dummy-key");
  expect(screen.getByLabelText("Replace API key")).toHaveProperty("value", "");
});
