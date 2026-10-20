import { useCallback, useEffect, useState, type FormEvent } from "react";
import { BridgeError, projectBridge } from "./bridge";
import type { CommissionDto, ProjectDto } from "./types";

type WorkspaceView = "projects" | "uploads" | "notes" | "analytics" | "settings";
const viewNames: Record<WorkspaceView, string> = { projects: "Projekte", uploads: "Uploads", notes: "Notizen", analytics: "Analysen", settings: "Einstellungen" };

export default function App() {
  const [view, setView] = useState<WorkspaceView>("projects");
  const [projects, setProjects] = useState<ProjectDto[]>([]);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [tagText, setTagText] = useState("");
  const [editing, setEditing] = useState<ProjectDto | null>(null);
  const [selectedProject, setSelectedProject] = useState<ProjectDto | null>(null);
  const [commissions, setCommissions] = useState<CommissionDto[]>([]);
  const [commissionsLoading, setCommissionsLoading] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setProjects(await projectBridge.list());
      setError(null);
    } catch (reason: unknown) {
      setError(reason instanceof BridgeError ? reason.message : "Projekte konnten nicht geladen werden.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    if (!selectedProject) return;
    let cancelled = false;
    setCommissionsLoading(true);
    projectBridge.listCommissions(selectedProject.id).then((items) => {
      if (!cancelled) setCommissions(items);
    }).catch((reason: unknown) => {
      if (!cancelled) setError(reason instanceof BridgeError ? reason.message : "Commissions konnten nicht geladen werden.");
    }).finally(() => {
      if (!cancelled) setCommissionsLoading(false);
    });
    return () => { cancelled = true; };
  }, [selectedProject?.id]);

  async function createProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const project = await projectBridge.create({ name, description: description.trim() || null, tags: parseTags(tagText) });
      setProjects((current) => [project, ...current]);
      setName("");
      setDescription("");
      setTagText("");
    } catch (reason: unknown) {
      setError(reason instanceof BridgeError ? reason.message : "Projekt konnte nicht angelegt werden.");
    } finally {
      setSaving(false);
    }
  }

  async function updateProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!editing) return;
    const data = new FormData(event.currentTarget);
    setSaving(true);
    setError(null);
    try {
      const updated = await projectBridge.update({
        id: editing.id,
        expected_revision: editing.revision,
        name: String(data.get("name") ?? ""),
        description: String(data.get("description") ?? "").trim() || null,
        tags: parseTags(String(data.get("tags") ?? "")),
      });
      setProjects((current) => current.map((project) => project.id === updated.id ? updated : project));
      setEditing(null);
    } catch (reason: unknown) {
      setError(reason instanceof BridgeError ? reason.message : "Projekt konnte nicht gespeichert werden.");
    } finally {
      setSaving(false);
    }
  }

  async function archiveProject(project: ProjectDto) {
    setError(null);
    try {
      await projectBridge.archive({ id: project.id, expected_revision: project.revision });
      setProjects((current) => current.filter((item) => item.id !== project.id));
      if (selectedProject?.id === project.id) setSelectedProject(null);
    } catch (reason: unknown) {
      setError(reason instanceof BridgeError ? reason.message : "Projekt konnte nicht archiviert werden.");
    }
  }

  async function createCommission(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedProject) return;
    const form = event.currentTarget;
    const data = new FormData(form);
    setSaving(true);
    setError(null);
    try {
      const commission = await projectBridge.createCommission({
        project_id: selectedProject.id,
        name: String(data.get("commission-name") ?? ""),
        client_name: String(data.get("client") ?? "").trim() || null,
        organisation_name: String(data.get("organisation") ?? "").trim() || null,
        commission_type: String(data.get("type") ?? "").trim() || null,
        description: String(data.get("description") ?? "").trim() || null,
        requirements: String(data.get("requirements") ?? "").split(";").map((item) => item.trim()).filter(Boolean),
        due_date: String(data.get("due-date") ?? "") || null,
      });
      setCommissions((current) => [commission, ...current]);
      form.reset();
    } catch (reason: unknown) {
      setError(reason instanceof BridgeError ? reason.message : "Commission konnte nicht angelegt werden.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <main className="shell">
      <aside className="rail" aria-label="Hauptnavigation">
        <div className="brand-lockup"><div className="brand-mark" aria-hidden="true">A</div><div className="rail-label">APEX<br /><span>STUDIO</span></div></div>
        <div className="rail-divider" />
        <nav className="nav-list" aria-label="Bereiche">
          {(["projects", "uploads", "notes", "analytics", "settings"] as WorkspaceView[]).map((item) => <button key={item} className={`nav-item${view === item ? " active" : ""}`} aria-current={view === item ? "page" : undefined} onClick={() => { setView(item); setSelectedProject(null); }}><span className="nav-glyph" aria-hidden="true">{{ projects: "▦", uploads: "⇧", notes: "▤", analytics: "◌", settings: "⚙" }[item]}</span><span>{viewNames[item]}</span></button>)}
        </nav>
        <div className="rail-bottom"><span className="status-dot" /> LOKALER WORKSPACE</div>
      </aside>

      <section className="content">
        <header className="topbar">
          <div><span className="eyebrow">APEX STUDIO <span className="slash">/</span> WORKSPACE</span><h1>{selectedProject?.name ?? viewNames[view]}</h1></div>
          <div className="local-badge"><i /> Offline verfügbar</div>
        </header>

        {view === "projects" && <section className="intro">
          <div><p className="eyebrow">{selectedProject ? selectedProject.name : "DEIN ARBEITSBEREICH"}</p><h2>{selectedProject ? "Commissions verwalten." : "Livery-Arbeit beginnt hier."}</h2><p className="muted">{selectedProject ? "Aufträge, Anforderungen und Termine für dieses Projekt." : "Lege Projekte und Commissions lokal an und verwalte ihre Metadaten."}</p></div>
          <span className="intro-glyph" aria-hidden="true">A</span>
        </section>}

        {view === "projects" ? <section className="project-area">
          {selectedProject ? <>
            <div className="section-heading"><div><button className="back-link" onClick={() => setSelectedProject(null)}>← Alle Projekte</button><p className="eyebrow">{selectedProject.name}</p><h2>Commissions <span className="count">{commissions.length.toString().padStart(2, "0")}</span></h2></div><span className="storage-note">PROJEKT · {selectedProject.id.slice(0, 8)}</span></div>
            <form className="create-form commission-form" onSubmit={createCommission}>
              <label htmlFor="commission-name">Neue Commission</label>
              <div className="form-row"><input id="commission-name" name="commission-name" placeholder="Zum Beispiel: Winterflotte 2026" maxLength={160} required disabled={saving} /><button type="submit" disabled={saving}>{saving ? "Wird angelegt …" : "Commission anlegen"}<span aria-hidden="true">↗</span></button></div>
              <div className="metadata-row"><input name="client" placeholder="Client (optional)" /><input name="organisation" placeholder="Organisation (optional)" /><input name="type" placeholder="Typ (optional)" /></div>
              <div className="metadata-row"><input name="description" placeholder="Beschreibung (optional)" /><input name="due-date" type="date" aria-label="Fällig am (optional)" /><input name="requirements" placeholder="Anforderungen, getrennt mit Semikolon" /></div>
            </form>
            {error && <div className="message error" role="alert">{error}</div>}
            <div className="project-list" aria-live="polite">
              {commissionsLoading ? <p className="empty-state">Commissions werden geladen …</p> : commissions.length === 0 ? <div className="empty-state"><span className="empty-icon">＋</span><strong>Noch keine Commissions</strong><span>Lege den ersten Auftrag für dieses Projekt an.</span></div> : commissions.map((commission) => <article className="project-card commission-card" key={commission.id}><div className="project-icon">C</div><div className="project-info"><strong>{commission.display_id} · {commission.name}</strong><span>{commission.due_date ? `Fällig am ${new Date(`${commission.due_date}T00:00:00`).toLocaleDateString("de-DE")}` : "Kein Fälligkeitsdatum"} · Status: {commission.status}</span></div><span className="project-arrow">↗</span></article>)}
            </div>
          </> : <>
          <div className="section-heading"><div><p className="eyebrow">ÜBERSICHT</p><h2>Deine Projekte <span className="count">{projects.length.toString().padStart(2, "0")}</span></h2></div><span className="storage-note">LOKALE DATENBANK · OFFLINE</span></div>
          <form className="create-form" onSubmit={createProject}>
            <label htmlFor="project-name">Neues Projekt</label>
            <div className="form-row"><input id="project-name" maxLength={120} value={name} onChange={(event) => setName(event.target.value)} placeholder="Zum Beispiel: NYPD Winterflotte" disabled={saving} /><button type="submit" disabled={saving || !name.trim()}>{saving ? "Wird angelegt …" : "Projekt anlegen"}<span aria-hidden="true">↗</span></button></div>
            <div className="metadata-row"><input aria-label="Projektbeschreibung" value={description} onChange={(event) => setDescription(event.target.value)} placeholder="Beschreibung (optional)" maxLength={10000} disabled={saving} /><input aria-label="Projekt-Tags" value={tagText} onChange={(event) => setTagText(event.target.value)} placeholder="Tags, durch Kommas getrennt" disabled={saving} /></div>
          </form>
          {error && <div className="message error" role="alert">{error}</div>}
          <div className="project-list" aria-live="polite">
            {loading ? <p className="empty-state">Projekte werden geladen …</p> : projects.length === 0 ? <div className="empty-state"><span className="empty-icon">＋</span><strong>Noch keine Projekte</strong><span>Dein erstes Projekt erscheint hier.</span></div> : projects.map((project) => <article className="project-card" key={project.id}>
              <div className="project-icon">A</div>
              <div className="project-info"><strong>{project.name}</strong><span>{project.description || `Erstellt am ${new Date(project.created_at).toLocaleDateString("de-DE")}`}</span><div className="tag-list">{project.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div></div>
              <button className="card-action open-action" onClick={() => setSelectedProject(project)}>Commissions</button>
              <button className="card-action" onClick={() => setEditing(project)}>Bearbeiten</button>
              <button className="card-action archive-action" onClick={() => void archiveProject(project)}>Archivieren</button>
              {editing?.id === project.id && <form className="edit-form" onSubmit={updateProject}>
                <input name="name" aria-label="Projektname" defaultValue={project.name} maxLength={120} required />
                <input name="description" aria-label="Projektbeschreibung" defaultValue={project.description ?? ""} maxLength={10000} />
                <input name="tags" aria-label="Projekt-Tags" defaultValue={project.tags.join(", ")} />
                <button type="submit" disabled={saving}>Speichern · Revision {project.revision}</button>
                <button type="button" onClick={() => setEditing(null)}>Abbrechen</button>
              </form>}
            </article>)}
          </div>
          </>}
        </section> : <section className="workspace-panel" aria-live="polite">
          <p className="eyebrow">{viewNames[view].toLocaleUpperCase("de-DE")}</p>
          <h2>{view === "uploads" ? "Roblox ist noch nicht verbunden." : view === "notes" ? "Notizen folgen in Phase 10." : view === "analytics" ? "Noch keine lokalen Kennzahlen." : "Dein lokaler Arbeitsbereich."}</h2>
          <p className="muted">{view === "uploads" ? "Der Open-Cloud-Adapter ist im Backend vorhanden. Profilverwaltung und Queuebedienung folgen; aktuell ist kein Creatorprofil eingerichtet und Apex Studio sendet keine Roblox-Anfragen." : view === "notes" ? "Notizdateien und eine sichere Vorschau werden später ergänzt. Deine Projektdaten bleiben offline verfügbar." : view === "analytics" ? "Analysen erscheinen, sobald lokale Upload- und Projektdaten verfügbar sind. Es werden keine Beispieldaten als echte Werte angezeigt." : "Apex Studio arbeitet lokal. Konten, Supportlinks und externe Dienste werden erst nach einer ausdrücklichen Konfiguration aktiviert."}</p>
          <div className="workspace-state"><span className="status-dot" /> {view === "uploads" ? "Keine Verbindung · kein Upload aktiv" : "Offline verfügbar · keine externen Anfragen"}</div>
        </section>}
        <footer><span>Apex Studio <span className="muted">· Lokaler Workspace</span></span><span>Deine Daten bleiben lokal.</span></footer>
      </section>
    </main>
  );
}

function parseTags(value: string): string[] {
  return value.split(",").map((tag) => tag.trim()).filter(Boolean);
}
