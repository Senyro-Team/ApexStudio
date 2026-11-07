import { useEffect, useMemo, useState } from "react";
import AppSelect from "./AppSelect";
import {
  robloxLiveries,
  robloxLivery,
  errorMessage,
  type RobloxLiverySummary,
  type PublishingLivery,
  type Catalogue,
} from "./bridge";
import { publishingLivery } from "./RobloxUpload";
export default function PostLiveryPicker({
  value,
  disabled,
  onChange,
  onBusy,
  catalogue,
}: {
  value: PublishingLivery[];
  disabled: boolean;
  onChange: (v: PublishingLivery[]) => void;
  onBusy: (v: boolean) => void;
  catalogue: Catalogue | null;
}) {
  const [rows, setRows] = useState<RobloxLiverySummary[]>([]),
    [project, setProject] = useState(""),
    [search, setSearch] = useState(""),
    [page, setPage] = useState(0),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [loadFailed, setLoadFailed] = useState(false),
    [loadAttempt, setLoadAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    setError("");
    setLoadFailed(false);
    void robloxLiveries()
      .then((items) => {
        if (active) setRows(items);
      })
      .catch((e) => {
        if (active) {
          setError(errorMessage(e));
          setLoadFailed(true);
        }
      });
    return () => {
      active = false;
    };
  }, [loadAttempt]);
  const vehicleNames = useMemo(
    () => new Map(catalogue?.vehicles.map((v) => [v.id, v.name]) ?? []),
    [catalogue],
  );
  const vehicleName = (id: string | null) =>
    id ? (vehicleNames.get(id) ?? id) : "Unassigned vehicle";
  const filtered = useMemo(
    () =>
      rows.filter(
        (r) =>
          (!project || r.projectId === project) &&
          `${r.name} ${vehicleName(r.vehicleId)} ${r.projectName}`
            .toLowerCase()
            .includes(search.toLowerCase()),
      ),
    [rows, project, search, vehicleNames],
  );
  const selected = new Set(
    value.flatMap((l) => (l.source ? [l.source.folderId] : [])),
  );
  async function choose(ids: string[], remove = false) {
    setBusy(true);
    onBusy(true);
    setError("");
    try {
      if (remove) {
        onChange(
          value.filter((l) => !l.source || !ids.includes(l.source.folderId)),
        );
        return;
      }
      const extra = ids.filter((id) => !selected.has(id));
      if (value.length + extra.length > 20)
        throw Error("Select up to twenty liveries per post.");
      const added = await Promise.all(
        extra.map(async (id) => publishingLivery(await robloxLivery(id))),
      );
      onChange([...value, ...added]);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <section className="post-livery-picker" aria-label="Select liveries">
      {error && (
        <div role="alert" className="error">
          <p>{error}</p>
          {loadFailed && (
            <button
              className="secondary"
              type="button"
              onClick={() => setLoadAttempt((n) => n + 1)}
            >
              Retry loading liveries
            </button>
          )}
        </div>
      )}
      <div className="file-toolbar">
        <label>
          Project
          <AppSelect
            value={project}
            disabled={disabled || busy}
            onChange={(e) => {
              setProject(e.target.value);
              setPage(0);
            }}
          >
            <option value="">All projects</option>
            {[
              ...new Map(
                rows.map((r) => [r.projectId, r.projectName]),
              ).entries(),
            ].map(([id, name]) => (
              <option key={id} value={id}>
                {name}
              </option>
            ))}
          </AppSelect>
        </label>
        <label>
          Search
          <input
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setPage(0);
            }}
            placeholder="Search liveries"
          />
        </label>
      </div>
      <div className="button-row">
        <button
          className="secondary"
          disabled={disabled || busy || !filtered.length}
          onClick={() => void choose(filtered.map((r) => r.id))}
        >
          Select all filtered
        </button>
        <span>{value.length} / 20 selected</span>
      </div>
      <ul className="post-choice-list">
        {filtered.slice(page * 20, page * 20 + 20).map((r) => (
          <li key={r.id}>
            <label>
              <input
                type="checkbox"
                checked={selected.has(r.id)}
                disabled={
                  disabled ||
                  busy ||
                  (!selected.has(r.id) && value.length >= 20)
                }
                onChange={(e) => void choose([r.id], !e.target.checked)}
              />
              <span>
                <strong>{r.name}</strong>
                <small>
                  {r.projectName} · {vehicleName(r.vehicleId)} · {r.uploaded}/
                  {r.textures} uploaded
                </small>
              </span>
            </label>
          </li>
        ))}
      </ul>
      {!filtered.length && <p>No matching liveries.</p>}
      {filtered.length > 20 && (
        <div className="button-row">
          <button disabled={!page} onClick={() => setPage((p) => p - 1)}>
            Previous
          </button>
          <span>
            {page + 1} / {Math.ceil(filtered.length / 20)}
          </span>
          <button
            disabled={(page + 1) * 20 >= filtered.length}
            onClick={() => setPage((p) => p + 1)}
          >
            Next
          </button>
        </div>
      )}
    </section>
  );
}
