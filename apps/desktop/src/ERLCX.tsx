import { useEffect, useState, lazy, Suspense } from "react";
import { ArrowUpRight, FilePenLine, Trash2 } from "lucide-react";
import {
  listPosts,
  createPost,
  deletePost,
  listProjects,
  vehicleCatalogue,
  errorMessage,
  type PostSummary,
  type Project,
  type Catalogue,
  type RobloxFocus,
} from "./bridge";
const Publishing = lazy(() => import("./Publishing"));
export default function ERLCX({
  onBusy,
  onOpenRoblox,
}: {
  onBusy: (v: boolean) => void;
  onOpenRoblox: (focus: RobloxFocus) => void;
}) {
  const [rows, setRows] = useState<PostSummary[]>([]),
    [id, setId] = useState<string | null>(null),
    [projects, setProjects] = useState<Project[]>([]),
    [catalogue, setCatalogue] = useState<Catalogue | null>(null),
    [dirty, setDirty] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const load = () => listPosts().then(setRows);
  useEffect(() => {
    void Promise.all([
      load(),
      listProjects().then(setProjects),
      vehicleCatalogue().then(setCatalogue),
    ]).catch((e) => setError(errorMessage(e)));
  }, []);
  useEffect(() => {
    onBusy(dirty || busy);
    return () => onBusy(false);
  }, [dirty, busy, onBusy]);
  async function create() {
    setBusy(true);
    try {
      setId(await createPost());
      await load();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove(r: PostSummary) {
    const title = r.title || "Untitled post";
    if (!window.confirm(`Delete “${title}”? This removes the saved ERLCX draft.`)) return;
    setBusy(true);
    setError("");
    try {
      await deletePost(r.id, r.revision);
      await load();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <header className="page-heading">
        <h1>ERLCX</h1>
        {id ? (
          <button
            className="secondary"
            disabled={dirty || busy}
            onClick={() => {
              setId(null);
              void load();
            }}
          >
            All drafts
          </button>
        ) : (
          <button
            className="primary"
            disabled={busy}
            onClick={() => void create()}
          >
            New draft
          </button>
        )}
      </header>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {id ? (
        <Suspense fallback={<p role="status">Opening draft…</p>}>
          <Publishing
            key={id}
            draftId={id}
            projects={projects}
            catalogue={catalogue}
            onEditingChange={setDirty}
            onOpenRoblox={(folderId) => onOpenRoblox({ folderIds: [folderId] })}
          />
        </Suspense>
      ) : (
        <ul className="post-draft-list">
          {rows.map((r) => (
            <li key={r.id}>
              <div className="post-draft-row">
                <button
                  className="post-draft-open-row"
                  onClick={() => setId(r.id)}
                  disabled={busy}
                  aria-label={`Open draft: ${r.title || "Untitled post"}`}
                >
                  <span className="post-draft-icon" aria-hidden="true"><FilePenLine size={22} /></span>
                  <span className="post-draft-copy">
                    <strong>{r.title || "Untitled post"}</strong>
                    <small>
                      <span>{r.liveries} {r.liveries === 1 ? "livery" : "liveries"}</span>
                      <span aria-hidden="true">·</span>
                      <time dateTime={r.updatedAt}>{new Date(r.updatedAt).toLocaleString()}</time>
                    </small>
                  </span>
                  <span className="post-draft-open">Open <ArrowUpRight size={17} aria-hidden="true" /></span>
                </button>
                <button
                  className="post-draft-delete secondary"
                  type="button"
                  disabled={busy}
                  aria-label={`Delete draft: ${r.title || "Untitled post"}`}
                  onClick={() => void remove(r)}
                >
                  <Trash2 size={17} aria-hidden="true" />
                </button>
              </div>
            </li>
          ))}
          {!rows.length && <li>No saved drafts.</li>}
        </ul>
      )}
    </>
  );
}
