import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Trash from "./Trash";
import { listTrash, restoreTrash, purgeTrash } from "./bridge";
vi.mock("./bridge", () => ({
  listTrash: vi.fn(),
  restoreTrash: vi.fn(),
  purgeTrash: vi.fn(),
  errorMessage: (e: Error) => e.message,
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
const row = {
  id: "trash",
  projectId: "p",
  projectName: "Studio",
  rootId: "f",
  kind: "folder" as const,
  name: "Livery",
  deletedAt: "2026-10-28T10:00:00Z",
  files: 3,
  folders: 1,
};
it("restores an entry and reloads the list", async () => {
  vi.mocked(listTrash).mockResolvedValueOnce([row]).mockResolvedValue([]);
  vi.mocked(restoreTrash).mockResolvedValue("Livery (restored 1)");
  const changed = vi.fn();
  render(<Trash onBusy={() => {}} onChange={changed} />);
  await userEvent.click(await screen.findByRole("button", { name: "Restore" }));
  expect(restoreTrash).toHaveBeenCalledWith("trash");
  expect(await screen.findByText("Restored Livery (restored 1).")).toBeTruthy();
  expect(changed).toHaveBeenCalledOnce();
});
it("requires a separate confirmation before permanently deleting", async () => {
  HTMLDialogElement.prototype.showModal = vi.fn(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.mocked(listTrash).mockResolvedValue([row]);
  vi.mocked(purgeTrash).mockResolvedValue();
  render(<Trash onBusy={() => {}} onChange={() => {}} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Delete permanently" }),
  );
  expect(purgeTrash).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getAllByRole("button", { name: "Delete permanently" })[1],
  );
  expect(purgeTrash).toHaveBeenCalledWith("trash");
});
