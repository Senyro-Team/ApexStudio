import { useEffect, useState, type DragEvent, type KeyboardEvent } from "react";
import type { useStatistics } from "./Statistics";

type Stats = ReturnType<typeof useStatistics>;
export const dashboardWidgetOptions = [
  { id: "robux", label: "Robux revenue" },
  { id: "sales", label: "All-time sales" },
  { id: "clock", label: "Live clock" },
  { id: "views", label: "Views" },
  { id: "lastPosts", label: "Latest posts" },
  { id: "likes", label: "Likes" },
  { id: "copies", label: "Copies" },
  { id: "topPosts", label: "Top posts" },
  { id: "date", label: "Date" },
  { id: "shortcuts", label: "Shortcuts" },
] as const;
export type DashboardWidgetId = (typeof dashboardWidgetOptions)[number]["id"];
export type DashboardLayout = Partial<Record<DashboardWidgetId, number>>;
const storageKey = "apex-studio-dashboard-widgets";
const columns = 4;
const cells = 16;
const defaults: DashboardLayout = {
  clock: 0,
  date: 1,
  shortcuts: 2,
  views: 4,
  likes: 5,
  copies: 6,
  lastPosts: 8,
  topPosts: 9,
};

export function loadDashboardLayout(): DashboardLayout {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "null");
    if (!saved || typeof saved !== "object" || Array.isArray(saved)) return defaults;
    const result: DashboardLayout = {};
    const used = new Set<number>();
    for (const option of dashboardWidgetOptions) {
      const position = (saved as Record<string, unknown>)[option.id];
      if (typeof position === "number" && Number.isInteger(position) && position >= 0 && position < cells && !used.has(position)) {
        result[option.id] = position;
        used.add(position);
      }
    }
    return result;
  } catch {
    return defaults;
  }
}

export function saveDashboardLayout(layout: DashboardLayout) {
  localStorage.setItem(storageKey, JSON.stringify(layout));
}

function useNow() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}

const number = (value: number | null | undefined) =>
  value == null ? "—" : new Intl.NumberFormat().format(value);

function metricTotal(stats: Stats, key: "views" | "likes" | "copies") {
  const posts = stats.history.at(-1)?.posts;
  if (!posts || posts.some((post) => post[key] == null)) return null;
  return posts.reduce((sum, post) => sum + (post[key] ?? 0), 0);
}

