import AssetPreview from "./AssetPreview";
import AssetBrowser from "./AssetBrowser";
import { lazy, Suspense } from "react";
const Notes = lazy(() => import("./Notes"));
const Publishing = lazy(() => import("./Publishing"));
import { ArchiveImport, ProjectExport } from "./Packages";
import { useEffect, useState, type FormEvent } from "react";
import {
  ArrowLeft,
  ArrowRight,
  File,
  FolderOpen,
  MoreHorizontal,
  X,
  Search,
} from "lucide-react";
import {
  desktopAvailable,
  listProjects,
  saveProject,
  archiveProject,
  listAssets,
  importAssets,
  saveAssetCopy,
  vehicleCatalogue,
  errorMessage,
  type RobloxFocus,
  type Project,
  type ProjectFields,
  type Asset,
  type Catalogue,
} from "./bridge";

const blank: ProjectFields = {
  name: "",
  description: "",
  tags: [],
  vehicles: [],
  referenceLinks: [],
  deadline: null,
};
const fieldsOf = (project: Project | null): ProjectFields =>
  project
    ? {
        name: project.name,
        description: project.description,
        tags: project.tags,
        vehicles: project.vehicles,
        referenceLinks: project.referenceLinks,
        deadline: project.deadline,
      }
    : blank;
const date = (value: string) =>
  new Date(value).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
const deadlineDate = (value: string) =>
  new Date(`${value}T12:00:00`).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
const size = (bytes: number) =>
  bytes < 1024
    ? `${bytes} B`
    : bytes < 1024 ** 2
      ? `${(bytes / 1024).toFixed(1)} KB`
      : `${(bytes / 1024 ** 2).toFixed(1)} MB`;

