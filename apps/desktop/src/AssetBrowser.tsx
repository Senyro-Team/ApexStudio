import { useEffect, useState } from "react";
import { FolderOpen, File, ChevronRight, X, Layers } from "lucide-react";
import {
  assetBoard,
  saveFolder,
  moveAsset,
  assetVersions,
  restoreAssetVersion,
  setFolderCover,
  prepareLivery,
  commitLivery,
  cancelLivery,
  importAssets,
  saveAssetCopy,
  errorMessage,
  type AssetBoard,
  type AssetVersion,
  type Catalogue,
  type Project,
  type LiveryDraft,
} from "./bridge";
const empty: AssetBoard = { assets: [], folders: [], placements: [] };
export default function AssetBrowser({
  project,
  initialAssetId = null,
  catalogue,
  onBusy,
  onChange,
}: {
  project: Project;
  initialAssetId?: string | null;
  catalogue: Catalogue | null;
  onBusy: (value: boolean) => void;
  onChange: () => void;
}) {
  const [board, setBoard] = useState<AssetBoard>(empty),
    [folderId, setFolderId] = useState<string | null>(null),
    [selected, setSelected] = useState<string | null>(null),
    [versions, setVersions] = useState<AssetVersion[]>([]),
    [query, setQuery] = useState(""),
    [busy, setBusy] = useState(false),
    [loading, setLoading] = useState(true),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [folderForm, setFolderForm] = useState<null | "new" | "edit">(null),
    [folderName, setFolderName] = useState(""),
    [parent, setParent] = useState<string | null>(null);
  const [draft, setDraft] = useState<LiveryDraft | null>(null),
    [liveryName, setLiveryName] = useState(""),
    [vehicleId, setVehicleId] = useState(""),
    [vehicleQuery, setVehicleQuery] = useState(""),
    [slots, setSlots] = useState<(string | null)[]>([]),
    [cover, setCover] = useState<number | null>(null),
    [moveTo, setMoveTo] = useState<string | null>(null);
  const folder = board.folders.find((f) => f.id === folderId),
    asset = board.assets.find((a) => a.id === selected),
    placement = board.placements.find((p) => p.assetId === selected),
    vehicle = catalogue?.vehicles.find((v) => v.id === vehicleId);
  useEffect(() => {
    onBusy(busy || Boolean(draft) || Boolean(folderForm));
    return () => onBusy(false);
  }, [busy, draft, folderForm, onBusy]);
  async function reload() {
    const next = await assetBoard(project.id);
    setBoard(next);
    return next;
  }
  useEffect(() => {
    let active = true;
    assetBoard(project.id)
      .then((value) => {
        if (active) {
          setBoard(value);
          if (initialAssetId) {
            setFolderId(
              value.placements.find((p) => p.assetId === initialAssetId)
                ?.folderId ?? null,
            );
            setSelected(initialAssetId);
            setMoveTo(
              value.placements.find((p) => p.assetId === initialAssetId)
                ?.folderId ?? null,
            );
          }
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
  }, [project.id, initialAssetId]);
  useEffect(() => {
    if (!selected) {
      setVersions([]);
      return;
    }
    let active = true;
    assetVersions(selected)
      .then((v) => {
        if (active) setVersions(v);
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [selected, board]);
  async function act(operation: () => Promise<unknown>, message = "") {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await operation();
      await reload();
      onChange();
      if (result !== false && result !== null) setNotice(message);
    } catch (e) {
      setError(errorMessage(e));
      await reload().catch(() => {});
    } finally {
      setBusy(false);
    }
  }
  function navigate(id: string | null) {
    setFolderId(id);
    setSelected(null);
    setQuery("");
    setNotice("");
  }
  async function importFiles() {
    await act(async () => {
      const result = await importAssets(project.id, folderId);
      if (result?.failed.length) throw Error(result.failed.join("\n"));
      return result;
    }, "Files imported.");
  }
  async function chooseLivery() {
    setBusy(true);
    setError("");
    try {
      const d = await prepareLivery(project.id);
      if (d) {
        setDraft(d);
        setLiveryName(folder?.kind === "livery" ? folder.name : "");
        setVehicleId(folder?.kind === "livery" ? folder.vehicleId || "" : "");
        setSlots(d.files.map((f) => f.suggestedSlot));
        setCover(null);
        setVehicleQuery("");
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const issues = slots.flatMap((slot, i) =>
    slot &&
    ((vehicle && !vehicle.slots.some((s) => s.key === slot)) ||
      slots.indexOf(slot) !== i)
      ? [
          `${draft?.files[i].name}: ${slots.indexOf(slot) !== i ? "duplicate slot" : "slot unavailable for this vehicle"}`,
        ]
      : [],
  );
  async function finish() {
    if (!draft) return;
    await act(async () => {
      const id = await commitLivery(
        draft.token,
        folder?.kind === "livery" ? folder.parentId : folderId,
        folder?.kind === "livery" ? folder.id : null,
        liveryName,
        vehicleId,
        draft.files.map((f, i) => ({ index: f.index, slot: slots[i] })),
        cover,
      );
      setDraft(null);
      setFolderId(id);
      setSelected(null);
    }, "Livery imported.");
  }
  let cursor = folder;
  const crumbs = [];
  const seen = new Set<string>();
  while (cursor && !seen.has(cursor.id)) {
    seen.add(cursor.id);
    crumbs.unshift(cursor);
    cursor = board.folders.find((f) => f.id === cursor?.parentId);
  }
  const currentFiles = board.assets.filter(
    (a) =>
      (board.placements.find((p) => p.assetId === a.id)?.folderId ?? null) ===
        folderId && a.name.toLowerCase().includes(query.toLowerCase()),
  );
  const currentFolders = board.folders.filter(
    (f) =>
      f.parentId === folderId &&
      f.name.toLowerCase().includes(query.toLowerCase()),
  );
  const descendants = new Set<string>(folderId ? [folderId] : []);
  let changed = true;
  while (changed) {
    changed = false;
    for (const f of board.folders) {
      if (f.parentId && descendants.has(f.parentId) && !descendants.has(f.id)) {
        descendants.add(f.id);
        changed = true;
      }
    }
  }
  return (
    <section className="asset-browser" aria-label="Project assets">
      {error && (
        <p className="error pre-wrap" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="asset-path" aria-label="Asset folder path">
        <button
          className="text-button"
          disabled={busy || !!draft || !!folderForm}
          onClick={() => navigate(null)}
        >
          All assets
        </button>
        {crumbs.map((f) => (
          <span key={f.id}>
            <ChevronRight size={15} />
            <button
              className="text-button"
              disabled={busy || !!draft || !!folderForm}
              onClick={() => navigate(f.id)}
            >
              {f.name}
            </button>
          </span>
        ))}
      </div>
      {!draft && !folderForm && (
        <div className="asset-actions">
          <div className="button-row">
            <button
              className="primary"
              disabled={project.archived || busy}
              onClick={() => void importFiles()}
            >
              Import files
            </button>
            <button
              className="secondary"
              disabled={project.archived || busy}
              onClick={() => void chooseLivery()}
            >
              {folder?.kind === "livery" ? "Update livery" : "Import livery"}
            </button>
            {folder?.kind !== "livery" && (
              <button
                className="secondary"
                disabled={project.archived || busy}
                onClick={() => {
                  setFolderForm("new");
                  setFolderName("");
                  setParent(folderId);
                }}
              >
                New folder
              </button>
            )}
          </div>
          {folder && (
            <div className="button-row">
              <button
                className="text-button"
                disabled={project.archived || busy}
                onClick={() => {
                  setFolderForm("edit");
                  setFolderName(folder.name);
                  setParent(folder.parentId);
                }}
              >
                Rename / move folder
              </button>
              <button
                className="text-button"
                disabled={project.archived || busy}
                onClick={() => void act(() => setFolderCover(folder.id))}
              >
                Set cover
              </button>
            </div>
          )}
        </div>
      )}
      {folder?.kind === "livery" && !draft && (
        <p className="field-hint">
          {catalogue?.vehicles.find((v) => v.id === folder.vehicleId)?.name ??
            "Stored vehicle"}{" "}
          · {catalogue?.vehicles.find((v) => v.id === folder.vehicleId)?.team}
        </p>
      )}
      {folderForm && (
        <form
          className="folder-form"
          onSubmit={(e) => {
            e.preventDefault();
            void act(async () => {
              const f = await saveFolder(
                project.id,
                folderForm === "edit" ? folderId : null,
                parent,
                folderName,
              );
              setFolderForm(null);
              if (folderForm === "new") navigate(f.id);
            });
          }}
        >
          <h3>{folderForm === "new" ? "New folder" : "Edit folder"}</h3>
          <label>
            Folder name
            <input
              aria-label="Folder name"
              required
              maxLength={120}
              value={folderName}
              onChange={(e) => setFolderName(e.target.value)}
              disabled={busy}
            />
          </label>
          <label>
            Parent folder
            <select
              value={parent ?? ""}
              onChange={(e) => setParent(e.target.value || null)}
              disabled={busy}
            >
              <option value="">All assets</option>
              {board.folders
                .filter(
                  (f) =>
                    f.kind === "folder" &&
                    (folderForm === "new" || !descendants.has(f.id)),
                )
                .map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.name}
                  </option>
                ))}
            </select>
          </label>
          <div className="button-row">
            <button className="primary" disabled={busy || !folderName.trim()}>
              Save folder
            </button>
            <button
              className="text-button"
              type="button"
              disabled={busy}
              onClick={() => setFolderForm(null)}
            >
              Cancel
            </button>
          </div>
        </form>
      )}
      {draft ? (
        <section className="livery-import">
          <h2>
            {folder?.kind === "livery" ? "Update livery" : "Import livery"}
          </h2>
          <div className="form-columns">
            <label>
              Livery name
              <input
                aria-label="Livery name"
                value={liveryName}
                maxLength={120}
                onChange={(e) => setLiveryName(e.target.value)}
                disabled={busy || folder?.kind === "livery"}
              />
            </label>
            <label>
              Find vehicle
              <input
                aria-label="Find vehicle"
                placeholder="Vehicle name or team"
                value={vehicleQuery}
                onChange={(e) => setVehicleQuery(e.target.value)}
                disabled={busy || folder?.kind === "livery"}
              />
            </label>
          </div>
          <label>
            Vehicle
            <select
              aria-label="Livery vehicle"
              value={vehicleId}
              onChange={(e) => setVehicleId(e.target.value)}
              disabled={busy || folder?.kind === "livery"}
            >
              <option value="">Choose vehicle</option>
              {catalogue?.vehicles
                .filter(
                  (v) =>
                    v.id === vehicleId ||
                    `${v.name} ${v.team}`
                      .toLowerCase()
                      .includes(vehicleQuery.toLowerCase()),
                )
                .map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.name} · {v.team}
                  </option>
                ))}
            </select>
          </label>
          {vehicle && (
            <div className="catalogue-status">
              <p>
                Body: {vehicle.bodyStatus ?? "Unspecified"} · Windows:{" "}
                {vehicle.windowStatus ?? "Unspecified"}
              </p>
              {vehicle.notes.map((n, i) => (
                <p key={i} className="field-hint">
                  {n}
                </p>
              ))}
            </div>
          )}
          <div className="assignment-list">
            {draft.files.map((f, i) => (
              <div className="assignment-row" key={f.index}>
                <span>{f.name}</span>
                <select
                  aria-label={`Slot for ${f.name}`}
                  disabled={busy}
                  value={slots[i] ?? ""}
                  onChange={(e) =>
                    setSlots((values) =>
                      values.map((s, j) =>
                        j === i ? e.target.value || null : s,
                      ),
                    )
                  }
                >
                  <option value="">Unassigned</option>
                  {slots[i] &&
                    !vehicle?.slots.some((s) => s.key === slots[i]) && (
                      <option value={slots[i]!}>
                        {slots[i]} — {vehicle ? "unavailable" : "suggested"}
                      </option>
                    )}
                  {vehicle?.slots.map((s) => (
                    <option key={s.key} value={s.key}>
                      {s.key} · {s.status}
                    </option>
                  ))}
                </select>
              </div>
            ))}
          </div>
          <label>
            Cover image
            <select
              aria-label="Livery cover"
              disabled={busy}
              value={cover ?? ""}
              onChange={(e) =>
                setCover(e.target.value === "" ? null : Number(e.target.value))
              }
            >
              <option value="">None</option>
              {draft.files.map((f) => (
                <option key={f.index} value={f.index}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
          {issues.length > 0 && (
            <p className="error pre-wrap" role="alert">
              {issues.join("\n")}
            </p>
          )}
          <p className="field-hint">
            Review the suggested slots. Unassigned images are kept as files.
            {folder?.kind === "livery"
              ? " Matching slots create recoverable versions."
              : ""}
          </p>
          <div className="button-row">
            <button
              className="primary"
              disabled={
                busy || !vehicleId || !liveryName.trim() || issues.length > 0
              }
              onClick={() => void finish()}
            >
              {busy ? "Importing…" : "Confirm import"}
            </button>
            <button
              className="text-button"
              disabled={busy}
              onClick={() => {
                void cancelLivery(draft.token).catch(() => {});
                setDraft(null);
              }}
            >
              Cancel
            </button>
          </div>
        </section>
      ) : (
        !folderForm && (
          <>
            <div className="file-toolbar">
              <input
                aria-label="Search folder"
                placeholder="Search this folder"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <span>
                {currentFolders.length} folders · {currentFiles.length} files
              </span>
            </div>
            {loading ? (
              <p role="status">Loading files…</p>
            ) : (
              <div className="asset-grid">
                {currentFolders.map((f) => (
                  <button
                    key={f.id}
                    className="asset-tile folder-tile"
                    onClick={() => navigate(f.id)}
                  >
                    <div className="asset-thumbnail">
                      {f.preview ? (
                        <img src={f.preview} alt="" />
                      ) : f.kind === "livery" ? (
                        <Layers size={40} />
                      ) : (
                        <FolderOpen size={40} />
                      )}
                    </div>
                    <strong>{f.name}</strong>
                    <small>{f.kind === "livery" ? "Livery" : "Folder"}</small>
                  </button>
                ))}
                {currentFiles.map((a) => (
                  <button
                    key={a.id}
                    className="asset-tile"
                    onClick={() => {
                      setSelected(a.id);
                      setMoveTo(
                        board.placements.find((p) => p.assetId === a.id)
                          ?.folderId ?? null,
                      );
                    }}
                  >
                    <div className="asset-thumbnail">
                      {a.preview ? (
                        <img src={a.preview} alt="" />
                      ) : (
                        <File size={32} />
                      )}
                    </div>
                    <strong>{a.name}</strong>
                    <small>
                      {board.placements.find((p) => p.assetId === a.id)?.slot ??
                        "File"}{" "}
                      · {(a.size / 1024).toFixed(1)} KB
                    </small>
                  </button>
                ))}
              </div>
            )}
            {!loading && !currentFolders.length && !currentFiles.length && (
              <div className="empty-state">
                <h2>
                  {query
                    ? "No matching files or folders"
                    : "Add your first files"}
                </h2>
              </div>
            )}
          </>
        )
      )}
      {asset && !draft && (
        <aside className="asset-inspector" aria-label="File details">
          <button
            className="icon-button inspector-close"
            aria-label="Close file details"
            onClick={() => setSelected(null)}
          >
            <X size={20} />
          </button>
          <h2>{asset.name}</h2>
          {asset.preview && (
            <img className="inspector-preview" src={asset.preview} alt="" />
          )}
          <p>{placement?.slot ?? "Unassigned file"}</p>
          <button
            className="secondary"
            disabled={busy}
            onClick={() =>
              void act(() => saveAssetCopy(asset.id), "Copy saved.")
            }
          >
            Save a copy
          </button>
          <label>
            Move to folder
            <select
              aria-label="Move file to"
              disabled={busy || project.archived}
              value={moveTo ?? ""}
              onChange={(e) => setMoveTo(e.target.value || null)}
            >
              <option value="">All assets</option>
              {board.folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
          <button
            className="secondary"
            disabled={
              busy ||
              project.archived ||
              (moveTo ?? null) === (placement?.folderId ?? null)
            }
            onClick={() =>
              void act(
                () => moveAsset(asset.id, moveTo),
                "File moved; its slot assignment was cleared.",
              )
            }
          >
            Move file
          </button>
          {versions.length > 0 && (
            <section>
              <h3>Versions</h3>
              {versions.map((v) => (
                <div className="version-row" key={v.id}>
                  <span>
                    {new Date(v.createdAt).toLocaleString("en-US")}
                    <small>{v.name}</small>
                  </span>
                  <button
                    className="text-button"
                    disabled={busy || project.archived || v.hash === asset.hash}
                    onClick={() =>
                      void act(
                        () => restoreAssetVersion(asset.id, v.id),
                        "Version restored.",
                      )
                    }
                  >
                    {v.hash === asset.hash ? "Current" : "Restore"}
                  </button>
                </div>
              ))}
            </section>
          )}
        </aside>
      )}
    </section>
  );
}
