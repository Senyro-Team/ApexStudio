import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import LiveryFolderImport from "./LiveryFolderImport";
import {
  prepareLivery,
  commitLivery,
  cancelLivery,
  type Catalogue,
  type LiveryFolderDraft,
} from "./bridge";
vi.mock("./bridge", () => ({
  commitLivery: vi.fn(),
  cancelLivery: vi.fn().mockResolvedValue(undefined),
  prepareLivery: vi.fn(),
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : "Failed"),
}));
const catalogue: Catalogue = {
  source: "Official",
  completeness: "Snapshot",
  capturedAt: "",
  vehicles: [
    {
      id: "car",
      name: "Vehicle",
      team: "Police",
      source: "Official",
      liverySupport: "supported",
      bodyStatus: null,
      windowStatus: null,
      notes: [],
      slots: [],
    },
  ],
};
const folders: LiveryFolderDraft[] = ["Patrol A", "Patrol B"].map(
  (name, i) => ({
    name,
    warning: null,
    error: null,
    draft: {
      token: `token${i}`,
      files: [{ index: 0, name: "Left1.png", suggestedSlot: "Left 1" }],
    },
  }),
);
beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);
async function chooseVehicle(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText("Batch livery vehicle"), "Vehicle");
  await user.click(screen.getByRole("button", { name: "Vehicle — Police" }));
}
it("imports each selected folder separately and retries only failed liveries", async () => {
  const user = userEvent.setup();
  let attempts = 0;
  vi.mocked(commitLivery).mockImplementation(async (token) => {
    if (token === "token1" && attempts++ === 0)
      throw Error("Folder name already exists.");
    return token + "-folder";
  });
  render(
    <LiveryFolderImport
      projectId="p"
      parentId="parent"
      folders={folders}
      catalogue={catalogue}
      onClose={() => {}}
    />,
  );
  await chooseVehicle(user);
  await user.selectOptions(screen.getByLabelText("Batch livery cover"), "0");
  const nav = screen.getByRole("navigation", {
    name: "Selected livery folders",
  });
  await user.click(within(nav).getByRole("button", { name: /Patrol B/ }));
  await chooseVehicle(user);
  await user.click(screen.getByRole("button", { name: "Import 2 liveries" }));
  await screen.findByText("Folder name already exists.");
  expect(commitLivery).toHaveBeenCalledWith(
    "token0",
    "parent",
    null,
    "Patrol A",
    "car",
    [{ index: 0, slot: "Left 1" }],
    0,
  );
  await user.click(screen.getByRole("button", { name: "Import 1 livery" }));
  await screen.findByText("1 livery imported.");
  expect(
    vi.mocked(commitLivery).mock.calls.filter((c) => c[0] === "token0"),
  ).toHaveLength(1);
  expect(
    vi.mocked(commitLivery).mock.calls.filter((c) => c[0] === "token1"),
  ).toHaveLength(2);
});
it("keeps per-folder edits when navigating and permits skipping a folder with no PNGs", async () => {
  const user = userEvent.setup();
  vi.mocked(commitLivery).mockResolvedValue("folder");
  const invalid: LiveryFolderDraft = {
    name: "Empty",
    draft: null,
    warning: null,
    error: "No PNG files.",
  };
  const result = render(
    <LiveryFolderImport
      projectId="p"
      parentId={null}
      folders={[folders[0], invalid]}
      catalogue={catalogue}
      onClose={() => {}}
    />,
  );
  await chooseVehicle(user);
  await user.selectOptions(
    screen.getByLabelText("Side for batch file 1"),
    "Right",
  );
  const nav = screen.getByRole("navigation", {
    name: "Selected livery folders",
  });
  await user.click(within(nav).getByRole("button", { name: /Empty/ }));
  await user.click(screen.getByLabelText("Skip this folder"));
  await user.click(within(nav).getByRole("button", { name: /Patrol A/ }));
  expect(screen.getByLabelText("Side for batch file 1")).toHaveProperty(
    "value",
    "Right",
  );
  await user.click(screen.getByRole("button", { name: "Import 1 livery" }));
  await screen.findByText("1 livery imported.");
  expect(commitLivery).toHaveBeenCalledWith(
    "token0",
    null,
    null,
    "Patrol A",
    "car",
    [{ index: 0, slot: "Right 1" }],
    null,
  );
  result.unmount();
  expect(cancelLivery).not.toHaveBeenCalled();
});
it("releases unimported staged folders when the review is closed", async () => {
  const result = render(
    <LiveryFolderImport
      projectId="p"
      parentId={null}
      folders={folders}
      catalogue={catalogue}
      onClose={() => {}}
    />,
  );
  result.unmount();
  await Promise.resolve();
  expect(cancelLivery).toHaveBeenCalledWith("token0");
  expect(cancelLivery).toHaveBeenCalledWith("token1");
});

it("keeps staged folders during the StrictMode effect replay", async () => {
  const result = render(
    <StrictMode>
      <LiveryFolderImport
        projectId="p"
        parentId={null}
        folders={folders}
        catalogue={catalogue}
        onClose={() => {}}
      />
    </StrictMode>,
  );
  await Promise.resolve();
  expect(cancelLivery).not.toHaveBeenCalled();
  result.unmount();
  await Promise.resolve();
  expect(cancelLivery).toHaveBeenCalledWith("token0");
  expect(cancelLivery).toHaveBeenCalledWith("token1");
});

it("shows a recoverable error when adding a missed image fails", async () => {
  const user = userEvent.setup();
  vi.mocked(prepareLivery).mockRejectedValue(
    Error("This image is not a valid PNG."),
  );
  render(
    <LiveryFolderImport
      projectId="p"
      parentId={null}
      folders={folders}
      catalogue={catalogue}
      onClose={() => {}}
    />,
  );
  await user.click(screen.getByRole("button", { name: "Add images" }));
  await screen.findByRole("alert");
  expect(screen.getByText("This image is not a valid PNG.")).toBeTruthy();
});
