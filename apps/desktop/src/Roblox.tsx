import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import AppSelect from "./AppSelect";
import { liveryCodesMarkdown } from "./liveryCodesMarkdown";
import { listen } from "@tauri-apps/api/event";
import { CheckCheck, CloudUpload, FileCode2, Gamepad2, Layers3, ListTodo, RefreshCw, Search, Settings2, Trash2 } from "lucide-react";
import {
  robloxLiveries,
  robloxLivery,
  saveLiveryCodesMarkdown,
  robloxSettings,
  uploadSnapshot,
  previewUploadBatch,
  startUploadBatch,
  controlUploadBatch,
  errorMessage,
  type RobloxFocus,
  type RobloxLiverySummary,
  type LiveryUploads,
  type UploadSnapshot,
  type UploadReview,
} from "./bridge";
const initial: UploadSnapshot = {
  revision: 0,
  queue: { batches: [], jobs: [], items: [] },
  concurrency: 5,
  activeTransfers: 0,
  nextAllowedAt: 0,
};
const stateLabel: Record<string, string> = {
  queued: "Queued",
  submitting: "Uploading",
  processing: "Processing",
  polling: "Checking processing",
  complete: "Complete",
  failed: "Failed",
  needs_review: "Needs review",
  cancelled: "Cancelled",
};
export default function Roblox({
  focus,
  onSettings,
}: {
  focus: RobloxFocus;
  onSettings: () => void;
}) {
  const [liveries, setLiveries] = useState<RobloxLiverySummary[]>([]),
    [selected, setSelected] = useState<Set<string>>(
      () => new Set(focus.folderIds ?? []),
    ),
    [project, setProject] = useState(focus.projectId ?? ""),
    [search, setSearch] = useState(""),
    [page, setPage] = useState(0),
    [snapshot, setSnapshot] = useState(initial),
    [account, setAccount] = useState<string | null>(null),
    [doubleUpload, setDoubleUpload] = useState(true),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [review, setReview] = useState<
      (UploadReview & { force: boolean; folderIds: string[] }) | null
    >(null),
    [details, setDetails] = useState<Record<string, LiveryUploads | undefined>>(
      {},
    ),
    [now, setNow] = useState(Date.now()),
    [notice, setNotice] = useState(""),
    [codesPreview, setCodesPreview] = useState<{ title: string; markdown: string; codes: number; missing: number; liveries: number } | null>(null),
    [codesNotice, setCodesNotice] = useState(""),
    [codesError, setCodesError] = useState(""),
    [confirmDeleteBatch, setConfirmDeleteBatch] = useState<string | null>(null),
    [jobPages, setJobPages] = useState<Record<string, number>>({});
  const dialog = useRef<HTMLDialogElement>(null),
    codesDialog = useRef<HTMLDialogElement>(null),
    requestId = useRef<string | null>(null);
  const load = useCallback(async () => {
    const [rows, settings] = await Promise.all([
      robloxLiveries(),
      robloxSettings(),
    ]);
    setLiveries(rows);
    setAccount(settings.accountId);
    setDoubleUpload(settings.doubleUpload);
  }, []);
  useEffect(() => {
    let active = true;
    void load().catch((e) => {
      if (active) setError(errorMessage(e));
    });
    void uploadSnapshot()
      .then((s) => {
        if (active)
          setSnapshot((old) => (s.revision >= old.revision ? s : old));
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    let dispose: (() => void) | undefined;
    void listen<UploadSnapshot>("roblox-queue", (e) => {
      if (active)
        setSnapshot((old) =>
          e.payload.revision >= old.revision ? e.payload : old,
        );
    })
      .then((fn) => {
        if (active) dispose = fn;
        else fn();
      })
      .catch(() => {});
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      active = false;
      dispose?.();
      clearInterval(timer);
    };
  }, [load]);
  const completed = snapshot.queue.jobs
    .filter((j) => j.state === "complete")
    .map((j) => j.id)
    .join(",");
  useEffect(() => {
    if (completed) void load().catch(() => {});
  }, [completed, load]);
  useEffect(() => {
    setProject(focus.projectId ?? "");
    setSelected(new Set(focus.folderIds ?? []));
  }, [focus]);
  useEffect(() => {
    if (review) dialog.current?.showModal();
  }, [review]);
  useEffect(() => {
    if (codesPreview && !codesDialog.current?.open) codesDialog.current?.showModal();
  }, [codesPreview]);
  useEffect(() => setPage(0), [project, search]);
  const projects = useMemo(
    () => [
      ...new Map(liveries.map((l) => [l.projectId, l.projectName])).entries(),
    ],
    [liveries],
  );
  const filtered = useMemo(
    () =>
      liveries.filter(
        (l) =>
          (!project || l.projectId === project) &&
          `${l.name} ${l.projectName}`
            .toLowerCase()
            .includes(search.trim().toLowerCase()),
      ),
    [liveries, project, search],
  );
  const jobsById = useMemo(
    () => new Map(snapshot.queue.jobs.map((j) => [j.id, j])),
    [snapshot.queue.jobs],
  );
  const jobsByBatch = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const item of snapshot.queue.items) {
      if (!map.has(item.batchId)) map.set(item.batchId, new Set());
      map.get(item.batchId)!.add(item.jobId);
    }
    return map;
  }, [snapshot.queue.items]);
  const liveryProgress = useMemo(() => {
    const folders = new Map<
      string,
      Map<string, (typeof snapshot.queue.jobs)[number]>
    >();
    for (const item of snapshot.queue.items) {
      const job = jobsById.get(item.jobId);
      if (!job || job.accountId !== account) continue;
      let jobs = folders.get(item.folderId);
      if (!jobs) {
        jobs = new Map();
        folders.set(item.folderId, jobs);
      }
      const previous = jobs.get(job.hash);
      if (!previous || previous.generation < job.generation)
        jobs.set(job.hash, job);
    }
    return new Map(
      [...folders].map(([id, jobs]) => {
        const values = [...jobs.values()];
        return [
          id,
          `${values.filter((j) => j.state === "complete").length}/${values.length} textures complete${values.some((j) => ["failed", "needs_review"].includes(j.state)) ? " · Needs attention" : ""}`,
        ];
      }),
    );
  }, [snapshot.queue.items, jobsById, account]);
  function toggle(id: string) {
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  async function previewCodes() {
    const rows = liveries.filter((l) => selected.has(l.id));
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const entries: { projectName: string; livery: LiveryUploads }[] = [];
      for (let start = 0; start < rows.length; start += 8) {
        entries.push(
          ...(await Promise.all(
            rows
              .slice(start, start + 8)
              .map(async (row) => ({
                projectName: row.projectName,
                livery: await robloxLivery(row.id),
              })),
          )),
        );
      }
      const result = liveryCodesMarkdown(entries);
      if (!result.codes)
        throw new Error(
          "No usable saved decal IDs. Upload the selected textures to Roblox first.",
        );
      const projectNames = [...new Set(rows.map((row) => row.projectName))];
      setCodesNotice("");
      setCodesError("");
      setCodesPreview({
        title: projectNames.length === 1 ? `Codes for ${projectNames[0]}` : "Livery codes",
        markdown: result.markdown,
        codes: result.codes,
        missing: result.missing,
        liveries: rows.length,
      });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }
  async function copyPreviewCodes() {
    if (!codesPreview) return;
    setCodesError("");
    try {
      await navigator.clipboard.writeText(codesPreview.markdown);
      setCodesNotice(`${codesPreview.codes} decal IDs copied as Markdown.`);
    } catch (reason) {
      setCodesError(errorMessage(reason));
    }
  }
  async function savePreviewCodes() {
    if (!codesPreview) return;
    setBusy(true);
    setCodesError("");
    try {
      if (await saveLiveryCodesMarkdown(codesPreview.markdown)) setCodesNotice("Markdown file saved.");
    } catch (reason) {
      setCodesError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }
  async function prepare(force: boolean) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const folderIds = [...selected];
      const result = await previewUploadBatch(folderIds, force);
      requestId.current = crypto.randomUUID();
      setReview({ ...result, folderIds, force });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function start() {
    if (!review || !requestId.current) return;
    setBusy(true);
    setError("");
    try {
      await startUploadBatch(requestId.current, review.folderIds, review.force);
      setReview(null);
      setSnapshot(await uploadSnapshot());
      setNotice(
        "Upload batch queued. You can continue working in Apex Studio.",
      );
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function action(
    id: string | null,
    kind: "pause" | "resume" | "cancel" | "retry" | "delete",
  ) {
    setError("");
    setBusy(true);
    try {
      await controlUploadBatch(id, kind);
      setSnapshot(await uploadSnapshot());
      if (kind === "delete") {
        setConfirmDeleteBatch(null);
        setNotice("Upload batch deleted from the queue.");
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function expand(id: string) {
    try {
      setDetails((s) => ({ ...s, [id]: undefined }));
      const value = await robloxLivery(id);
      setDetails((s) => ({ ...s, [id]: value }));
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  const wait = Math.max(0, Math.ceil((snapshot.nextAllowedAt - now) / 1000));
  return (
    <section className="roblox-page" aria-label="Roblox uploads">
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="roblox-account">
        <div className="roblox-account-info">
          <span className="roblox-account-icon"><Gamepad2 size={22} aria-hidden="true" /></span>
          <div>
            <small>{account ? "Connected account" : "Account needed"}</small>
            <strong>{account ? `Roblox account ${account}` : "Connect Roblox to start uploading"}</strong>
          </div>
        </div>
        <button className="secondary" onClick={onSettings}>
          <Settings2 size={16} aria-hidden="true" /> Roblox settings
        </button>
      </div>
      <nav className="roblox-tabs" aria-label="Roblox sections">
        <button
          className="roblox-tab"
          onClick={() =>
            document.getElementById("roblox-liveries")?.scrollIntoView()
          }
        >
          <Layers3 size={17} aria-hidden="true" /> Liveries
        </button>
        <button
          className="roblox-tab"
          onClick={() =>
            document.getElementById("roblox-queue")?.scrollIntoView()
          }
        >
          <ListTodo size={17} aria-hidden="true" /> Upload queue
        </button>
      </nav>
      <section id="roblox-liveries">
        <div className="roblox-section-heading">
          <div><span className="roblox-section-icon"><Layers3 size={21} aria-hidden="true" /></span><div><h2>Select liveries</h2><p>Choose the designs you want to upload or export codes for.</p></div></div>
          <span className="roblox-section-count">{filtered.length} liveries</span>
        </div>
        <div className="roblox-filters">
          <label>
            Project
            <AppSelect
              aria-label="Roblox project"
              value={project}
              onChange={(e) => setProject(e.target.value)}
            >
              <option value="">All projects</option>
              {projects.map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </AppSelect>
          </label>
          <label>
            Search
            <span className="roblox-search-field"><Search size={17} aria-hidden="true" /><input
                aria-label="Search liveries"
                type="search"
                placeholder="Find a livery"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              /></span>
          </label>
        </div>
        <div className="roblox-selection">
          <label>
            <input
              type="checkbox"
              checked={
                filtered.length > 0 && filtered.every((l) => selected.has(l.id))
              }
              disabled={!filtered.length}
              onChange={(e) =>
                setSelected((s) => {
                  const next = new Set(s);
                  for (const l of filtered)
                    if (e.target.checked) next.add(l.id);
                    else next.delete(l.id);
                  return next;
                })
              }
            />
            Select all filtered liveries
          </label>
          <span>{selected.size} selected</span>
        </div>
        <div className="roblox-start-actions">
          <button
            className="primary"
            disabled={busy || !account || !selected.size}
            onClick={() => void prepare(false)}
          >
            <CloudUpload size={17} aria-hidden="true" /> Upload missing/changed
          </button>
          <button
            className="secondary"
            disabled={busy || !account || !selected.size}
            onClick={() => void prepare(true)}
          >
            <RefreshCw size={16} aria-hidden="true" /> Re-upload selected
          </button>
          <button
            className="secondary"
            disabled={busy || !selected.size}
            onClick={() => void previewCodes()}
          >
            <FileCode2 size={17} aria-hidden="true" /> Preview codes
          </button>
        </div>
        <ul className="roblox-livery-rows">
          {filtered.slice(page * 50, (page + 1) * 50).map((l) => (
            <li key={l.id}>
              <label className="roblox-livery-choice">
                <input
                  type="checkbox"
                  checked={selected.has(l.id)}
                  onChange={() => toggle(l.id)}
                />
                <span>
                  <strong>{l.name}</strong>
                  <small>
                    {l.projectName} · {l.textures} textures · {l.uploaded} saved
                    IDs
                  </small>
                  {liveryProgress.get(l.id) && (
                    <small>{liveryProgress.get(l.id)}</small>
                  )}
                </span>
              </label>
              <details
                onToggle={(e) => {
                  if (e.currentTarget.open && !details[l.id]) void expand(l.id);
                }}
              >
                <summary>Textures</summary>
                {details[l.id] ? (
                  <ul className="roblox-texture-details">
                    {details[l.id]!.textures.map((t) => (
                      <li key={t.assetId}>
                        {t.side} · {t.name}
                        <span>
                          {t.upload?.assetId
                            ? `Decal ${t.upload.assetId} · ${t.upload.moderation}`
                            : "Not uploaded"}
                        </span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p role="status">Loading textures…</p>
                )}
              </details>
            </li>
          ))}
        </ul>
        {!filtered.length && <p>No matching liveries.</p>}
        {filtered.length > 50 && (
          <div className="button-row">
            <button
              className="secondary"
              disabled={!page}
              onClick={() => setPage((p) => p - 1)}
            >
              Previous
            </button>
            <span>
              Page {page + 1} of {Math.ceil(filtered.length / 50)}
            </span>
            <button
              className="secondary"
              disabled={(page + 1) * 50 >= filtered.length}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </button>
          </div>
        )}
        <p className="field-hint">
          {doubleUpload
            ? "New textures upload twice. The second decal ID is used for ERLCX; both IDs remain in history."
            : "New textures upload once. Its decal ID is used for ERLCX and remains in upload history."}
        </p>
      </section>
      <section id="roblox-queue" className="roblox-queue">
        <div className="roblox-section-heading">
          <div><span className="roblox-section-icon"><ListTodo size={21} aria-hidden="true" /></span><div><h2>Upload queue</h2><p>Track the progress of your textures.</p></div></div>
          <span className="roblox-section-count">{snapshot.activeTransfers} transferring · limit {snapshot.concurrency}</span>
        </div>
        {wait > 0 && (
          <p role="status">Waiting for Roblox rate limit · {wait}s</p>
        )}
        <div className="button-row">
          <button
            className="secondary"
            onClick={() => void action(null, "pause")}
            disabled={
              !snapshot.queue.batches.some((b) => b.state === "running")
            }
          >
            Pause all
          </button>
          <button
            className="secondary"
            onClick={() => void action(null, "resume")}
            disabled={!snapshot.queue.batches.some((b) => b.state === "paused")}
          >
            Resume all
          </button>
        </div>
        {!snapshot.queue.batches.length && <p className="roblox-empty-state"><CheckCheck size={22} aria-hidden="true" /> No upload batches.</p>}
        {snapshot.queue.batches.map((b) => {
          const jobs = [...(jobsByBatch.get(b.id) ?? [])]
            .map((id) => jobsById.get(id))
            .filter((j) => !!j);
          const done = jobs.filter((j) => j.state === "complete").length;
          const canDelete = ["complete", "cancelled"].includes(b.state)
            && jobs.every((j) => ["complete", "cancelled"].includes(j.state));
          return (
            <details key={b.id} className="roblox-batch">
              <summary>
                {new Date(b.createdAt).toLocaleString()} · {done}/{jobs.length}{" "}
                textures · {b.state}
              </summary>
              <p>Roblox account {b.accountId}</p>
              <div className="button-row">
                <button
                  className="secondary"
                  disabled={b.state !== "running"}
                  onClick={() => void action(b.id, "pause")}
                >
                  Pause
                </button>
                <button
                  className="secondary"
                  disabled={b.state !== "paused"}
                  onClick={() => void action(b.id, "resume")}
                >
                  Resume
                </button>
                <button
                  className="secondary"
                  disabled={!jobs.some((j) => j.state === "failed")}
                  onClick={() => void action(b.id, "retry")}
                >
                  Retry failed
                </button>
                <button
                  className="text-button"
                  disabled={!["running", "paused"].includes(b.state)}
                  onClick={() => void action(b.id, "cancel")}
                >
                  Cancel pending
                </button>
                <button
                  className="text-button queue-delete-button"
                  disabled={!canDelete || busy}
                  title={canDelete ? "Remove this batch from the queue" : "Finish or cancel pending uploads before deleting this batch"}
                  onClick={() => setConfirmDeleteBatch(b.id)}
                >
                  <Trash2 size={16} aria-hidden="true" /> Delete batch
                </button>
              </div>
              {confirmDeleteBatch === b.id && (
                <div className="confirm-panel queue-delete-confirm" role="group" aria-label="Confirm batch deletion">
                  <p>Delete this batch from the queue? Uploaded decals and saved IDs will remain available.</p>
                  <div className="button-row">
                    <button className="danger" disabled={busy} onClick={() => void action(b.id, "delete")}>Delete batch</button>
                    <button className="secondary" disabled={busy} onClick={() => setConfirmDeleteBatch(null)}>Keep batch</button>
                  </div>
                </div>
              )}
              <ul className="roblox-job-list">
                {jobs
                  .slice(
                    (jobPages[b.id] ?? 0) * 50,
                    ((jobPages[b.id] ?? 0) + 1) * 50,
                  )
                  .map((j) => (
                    <li key={j.id}>
                      <strong>{j.name}</strong>
                      <span>
                        {stateLabel[j.state] ?? j.state} · pass {j.pass}/
                        {j.passesRequired}
                      </span>
                      {j.message && <p>{j.message}</p>}
                      <details>
                        <summary>IDs and details</summary>
                        <dl>
                          <dt>First decal ID</dt>
                          <dd>{j.firstId ?? "Not available"}</dd>
                          <dt>Second decal ID</dt>
                          <dd>{j.secondId ?? "Not available"}</dd>
                          <dt>Moderation</dt>
                          <dd>{j.moderation}</dd>
                          <dt>Generation</dt>
                          <dd>{j.generation}</dd>
                        </dl>
                      </details>
                    </li>
                  ))}
              </ul>
              {jobs.length > 50 && (
                <div className="button-row">
                  <button
                    disabled={!(jobPages[b.id] ?? 0)}
                    onClick={() =>
                      setJobPages((p) => ({ ...p, [b.id]: (p[b.id] ?? 0) - 1 }))
                    }
                  >
                    Previous
                  </button>
                  <span>Page {(jobPages[b.id] ?? 0) + 1}</span>
                  <button
                    disabled={((jobPages[b.id] ?? 0) + 1) * 50 >= jobs.length}
                    onClick={() =>
                      setJobPages((p) => ({ ...p, [b.id]: (p[b.id] ?? 0) + 1 }))
                    }
                  >
                    Next
                  </button>
                </div>
              )}
            </details>
          );
        })}
        <p className="field-hint">
          Closing the window keeps active batches running in the system tray.
          Use Exit in the tray to stop Apex Studio.
        </p>
      </section>
      {review && (
        <dialog
          ref={dialog}
          className="roblox-dialog"
          aria-labelledby="review-upload-title"
          onCancel={(e) => {
            if (busy) e.preventDefault();
            else setReview(null);
          }}
        >
          <h2 id="review-upload-title">
            {review.force
              ? "Re-upload selected liveries"
              : "Review Roblox uploads"}
          </h2>
          <p>
            Account {review.accountId} · {review.liveries} liveries ·{" "}
            {review.textures} assigned textures
          </p>
          <p>
            {review.uniqueTextures} unique textures · {review.reused} existing
            IDs reused
          </p>
          <p>
            {review.creates} upload submissions will be sent. Each new texture
            {review.doubleUpload
              ? " uses two sequential passes; the second Decal ID is selected after processing succeeds."
              : " is uploaded once; its Decal ID is selected after processing succeeds."}
          </p>
          {review.queued > 0 && (
            <p>
              {review.queued} textures already have queued work. This batch will
              share their existing jobs.
            </p>
          )}
          {!review.creates && !review.queued && (
            <p>All selected textures already have saved IDs.</p>
          )}
          {error && (
            <p role="alert" className="error">
              {error}
            </p>
          )}
          <div className="button-row">
            <button
              className="primary"
              disabled={busy || (!review.creates && !review.queued)}
              onClick={() => void start()}
            >
              {busy ? "Queuing…" : "Start batch"}
            </button>
            <button
              className="text-button"
              disabled={busy}
              onClick={() => setReview(null)}
            >
              Cancel
            </button>
          </div>
        </dialog>
      )}
      {codesPreview && (
        <dialog ref={codesDialog} className="roblox-dialog roblox-codes-dialog" aria-labelledby="livery-codes-title" onCancel={(event) => { if (busy) event.preventDefault(); else setCodesPreview(null); }}>
          <div className="roblox-codes-heading">
            <div><span className="roblox-codes-icon"><FileCode2 size={22} aria-hidden="true" /></span><h2 id="livery-codes-title">{codesPreview.title}</h2></div>
            <p>{codesPreview.codes} codes from {codesPreview.liveries} {codesPreview.liveries === 1 ? "livery" : "liveries"}{codesPreview.missing ? ` · ${codesPreview.missing} unavailable` : ""}</p>
          </div>
          <pre className="roblox-codes-content" aria-label="Markdown preview">{codesPreview.markdown}</pre>
          {codesError && <p className="error" role="alert">{codesError}</p>}
          {codesNotice && <p role="status">{codesNotice}</p>}
          <div className="button-row roblox-codes-actions">
            <button className="primary" disabled={busy} onClick={() => void copyPreviewCodes()}>Copy all</button>
            <button className="secondary" disabled={busy} onClick={() => void savePreviewCodes()}>Save as Markdown</button>
            <button className="text-button" disabled={busy} onClick={() => setCodesPreview(null)}>Close</button>
          </div>
        </dialog>
      )}
    </section>
  );
}
