import { useEffect, useMemo, useState } from "react";
import { ExternalLink, FileImage, Link2, PencilLine, Plus, Trash2, Upload, X } from "lucide-react";
import {
  addMediaLibraryAsset,
  addMediaLibraryUrl,
  deleteMediaLibraryItem,
  errorMessage,
  listAssets,
  mediaLibrary,
  pickMediaLibraryImage,
  updateMediaLibraryItem,
  syncImageLibrary,
  syncUploadLibraryImages,
  syncSetLibraryImageScope,
  type Asset,
  type MediaLibraryItem,
} from "./bridge";

type FormValues = {
  name: string;
  altText: string;
  imageUrl: string;
  defaultLinkUrl: string;
  folder: string;
  tags: string;
};
const blank: FormValues = { name: "", altText: "", imageUrl: "", defaultLinkUrl: "", folder: "", tags: "" };

export default function MediaLibrary({ onBusy }: { onBusy: (busy: boolean) => void }) {
  const [items, setItems] = useState<MediaLibraryItem[]>([]);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [query, setQuery] = useState("");
  const [folderFilter, setFolderFilter] = useState("");
  const [tagFilter, setTagFilter] = useState("");
  const [form, setForm] = useState<FormValues | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [assetId, setAssetId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [serverUrl, setServerUrl] = useState("");
  const [teamId, setTeamId] = useState("");
  const [uploadScope, setUploadScope] = useState<"personal" | "team">("personal");
  const [selectedForUpload, setSelectedForUpload] = useState<string[]>([]);

  async function refresh() {
    const savedUrl = window.localStorage.getItem("apex-studio.team-sync.server-url") ?? "";
    const savedTeam = window.localStorage.getItem("apex-studio.team-sync.active-team") ?? "";
    setServerUrl(savedUrl);
    setTeamId(savedTeam);
    const [library, projectAssets] = await Promise.all([mediaLibrary(), listAssets(null)]);
    let combined = library;
    if (savedUrl && savedTeam) {
      try {
        const shared = await syncImageLibrary(savedUrl, savedTeam);
        const localRemoteIds = new Set(library.map((item) => item.providerImageId).filter(Boolean));
        const remoteItems: MediaLibraryItem[] = shared.images
          .filter((image) => !localRemoteIds.has(image.id))
          .map((image) => ({ id: `remote-${image.id}`, hash: null, name: image.name, altText: image.altText, imageUrl: image.imageUrl, defaultLinkUrl: image.defaultLinkUrl, folder: "", tags: [], overlayVersionGroup: null, overlayVersion: null, overlayTarget: null, ownerScope: image.ownerScope, teamId: image.ownerScope === "team" ? savedTeam : null, providerImageId: image.id, createdAt: image.createdAt, updatedAt: image.createdAt, preview: null, isRemote: true }));
        combined = [...remoteItems, ...library];
      } catch {
        setNotice("Team Image Hosting could not be reached. Local images and manual URLs are still available.");
      }
    }
    setItems(combined);
    const availableIds = new Set(combined.filter((item) => item.hash && !item.providerImageId && !item.isRemote).map((item) => item.id));
    setSelectedForUpload((current) => current.filter((id) => availableIds.has(id)));
    setAssets(projectAssets.filter((asset) => asset.name.toLowerCase().endsWith(".png")));
  }
  useEffect(() => {
    let active = true;
    void refresh().then(() => { if (active) setError(""); })
      .catch((reason: unknown) => { if (active) setError(errorMessage(reason)); });
    return () => { active = false; };
  }, []);
  useEffect(() => { onBusy(busy); return () => onBusy(false); }, [busy, onBusy]);

  const folders = useMemo(() => [...new Set(items.map((item) => item.folder.trim()).filter(Boolean))].sort((a, b) => a.localeCompare(b)), [items]);
  const tags = useMemo(() => [...new Set(items.flatMap((item) => item.tags))].sort((a, b) => a.localeCompare(b)), [items]);
  const filtered = useMemo(() => {
    const term = query.trim().toLowerCase();
    return items.filter((item) =>
      (!term || `${item.name} ${item.altText} ${item.imageUrl} ${item.folder} ${item.tags.join(" ")}`.toLowerCase().includes(term)) &&
      (!folderFilter || (folderFilter === "__unfiled" ? !item.folder : item.folder === folderFilter)) &&
      (!tagFilter || item.tags.some((tag) => tag.toLowerCase() === tagFilter.toLowerCase()))
    );
  }, [items, query, folderFilter, tagFilter]);

  async function run(action: () => Promise<void>, message = "") {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
      if (message) setNotice(message);
      await refresh();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function importPng() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const item = await pickMediaLibraryImage();
      if (!item) return;
      setItems((current) => [item, ...current.filter((row) => row.id !== item.id)]);
      setNotice(item.imageUrl ? "Image added to the library." : "Image added. Add its public URL after uploading it to an image host.");
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function uploadSelected() {
    if (!serverUrl || !teamId) { setError("Connect to a team server and select a team before using Image Hosting."); return; }
    const selected = items.filter((item) => selectedForUpload.includes(item.id) && item.hash && !item.providerImageId && !item.isRemote).slice(0, 10);
    if (!selected.length) return;
    const scopeText = uploadScope === "team" ? "upload these images as public images shared with the selected team" : "upload these images as public images owned by your account";
    if (!window.confirm(`This sends ${selected.length} image${selected.length === 1 ? "" : "s"} to your configured Image Provider. Their URLs will be public. Continue and ${scopeText}?`)) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await syncUploadLibraryImages(serverUrl, teamId, selected.map((item) => item.id), uploadScope);
      await refresh();
      const successfulIds = new Set(result.images.map((image) => image.clientId).filter((id): id is string => Boolean(id)));
      const retryIds = selected.filter((item) => !successfulIds.has(item.id)).map((item) => item.id);
      setSelectedForUpload(retryIds);
      const reused = result.images.filter((image) => image.reused).length;
      const failed = result.errors.length;
      setNotice(`${result.images.length} image${result.images.length === 1 ? "" : "s"} available by public URL${reused ? ` · ${reused} reused` : ""}${failed ? ` · ${failed} failed` : ""}.`);
      if (failed) setError(result.errors.map((entry) => `${entry.name || "Image"}: ${entry.error || "Upload failed"}`).join("\n"));
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  function toggleUploadSelection(itemId: string) {
    setSelectedForUpload((current) => current.includes(itemId)
      ? current.filter((id) => id !== itemId)
      : current.length >= 10 ? current : [...current, itemId]);
  }

  async function changeSharing(item: MediaLibraryItem) {
    if (!serverUrl || !teamId || !item.providerImageId || item.isRemote) return;
    const next = item.ownerScope === "team" ? "personal" : "team";
    if (!window.confirm(next === "team" ? "Share this image and its public URL with your selected team?" : "Stop sharing this image with your selected team?")) return;
    setBusy(true);
    setError("");
    try {
      await syncSetLibraryImageScope(serverUrl, teamId, item.id, next);
      await refresh();
      setNotice(next === "team" ? "Image shared with the selected team." : "Image is personal again.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function addProjectAsset() {
    const asset = assets.find((candidate) => candidate.id === assetId);
    if (!asset) return;
    await run(async () => {
      const item = await addMediaLibraryAsset(asset.projectId, asset.id);
      setItems((current) => [item, ...current.filter((row) => row.id !== item.id)]);
      setAssetId("");
    }, "Project image added to the library.");
  }

  async function saveForm(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!form) return;
    await run(async () => {
      if (editing) {
        const current = items.find((item) => item.id === editing);
        if (!current) throw Error("This library item is no longer available.");
        const normalizedTags = form.tags.split(",").map((tag) => tag.trim()).filter(Boolean);
        await updateMediaLibraryItem({ ...current, ...form, folder: form.folder.trim(), tags: normalizedTags, defaultLinkUrl: form.defaultLinkUrl || null });
      } else {
        await addMediaLibraryUrl(form.name, form.altText, form.imageUrl, form.defaultLinkUrl, form.folder, form.tags.split(",").map((tag) => tag.trim()).filter(Boolean));
      }
      setForm(null);
      setEditing(null);
    }, editing ? "Library item saved." : "Image URL added to the library.");
  }

  return (
    <section className="media-library" aria-label="Image Library">
      <header className="media-library-heading">
        <div>
          <h2>Image Library</h2>
          <p>Personal library · Images stay in this workspace until you choose to upload them.</p>
        </div>
        <div className="media-library-actions">
          <button className="secondary" type="button" disabled={busy} onClick={() => void importPng()}>
            <Upload size={16} aria-hidden="true" /> Add PNG
          </button>
          <button className="primary" type="button" disabled={busy} onClick={() => { setEditing(null); setForm(blank); }}>
            <Plus size={16} aria-hidden="true" /> Add image URL
          </button>
          <button className="secondary" type="button" disabled={busy} onClick={() => void refresh()}>Refresh library</button>
        </div>
      </header>
      <p className="field-hint">Adding an image here does not upload it to a server. Add the public URL manually after uploading it wherever you choose.</p>
      {serverUrl && teamId && <div className="media-library-project-add"><label htmlFor="image-upload-scope">For uploads</label><select id="image-upload-scope" value={uploadScope} disabled={busy} onChange={(event) => setUploadScope(event.target.value as "personal" | "team")}><option value="personal">Personal</option><option value="team">Selected team</option></select><span className="field-hint">Uploads are public and require confirmation each time.</span></div>}
      {serverUrl && teamId && <div className="media-library-batch-toolbar">
        <span>{selectedForUpload.length} of 10 selected</span>
        <button className="text-button" type="button" disabled={busy || !filtered.some((item) => item.hash && !item.providerImageId && !item.isRemote)} onClick={() => setSelectedForUpload(filtered.filter((item) => item.hash && !item.providerImageId && !item.isRemote).slice(0, 10).map((item) => item.id))}>Select first 10 available</button>
        <button className="text-button" type="button" disabled={busy || !selectedForUpload.length} onClick={() => setSelectedForUpload([])}>Clear selection</button>
        <button className="primary" type="button" disabled={busy || !selectedForUpload.length} onClick={() => void uploadSelected()}><Upload size={15} /> {busy ? "Uploading…" : `Upload ${selectedForUpload.length || "selected"}`}</button>
      </div>}
      {error && <p role="alert" className="error">{error}</p>}
      {notice && <p role="status" className="field-hint">{notice}</p>}
      {assets.length > 0 && (
        <div className="media-library-project-add">
          <label htmlFor="library-project-asset">Add a PNG from project assets</label>
          <select id="library-project-asset" value={assetId} disabled={busy} onChange={(event) => setAssetId(event.target.value)}>
            <option value="">Choose project image</option>
            {assets.map((asset) => <option key={asset.id} value={asset.id}>{asset.name}</option>)}
          </select>
          <button className="secondary" type="button" disabled={busy || !assetId} onClick={() => void addProjectAsset()}>Add selected</button>
        </div>
      )}
      <label className="search-field media-library-search">
        <FileImage size={17} aria-hidden="true" />
        <input aria-label="Search image library" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search images" />
      </label>
      <div className="media-library-filters" aria-label="Filter library images">
        <label>Folder<select value={folderFilter} disabled={busy} onChange={(event) => setFolderFilter(event.target.value)}><option value="">All folders</option><option value="__unfiled">Unfiled</option>{folders.map((folder) => <option key={folder} value={folder}>{folder}</option>)}</select></label>
        <label>Tag<select value={tagFilter} disabled={busy} onChange={(event) => setTagFilter(event.target.value)}><option value="">All tags</option>{tags.map((tag) => <option key={tag} value={tag}>{tag}</option>)}</select></label>
        {(folderFilter || tagFilter) && <button className="text-button" type="button" disabled={busy} onClick={() => { setFolderFilter(""); setTagFilter(""); }}>Clear filters</button>}
      </div>
      {form && (
        <form className="media-library-form" onSubmit={(event) => void saveForm(event)}>
          <div className="media-library-form-heading">
            <h3>{editing ? "Edit image" : "Add image URL"}</h3>
            <button className="icon-button" type="button" aria-label="Close image form" disabled={busy} onClick={() => { setForm(null); setEditing(null); }}><X size={18} /></button>
          </div>
          <label>Name<input required maxLength={160} value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} /></label>
          <label>Alt text<input maxLength={500} value={form.altText} onChange={(event) => setForm({ ...form, altText: event.target.value })} /></label>
          <label>Public image URL<input type="url" required={items.find((item) => item.id === editing)?.hash == null} value={form.imageUrl} onChange={(event) => setForm({ ...form, imageUrl: event.target.value })} placeholder="https://…" /></label>
          <label>Default link <span className="optional">(optional)</span><input type="url" value={form.defaultLinkUrl} onChange={(event) => setForm({ ...form, defaultLinkUrl: event.target.value })} placeholder="https://…" /></label>
          <label>Folder <span className="optional">(optional)</span><input maxLength={80} value={form.folder} onChange={(event) => setForm({ ...form, folder: event.target.value })} placeholder="Banners" /></label>
          <label>Tags <span className="optional">(comma separated)</span><input maxLength={800} value={form.tags} onChange={(event) => setForm({ ...form, tags: event.target.value })} placeholder="studio, support, reusable" /></label>
          <div className="media-library-form-actions">
            <button className="primary" disabled={busy}>{busy ? "Saving…" : "Save image"}</button>
            <button className="secondary" type="button" disabled={busy} onClick={() => { setForm(null); setEditing(null); }}>Cancel</button>
          </div>
        </form>
      )}
      {filtered.length ? (
        <div className="media-library-grid">
          {filtered.map((item) => (
            <article className="media-library-card" key={item.id}>
              {serverUrl && teamId && item.hash && !item.providerImageId && !item.isRemote && <label className="media-library-select"><input type="checkbox" checked={selectedForUpload.includes(item.id)} disabled={busy || (!selectedForUpload.includes(item.id) && selectedForUpload.length >= 10)} onChange={() => toggleUploadSelection(item.id)} /><span>Select for upload</span></label>}
              <div className="media-library-preview">
                {item.preview ? <img src={item.preview} alt="" /> : item.imageUrl ? <img src={item.imageUrl} alt="" /> : <FileImage size={30} aria-hidden="true" />}
              </div>
              <div className="media-library-card-body">
                <div className="media-library-card-title"><strong>{item.name}</strong><span>{item.isRemote ? "Shared with team" : item.ownerScope === "personal" ? "Personal" : "Team"}</span></div>
                <small>{item.altText || "No alt text"}</small>
                <div className="media-library-card-meta"><span>{item.folder || "Unfiled"}</span>{item.tags.map((tag) => <span key={tag}>#{tag}</span>)}</div>
                {item.overlayVersion !== null && item.overlayVersion !== undefined && <small>Overlay v{item.overlayVersion} · {item.overlayTarget === "erlcx" ? "ERLCX 16:9" : "ExecLab 16:10"}</small>}
                {item.imageUrl ? <a href={item.imageUrl} target="_blank" rel="noreferrer"><Link2 size={14} aria-hidden="true" /> Open image <ExternalLink size={13} aria-hidden="true" /></a> : <small>Local only · public URL not set</small>}
                <div className="media-library-card-actions">
                  {item.hash && !item.providerImageId && <small>Select this image above to include it in a batch upload.</small>}
                  {item.providerImageId && !item.isRemote && <button className="text-button" type="button" disabled={busy} onClick={() => void changeSharing(item)}>{item.ownerScope === "team" ? "Make personal" : "Share with team"}</button>}
                  <button className="text-button" type="button" disabled={busy || item.isRemote} onClick={() => { setEditing(item.id); setForm({ name: item.name, altText: item.altText, imageUrl: item.imageUrl, defaultLinkUrl: item.defaultLinkUrl ?? "", folder: item.folder, tags: item.tags.join(", ") }); }}><PencilLine size={15} aria-hidden="true" /> Edit</button>
                  <button className="text-button" type="button" onClick={() => {
                    if (!window.confirm(`Remove “${item.name}” from the Image Library?`)) return;
                    void run(async () => deleteMediaLibraryItem(item.id), "Image removed from the library.");
                  }} disabled={busy || item.isRemote}><Trash2 size={15} aria-hidden="true" /> Remove</button>
                </div>
              </div>
            </article>
          ))}
        </div>
      ) : (
        <div className="empty-state"><h3>{items.length ? "No images match this search" : "No library images yet"}</h3><p>Add a PNG from your workspace or save a public image URL to reuse it in post drafts.</p></div>
      )}
    </section>
  );
}