function WidgetContent({ id, stats, now, onNavigate }: {
  id: DashboardWidgetId;
  stats: Stats;
  now: Date;
  onNavigate?: (section: "Projects" | "Notes" | "Statistics") => void;
}) {
  const posts = stats.history.at(-1)?.posts ?? [];
  if (id === "clock") return <strong className="dashboard-widget-value">{now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</strong>;
  if (id === "date") return <strong className="dashboard-widget-date">{now.toLocaleDateString([], { weekday: "long", year: "numeric", month: "long", day: "numeric" })}</strong>;
  if (id === "shortcuts") return <div className="dashboard-shortcuts">{(["Projects", "Notes", "Statistics"] as const).map((section) => <button className="secondary" key={section} onClick={(event) => { event.stopPropagation(); onNavigate?.(section); }} disabled={!onNavigate}>{section}</button>)}</div>;
  if (id === "robux" || id === "sales") return <><strong className="dashboard-widget-value">{stats.sales ? `${id === "robux" ? "R$ " : ""}${number(id === "robux" ? stats.sales.data.netRobux : stats.sales.data.allTimeSales)}` : "—"}</strong><span className="field-hint">{stats.sales ? "Creator dashboard snapshot" : "Load sales in Statistics"}</span></>;
  if (id === "views" || id === "likes" || id === "copies") return <><strong className="dashboard-widget-value">{number(metricTotal(stats, id))}</strong><span className="field-hint">{stats.history.length ? "Public posts" : "Refresh in Statistics"}</span></>;
  const ranked = id === "topPosts"
    ? [...posts].sort((a, b) => (b.views ?? -1) - (a.views ?? -1))
    : [...posts].sort((a, b) => b.bundleId - a.bundleId);
  return ranked.length ? <ol className="dashboard-posts">{ranked.slice(0, 3).map((post) => <li key={post.bundleId}><span title={post.title}>{post.title}</span><b>{id === "topPosts" ? number(post.views) : `#${post.bundleId}`}</b></li>)}</ol> : <p className="field-hint">Refresh public posts in Statistics.</p>;
}

function WidgetCard({ id, stats, now, onNavigate }: {
  id: DashboardWidgetId;
  stats: Stats;
  now: Date;
  onNavigate?: (section: "Projects" | "Notes" | "Statistics") => void;
}) {
  return <div className="dashboard-widget-card"><h3>{dashboardWidgetOptions.find((option) => option.id === id)?.label}</h3><WidgetContent id={id} stats={stats} now={now} onNavigate={onNavigate} /></div>;
}

export function DashboardWidgets({ layout, stats, onNavigate }: {
  layout: DashboardLayout;
  stats: Stats;
  onNavigate: (section: "Projects" | "Notes" | "Statistics") => void;
}) {
  const now = useNow();
  const placed = dashboardWidgetOptions.filter((option) => layout[option.id] !== undefined).sort((a, b) => layout[a.id]! - layout[b.id]!);
  if (!placed.length) return null;
  return <section className="dashboard-widgets" aria-label="Dashboard widgets"><div className="dashboard-widgets-heading"><h2>My widgets</h2><span className="field-hint">Arrange widgets in Settings</span></div><div className="dashboard-widget-grid">{placed.map(({ id }) => <div key={id} className="dashboard-widget" style={{ gridColumn: (layout[id]! % columns) + 1, gridRow: Math.floor(layout[id]! / columns) + 1 }}><WidgetCard id={id} stats={stats} now={now} onNavigate={onNavigate} /></div>)}</div></section>;
}

export function DashboardWidgetEditor({ layout, stats, onConfirm, onCancel }: {
  layout: DashboardLayout;
  stats: Stats;
  onConfirm: (layout: DashboardLayout) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState<DashboardLayout>(() => ({ ...layout }));
  const [selected, setSelected] = useState<DashboardWidgetId | null>(null);
  const [hint, setHint] = useState("Choose a widget, then choose a grid cell. You can also drag widgets.");
  const now = useNow();
  const at = (index: number) => dashboardWidgetOptions.find(({ id }) => draft[id] === index)?.id;
  function place(id: DashboardWidgetId, index: number) {
    const occupant = at(index);
    const previous = draft[id];
    if (occupant && occupant !== id && previous === undefined) {
      setHint("Choose an empty cell, or move a placed widget first.");
      return;
    }
    setDraft((current) => {
      const next = { ...current, [id]: index };
      if (occupant && occupant !== id && previous !== undefined) next[occupant] = previous;
      return next;
    });
    setSelected(null);
    setHint(`${dashboardWidgetOptions.find((option) => option.id === id)?.label} placed. Confirm to save your dashboard.`);
  }
  function drop(event: DragEvent<HTMLDivElement>, index: number) {
    event.preventDefault();
    const id = event.dataTransfer.getData("text/plain");
    if (dashboardWidgetOptions.some((option) => option.id === id)) place(id as DashboardWidgetId, index);
  }
  function keyPlace(event: KeyboardEvent<HTMLDivElement>, index: number) {
    if (event.target !== event.currentTarget) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    const occupant = at(index);
    if (selected) place(selected, index);
    else if (occupant) setSelected(occupant);
  }
  return <div className="dashboard-editor">
    <p>Pick widgets and place them on the 4 × 4 grid. Click a placed widget to move it, or drag it to another cell.</p>
    <div className="dashboard-widget-palette" aria-label="Available widgets">{dashboardWidgetOptions.map(({ id, label }) => <button key={id} className={`secondary ${selected === id ? "selected" : ""}`} aria-pressed={selected === id} draggable onDragStart={(event) => event.dataTransfer.setData("text/plain", id)} onClick={() => { setSelected(id); setHint(`Choose a grid cell for ${label}.`); }}>{label}{draft[id] !== undefined && <span aria-label="placed"> ✓</span>}</button>)}</div>
    <p className="field-hint" role="status">{hint}</p>
    <div className="dashboard-editor-grid" role="grid" aria-label="Dashboard layout">{Array.from({ length: cells }, (_, index) => {
      const id = at(index);
      return <div key={index} role="gridcell" tabIndex={0} draggable={!!id} className={`dashboard-editor-cell ${selected ? "can-place" : ""}`} aria-label={`Row ${Math.floor(index / columns) + 1}, column ${(index % columns) + 1}: ${id ? dashboardWidgetOptions.find((option) => option.id === id)?.label : "Empty"}`} onClick={() => { if (selected) place(selected, index); else if (id) setSelected(id); }} onKeyDown={(event) => keyPlace(event, index)} onDragStart={(event) => { if (id) event.dataTransfer.setData("text/plain", id); }} onDragOver={(event) => event.preventDefault()} onDrop={(event) => drop(event, index)}>{id ? <><WidgetCard id={id} stats={stats} now={now} /><button className="dashboard-widget-remove" aria-label={`Remove ${dashboardWidgetOptions.find((option) => option.id === id)?.label}`} onClick={(event) => { event.stopPropagation(); setDraft((current) => { const next = { ...current }; delete next[id]; return next; }); if (selected === id) setSelected(null); }}>×</button></> : <span className="dashboard-empty-cell">+ Add widget</span>}</div>;
    })}</div>
    <div className="dashboard-editor-actions"><button className="primary" onClick={() => onConfirm(draft)}>Confirm dashboard</button><button className="secondary" onClick={onCancel}>Cancel</button></div>
  </div>;
}
