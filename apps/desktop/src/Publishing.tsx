import { useEffect, useState } from "react";
import SlotEditor from "./SlotEditor";
import {
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
const steps = ["Details", "Previews", "Codes", "Review"] as const;
export default function Publishing({
  project,
  catalogue,
  onEditingChange,
}: {
  project: Project;
  catalogue: Catalogue | null;
  onEditingChange: (dirty: boolean) => void;
}) {
  const [saved, setSaved] = useState<PublishingDraft | null>(null),
    [data, setData] = useState<PublishingData | null>(null),
    [step, setStep] = useState<(typeof steps)[number]>("Details"),
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
    Promise.all([publishingDraft(project.id), assetBoard(project.id)])
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
  }, [project.id]);
  useEffect(() => {
    onEditingChange(dirty || busy);
    return () => onEditingChange(false);
  }, [dirty, busy, onEditingChange]);
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
  const locked = busy || project.archived;
  function edit(patch: Partial<PublishingData>) {
    if (!data) return;
    const next = { ...data, ...patch };
    if (JSON.stringify(next) === JSON.stringify(data)) return;
    setData(next);
    setDirty(JSON.stringify(next) !== JSON.stringify(saved?.data));
    setNotice("");
  }
  function changeLivery(patch: Partial<PublishingLivery>) {
    if (!data) return;
    edit({
      liveries: data.liveries.map((l, i) =>
        i === activeLivery ? { ...l, ...patch } : l,
      ),
    });
  }
  async function addImages() {
    setBusy(true);
    setError("");
    try {
      const selected = await pickPublishingImages(project.id);
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
      const next = await savePublishing(project.id, saved.revision, data);
      setSaved(next);
      setData(next.data);
      setDirty(false);
      setNotice("Publishing draft saved.");
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
      if (await exportPublishing(project.id))
        setNotice("Publishing bundle exported.");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  function addLivery() {
    const f = board.folders.find((f) => f.id === source);
    const vehicle = catalogue?.vehicles.find((v) => v.id === f?.vehicleId);
    const sides = f
      ? board.placements
          .filter((p) => p.folderId === f.id && p.slot)
          .map((p) => ({ side: p.slot!, decalId: "" }))
      : [];
    if (!data) return;
    edit({
      liveries: [
        ...data.liveries,
        {
          vehicle: vehicle?.name ?? "",
          primaryColor: "#FFFFFF",
          secondaryColor: null,
          opacity: 100,
          sides: sides.length ? sides : [{ side: "Left 1", decalId: "" }],
        },
      ],
    });
    setActiveLivery(data.liveries.length);
  }
  if (error && !data)
    return (
      <p role="alert" className="error">
        {error}
      </p>
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
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <nav className="publishing-steps" aria-label="Publishing sections">
        {steps.map((s) => (
          <button
            key={s}
            className="text-button"
            aria-current={step === s ? "step" : undefined}
            onClick={() => setStep(s)}
            disabled={busy}
          >
            {s}
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
                    onClick={() =>
                      edit({
                        media: [...data.media, { hash: a.hash, name: a.name }],
                      })
                    }
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
      {step === "Codes" && (
        <section>
          <div className="publishing-section-actions">
            <h2>Livery codes</h2>
            <div className="publishing-add-livery">
              <select
                aria-label="Livery source"
                value={source}
                disabled={locked}
                onChange={(e) => setSource(e.target.value)}
              >
                <option value="">Blank livery</option>
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
                disabled={locked || data.liveries.length >= 20}
                onClick={addLivery}
              >
                Add livery
              </button>
            </div>
          </div>
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
                  <label>
                    Vehicle
                    <input
                      aria-label="Publishing vehicle"
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
                          disabled={locked}
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
                          disabled={locked}
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
                          disabled={locked}
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
                      disabled={locked || current.sides.length >= 100}
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
          <h2>{data.title || "Untitled post"}</h2>
          <p className="pre-wrap">{data.description}</p>
          <p>{data.tags.filter(Boolean).join(" · ")}</p>
          <p>
            {data.media.length} preview images · {data.liveries.length}{" "}
            {data.liveries.length === 1 ? "livery" : "liveries"}
          </p>
          {data.liveries.map((l, i) => (
            <div key={i}>
              <h3>{l.vehicle || `Livery ${i + 1}`}</h3>
              {l.sides.map((s, j) => (
                <p key={j}>
                  {s.side || "Unassigned"} — {s.decalId || "No decal ID"}
                </p>
              ))}
            </div>
          ))}
          {requirements.length > 0 && (
            <ul aria-label="Publishing requirements">
              {requirements.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          )}
          <button
            className="primary"
            disabled={busy || dirty || requirements.length > 0}
            onClick={() => void exportBundle()}
          >
            Export publishing bundle
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
            }}
          >
            Discard changes
          </button>
        )}
      </footer>
    </section>
  );
}
