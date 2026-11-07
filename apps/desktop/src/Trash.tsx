import { useEffect, useRef, useState } from "react";
import AppSelect from "./AppSelect";
import {
  listTrash,
  restoreTrash,
  purgeTrash,
  errorMessage,
  type TrashItem,
} from "./bridge";
export default function Trash({
  onBusy,
  onChange,
}: {
  onBusy: (v: boolean) => void;
  onChange: () => void;
}) {
  const [rows, setRows] = useState<TrashItem[]>([]),
    [project, setProject] = useState(""),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(false),
    [confirm, setConfirm] = useState<TrashItem | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const load = async () => setRows(await listTrash());
  useEffect(() => {
    void load().catch((e) => setError(errorMessage(e)));
  }, []);
  useEffect(() => {
    onBusy(busy || !!confirm);
    return () => onBusy(false);
  }, [busy, confirm, onBusy]);
  useEffect(() => {
    if (confirm) dialog.current?.showModal();
  }, [confirm]);
  async function action(row: TrashItem, purge: boolean) {
    setBusy(true);
    setError("");
    try {
      if (purge) {
        await purgeTrash(row.id);
        setNotice("Permanently deleted.");
      } else {
        const name = await restoreTrash(row.id);
        setNotice(`Restored ${name}.`);
      }
      setConfirm(null);
      await load();
      onChange();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="trash-panel" aria-label="Project trash">
      <h2>Trash</h2>
      {error && !confirm && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <label>
        Project
        <AppSelect value={project} onChange={(e) => setProject(e.target.value)}>
          <option value="">All projects</option>
          {[
            ...new Map(rows.map((r) => [r.projectId, r.projectName])).entries(),
          ].map(([id, name]) => (
            <option key={id} value={id}>
              {name}
            </option>
          ))}
        </AppSelect>
      </label>
      <ul className="trash-list">
        {rows
          .filter((r) => !project || r.projectId === project)
          .map((row) => (
            <li key={row.id}>
              <div>
                <strong>{row.name}</strong>
                <small>
                  {row.projectName} · {row.folders} folders · {row.files} files
                  · {new Date(row.deletedAt).toLocaleDateString()}
                </small>
              </div>
              <div className="trash-actions">
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() => void action(row, false)}
                >
                  Restore
                </button>
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => setConfirm(row)}
                >
                  Delete permanently
                </button>
              </div>
            </li>
          ))}
      </ul>
      {!rows.length && <p>Trash is empty.</p>}
      {confirm && (
        <dialog
          ref={dialog}
          onCancel={(e) => {
            if (busy) e.preventDefault();
            else setConfirm(null);
          }}
          aria-label="Permanently delete"
        >
          <h2>Delete permanently?</h2>
          {error && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
          <p>
            {confirm.name} · {confirm.folders} folders · {confirm.files} files
          </p>
          <p>
            This removes the project files and their recoverable versions.
            Content used elsewhere is kept.
          </p>
          <div className="button-row">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setConfirm(null)}
            >
              Cancel
            </button>
            <button
              className="primary"
              disabled={busy}
              onClick={() => void action(confirm, true)}
            >
              Delete permanently
            </button>
          </div>
        </dialog>
      )}
    </section>
  );
}