function Editor({
  project,
  catalogue,
  onSave,
  onCancel,
}: {
  project: Project | null;
  catalogue: Catalogue | null;
  onSave: (project: Project) => void;
  onCancel: () => void;
}) {
  const [fields, setFields] = useState<ProjectFields>(() => fieldsOf(project));
  const [tags, setTags] = useState(fields.tags.join(", "));
  const [links, setLinks] = useState(fields.referenceLinks.join("\n"));
  const [vehicleSearch, setVehicleSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [discard, setDiscard] = useState(false);
  const dirty =
    JSON.stringify(fields) !== JSON.stringify(fieldsOf(project)) ||
    tags !== (project?.tags ?? []).join(", ") ||
    links !== (project?.referenceLinks ?? []).join("\n");
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    setBusy(true);
    try {
      onSave(
        await saveProject(project?.id ?? null, {
          ...fields,
          tags: tags
            .split(",")
            .map((value) => value.trim())
            .filter(Boolean),
          referenceLinks: links
            .split("\n")
            .map((value) => value.trim())
            .filter(Boolean),
        }),
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="project-editor">
      <button
        className="text-button"
        disabled={busy}
        onClick={() => (dirty ? setDiscard(true) : onCancel())}
      >
        <ArrowLeft size={16} /> Back
      </button>
      <header className="page-heading">
        <h1>{project ? "Edit project" : "New project"}</h1>
      </header>
      {discard && (
        <div className="confirm-panel">
          <p>Discard your unsaved changes?</p>
          <button className="secondary" onClick={onCancel}>
            Discard changes
          </button>
          <button className="text-button" onClick={() => setDiscard(false)}>
            Keep editing
          </button>
        </div>
      )}
      <form onSubmit={(event) => void submit(event)}>
        <fieldset disabled={busy}>
          <label htmlFor="project-name">Project name</label>
          <input
            id="project-name"
            required
            maxLength={120}
            value={fields.name}
            onChange={(e) => setFields({ ...fields, name: e.target.value })}
            autoFocus
          />
          <label htmlFor="project-description">
            Description <span className="optional">(optional)</span>
          </label>
          <textarea
            id="project-description"
            rows={4}
            maxLength={20000}
            value={fields.description}
            onChange={(e) =>
              setFields({ ...fields, description: e.target.value })
            }
          />
          <div className="form-columns">
            <div>
              <label htmlFor="project-tags">
                Tags <span className="optional">(optional)</span>
              </label>
              <input
                id="project-tags"
                value={tags}
                onChange={(e) => setTags(e.target.value)}
              />
              <p className="field-hint">Separate tags with commas.</p>
            </div>
            <div>
              <label htmlFor="project-deadline">
                Deadline <span className="optional">(optional)</span>
              </label>
              <input
                type="date"
                id="project-deadline"
                value={fields.deadline ?? ""}
                onChange={(e) =>
                  setFields({ ...fields, deadline: e.target.value || null })
                }
              />
            </div>
          </div>
          <label htmlFor="vehicle-search">
            Vehicle models <span className="optional">(optional)</span>
          </label>
          <div className="selected-vehicles">
            {fields.vehicles.map((name) => (
              <button
                type="button"
                className="vehicle-chip"
                key={name}
                aria-label={`Remove ${name}`}
                onClick={() =>
                  setFields({
                    ...fields,
                    vehicles: fields.vehicles.filter((value) => value !== name),
                  })
                }
              >
                {name}
                <X size={13} />
              </button>
            ))}
          </div>
          <input
            id="vehicle-search"
            placeholder="Search ER:LC vehicle names"
            value={vehicleSearch}
            onChange={(e) => setVehicleSearch(e.target.value)}
          />
          {vehicleSearch.trim() && (
            <ul className="vehicle-results" aria-label="Vehicle suggestions">
              {(catalogue?.vehicles ?? [])
                .filter(
                  (vehicle) =>
                    vehicle.name
                      .toLowerCase()
                      .includes(vehicleSearch.toLowerCase()) &&
                    !fields.vehicles.includes(vehicle.name),
                )
                .slice(0, 12)
                .map((vehicle) => (
                  <li key={vehicle.id}>
                    <button
                      type="button"
                      onClick={() => {
                        setFields({
                          ...fields,
                          vehicles: [...fields.vehicles, vehicle.name],
                        });
                        setVehicleSearch("");
                      }}
                    >
                      {vehicle.name}
                      <small>
                        {vehicle.team} /{" "}
                        {vehicle.bodyStatus ?? "Status unspecified"}
                      </small>
                    </button>
                  </li>
                ))}
              {catalogue &&
                !catalogue.vehicles.some(
                  (vehicle) =>
                    vehicle.name
                      .toLowerCase()
                      .includes(vehicleSearch.toLowerCase()) &&
                    !fields.vehicles.includes(vehicle.name),
                ) && <li>No matching vehicles in this snapshot.</li>}
            </ul>
          )}
          <p className="field-hint">
            Official template catalogue: October 7, 2026.
          </p>
          <label htmlFor="reference-links">
            Reference links <span className="optional">(optional)</span>
          </label>
          <textarea
            id="reference-links"
            rows={3}
            placeholder="One https:// link per line"
            value={links}
            onChange={(e) => setLinks(e.target.value)}
          />
        </fieldset>
        {error && (
          <p role="alert" className="inline-error">
            {error}
          </p>
        )}
        <div className="form-actions">
          <button className="primary" disabled={busy || !fields.name.trim()}>
            {busy ? "Saving…" : "Save project"}
          </button>
          <span className="muted">{dirty ? "Unsaved changes" : ""}</span>
        </div>
      </form>
    </section>
  );
}

export default function Projects({
  section,
  onNavigate,
  onEditingChange,
  onOpenRoblox,
}: {
  onOpenRoblox?: (focus: RobloxFocus) => void;
  section: string;
  onNavigate: (section: "Dashboard" | "Projects" | "Assets") => void;
  onEditingChange: (editing: boolean) => void;
}) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [catalogue, setCatalogue] = useState<Catalogue | null>(null);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [assetToOpen, setAssetToOpen] = useState<string | null>(null);
  const [editor, setEditor] = useState<Project | "new" | null>(null);
  const [tab, setTab] = useState("Assets");
  const [query, setQuery] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [menu, setMenu] = useState(false);
  const [confirmArchive, setConfirmArchive] = useState(false);
  const [inspector, setInspector] = useState<Asset | null>(null);
  const [loading, setLoading] = useState(desktopAvailable);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [noteDirty, setNoteDirty] = useState(false);
  useEffect(() => {
    onEditingChange(Boolean(editor) || busy || noteDirty);
  }, [editor, busy, noteDirty, onEditingChange]);
  async function refresh() {
    setError("");
    setLoading(true);
    try {
      const [items, vehicles] = await Promise.all([
        listProjects(),
        vehicleCatalogue(),
      ]);
      setProjects(items);
      setCatalogue(vehicles);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    if (desktopAvailable) void refresh();
  }, []);
  useEffect(() => {
    if (section !== "Projects") {
      setSelected(null);
      setMenu(false);
      setConfirmArchive(false);
    }
    setQuery("");
    setInspector(null);
  }, [section]);
  useEffect(() => {
    if (!desktopAvailable) return;
    let cancelled = false;
    setAssets([]);
    if (section === "Assets") {
      setLoading(true);
      listAssets(null)
        .then((items) => {
          if (!cancelled) setAssets(items);
        })
        .catch((error) => {
          if (!cancelled) setError(errorMessage(error));
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }
    return () => {
      cancelled = true;
    };
  }, [selected, section]);
  if (!["Dashboard", "Projects", "Assets"].includes(section)) return null;
  const project = projects.find((project) => project.id === selected);
  const open = (id: string, assetId?: string) => {
    setAssetToOpen(assetId ?? null);
    setSelected(id);
    setEditor(null);
    setTab("Assets");
    setNotice("");
    setError("");
    onNavigate("Projects");
  };
  const create = () => {
    setSelected(null);
    setEditor("new");
    setError("");
    onNavigate("Projects");
  };
  const saved = (item: Project) => {
    setProjects((items) => [
      item,
      ...items.filter((existing) => existing.id !== item.id),
    ]);
    setEditor(null);
    setSelected(item.id);
    setTab("Assets");
    setNotice("Project saved.");
  };
  async function archive() {
    if (!project) return;
    setBusy(true);
    setError("");
    try {
      const item = await archiveProject(project.id, !project.archived);
      setProjects((items) =>
        items.map((existing) => (existing.id === item.id ? item : existing)),
      );
      setConfirmArchive(false);
      setMenu(false);
      setNotice(
        item.archived
          ? "Project archived. Files are preserved."
          : "Project restored.",
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  async function importFiles() {
    if (!project) return;
    setBusy(true);
    setNotice("");
    setError("");
    try {
      const report = await importAssets(project.id);
      if (report) {
        setAssets(report.imported);
        setProjects(await listProjects());
        setNotice(
          `Import finished. ${report.skipped.length} duplicate file(s) skipped.`,
        );
        if (report.failed.length) setError(report.failed.join("\n"));
      }
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  async function copy() {
    if (!inspector) return;
    setBusy(true);
    try {
      if (await saveAssetCopy(inspector.id))
        setNotice("A copy was saved. The managed original is unchanged.");
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  const rows = (items: Project[]) => (
    <div className="project-list">
      {items.map((item) => (
        <button
          className="project-row"
          key={item.id}
          onClick={() => open(item.id)}
        >
          <FolderOpen size={24} aria-hidden="true" />
          <span>
            <strong>{item.name}</strong>
            <small>
              {item.assetCount} files · Edited {date(item.updatedAt)}
              {item.archived ? " · Archived" : ""}
            </small>
          </span>
          <ArrowRight size={18} aria-hidden="true" />
        </button>
      ))}
    </div>
  );
  const active = projects.filter((project) => !project.archived);
  const deadlines = active
    .filter((project) => project.deadline)
    .sort((a, b) => a.deadline!.localeCompare(b.deadline!))
    .slice(0, 4);
  const filtered = projects.filter(
    (item) =>
      item.archived === showArchived &&
      [item.name, item.description, ...item.tags, ...item.vehicles].some(
        (value) => value.toLowerCase().includes(query.toLowerCase()),
      ),
  );
  const matchingAssets = assets.filter((asset) =>
    asset.name.toLowerCase().includes(query.toLowerCase()),
  );
  return (
    <div className="projects-area">
      {error && (
        <div className="error" role="alert">
          <p className="pre-wrap">{error}</p>
          <button onClick={() => void refresh()}>Refresh</button>
        </div>
      )}
      {notice && (
        <p className="operation-status" role="status">
          {notice}
        </p>
      )}
      {editor && section === "Projects" ? (
        <Editor
          key={editor === "new" ? "new" : editor.id}
          project={editor === "new" ? null : editor}
          catalogue={catalogue}
          onSave={saved}
          onCancel={() => setEditor(null)}
        />
      ) : section === "Dashboard" ? (
        <>
          <header className="page-heading heading-actions">
            <div>
              <h1>Dashboard</h1>
            </div>
            <button
              className="primary"
              disabled={!desktopAvailable || loading || busy}
              onClick={create}
            >
              New project
            </button>
          </header>
          <section className="content-section">
            <h2>Recent projects</h2>
            {loading ? (
              <p role="status">Opening projects…</p>
            ) : active.length ? (
              rows(active.slice(0, 3))
            ) : (
              <div className="empty-state">
                <FolderOpen size={26} />
                <h3>No projects</h3>

                <button
                  className="text-button"
                  disabled={!desktopAvailable}
                  onClick={create}
                >
                  Create your first project <ArrowRight size={17} />
                </button>
              </div>
            )}
            {active.length > 3 && (
              <button
                className="text-button"
                onClick={() => onNavigate("Projects")}
              >
                View all projects <ArrowRight size={16} />
              </button>
            )}
          </section>
          <section className="content-section deadlines">
            <h2>Upcoming deadlines</h2>
            {deadlines.length ? (
              <div>
                {deadlines.map((item) => (
                  <button
                    className="deadline-row"
                    key={item.id}
                    onClick={() => open(item.id)}
                  >
                    <time dateTime={item.deadline!}>
                      {deadlineDate(item.deadline!)}
                    </time>
                    <span>{item.name}</span>
                  </button>
                ))}
              </div>
            ) : (
              <p>No upcoming deadlines.</p>
            )}
          </section>
        </>
      ) : section === "Projects" && project ? (
        <>
          <button
            className="text-button back-button"
            disabled={busy || noteDirty}
            onClick={() => {
              setSelected(null);
              setInspector(null);
              setNotice("");
            }}
          >
            <ArrowLeft size={16} /> All projects
          </button>
          <header className="page-heading heading-actions">
            <div>
              <h1>{project.name}</h1>
              {project.description && <p>{project.description}</p>}
              {project.deadline && (
                <p className="project-deadline">
                  Due {deadlineDate(project.deadline)}
                </p>
              )}
              {project.archived && (
                <p className="phase-note">
                  Archived · Files are preserved. Restore to edit.
                </p>
              )}
            </div>
            <div className="project-actions">
              {!noteDirty && <ProjectExport id={project.id} onBusy={setBusy} />}
              <button
                className="icon-button"
                aria-label="Project actions"
                disabled={noteDirty}
                aria-expanded={menu}
                onClick={() => setMenu(!menu)}
              >
                <MoreHorizontal size={21} />
              </button>
              {menu && (
                <div className="action-menu">
                  <button
                    disabled={project.archived || busy}
                    onClick={() => {
                      setEditor(project);
                      setMenu(false);
                    }}
                  >
                    Edit project
                  </button>
                  <button
                    disabled={busy}
                    onClick={() => {
                      setConfirmArchive(true);
                      setMenu(false);
                    }}
                  >
                    {project.archived ? "Restore project" : "Archive project"}
                  </button>
                </div>
              )}
            </div>
          </header>
          {confirmArchive && (
            <div className="confirm-panel">
              <p>
                {project.archived
                  ? "Restore this project?"
                  : "Archive this project? Its files will be preserved."}
              </p>
              <button
                className="secondary"
                disabled={busy}
                onClick={() => void archive()}
              >
                {project.archived ? "Restore project" : "Archive project"}
              </button>
              <button
                className="text-button"
                onClick={() => setConfirmArchive(false)}
              >
                Cancel
              </button>
            </div>
          )}
          <div
            className="project-tabs"
            role="group"
            aria-label="Project sections"
          >
            {["Assets", "Notes", "ERLCX", "Details"].map((item) => (
              <button
                key={item}
                disabled={noteDirty && tab !== item}
                aria-pressed={tab === item}
                onClick={() => {
                  setTab(item);
                  setInspector(null);
                  setQuery("");
                }}
              >
                {item}
              </button>
            ))}
          </div>
          {tab === "Notes" ? (
            <Suspense fallback={<p role="status">Loading editor…</p>}>
              <Notes
                projectId={project.id}
                onEditingChange={setNoteDirty}
                readOnly={project.archived}
              />
            </Suspense>
          ) : tab === "Assets" ? (
            <AssetBrowser
              project={project}
              initialAssetId={assetToOpen}
              catalogue={catalogue}
              onOpenRoblox={(folderId) =>
                onOpenRoblox?.({ projectId: project.id, folderIds: [folderId] })
              }
              onBusy={setNoteDirty}
              onChange={() => void refresh()}
            />
          ) : tab === "ERLCX" ? (
            <Suspense fallback={<p role="status">Loading publishing draft…</p>}>
              <Publishing
                project={project}
                catalogue={catalogue}
                onOpenRoblox={(folderId) =>
                  onOpenRoblox?.({
                    projectId: project.id,
                    folderIds: [folderId],
                  })
                }
                onEditingChange={setNoteDirty}
              />
            </Suspense>
          ) : tab === "Details" ? (
            <section className="project-metadata">
              <h2>Project details</h2>
              <dl>
                <dt>Created</dt>
                <dd>{date(project.createdAt)}</dd>
                <dt>Tags</dt>
                <dd>{project.tags.join(", ") || "No tags"}</dd>
                <dt>Vehicles</dt>
                <dd>
                  {project.vehicles.length
                    ? project.vehicles.map((name) => (
                        <p key={name}>
                          {name} <small>· Livery support unverified</small>
                        </p>
                      ))
                    : "No vehicles selected"}
                </dd>
                <dt>Reference links</dt>
                <dd>
                  {project.referenceLinks.length
                    ? project.referenceLinks.map((link) => (
                        <p key={link}>
                          {link}{" "}
                          <button
                            className="text-button"
                            aria-label={`Copy link ${link}`}
                            onClick={() => {
                              void navigator.clipboard
                                .writeText(link)
                                .then(() => setNotice("Reference link copied."))
                                .catch(() =>
                                  setError(
                                    "Copy failed. Select and copy the displayed link manually.",
                                  ),
                                );
                            }}
                          >
                            Copy link
                          </button>
                        </p>
                      ))
                    : "No reference links"}
                </dd>
              </dl>
            </section>
          ) : null}
        </>
      ) : section === "Projects" ? (
        <>
          <header className="page-heading heading-actions">
            <div>
              <h1>Projects</h1>
              <ArchiveImport
                onComplete={() => void refresh()}
                onBusy={setBusy}
              />
            </div>
            <button
              className="primary"
              disabled={!desktopAvailable || loading || busy}
              onClick={create}
            >
              New project
            </button>
          </header>
          <div className="file-toolbar">
            <label className="search-field">
              <Search size={17} />
              <input
                aria-label="Search projects"
                placeholder="Search projects, tags, vehicles"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <button
              className="text-button"
              aria-pressed={showArchived}
              onClick={() => setShowArchived(!showArchived)}
            >
              {showArchived ? "Show active projects" : "Show archived projects"}
            </button>
          </div>
          {loading ? (
            <p role="status">Opening projects…</p>
          ) : filtered.length ? (
            rows(filtered)
          ) : (
            <div className="empty-state">
              <h2>
                {query
                  ? "No matching projects"
                  : showArchived
                    ? "No archived projects"
                    : "No projects"}
              </h2>
            </div>
          )}
        </>
      ) : (
        <>
          <header className="page-heading">
            <h1>Assets</h1>
          </header>
          <div className="file-toolbar">
            <label className="search-field">
              <Search size={17} />
              <input
                aria-label="Search assets"
                placeholder="Search files"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
          </div>
          {assets.length ? (
            <div className="asset-grid">
              {matchingAssets.map((asset) => (
                <button
                  key={asset.id}
                  className="asset-tile"
                  onClick={() => open(asset.projectId, asset.id)}
                >
                  <div className="asset-thumbnail">
                    {asset.preview ? (
                      <AssetPreview src={asset.preview} />
                    ) : (
                      <File size={32} />
                    )}
                  </div>
                  <strong>{asset.name}</strong>
                  <small>
                    {projects.find((project) => project.id === asset.projectId)
                      ?.name ?? "Project"}
                  </small>
                </button>
              ))}
            </div>
          ) : (
            <div className="empty-state">
              <h2>No imported files yet</h2>
              <p>Open a project to import files into your workspace.</p>
            </div>
          )}
        </>
      )}
      {inspector && (
        <aside className="file-inspector" aria-label="File details">
          <button
            className="icon-button close-inspector"
            aria-label="Close file details"
            onClick={() => setInspector(null)}
          >
            <X size={20} />
          </button>
          <h2>{inspector.name}</h2>
          {inspector.preview && (
            <img
              className="inspector-image"
              src={inspector.preview}
              alt={`Preview of ${inspector.name}`}
            />
          )}
          <dl>
            <dt>File size</dt>
            <dd>{size(inspector.size)}</dd>
            <dt>Imported</dt>
            <dd>{date(inspector.importedAt)}</dd>
            <dt>SHA-256</dt>
            <dd className="hash">{inspector.hash}</dd>
          </dl>
          <p className="field-hint">Your source file was preserved.</p>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void copy()}
          >
            Save a copy
          </button>
        </aside>
      )}
    </div>
  );
}
