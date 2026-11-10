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
  type Asset,
  type MediaLibraryItem,
} from "./bridge";

type FormValues = {
  name: string;
  altText: string;
  imageUrl: string;
  defaultLinkUrl: string;
};
const blank: FormValues = { name: "", altText: "", imageUrl: "", defaultLinkUrl: "" };

export default function MediaLibrary({ onBusy }: { onBusy: (busy: boolean) => void }) {
  const [items, setItems] = useState<MediaLibraryItem[]>([]);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [query, setQuery] = useState("");
  const [form, setForm] = useState<FormValues | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [assetId, setAssetId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  async function refresh() {
    const [library, projectAssets] = await Promise.all([mediaLibrary(), listAssets(null)]);
    setItems(library);
    setAssets(projectAssets.filter((asset) => asset.name.toLowerCase().endsWith(".png")));
  }
  useEffect(() => {
    let active = true;
    void Promise.all([mediaLibrary(), listAssets(null)])
      .then(([library, projectAssets]) => {
        if (!active) return;
        setItems(library);
        setAssets(projectAssets.filter((asset) => asset.name.toLowerCase().endsWith(".png")));
      })
      .catch((reason: unknown) => { if (active) setError(errorMessage(reason)); });
    return () => { active = false; };
  }, []);
  useEffect(() => { onBusy(busy); return () => onBusy(false); }, [busy, onBusy]);

  const filtered = useMemo(() => {
    const term = query.trim().toLowerCase();
    return items.filter((item) => !term || `${item.name} ${item.altText} ${item.imageUrl}`.toLowerCase().includes(term));
  }, [items, query]);

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
        await updateMediaLibraryItem({ ...current, ...form, defaultLinkUrl: form.defaultLinkUrl || null });
      } else {
        await addMediaLibraryUrl(form.name, form.altText, form.imageUrl, form.defaultLinkUrl);
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
        </div>
      </header>
      <p className="field-hint">Adding an image here does not upload it to a server. Add the public URL manually after uploading it wherever you choose.</p>
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
              <div className="media-library-preview">
                {item.preview ? <img src={item.preview} alt="" /> : item.imageUrl ? <img src={item.imageUrl} alt="" /> : <FileImage size={30} aria-hidden="true" />}
              </div>
              <div className="media-library-card-body">
                <div className="media-library-card-title"><strong>{item.name}</strong><span>{item.ownerScope === "personal" ? "Personal" : "Team"}</span></div>
                <small>{item.altText || "No alt text"}</small>
                {item.imageUrl ? <a href={item.imageUrl} target="_blank" rel="noreferrer"><Link2 size={14} aria-hidden="true" /> Open image <ExternalLink size={13} aria-hidden="true" /></a> : <small>Local only · public URL not set</small>}
                <div className="media-library-card-actions">
                  <button className="text-button" type="button" disabled={busy} onClick={() => { setEditing(item.id); setForm({ name: item.name, altText: item.altText, imageUrl: item.imageUrl, defaultLinkUrl: item.defaultLinkUrl ?? "" }); }}><PencilLine size={15} aria-hidden="true" /> Edit</button>
                  <button className="text-button" type="button" disabled={busy} onClick={() => {
                    if (!window.confirm(`Remove “${item.name}” from the Image Library?`)) return;
                    void run(async () => deleteMediaLibraryItem(item.id), "Image removed from the library.");
                  }}><Trash2 size={15} aria-hidden="true" /> Remove</button>
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
