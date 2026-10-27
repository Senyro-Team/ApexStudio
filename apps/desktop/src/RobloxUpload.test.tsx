import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { publishingLivery, readyForPublishing } from "./RobloxUpload";
import RobloxSettings from "./RobloxSettings";
import {
  robloxLivery,
  robloxSettings,
  saveRobloxSettings,
  type LiveryUploads,
} from "./bridge";
vi.mock("./bridge", () => ({
  robloxLivery: vi.fn(),
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
it("includes selected decal IDs and rejects moderated textures", () => {
  const uploaded = structuredClone(plan);
  uploaded.textures[0].upload = {
    accountId: "123",
    hash: plan.textures[0].hash,
    state: "uploaded",
    assetId: "456",
    operation: null,
    moderation: "reviewing",
    message: "",
  };
  expect(readyForPublishing(uploaded)).toBe(true);
  expect(publishingLivery(uploaded).sides).toEqual([
    { side: "Left 1", decalId: "456" },
  ]);
  uploaded.textures[0].upload!.moderation = "rejected";
  expect(readyForPublishing(uploaded)).toBe(false);
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
