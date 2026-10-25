import { useEffect, useRef, useState } from "react";
import SlotEditor from "./SlotEditor";
import {
  cancelLivery,
  commitLivery,
  prepareLivery,
  errorMessage,
  type Catalogue,
  type LiveryFolderDraft,
} from "./bridge";
type Row = LiveryFolderDraft & {
  name: string;
  vehicleId: string;
  query: string;
  slots: (string | null)[];
  cover: number | null;
  importedId?: string;
  failure?: string;
  excluded: boolean;
};
export default function LiveryFolderImport({
  projectId,
  parentId,
  folders,
  catalogue,
  onClose,
}: {
  projectId: string;
  parentId: string | null;
  folders: LiveryFolderDraft[];
  catalogue: Catalogue | null;
  onClose: () => void;
}) {
  const [rows, setRows] = useState<Row[]>(() =>
      folders.map((f) => ({
        ...f,
        vehicleId: "",
        query: "",
        slots: f.draft?.files.map((file) => file.suggestedSlot) ?? [],
        cover: null,
        excluded: false,
      })),
    ),
    [active, setActive] = useState(0),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const pending = useRef(
    new Set(folders.flatMap((f) => (f.draft ? [f.draft.token] : []))),
  );
  useEffect(
    () => () => {
      for (const token of pending.current)
        void cancelLivery(token).catch(() => {});
    },
    [],
  );
  const row = rows[active];
  function update(patch: Partial<Row>) {
    setRows((rs) =>
      rs.map((r, i) =>
        i === active ? { ...r, failure: undefined, ...patch } : r,
      ),
    );
    setNotice("");
  }
  function problems(r: Row) {
    if (r.excluded || r.importedId) return [];
    const issues: string[] = [];
    if (!r.draft) issues.push(r.error ?? "This folder cannot be imported.");
    if (
      !r.name.trim() ||
      r.name.trim().length > 120 ||
      /[\\/\x00-\x1f]/.test(r.name)
    )
      issues.push("Enter a livery name up to 120 characters without slashes.");
    if (
      rows.some(
        (other) =>
          other !== r &&
          !other.excluded &&
          other.name.trim().toLowerCase() === r.name.trim().toLowerCase(),
      )
    )
      issues.push("Use a distinct name for each livery in this batch.");
    if (!r.vehicleId) issues.push("Choose a vehicle.");
    r.slots.forEach((slot, i) => {
      if (
        slot &&
        (!/^(Window )?(Left|Right|Front|Back|Top) ([1-9][0-9]?|100)$/.test(
          slot,
        ) ||
          r.slots.indexOf(slot) !== i)
      )
        issues.push(
          `${r.draft?.files[i].name}: resolve the duplicate or invalid side.`,
        );
    });
    return issues;
  }
  const remaining = rows.filter((r) => !r.excluded && !r.importedId),
    imported = rows.filter((r) => r.importedId).length,
    ready = remaining.length > 0 && remaining.every((r) => !problems(r).length);
  const matches =
    catalogue?.vehicles
      .filter((v) =>
        `${v.name} ${v.team}`
          .toLowerCase()
          .includes(row.query.trim().toLowerCase()),
      )
      .slice(0, 8) ?? [];
  async function addImages() {
    if (!row.draft) return;
    setBusy(true);
    try {
      const next = await prepareLivery(projectId, row.draft.token);
      if (next)
        update({
          draft: next,
          slots: [
            ...row.slots,
            ...next.files.slice(row.slots.length).map((f) => f.suggestedSlot),
          ],
        });
    } catch (e) {
      update({ failure: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  }
  async function importAll() {
    if (!ready) return;
    setBusy(true);
    setError("");
    let success = 0,
      failed = 0;
    for (let i = 0; i < rows.length; i++) {
      const current = rows[i];
      if (current.excluded || current.importedId || !current.draft) continue;
      try {
        const id = await commitLivery(
          current.draft.token,
          parentId,
          null,
          current.name.trim(),
          current.vehicleId,
          current.draft.files.map((f, n) => ({
            index: f.index,
            slot: current.slots[n],
          })),
          current.cover,
        );
        pending.current.delete(current.draft.token);
        setRows((rs) =>
          rs.map((r, n) =>
            n === i ? { ...r, importedId: id, failure: undefined } : r,
          ),
        );
        success++;
      } catch (e) {
        failed++;
        setRows((rs) =>
          rs.map((r, n) => (n === i ? { ...r, failure: errorMessage(e) } : r)),
        );
        setActive(i);
      }
    }
    setNotice(
      `${success} ${success === 1 ? "livery imported" : "liveries imported"}.${failed ? ` ${failed} failed; review the results and retry.` : ""}`,
    );
    setBusy(false);
  }
  return (
    <section
      className="folder-livery-import"
      aria-label="Import livery folders"
    >
      <header className="publishing-section-actions">
        <h2>Import livery folders</h2>
        <button className="text-button" disabled={busy} onClick={onClose}>
          {imported ? "Done" : "Cancel"}
        </button>
      </header>
      <p className="field-hint">
        One selected folder becomes one livery. Review each vehicle and texture
        side before importing. PNG files directly inside each folder are
        included.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="folder-livery-layout">
        <nav
          aria-label="Selected livery folders"
          className="folder-livery-list"
        >
          {rows.map((r, i) => (
            <button
              key={r.draft?.token ?? i}
              className="secondary"
              disabled={busy}
              aria-pressed={active === i}
              onClick={() => setActive(i)}
            >
              <strong>{r.name || `Folder ${i + 1}`}</strong>
              <small>
                {r.importedId
                  ? "Imported"
                  : r.excluded
                    ? "Skipped"
                    : r.failure
                      ? "Import failed"
                      : !r.draft
                        ? "Cannot import"
                        : problems(r).length
                          ? "Needs review"
                          : "Ready"}
              </small>
            </button>
          ))}
        </nav>
        {row && (
          <div className="folder-livery-detail">
            {row.importedId ? (
              <p>Livery imported. Its files are now in managed storage.</p>
            ) : (
              <>
                <label className="batch-check">
                  <input
                    type="checkbox"
                    checked={row.excluded}
                    disabled={busy}
                    onChange={(e) => update({ excluded: e.target.checked })}
                  />
                  Skip this folder
                </label>
                {row.warning && <p className="field-hint">{row.warning}</p>}
                {row.error && (
                  <p role="alert" className="error">
                    {row.error}
                  </p>
                )}
                {row.failure && (
                  <p role="alert" className="error">
                    {row.failure}
                  </p>
                )}
                {row.draft && (
                  <fieldset disabled={busy || row.excluded}>
                    <label>
                      Livery name
                      <input
                        aria-label="Batch livery name"
                        value={row.name}
                        maxLength={120}
                        onChange={(e) => update({ name: e.target.value })}
                      />
                    </label>
                    <label>
                      Vehicle
                      <input
                        aria-label="Batch livery vehicle"
                        value={row.query}
                        autoComplete="off"
                        placeholder="Type a vehicle name or team"
                        onChange={(e) =>
                          update({ query: e.target.value, vehicleId: "" })
                        }
                      />
                    </label>
                    {row.query.trim() && !row.vehicleId && (
                      <ul
                        className="batch-vehicle-results"
                        aria-label="Vehicle suggestions"
                      >
                        {matches.map((v) => (
                          <li key={v.id}>
                            <button
                              type="button"
                              className="text-button"
                              onClick={() =>
                                update({
                                  vehicleId: v.id,
                                  query: `${v.name} — ${v.team}`,
                                })
                              }
                            >
                              {v.name} — {v.team}
                            </button>
                          </li>
                        ))}
                        {!matches.length && <li>No matching vehicles.</li>}
                      </ul>
                    )}
                    <div className="livery-file-list">
                      {row.draft.files.map((file, i) => (
                        <div className="livery-file-row" key={file.index}>
                          <span>{file.name}</span>
                          <SlotEditor
                            label={`batch file ${i + 1}`}
                            value={row.slots[i]}
                            disabled={busy || row.excluded}
                            onChange={(slot) =>
                              update({
                                slots: row.slots.map((s, n) =>
                                  n === i ? slot : s,
                                ),
                              })
                            }
                          />
                        </div>
                      ))}
                    </div>
                    <button
                      className="secondary"
                      type="button"
                      disabled={
                        busy || row.excluded || row.draft.files.length >= 100
                      }
                      onClick={() => void addImages()}
                    >
                      Add images
                    </button>
                    <label>
                      Cover image
                      <select
                        aria-label="Batch livery cover"
                        value={row.cover ?? ""}
                        onChange={(e) =>
                          update({
                            cover:
                              e.target.value === ""
                                ? null
                                : Number(e.target.value),
                          })
                        }
                      >
                        <option value="">None</option>
                        {row.draft.files.map((f) => (
                          <option key={f.index} value={f.index}>
                            {f.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <p className="field-hint">
                      Unassigned PNGs remain files in this livery. Originals are
                      preserved.
                    </p>
                    {problems(row).length > 0 && (
                      <ul className="batch-issues">
                        {problems(row).map((p, i) => (
                          <li key={i}>{p}</li>
                        ))}
                      </ul>
                    )}
                  </fieldset>
                )}
              </>
            )}
          </div>
        )}
      </div>
      <footer className="folder-livery-footer">
        <span>
          {imported} imported · {remaining.length} remaining
        </span>
        <button
          className="primary"
          disabled={busy || !ready}
          onClick={() => void importAll()}
        >
          {busy
            ? "Importing liveries…"
            : `Import ${remaining.length} ${remaining.length === 1 ? "livery" : "liveries"}`}
        </button>
      </footer>
    </section>
  );
}
