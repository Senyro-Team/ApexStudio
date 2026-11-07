import { useEffect, useState } from "react";
import AppSelect from "./AppSelect";
import {
  EditorContent,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  useEditor,
  useEditorState,
  type NodeViewProps,
} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Extension } from "@tiptap/core";
import { canSplit } from "@tiptap/pm/transform";
export const HeadingEnter = Extension.create({
  name: "headingEnter",
  priority: 1000,
  addKeyboardShortcuts() {
    return {
      Enter: () => {
        if (!this.editor.isActive("heading")) return false;
        const { state, view } = this.editor;
        const transaction = state.tr.deleteSelection();
        const position = transaction.selection.from;
        const types = [{ type: state.schema.nodes.paragraph }];
        if (!canSplit(transaction.doc, position, 1, types)) return false;
        view.dispatch(transaction.split(position, 1, types).scrollIntoView());
        return true;
      },
    };
  },
});
import Image from "@tiptap/extension-image";
import { TableKit } from "@tiptap/extension-table";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { Markdown } from "@tiptap/markdown";
import {
  Bold,
  Italic,
  Strikethrough,
  List,
  ListOrdered,
  ListChecks,
  Quote,
  Code2,
  Link,
  ImagePlus,
  Table2,
  Undo2,
  Redo2,
  Minus,
} from "lucide-react";
import {
  archiveNote,
  deleteArchivedNote,
  attachNoteImage,
  createNote,
  desktopAvailable,
  errorMessage,
  listNotes,
  noteImage,
  saveNote,
  type Note,
} from "./bridge";
function LocalImage({ node, extension }: NodeViewProps) {
  const [src, setSrc] = useState(""),
    [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    setSrc("");
    setFailed(false);
    const hash = String(node.attrs.src).replace("apex-image:", "");
    noteImage(extension.options.noteId, hash)
      .then((value) => {
        if (active) setSrc(value);
      })
      .catch(() => {
        if (active) setFailed(true);
      });
    return () => {
      active = false;
    };
  }, [node.attrs.src, extension.options.noteId]);
  return (
    <NodeViewWrapper className="note-image" contentEditable={false}>
      {src ? (
        <img src={src} alt={node.attrs.alt || ""} />
      ) : (
        <span>{failed ? "Image unavailable" : "Loading image…"}</span>
      )}
    </NodeViewWrapper>
  );
}
const ManagedImage = Image.extend<{ noteId: string }>({
  addOptions() {
    return { ...this.parent?.(), noteId: "" };
  },
  addNodeView() {
    return ReactNodeViewRenderer(LocalImage);
  },
});
function NoteEditor({
  note,
  onSaved,
  onDirty,
  onDiscard,
  readOnly,
}: {
  note: Note;
  onSaved: (note: Note) => void;
  onDirty: (value: boolean) => void;
  onDiscard: () => void;
  readOnly: boolean;
}) {
  const [title, setTitle] = useState(note.title),
    [dirty, setDirty] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [link, setLink] = useState<string | null>(null);
  const markDirty = () => {
    setDirty(true);
    onDirty(true);
  };
  const editor = useEditor({
    extensions: [
      HeadingEnter,
      StarterKit.configure({
        link: { openOnClick: false, protocols: ["https", "http", "mailto"] },
      }),
      ManagedImage.configure({ noteId: note.id }),
      TableKit.configure({ table: { resizable: false } }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Markdown,
    ],
    content: note.document,
    editable: !readOnly,
    shouldRerenderOnTransaction: false,
    onUpdate: markDirty,
    editorProps: {
      attributes: {
        "aria-label": "Note content",
        role: "textbox",
        "aria-multiline": "true",
      },
      transformPastedHTML(html) {
        const doc = new DOMParser().parseFromString(html, "text/html");
        doc.querySelectorAll("img").forEach((img) => img.remove());
        return doc.body.innerHTML;
      },
    },
  });
  const state = useEditorState({
    editor,
    selector: ({ editor }) => ({
      bold: editor?.isActive("bold"),
      italic: editor?.isActive("italic"),
      strike: editor?.isActive("strike"),
      heading: editor?.getAttributes("heading").level || 0,
      table: editor?.isActive("table"),
    }),
  });
  useEffect(() => {
    editor?.setEditable(!readOnly && !busy, false);
  }, [editor, readOnly, busy]);
  async function save() {
    if (!editor) return;
    setBusy(true);
    setError("");
    try {
      const saved = await saveNote(
        note,
        title,
        editor.getJSON(),
        editor.getMarkdown(),
      );
      onSaved(saved);
      setDirty(false);
      onDirty(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function image() {
    setBusy(true);
    setError("");
    try {
      const hash = await attachNoteImage(note.id);
      if (hash)
        editor
          ?.chain()
          .focus()
          .setImage({ src: `apex-image:${hash}` })
          .run();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const tools = [
    {
      name: "Bold",
      icon: Bold,
      active: state?.bold,
      run: () => editor?.chain().focus().toggleBold().run(),
    },
    {
      name: "Italic",
      icon: Italic,
      active: state?.italic,
      run: () => editor?.chain().focus().toggleItalic().run(),
    },
    {
      name: "Strikethrough",
      icon: Strikethrough,
      active: state?.strike,
      run: () => editor?.chain().focus().toggleStrike().run(),
    },
    {
      name: "Bullet list",
      icon: List,
      run: () => editor?.chain().focus().toggleBulletList().run(),
    },
    {
      name: "Numbered list",
      icon: ListOrdered,
      run: () => editor?.chain().focus().toggleOrderedList().run(),
    },
    {
      name: "Checklist",
      icon: ListChecks,
      run: () => editor?.chain().focus().toggleTaskList().run(),
    },
    {
      name: "Quote",
      icon: Quote,
      run: () => editor?.chain().focus().toggleBlockquote().run(),
    },
    {
      name: "Code block",
      icon: Code2,
      run: () => editor?.chain().focus().toggleCodeBlock().run(),
    },
    {
      name: "Link",
      icon: Link,
      run: () => setLink(editor?.getAttributes("link").href || "https://"),
    },
    { name: "Insert PNG image", icon: ImagePlus, run: () => void image() },
    {
      name: "Insert table",
      icon: Table2,
      run: () =>
        editor
          ?.chain()
          .focus()
          .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
          .run(),
    },
    {
      name: "Divider",
      icon: Minus,
      run: () => editor?.chain().focus().setHorizontalRule().run(),
    },
    {
      name: "Undo",
      icon: Undo2,
      run: () => editor?.chain().focus().undo().run(),
    },
    {
      name: "Redo",
      icon: Redo2,
      run: () => editor?.chain().focus().redo().run(),
    },
  ];
  return (
    <section className="note-editor">
      <div className="note-title-row">
        <input
          aria-label="Note title"
          maxLength={120}
          disabled={readOnly || busy}
          value={title}
          onChange={(e) => {
            setTitle(e.target.value);
            markDirty();
          }}
        />
        <button
          className="primary"
          disabled={readOnly || busy || !dirty || !title.trim()}
          onClick={() => void save()}
        >
          {busy ? "Saving…" : "Save note"}
        </button>
      </div>
      <span className="note-status" role="status">
        {dirty
          ? "Unsaved changes"
          : `Saved · ${new Date(note.updatedAt).toLocaleDateString("en-US")}`}
      </span>
      {dirty && (
        <button className="text-button" onClick={onDiscard}>
          Discard changes
        </button>
      )}
      {!readOnly && (
        <div className="editor-toolbar" role="group" aria-label="Formatting">
          <AppSelect
            aria-label="Text style"
            value={state?.heading || 0}
            disabled={busy}
            onChange={(e) => {
              const level = Number(e.target.value);
              if (level)
                editor
                  ?.chain()
                  .focus()
                  .setHeading({ level: level as 1 | 2 | 3 })
                  .run();
              else editor?.chain().focus().setParagraph().run();
            }}
          >
            <option value={0}>Paragraph</option>
            <option value={1}>Heading 1</option>
            <option value={2}>Heading 2</option>
            <option value={3}>Heading 3</option>
          </AppSelect>
          {tools.map(({ name, icon: Icon, run, active }) => (
            <button
              key={name}
              title={name}
              aria-label={name}
              aria-pressed={active === undefined ? undefined : active}
              disabled={busy}
              onMouseDown={(e) => e.preventDefault()}
              onClick={run}
            >
              <Icon size={17} />
            </button>
          ))}
        </div>
      )}
      {state?.table && !readOnly && (
        <div className="table-actions" role="group" aria-label="Table controls">
          <button
            disabled={busy}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().addRowAfter().run()}
          >
            Add row
          </button>
          <button
            disabled={busy}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().addColumnAfter().run()}
          >
            Add column
          </button>
          <button
            disabled={busy}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().deleteRow().run()}
          >
            Delete row
          </button>
          <button
            disabled={busy}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().deleteColumn().run()}
          >
            Delete column
          </button>
          <button
            disabled={busy}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().deleteTable().run()}
          >
            Remove table
          </button>
        </div>
      )}
      {link !== null && (
        <form
          className="link-editor"
          onSubmit={(e) => {
            e.preventDefault();
            if (!editor) return;
            if (!link) {
              editor.chain().focus().extendMarkRange("link").unsetLink().run();
              setLink(null);
              return;
            }
            try {
              const url = new URL(link);
              if (
                !["https:", "http:", "mailto:"].includes(url.protocol) ||
                url.username ||
                url.password
              )
                throw Error("Enter an HTTP, HTTPS or mailto link.");
              editor
                .chain()
                .focus()
                .extendMarkRange("link")
                .setLink({ href: link })
                .run();
              setLink(null);
            } catch (e) {
              setError(errorMessage(e));
            }
          }}
        >
          <input
            aria-label="Link URL"
            value={link}
            onChange={(e) => setLink(e.target.value)}
          />
          <button className="secondary">Apply link</button>
          <button
            type="button"
            className="text-button"
            onClick={() => setLink(null)}
          >
            Cancel
          </button>
        </form>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <EditorContent editor={editor} />
    </section>
  );
}
export default function Notes({
  projectId = null,
  onEditingChange,
  readOnly = false,
}: {
  projectId?: string | null;
  onEditingChange: (value: boolean) => void;
  readOnly?: boolean;
}) {
  const [notes, setNotes] = useState<Note[]>([]),
    [selected, setSelected] = useState<string | null>(null),
    [archived, setArchived] = useState(false),
    [query, setQuery] = useState(""),
    [dirty, setDirty] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(desktopAvailable),
    [pending, setPending] = useState<string | null>(null),
    [confirmDelete, setConfirmDelete] = useState(false),
    [generation, setGeneration] = useState(0);
  useEffect(() => {
    onEditingChange(dirty || busy);
    return () => onEditingChange(false);
  }, [dirty, busy, onEditingChange]);
  useEffect(() => {
    if (!desktopAvailable) return;
    let active = true;
    setLoading(true);
    listNotes(projectId)
      .then((items) => {
        if (active) {
          setNotes(items);
          setSelected(null);
        }
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [projectId]);
  async function add() {
    setBusy(true);
    setError("");
    try {
      const note = await createNote(projectId);
      setNotes((items) => [note, ...items]);
      setSelected(note.id);
      setArchived(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  function choose(id: string) {
    if (dirty) {
      setPending(id);
      return;
    }
    setSelected(id);
    setConfirmDelete(false);
  }
  const selectedNote = notes.find((n) => n.id === selected);
  const update = (note: Note) =>
    setNotes((items) => [note, ...items.filter((n) => n.id !== note.id)]);
  async function archive() {
    if (!selectedNote) return;
    setBusy(true);
    try {
      update(await archiveNote(selectedNote, !selectedNote.archived));
      setSelected(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function deleteNote() {
    if (!selectedNote?.archived) return;
    setBusy(true);
    setError("");
    try {
      await deleteArchivedNote(selectedNote);
      setNotes((items) => items.filter((item) => item.id !== selectedNote.id));
      setSelected(null);
      setConfirmDelete(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="notes-area">
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="notes-heading">
        {projectId ? <h2>Project notes</h2> : <span />}
        <button
          className="primary"
          disabled={!desktopAvailable || readOnly || busy || dirty}
          onClick={() => void add()}
        >
          New note
        </button>
      </div>
      {pending && (
        <div className="confirm-panel">
          <p>Discard unsaved changes?</p>
          <button
            className="secondary"
            onClick={() => {
              setDirty(false);
              onEditingChange(false);
              setSelected(pending);
              setPending(null);
              setGeneration((v) => v + 1);
            }}
          >
            Discard changes
          </button>
          <button className="text-button" onClick={() => setPending(null)}>
            Keep editing
          </button>
        </div>
      )}
      <div className="notes-layout">
        <aside className="note-list" aria-label="Notes">
          <input
            aria-label="Search notes"
            placeholder="Search notes"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            className="text-button"
            disabled={dirty}
            onClick={() => {
              setArchived(!archived);
              setSelected(null);
              setConfirmDelete(false);
            }}
          >
            {archived ? "Active notes" : "Archived notes"}
          </button>
          {loading ? (
            <p role="status">Loading notes…</p>
          ) : (
            notes
              .filter(
                (n) =>
                  n.archived === archived &&
                  `${n.title} ${n.markdown}`
                    .toLowerCase()
                    .includes(query.toLowerCase()),
              )
              .map((note) => (
                <button
                  className={`note-item ${selected === note.id ? "selected" : ""}`}
                  key={note.id}
                  onClick={() => choose(note.id)}
                >
                  <strong>{note.title}</strong>
                  <small>
                    {new Date(note.updatedAt).toLocaleDateString("en-US")}
                  </small>
                </button>
              ))
          )}
          {!loading && !notes.some((n) => n.archived === archived) && (
            <p className="muted">
              {archived ? "No archived notes." : "No notes."}
            </p>
          )}
        </aside>
        <div>
          {selectedNote ? (
            <>
              <NoteEditor
                key={`${selectedNote.id}-${generation}`}
                note={selectedNote}
                onSaved={update}
                onDirty={setDirty}
                onDiscard={() => setPending(selectedNote.id)}
                readOnly={readOnly || selectedNote.archived}
              />
              <button
                className="text-button note-archive"
                disabled={dirty || busy || readOnly}
                onClick={() => void archive()}
              >
                {selectedNote.archived ? "Restore note" : "Archive note"}
              </button>
              {selectedNote.archived && (
                <>
                  <button
                    className="text-button note-archive"
                    disabled={dirty || busy}
                    onClick={() => setConfirmDelete(true)}
                  >
                    Delete permanently
                  </button>
                  {confirmDelete && (
                    <div className="confirm-panel" role="alertdialog" aria-label="Delete note permanently">
                      <p>Delete “{selectedNote.title}” permanently from this workspace? Existing backups are unchanged.</p>
                      <button className="danger" disabled={busy} onClick={() => void deleteNote()}>
                        Delete permanently
                      </button>
                      <button className="text-button" disabled={busy} onClick={() => setConfirmDelete(false)}>
                        Cancel
                      </button>
                    </div>
                  )}
                </>
              )}
            </>
          ) : (
            <div className="note-empty">Select a note.</div>
          )}
        </div>
      </div>
    </div>
  );
}
