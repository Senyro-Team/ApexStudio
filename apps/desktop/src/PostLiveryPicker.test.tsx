import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PostLiveryPicker from "./PostLiveryPicker";
import { listProjectFolderOptions, robloxLiveries, robloxLivery } from "./bridge";

vi.mock("./bridge", () => ({
  listProjectFolderOptions: vi.fn(),
  robloxLiveries: vi.fn(),
  robloxLivery: vi.fn(),
  errorMessage: (cause: unknown) => String(cause),
}));

afterEach(cleanup);

it("adds only liveries in the selected folder subtree to an ERLCX draft", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const onBusy = vi.fn();
  vi.mocked(robloxLiveries).mockResolvedValue([
    { id: "direct", parentId: "ems", name: "EMS Rescue", projectId: "p", projectName: "FDNY", vehicleId: null, textures: 1, uploaded: 1 },
    { id: "nested", parentId: "special", name: "EMS Special", projectId: "p", projectName: "FDNY", vehicleId: null, textures: 1, uploaded: 1 },
    { id: "sibling", parentId: "patrol", name: "Patrol", projectId: "p", projectName: "FDNY", vehicleId: null, textures: 1, uploaded: 1 },
  ]);
  vi.mocked(listProjectFolderOptions).mockResolvedValue([
    { id: "root", parentId: null, name: "Liveries" },
    { id: "ems", parentId: "root", name: "EMS" },
    { id: "special", parentId: "ems", name: "Special" },
    { id: "patrol", parentId: "root", name: "Patrol" },
  ]);
  vi.mocked(robloxLivery).mockImplementation(async (id) => ({
    folderId: id,
    accountId: "123",
    name: id,
    vehicle: "Vehicle",
    primaryColor: "#FFFFFF",
    secondaryColor: null,
    textures: [{
      assetId: id,
      hash: id,
      name: "Left1.png",
      side: "Left 1",
      upload: {
        accountId: "123",
        hash: id,
        assetId: `${id}-decal`,
        state: "uploaded",
        operation: null,
        moderation: "approved",
        message: "",
      },
    }],
  }));
  render(
    <PostLiveryPicker
      value={[]}
      disabled={false}
      onChange={onChange}
      onBusy={onBusy}
      catalogue={null}
    />,
  );
  await user.selectOptions(await screen.findByLabelText("Project"), "p");
  await user.selectOptions(screen.getByLabelText("ERLCX folder"), "ems");
  expect(screen.queryByText("Patrol")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Select all filtered" }));
  await waitFor(() => expect(robloxLivery).toHaveBeenCalledTimes(2));
  expect(robloxLivery).toHaveBeenNthCalledWith(1, "direct");
  expect(robloxLivery).toHaveBeenNthCalledWith(2, "nested");
  expect(onChange).toHaveBeenCalledWith([
    expect.objectContaining({ source: expect.objectContaining({ folderId: "direct" }) }),
    expect.objectContaining({ source: expect.objectContaining({ folderId: "nested" }) }),
  ]);
});
