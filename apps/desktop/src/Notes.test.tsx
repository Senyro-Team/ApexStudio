import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Notes from "./Notes";
import { deleteArchivedNote, listNotes, saveNote, type Note } from "./bridge";
vi.mock("./bridge", () => ({
  desktopAvailable: true,
  listNotes: vi.fn(),
  createNote: vi.fn(),
  saveNote: vi.fn(),
  archiveNote: vi.fn(),
  deleteArchivedNote: vi.fn(),
  attachNoteImage: vi.fn(),
  noteImage: vi.fn(),
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : "Failed"),
}));
const note: Note = {
  id: "n1",
  projectId: null,
  title: "Reference notes",
  document: {
    type: "doc",
    content: [
      {
        type: "paragraph",
        content: [{ type: "text", text: "Keep the original files." }],
      },
    ],
  },
  markdown: "Keep the original files.",
  revision: 2,
  archived: false,
  createdAt: "2026-10-22T12:00:00Z",
  updatedAt: "2026-10-22T12:00:00Z",
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listNotes).mockResolvedValue([note]);
});
afterEach(cleanup);
it("deletes an archived note only after confirmation", async () => {
  const user = userEvent.setup();
  const archivedNote = { ...note, archived: true };
  vi.mocked(listNotes).mockResolvedValue([archivedNote]);
  vi.mocked(deleteArchivedNote).mockResolvedValue(undefined);
  render(<Notes onEditingChange={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "Archived notes" }));
  await user.click(screen.getByRole("button", { name: /Reference notes/ }));
  await user.click(screen.getByRole("button", { name: "Delete permanently" }));
  expect(deleteArchivedNote).not.toHaveBeenCalled();
  const dialog = screen.getByRole("alertdialog", { name: "Delete note permanently" });
  await user.click(within(dialog).getByRole("button", { name: "Delete permanently" }));
  expect(deleteArchivedNote).toHaveBeenCalledWith(archivedNote);
  expect(await screen.findByText("No archived notes.")).toBeTruthy();
});
it("saves formatted content with the current revision and clears unsaved state only after success", async () => {
  const user = userEvent.setup();
  const lock = vi.fn();
  vi.mocked(saveNote).mockResolvedValue({
    ...note,
    title: "Updated notes",
    revision: 3,
  });
  render(<Notes onEditingChange={lock} />);
  await user.click(
    await screen.findByRole("button", { name: /Reference notes/ }),
  );
  const title = screen.getByLabelText("Note title");
  await user.clear(title);
  await user.type(title, "Updated notes");
  expect(screen.getByText("Unsaved changes")).toBeTruthy();
  expect(lock).toHaveBeenLastCalledWith(true);
  await user.click(screen.getByRole("button", { name: "Save note" }));
  await screen.findByText(/Saved ·/);
  expect(saveNote).toHaveBeenCalledWith(
    note,
    "Updated notes",
    expect.objectContaining({ type: "doc" }),
    "Keep the original files.",
  );
  expect(lock).toHaveBeenLastCalledWith(false);
});
it("keeps failed saves editable and asks before discarding", async () => {
  const user = userEvent.setup();
  vi.mocked(saveNote).mockRejectedValue(
    new Error("Conflict: reopen the note."),
  );
  render(<Notes onEditingChange={() => {}} />);
  await user.click(
    await screen.findByRole("button", { name: /Reference notes/ }),
  );
  await user.type(screen.getByLabelText("Note title"), " changed");
  await user.click(screen.getByRole("button", { name: "Save note" }));
  await screen.findByRole("alert");
  expect(screen.getByText("Unsaved changes")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Discard changes" }));
  expect(screen.getByText("Discard unsaved changes?")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Keep editing" }));
  expect(screen.getByLabelText("Note title")).toHaveProperty(
    "value",
    "Reference notes changed",
  );
});

it("starts a paragraph when Enter splits a heading", async () => {
  const { Editor } = await import("@tiptap/core");
  const { default: StarterKit } = await import("@tiptap/starter-kit");
  const { HeadingEnter } = await import("./Notes");
  const editor = new Editor({
    element: document.createElement("div"),
    extensions: [StarterKit.configure({ trailingNode: false }), HeadingEnter],
    content: {
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 2 },
          content: [{ type: "text", text: "Title" }],
        },
      ],
    },
  });
  editor.commands.setTextSelection(6);
  editor.view.dom.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
    }),
  );
  expect(editor.getJSON().content?.map((n) => n.type)).toEqual([
    "heading",
    "paragraph",
  ]);
  editor.destroy();
});
