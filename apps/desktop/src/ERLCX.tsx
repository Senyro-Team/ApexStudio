import { useEffect, useState, lazy, Suspense } from "react";
import { ArrowUpRight, FilePenLine } from "lucide-react";
import {
  listPosts,
  createPost,
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
              <button className="post-draft-row" onClick={() => setId(r.id)}>
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
            </li>
          ))}
          {!rows.length && <li>No saved drafts.</li>}
        </ul>
      )}
    </>
  );
}
