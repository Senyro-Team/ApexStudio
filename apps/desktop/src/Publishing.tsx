import PostLiveryPicker from "./PostLiveryPicker";
import { readyForPublishing, publishingLivery } from "./RobloxUpload";
import { useEffect, useState } from "react";
import SlotEditor from "./SlotEditor";
import {
  browserStatus,
  prepareBrowser,
  controlBrowser,
  type BrowserStatus,
  postDraft,
  savePost,
  postSources,
  pickPostImages,
  pinPostImages,
  releasePostImages,
  exportPost,
  type PostSourceStatus,
  robloxLivery,
  publishingDraft,
  publishingPreviews,
  savePublishing,
  pickPublishingImages,
  exportPublishing,
  assetBoard,
  errorMessage,
  type PublishingData,
  type PublishingDraft,
  type PublishingPreview,
  type PublishingLivery,
  type AssetBoard,
  type Catalogue,
  type Project,
} from "./bridge";
const steps = ["Liveries", "Details", "Previews", "Review"] as const;
export default function Publishing({
  project,
  draftId,
  projects = [],
  catalogue,
  onEditingChange,
  onOpenRoblox,
}: {
  onOpenRoblox?: (folderId: string) => void;
  project?: Project;
  draftId?: string;
  projects?: Project[];
  catalogue: Catalogue | null;
  onEditingChange: (dirty: boolean) => void;
}) {
  const [browser, setBrowser] = useState<BrowserStatus | null>(null);
  useEffect(() => {
    if (!draftId) return;
    let active = true;
    const poll = () => {
      void browserStatus()
        .then((s) => {
          if (active) setBrowser(s);
        })
        .catch(() => {});
    };
    poll();
    const timer = setInterval(poll, 1500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [draftId]);
  const [mediaProject, setMediaProject] = useState(project?.id ?? "");
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [sources, setSources] = useState<PostSourceStatus[]>([]),
    [reviewed, setReviewed] = useState(false);
  const [pendingFolder, setPendingFolder] = useState<string | null>(null);
  const [saved, setSaved] = useState<PublishingDraft | null>(null),
    [data, setData] = useState<PublishingData | null>(null),
    [step, setStep] = useState<(typeof steps)[number]>("Liveries"),
    [busy, setBusy] = useState(false),
    [dirty, setDirty] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [images, setImages] = useState<PublishingPreview[]>([]),
    [board, setBoard] = useState<AssetBoard>({
      assets: [],
      folders: [],
      placements: [],
    }),
    [source, setSource] = useState(""),
    [activeLivery, setActiveLivery] = useState(0),
    [showAssets, setShowAssets] = useState(false);
  useEffect(() => {
    let active = true;
    setError("");
    Promise.all([
      draftId ? postDraft(draftId) : publishingDraft(project!.id),
      project
        ? assetBoard(project.id)
        : Promise.resolve({ assets: [], folders: [], placements: [] }),
    ])
      .then(([d, b]) => {
        if (active) {
          setSaved(d);
          setData(d.data);
          setBoard(b);
        }
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [project?.id, draftId, loadAttempt]);
  useEffect(() => {
    if (!draftId) return;
    void postSources(draftId)
      .then(setSources)
      .catch((e) => setError(errorMessage(e)));
  }, [draftId, saved?.revision]);
  useEffect(
    () => () => {
      if (draftId) void releasePostImages(draftId).catch(() => {});
    },
    [draftId],
  );
  useEffect(() => {
    if (!draftId || !data?.media.length) return;
    const renew = () => {
      void pinPostImages(
        draftId,
        data.media.map((m) => m.hash),
      ).catch((e) => setError(errorMessage(e)));
    };
    renew();
    const timer = setInterval(renew, 300000);
    return () => clearInterval(timer);
  }, [draftId, data?.media]);
  useEffect(() => {
    if (!draftId) return;
    if (!mediaProject) {
      setBoard({ assets: [], folders: [], placements: [] });
      return;
    }
    void assetBoard(mediaProject)
      .then(setBoard)
      .catch((e) => setError(errorMessage(e)));
  }, [draftId, mediaProject]);
  useEffect(() => {
    onEditingChange(dirty || busy);
    return () => onEditingChange(false);
  }, [dirty, busy, pendingFolder, onEditingChange]);
  useEffect(() => {
    let active = true;
    if (!data?.media.length) {
      setImages([]);
      return;
    }
    publishingPreviews(data.media)
      .then((v) => {
        if (active) setImages(v);
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [data?.media]);
  const locked = busy || !!project?.archived;
  function edit(patch: Partial<PublishingData>) {
    if (!data) return;
    const next = { ...data, ...patch };
    if (JSON.stringify(next) === JSON.stringify(data)) return;
    setReviewed(false);
    setData(next);
    setDirty(JSON.stringify(next) !== JSON.stringify(saved?.data));
    setNotice("");
  }
  function changeLivery(patch: Partial<PublishingLivery>) {
    if (!data) return;
    edit({
      liveries: data.liveries.map((l, i) =>
        i === activeLivery
          ? {
              ...l,
              ...patch,
              ...(patch.sides || patch.vehicle ? { source: undefined } : {}),
            }
          : l,
      ),
    });
  }
  async function addImages() {
    setBusy(true);
    setError("");
    try {
      const selected = await (draftId
        ? pickPostImages(draftId)
        : pickPublishingImages(project!.id));
      if (selected && data) {
        const merged = [...data.media];
        for (const image of selected)
          if (!merged.some((m) => m.hash === image.hash))
            merged.push({ hash: image.hash, name: image.name });
        if (merged.length > 10)
          throw Error(
            "Use up to ten preview images. Remove an image before adding more.",
          );
        edit({ media: merged });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    if (!saved || !data) return;
    setBusy(true);
    setError("");
    try {
      const next = await (draftId
        ? savePost(draftId, saved.revision, data)
        : savePublishing(project!.id, saved.revision, data));
      setSaved(next);
      setData(next.data);
      setDirty(false);
      setNotice("Publishing draft saved.");
      if (draftId) {
        try {
          await releasePostImages(draftId);
        } catch (e) {
          setError(errorMessage(e));
        }
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function exportBundle() {
    setBusy(true);
    setError("");
    try {
      if (
        await (draftId
          ? exportPost(draftId, saved!.revision, reviewed)
          : exportPublishing(project!.id))
      )
        setNotice("Publishing bundle exported.");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  function includeLivery(next: PublishingLivery) {
    if (!data) return;
    const existing = next.source
      ? data.liveries.findIndex(
          (l) => l.source?.folderId === next.source!.folderId,
        )
      : -1;
    if (existing >= 0) {
      edit({
        liveries: data.liveries.map((l, i) =>
          i === existing
            ? {
                ...next,
                primaryColor: l.primaryColor,
                secondaryColor: l.secondaryColor,
                opacity: l.opacity,
              }
            : l,
        ),
      });
      setActiveLivery(existing);
    } else {
      edit({ liveries: [...data.liveries, next] });
      setActiveLivery(data.liveries.length);
    }
    setPendingFolder(null);
  }
  async function addLivery() {
    if (!source || !data) return;
    setBusy(true);
    setError("");
    try {
      const l = await robloxLivery(source);
      if (readyForPublishing(l)) includeLivery(publishingLivery(l));
      else setPendingFolder(source);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  if (error && !data)
    return (
      <div role="alert" className="error">
        <p>{error}</p>
        <button
          className="secondary"
          type="button"
          onClick={() => setLoadAttempt((n) => n + 1)}
        >
          Retry loading draft
        </button>
      </div>
    );
  if (!data || !saved) return <p role="status">Loading publishing draft…</p>;
  const current = data.liveries[activeLivery];
  const requirements = [
    ...(data.title.trim().length < 5 ? ["Title: at least 5 characters."] : []),
    ...(!data.description.trim() ? ["Description is required."] : []),
    ...(data.media.length < 2 ? ["Previews: choose at least two images."] : []),
    ...(!data.liveries.length ? ["Add at least one livery."] : []),
    ...data.liveries.flatMap((l, i) =>
      !l.vehicle.trim() ||
      !l.sides.length ||
      l.sides.some((s) => !s.side || !/^[1-9][0-9]{0,19}$/.test(s.decalId))
        ? [`Livery ${i + 1}: enter a vehicle, sides and numeric decal IDs.`]
        : [],
    ),
  ];
  return (
    <section className="publishing-editor" aria-label="Publishing draft">
      {pendingFolder && (
        <div className="queue-prompt">
          <p>This livery needs Roblox decal IDs.</p>
          <button
            disabled={dirty || busy}
            onClick={() => onOpenRoblox?.(pendingFolder)}
          >
            Open Roblox uploads
          </button>
          <button onClick={() => setPendingFolder(null)}>Skip livery</button>
          {dirty && <p>Save your draft before opening uploads.</p>}
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <nav className="publishing-steps" aria-label="Publishing sections">
        {steps.map((s, index) => (
          <button
            key={s}
            className="publishing-step"
            aria-current={step === s ? "step" : undefined}
            onClick={() => setStep(s)}
            disabled={busy}
          >
            <span className="publishing-step-number" aria-hidden="true">{index + 1}</span>
            <span>{s}</span>
          </button>
        ))}
      </nav>
      {step === "Details" && (
        <div className="publishing-fields">
          <label>
            Title
            <input
              aria-label="Post title"
              maxLength={50}
              value={data.title}
              disabled={locked}
              onChange={(e) => edit({ title: e.target.value })}
            />
          </label>
          <span className="field-hint">{data.title.length}/50</span>
          <label>
            Description
            <textarea
              aria-label="Post description"
              rows={8}
              maxLength={2000}
              value={data.description}
              disabled={locked}
              onChange={(e) => edit({ description: e.target.value })}
            />
          </label>
          <span className="field-hint">
            {data.description.length}/2000 · Markdown supported
          </span>
          <label>
            Tags
            <input
              aria-label="Post tags"
              value={data.tags.join(", ")}
              disabled={locked}
              onChange={(e) =>
                edit({ tags: e.target.value.split(",").map((t) => t.trim()) })
              }
              onBlur={() => edit({ tags: data.tags.filter(Boolean) })}
            />
          </label>
          <p className="field-hint">
            Separate tags with commas. Up to 10 tags.
          </p>
          <label>
            YouTube URL
            <input
              aria-label="YouTube URL"
              type="url"
              value={data.youtubeUrl ?? ""}
              disabled={locked}
              onChange={(e) => edit({ youtubeUrl: e.target.value || null })}
            />
          </label>
        </div>
      )}
      {step === "Previews" && (
        <section>
          <div className="publishing-section-actions">
            <h2>Preview images</h2>
            <div className="button-row">
              <button
                className="secondary"
                disabled={locked || data.media.length >= 10}
                onClick={() => void addImages()}
              >
                Add images
              </button>
              {board.assets.some((a) => /\.png$/i.test(a.name)) && (
                <button
                  className="text-button"
                  disabled={locked}
                  onClick={() => setShowAssets((v) => !v)}
                >
                  Choose project images
                </button>
              )}
            </div>
          </div>
          <p className="field-hint">
            2–10 PNG images. The first image is the post cover.
          </p>
          {draftId && (
            <label>
              Image project
              <select
                value={mediaProject}
                disabled={locked}
                onChange={(e) => setMediaProject(e.target.value)}
              >
                <option value="">Choose project</option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
          )}
          {showAssets && (
            <div className="publishing-asset-choices">
              {board.assets
                .filter((a) => /\.png$/i.test(a.name))
                .map((a) => (
                  <button
                    key={a.id}
                    className="secondary"
                    disabled={
                      locked ||
                      data.media.length >= 10 ||
                      data.media.some((m) => m.hash === a.hash)
                    }
                    onClick={() => {
                      void (async () => {
                        try {
                          if (draftId) await pinPostImages(draftId, [a.hash]);
                          edit({
                            media: [
                              ...data.media,
                              { hash: a.hash, name: a.name },
                            ],
                          });
                        } catch (e) {
                          setError(errorMessage(e));
                        }
                      })();
                    }}
                  >
                    {a.name}
                  </button>
                ))}
            </div>
          )}
          <ol className="publishing-media-list">
            {data.media.map((m, i) => (
              <li key={m.hash}>
                {images.find((p) => p.hash === m.hash)?.preview && (
                  <img
                    src={images.find((p) => p.hash === m.hash)!.preview!}
                    alt={m.name}
                  />
                )}
                <div>
                  <strong>{m.name}</strong>
                  {i === 0 && <small>Post cover</small>}
                </div>
                <div className="publishing-media-actions">
                  <button
                    className="text-button"
                    aria-label={`Move ${m.name} up`}
                    disabled={locked || i === 0}
                    onClick={() => {
                      const media = [...data.media];
                      [media[i - 1], media[i]] = [media[i], media[i - 1]];
                      edit({ media });
                    }}
                  >
                    Up
                  </button>
                  <button
                    className="text-button"
                    aria-label={`Move ${m.name} down`}
                    disabled={locked || i === data.media.length - 1}
                    onClick={() => {
                      const media = [...data.media];
                      [media[i + 1], media[i]] = [media[i], media[i + 1]];
                      edit({ media });
                    }}
                  >
                    Down
                  </button>
                  <button
                    className="text-button"
                    disabled={locked}
                    onClick={() =>
                      edit({
                        media: data.media.filter((v) => v.hash !== m.hash),
                      })
                    }
                  >
                    Remove
                  </button>
                </div>
              </li>
            ))}
          </ol>
        </section>
      )}
      {step === "Liveries" && (
        <section>
          <div className="publishing-section-actions">
            <h2>Liveries for ERLCX</h2>
            {!draftId && (
              <div className="publishing-add-livery">
                <select
                  aria-label="Livery source"
                  value={source}
                  disabled={locked}
                  onChange={(e) => setSource(e.target.value)}
                >
                  <option value="">Choose a project livery</option>
                  {board.folders
                    .filter((f) => f.kind === "livery")
                    .map((f) => (
                      <option key={f.id} value={f.id}>
                        {f.name}
                      </option>
                    ))}
                </select>
                <button
                  className="secondary"
                  disabled={locked || !source || data.liveries.length >= 20}
                  onClick={() => void addLivery()}
                >
                  Include livery
                </button>
              </div>
            )}
          </div>
          <p className="field-hint">
            Upload livery textures to Roblox, then include their decal IDs here.
            Preview images for ERLCX are selected separately.
          </p>
          {draftId && (
            <PostLiveryPicker
              onBusy={setBusy}
              catalogue={catalogue}
              value={data.liveries}
              disabled={locked}
              onChange={(liveries) => {
                edit({ liveries });
                setActiveLivery(0);
              }}
            />
          )}
          {!data.liveries.length && <p>No liveries selected.</p>}
          <details className="publishing-manual">
            <summary>Use existing decal IDs manually</summary>
            <p className="field-hint">
              For textures already uploaded outside Apex Studio.
            </p>
            <button
              className="secondary"
              disabled={locked || data.liveries.length >= 20}
              onClick={() => {
                includeLivery({
                  vehicle: "",
                  primaryColor: "#FFFFFF",
                  secondaryColor: null,
                  opacity: 100,
                  sides: [{ side: "Left 1", decalId: "" }],
                });
              }}
            >
              Add manual livery
            </button>
          </details>
          {data.liveries.length > 0 && (
            <div className="publishing-code-layout">
              <div className="publishing-livery-list">
                {data.liveries.map((l, i) => (
                  <button
                    key={i}
                    className="secondary"
                    aria-pressed={activeLivery === i}
                    onClick={() => setActiveLivery(i)}
                  >
                    {l.vehicle || `Livery ${i + 1}`}
                  </button>
                ))}
              </div>
              {current && (
                <div className="publishing-fields">
                  {current.source && (
                    <div className="publishing-source">
                      <span>Roblox decal IDs linked to this livery.</span>
                      <button
                        className="secondary"
                        disabled={locked}
                        onClick={() => {
                          setSource(current.source!.folderId);
                          void (async () => {
                            setBusy(true);
                            try {
                              const l = await robloxLivery(
                                current.source!.folderId,
                              );
                              if (readyForPublishing(l))
                                includeLivery(publishingLivery(l));
                              else setPendingFolder(current.source!.folderId);
                            } catch (e) {
                              setError(errorMessage(e));
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      >
                        Refresh Roblox IDs
                      </button>
                    </div>
                  )}
                  <label>
                    Vehicle
                    <input
                      aria-label="Publishing vehicle"
                      readOnly={!!current.source}
                      list="publishing-vehicle-options"
                      maxLength={120}
                      value={current.vehicle}
                      disabled={locked}
                      onChange={(e) =>
                        changeLivery({ vehicle: e.target.value })
                      }
                    />
                    <datalist id="publishing-vehicle-options">
                      {current.vehicle.trim() &&
                        catalogue?.vehicles
                          .filter((v) =>
                            v.name
                              .toLowerCase()
                              .includes(current.vehicle.toLowerCase()),
                          )
                          .slice(0, 8)
                          .map((v) => <option key={v.id} value={v.name} />)}
                    </datalist>
                  </label>
                  <div className="publishing-colours">
                    <label>
                      Primary colour
                      <input
                        aria-label="Primary colour"
                        type="color"
                        value={current.primaryColor}
                        disabled={locked}
                        onChange={(e) =>
                          changeLivery({ primaryColor: e.target.value })
                        }
                      />
                    </label>
                    <label>
                      Opacity
                      <input
                        aria-label="Opacity"
                        type="number"
                        min={0}
                        max={100}
                        value={current.opacity}
                        disabled={locked}
                        onChange={(e) =>
                          changeLivery({ opacity: Number(e.target.value) })
                        }
                      />
                    </label>
                  </div>
                  <label className="check-label">
                    <input
                      type="checkbox"
                      checked={current.secondaryColor !== null}
                      disabled={locked}
                      onChange={(e) =>
                        changeLivery({
                          secondaryColor: e.target.checked ? "#000000" : null,
                        })
                      }
                    />
                    Secondary colour
                  </label>
                  {current.secondaryColor !== null && (
                    <input
                      aria-label="Secondary colour"
                      type="color"
                      value={current.secondaryColor}
                      disabled={locked}
                      onChange={(e) =>
                        changeLivery({ secondaryColor: e.target.value })
                      }
                    />
                  )}
                  <div className="publishing-sides">
                    {current.sides.map((s, i) => (
                      <div className="publishing-side" key={i}>
                        <SlotEditor
                          label={`code row ${i + 1}`}
                          value={s.side || null}
                          disabled={locked || !!current.source}
                          onChange={(side) =>
                            changeLivery({
                              sides: current.sides.map((v, j) =>
                                i === j ? { ...v, side: side ?? "" } : v,
                              ),
                            })
                          }
                        />
                        <input
                          aria-label={`Decal ID for row ${i + 1}`}
                          inputMode="numeric"
                          maxLength={20}
                          placeholder="Decal ID"
                          value={s.decalId}
                          disabled={locked || !!current.source}
                          onChange={(e) =>
                            changeLivery({
                              sides: current.sides.map((v, j) =>
                                j === i ? { ...v, decalId: e.target.value } : v,
                              ),
                            })
                          }
                        />
                        <button
                          className="text-button"
                          aria-label={`Remove code row ${i + 1}`}
                          disabled={locked || !!current.source}
                          onClick={() =>
                            changeLivery({
                              sides: current.sides.filter((_, j) => i !== j),
                            })
                          }
                        >
                          Remove
                        </button>
                      </div>
                    ))}
                  </div>
                  <div className="publishing-section-actions">
                    <button
                      className="secondary"
                      disabled={
                        locked ||
                        !!current.source ||
                        current.sides.length >= 100
                      }
                      onClick={() =>
                        changeLivery({
                          sides: [...current.sides, { side: "", decalId: "" }],
                        })
                      }
                    >
                      Add side
                    </button>
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => {
                        edit({
                          liveries: data.liveries.filter(
                            (_, i) => i !== activeLivery,
                          ),
                        });
                        setActiveLivery(0);
                      }}
                    >
                      Remove livery
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </section>
      )}
      {step === "Review" && (
        <section className="publishing-review">
          <p className="field-hint">
            The assistant stops at the final review. Agreement and publication
            remain your choice.
          </p>
          <h2>{data.title || "Untitled post"}</h2>
          <p className="pre-wrap">{data.description}</p>
          <p>{data.tags.filter(Boolean).join(" · ")}</p>
          <p>
            {data.media.length} preview images · {data.liveries.length}{" "}
            {data.liveries.length === 1 ? "livery" : "liveries"}
          </p>
          <details>
            <summary>
              View livery codes (
              {data.liveries.reduce((n, l) => n + l.sides.length, 0)})
            </summary>
            {data.liveries.map((l, i) => (
              <details key={i}>
                <summary>
                  {l.vehicle || `Livery ${i + 1}`} · {l.sides.length}{" "}
                  {l.sides.length === 1 ? "texture" : "textures"}
                </summary>
                {l.sides.map((s, j) => (
                  <p key={j}>
                    {s.side || "Unassigned"} — {s.decalId || "No decal ID"}
                  </p>
                ))}
              </details>
            ))}
          </details>
          {draftId &&
            sources
              .filter((s) => s.state !== "current")
              .map((s) => (
                <p key={s.folderId}>
                  {s.vehicle}: source {s.state}. Saved codes are unchanged.
                </p>
              ))}
          {draftId && sources.some((s) => s.state !== "current") && (
            <label className="post-check">
              <input
                type="checkbox"
                checked={reviewed}
                onChange={(e) => setReviewed(e.target.checked)}
              />
              I reviewed the saved codes and preview images and want to use this
              snapshot.
            </label>
          )}
          {data.liveries
            .filter((l) => l.source && l.sides.some((s) => !s.decalId))
            .map((l) => (
              <button
                key={l.source!.folderId}
                className="secondary"
                disabled={dirty || busy}
                onClick={() => onOpenRoblox?.(l.source!.folderId)}
              >
                Open Roblox uploads: {l.vehicle}
              </button>
            ))}
          {requirements.length > 0 && (
            <ul aria-label="Publishing requirements">
              {requirements.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          )}
          {draftId && (
            <div className="browser-handoff">
              <button
                className="primary"
                disabled={
                  busy ||
                  dirty ||
                  !!requirements.length ||
                  !browser?.connected ||
                  (sources.some((s) => s.state !== "current") && !reviewed) ||
                  ["running", "paused"].includes(browser?.state ?? "")
                }
                onClick={() => {
                  setBusy(true);
                  setError("");
                  void prepareBrowser(draftId, saved.revision, reviewed)
                    .then(() => browserStatus())
                    .then(setBrowser)
                    .catch((e) => setError(errorMessage(e)))
                    .finally(() => setBusy(false));
                }}
              >
                Prepare on ERLCX
              </button>
              {!browser?.connected && (
                <p>Verify the Chrome connection in Settings.</p>
              )}
              {browser?.postId === draftId && (
                <>
                  <p role="status">
                    {browser.step} {browser.message}
                  </p>
                  {browser.state === "paused" && (
                    <button
                      className="secondary"
                      onClick={() =>
                        void controlBrowser("resume").catch((e) =>
                          setError(errorMessage(e)),
                        )
                      }
                    >
                      Resume assistant
                    </button>
                  )}
                  {["running", "paused"].includes(browser.state) && (
                    <button
                      className="secondary"
                      onClick={() =>
                        void controlBrowser("cancel").catch((e) =>
                          setError(errorMessage(e)),
                        )
                      }
                    >
                      Cancel assistant
                    </button>
                  )}
                </>
              )}
            </div>
          )}
          <button
            className={draftId ? "secondary" : "primary"}
            disabled={
              busy ||
              dirty ||
              requirements.length > 0 ||
              (!!draftId &&
                sources.some((s) => s.state !== "current") &&
                !reviewed)
            }
            onClick={() => void exportBundle()}
          >
            Export for ERLCX extension
          </button>
          {dirty && (
            <p className="field-hint">Save changes before exporting.</p>
          )}
        </section>
      )}
      <footer className="publishing-footer">
        <button
          className="primary"
          disabled={locked || !dirty}
          onClick={() => void save()}
        >
          Save draft
        </button>
        {dirty && (
          <button
            className="text-button"
            disabled={busy}
            onClick={() => {
              setData(saved.data);
              setDirty(false);
              setError("");
              if (draftId)
                void releasePostImages(draftId).catch((e) =>
                  setError(errorMessage(e)),
                );
            }}
          >
            Discard changes
          </button>
        )}
      </footer>
    </section>
  );
}
