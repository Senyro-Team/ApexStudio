import RobloxUpload from "./RobloxUpload";
import useAssetDrag from "./useAssetDrag";
import { useEffect, useState, useRef, type DragEvent } from "react";
import {
  FolderOpen,
  File,
  ChevronRight,
  X,
  Layers,
  MoreHorizontal,
} from "lucide-react";
import {
  assetBoard,
  assignAssetSlot,
  copyAssetItem,
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
import SlotEditor from "./SlotEditor";
type Item = { id: string; folder: boolean };
type Clipboard = Item & { cut: boolean };
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
  const [uploadFolder, setUploadFolder] = useState<string | null>(null);
  const browserRef = useRef<HTMLElement>(null);
  const dragged = useRef<Item | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null | undefined>(
    undefined,
  );
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
  const [explorerSelection, setExplorerSelection] = useState<Item | null>(null);
  const [clipboard, setClipboard] = useState<Clipboard | null>(null);
  const [vehicleOpen, setVehicleOpen] = useState(false);
  const [vehicleIndex, setVehicleIndex] = useState(0);
  const matches =
    catalogue?.vehicles
      .filter((v) =>
        `${v.name} ${v.team}`
          .toLowerCase()
          .includes(vehicleQuery.trim().toLowerCase()),
      )
      .slice(0, 8) ?? [];
  function canDrop(item: Item, target: string | null) {
    if (
      item.folder &&
      board.folders.find((f) => f.id === target)?.kind === "livery"
    )
      return false;
    let ancestor = target;
    const visited = new Set<string>();
    while (ancestor) {
      if (item.folder && (ancestor === item.id || visited.has(ancestor)))
        return false;
      visited.add(ancestor);
      ancestor = board.folders.find((f) => f.id === ancestor)?.parentId ?? null;
    }
    return true;
  }
  const pointerDrag = useAssetDrag(
    busy || project.archived || !!draft || !!folderForm,
    canDrop,
    (item, target, copy) => {
      void transfer({ ...item, cut: !copy }, target);
    },
  );
  const locked = busy || project.archived || !!draft || !!folderForm;
  const folder = board.folders.find((f) => f.id === folderId),
    asset = board.assets.find((a) => a.id === selected),
    placement = board.placements.find((p) => p.assetId === selected),
    vehicle = catalogue?.vehicles.find((v) => v.id === vehicleId);
  useEffect(() => {
    browserRef.current?.focus({ preventScroll: true });
  }, [folderId]);
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
    setExplorerSelection(null);
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
        setVehicleQuery(
          folder?.kind === "livery"
            ? (catalogue?.vehicles.find((v) => v.id === folder.vehicleId)
                ?.name ?? "")
            : "",
        );
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const issues = slots.flatMap((slot, i) =>
    slot &&
    (!/^(Window )?(Left|Right|Front|Back|Top) ([1-9][0-9]?|100)$/.test(slot) ||
      slots.indexOf(slot) !== i)
      ? [
          `${draft?.files[i].name}: ${slots.indexOf(slot) !== i ? "duplicate slot" : "enter a slot number from 1 to 100"}`,
        ]
      : [],
  );
  async function addImages() {
    if (!draft) return;
    setBusy(true);
    setError("");
    try {
      const next = await prepareLivery(project.id, draft.token);
      if (next) {
        setSlots((old) => [
          ...old,
          ...next.files.slice(draft.files.length).map((f) => f.suggestedSlot),
        ]);
        setDraft(next);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  function copySelection(cut: boolean) {
    if (!locked && explorerSelection) {
      setClipboard({ ...explorerSelection, cut });
      setNotice(cut ? "Ready to move." : "Ready to copy.");
    }
  }
  async function transfer(item: Clipboard, target: string | null) {
    if (locked) return;
    await act(
      async () => {
        if (item.cut) {
          if (item.folder) {
            const source = board.folders.find((f) => f.id === item.id);
            if (!source) throw Error("Folder no longer exists.");
            await saveFolder(project.id, source.id, target, source.name);
          } else await moveAsset(item.id, target);
          setClipboard(null);
          setExplorerSelection(null);
          setSelected(null);
        } else await copyAssetItem(project.id, item.id, item.folder, target);
      },
      item.cut ? "Moved." : "Copied.",
    );
  }
  function dragOver(e: DragEvent, target: string | null) {
    const item = dragged.current;
    if (locked || !item) return;
    let ancestor = target;
    const visited = new Set<string>();
    while (ancestor) {
      if (item.folder && (ancestor === item.id || visited.has(ancestor)))
        return;
      visited.add(ancestor);
      ancestor = board.folders.find((f) => f.id === ancestor)?.parentId ?? null;
    }
    if (
      item.folder &&
      board.folders.find((f) => f.id === target)?.kind === "livery"
    )
      return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = e.ctrlKey ? "copy" : "move";
    setDropTarget(target);
  }
  function drop(e: DragEvent, target: string | null) {
    e.preventDefault();
    e.stopPropagation();
    setDropTarget(undefined);
    dragged.current = null;
    if (locked) return;
    try {
      const item = JSON.parse(
        e.dataTransfer.getData("application/x-apex-item"),
      );
      if (
        item.project !== project.id ||
        typeof item.id !== "string" ||
        typeof item.folder !== "boolean"
      )
        return;
      void transfer(
        { id: item.id, folder: item.folder, cut: !e.ctrlKey },
        target,
      );
    } catch {
      /* Ignore drags from outside the asset browser. */
    }
  }
  function drag(e: DragEvent, item: Item) {
    if (locked) {
      e.preventDefault();
      return;
    }
    dragged.current = item;
    e.dataTransfer.effectAllowed = "copyMove";
    e.dataTransfer.setData(
      "application/x-apex-item",
      JSON.stringify({ ...item, project: project.id }),
    );
    setExplorerSelection(item);
  }
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
    <section
      className="asset-browser"
      ref={browserRef}
      tabIndex={-1}
      aria-label="Project assets"
      onPointerMove={pointerDrag.move}
      onPointerUp={pointerDrag.end}
      onPointerCancel={pointerDrag.cancel}
      onClickCapture={pointerDrag.click}
      onDragEnd={() => {
        dragged.current = null;
        setDropTarget(undefined);
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") pointerDrag.cancel();
        if (
          (e.target as HTMLElement).closest(
            "input,textarea,select,[contenteditable=true]",
          )
        )
          return;
        if (
          (e.ctrlKey || e.metaKey) &&
          ["c", "x", "v"].includes(e.key.toLowerCase())
        ) {
          e.preventDefault();
          if (e.key.toLowerCase() === "v") {
            if (clipboard) void transfer(clipboard, folderId);
          } else copySelection(e.key.toLowerCase() === "x");
        }
      }}
    >
      {uploadFolder && (
        <RobloxUpload
          folderId={uploadFolder}
          onBusy={setBusy}
          onClose={() => setUploadFolder(null)}
          onDone={() => setUploadFolder(null)}
        />
      )}
      {error && (
        <p className="error pre-wrap" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="asset-path" aria-label="Asset folder path">
        <button
          className="text-button"
          data-drop-folder=""
          onDragEnter={(e) => dragOver(e, null)}
          onDragOver={(e) => dragOver(e, null)}
          onDrop={(e) => drop(e, null)}
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
              data-drop-folder={f.id}
              onDragEnter={(e) => dragOver(e, f.id)}
              onDragOver={(e) => dragOver(e, f.id)}
              onDrop={(e) => drop(e, f.id)}
              onClick={() => navigate(f.id)}
            >
              {f.name}
            </button>
          </span>
        ))}
        {folder && !draft && !folderForm && (
          <details className="asset-folder-menu">
            <summary aria-label="Folder actions" title="Folder actions">
              <MoreHorizontal size={20} />
            </summary>
            <div className="asset-folder-menu-items">
              <button
                className="text-button"
                disabled={project.archived || busy}
                onClick={(e) => {
                  e.currentTarget.closest("details")?.removeAttribute("open");
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
                onClick={(e) => {
                  e.currentTarget.closest("details")?.removeAttribute("open");
                  void act(() => setFolderCover(folder.id));
                }}
              >
                Set cover
              </button>
            </div>
          </details>
        )}
      </div>
      {folder?.kind === "livery" && (
        <div className="livery-upload-actions">
          <button
            className="primary"
            disabled={locked}
            onClick={() => setUploadFolder(folder.id)}
          >
            Upload to Roblox
          </button>
          <span className="field-hint">
            Assigned textures → Roblox decal IDs
          </span>
        </div>
      )}
      {!draft && !folderForm && (
        <div className="asset-command-bar">
          <input
            aria-label="Search folder"
            placeholder="Search this folder"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <div className="asset-create-actions">
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
          </div>
          <div className="vehicle-picker">
            <label>
              Vehicle
              <input
                role="combobox"
                aria-label="Livery vehicle"
                aria-expanded={
                  vehicleOpen && !!vehicleQuery.trim() && !vehicleId
                }
                aria-controls="livery-vehicle-results"
                aria-autocomplete="list"
                aria-activedescendant={
                  vehicleOpen &&
                  vehicleQuery.trim() &&
                  !vehicleId &&
                  matches[vehicleIndex]
                    ? `vehicle-option-${matches[vehicleIndex].id}`
                    : undefined
                }
                autoComplete="off"
                placeholder="Type a vehicle name or team"
                value={vehicleQuery}
                disabled={busy || folder?.kind === "livery"}
                onFocus={() => setVehicleOpen(true)}
                onBlur={() => setVehicleOpen(false)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setVehicleOpen(false);
                  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                    e.preventDefault();
                    setVehicleOpen(true);
                    setVehicleIndex((i) =>
                      Math.max(
                        0,
                        Math.min(
                          matches.length - 1,
                          i + (e.key === "ArrowDown" ? 1 : -1),
                        ),
                      ),
                    );
                  }
                  if (
                    e.key === "Enter" &&
                    vehicleOpen &&
                    !vehicleId &&
                    vehicleQuery.trim() &&
                    matches[vehicleIndex]
                  ) {
                    e.preventDefault();
                    const v = matches[vehicleIndex];
                    setVehicleId(v.id);
                    setVehicleQuery(`${v.name} - ${v.team}`);
                    setVehicleOpen(false);
                  }
                }}
                onChange={(e) => {
                  setVehicleQuery(e.target.value);
                  setVehicleId("");
                  setVehicleIndex(0);
                  setVehicleOpen(true);
                }}
              />
            </label>
            {vehicleOpen && vehicleQuery.trim() && !vehicleId && (
              <div
                id="livery-vehicle-results"
                role="listbox"
                aria-label="Vehicle suggestions"
              >
                {catalogue?.vehicles
                  .filter((v) =>
                    `${v.name} ${v.team}`
                      .toLowerCase()
                      .includes(vehicleQuery.trim().toLowerCase()),
                  )
                  .slice(0, 8)
                  .map((v) => (
                    <button
                      type="button"
                      role="option"
                      aria-selected={matches[vehicleIndex]?.id === v.id}
                      id={`vehicle-option-${v.id}`}
                      key={v.id}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => {
                        setVehicleId(v.id);
                        setVehicleQuery(`${v.name} - ${v.team}`);
                        setVehicleOpen(false);
                      }}
                    >
                      {v.name}
                      <small>
                        {v.team} - {v.liverySupport}
                      </small>
                    </button>
                  ))}
                {!catalogue?.vehicles.some((v) =>
                  `${v.name} ${v.team}`
                    .toLowerCase()
                    .includes(vehicleQuery.trim().toLowerCase()),
                ) && <p>No matching vehicles</p>}
              </div>
            )}
          </div>
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
                <SlotEditor
                  label={f.name}
                  value={slots[i]}
                  disabled={busy}
                  onChange={(value) =>
                    setSlots((old) =>
                      old.map((slot, j) => (j === i ? value : slot)),
                    )
                  }
                />
              </div>
            ))}
          </div>
          <div className="button-row">
            <button
              className="secondary"
              disabled={busy || draft.files.length >= 100}
              onClick={() => void addImages()}
            >
              Add images
            </button>
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
            <div className="asset-selection-strip">
              <span className="asset-selection-name">
                {explorerSelection
                  ? (board.folders.find((f) => f.id === explorerSelection.id)
                      ?.name ??
                    board.assets.find((a) => a.id === explorerSelection.id)
                      ?.name)
                  : `${currentFolders.length} folders · ${currentFiles.length} files`}
              </span>
              <div className="explorer-toolbar button-row">
                {explorerSelection?.folder &&
                  board.folders.some(
                    (f) => f.id === explorerSelection.id && f.kind === "livery",
                  ) && (
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => setUploadFolder(explorerSelection.id)}
                    >
                      Upload to Roblox
                    </button>
                  )}
                {explorerSelection?.folder && (
                  <button
                    className="text-button"
                    disabled={locked}
                    onClick={() =>
                      explorerSelection && navigate(explorerSelection.id)
                    }
                  >
                    Open folder
                  </button>
                )}
                {explorerSelection && (
                  <button
                    className="text-button"
                    disabled={locked}
                    onClick={() => copySelection(false)}
                  >
                    Copy
                  </button>
                )}
                {explorerSelection && (
                  <button
                    className="text-button"
                    disabled={locked}
                    onClick={() => copySelection(true)}
                  >
                    Cut
                  </button>
                )}
                {clipboard && (
                  <button
                    className="secondary"
                    disabled={
                      locked || (clipboard.folder && folder?.kind === "livery")
                    }
                    onClick={() =>
                      clipboard && void transfer(clipboard, folderId)
                    }
                  >
                    Paste
                  </button>
                )}
                {clipboard && (
                  <span className="field-hint">
                    {clipboard.cut ? "Move" : "Copy"}:{" "}
                    {board.folders.find((f) => f.id === clipboard.id)?.name ??
                      board.assets.find((a) => a.id === clipboard.id)?.name}
                  </span>
                )}
              </div>
            </div>
            {loading ? (
              <p role="status">Loading files…</p>
            ) : (
              <div
                className="asset-grid"
                data-drop-folder={folderId ?? ""}
                onDragEnter={(e) => dragOver(e, folderId)}
                onDragOver={(e) => dragOver(e, folderId)}
                onDrop={(e) => drop(e, folderId)}
              >
                {currentFolders.map((f) => (
                  <button
                    key={f.id}
                    className={`asset-tile folder-tile ${explorerSelection?.id === f.id ? "selected" : ""} ${clipboard?.cut && clipboard.id === f.id ? "is-cut" : ""} ${dropTarget === f.id || pointerDrag.target === f.id ? "drop-target" : ""}`}
                    disabled={busy}
                    aria-pressed={explorerSelection?.id === f.id}
                    title="Double-click to open"
                    onFocus={() =>
                      setExplorerSelection({ id: f.id, folder: true })
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        e.stopPropagation();
                        navigate(f.id);
                      }
                    }}
                    data-drop-folder={f.id}
                    data-draggable={!locked}
                    onPointerDown={(e) =>
                      pointerDrag.start(e, {
                        id: f.id,
                        folder: true,
                        name: f.name,
                      })
                    }
                    draggable={false}
                    onDragStart={(e) => drag(e, { id: f.id, folder: true })}
                    onDragEnter={(e) => dragOver(e, f.id)}
                    onDragOver={(e) => dragOver(e, f.id)}
                    onDrop={(e) => drop(e, f.id)}
                    onClick={() => {
                      setExplorerSelection({ id: f.id, folder: true });
                      setSelected(null);
                    }}
                    onDoubleClick={() => navigate(f.id)}
                  >
                    <div className="asset-thumbnail">
                      {f.preview ? (
                        <img src={f.preview} alt="" draggable={false} />
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
                    className={`asset-tile ${explorerSelection?.id === a.id ? "selected" : ""} ${clipboard?.cut && clipboard.id === a.id ? "is-cut" : ""}`}
                    disabled={busy}
                    aria-pressed={explorerSelection?.id === a.id}
                    onFocus={() =>
                      setExplorerSelection({ id: a.id, folder: false })
                    }
                    data-draggable={!locked}
                    onPointerDown={(e) =>
                      pointerDrag.start(e, {
                        id: a.id,
                        folder: false,
                        name: a.name,
                      })
                    }
                    draggable={false}
                    onDragStart={(e) => drag(e, { id: a.id, folder: false })}
                    onClick={() => {
                      setExplorerSelection({ id: a.id, folder: false });
                      setSelected(a.id);
                      setMoveTo(
                        board.placements.find((p) => p.assetId === a.id)
                          ?.folderId ?? null,
                      );
                    }}
                  >
                    <div className="asset-thumbnail">
                      {a.preview ? (
                        <img src={a.preview} alt="" draggable={false} />
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
      {pointerDrag.preview && (
        <div
          className="asset-drag-preview"
          style={{ left: pointerDrag.preview.x, top: pointerDrag.preview.y }}
        >
          {pointerDrag.preview.copy ? "Copy" : "Move"}:{" "}
          {pointerDrag.preview.name}
        </div>
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
          {folder?.kind === "livery" ? (
            <label>
              Texture slot
              <SlotEditor
                label={asset.name}
                value={placement?.slot ?? null}
                disabled={busy || project.archived}
                onChange={(slot) =>
                  void act(
                    () => assignAssetSlot(asset.id, slot),
                    "Slot updated.",
                  )
                }
              />
            </label>
          ) : (
            <p>{placement?.slot ?? "Unassigned file"}</p>
          )}
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
