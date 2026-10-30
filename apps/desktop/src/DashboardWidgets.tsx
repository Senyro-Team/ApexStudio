import { useEffect, useRef, useState, type DragEvent, type KeyboardEvent, type PointerEvent } from "react";
import { CalendarDays, Clock3, Copy, Eye, FileText, Heart, LayoutGrid, Plus, ShoppingBag, Trophy, Wallet, type LucideIcon } from "lucide-react";
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
export type WidgetDesign = "classic" | "glass" | "minimal" | "analog";
const generalDesigns = [
  { id: "classic", label: "Classic" },
  { id: "glass", label: "Glass" },
  { id: "minimal", label: "Minimal" },
] as const;
const clockDesigns = [
  { id: "classic", label: "Digital" },
  { id: "analog", label: "Round clock" },
  { id: "glass", label: "Glass" },
  { id: "minimal", label: "Minimal" },
] as const;
const widgetVisuals: Record<DashboardWidgetId, { icon: LucideIcon; description: string }> = {
  robux: { icon: Wallet, description: "Creator earnings" },
  sales: { icon: ShoppingBag, description: "Lifetime total" },
  clock: { icon: Clock3, description: "Time right now" },
  views: { icon: Eye, description: "Post reach" },
  lastPosts: { icon: FileText, description: "Recent uploads" },
  likes: { icon: Heart, description: "Audience reactions" },
  copies: { icon: Copy, description: "Saved copies" },
  topPosts: { icon: Trophy, description: "Most viewed" },
  date: { icon: CalendarDays, description: "Today at a glance" },
  shortcuts: { icon: LayoutGrid, description: "Quick navigation" },
};
export type WidgetPlacement = { column: number; row: number; width: number; height: number; design?: WidgetDesign };
export type DashboardLayout = Partial<Record<DashboardWidgetId, WidgetPlacement>>;
const storageKey = "apex-studio-dashboard-widgets";
const columns = 4;
const maxRows = 12;
const single = (index: number): WidgetPlacement => ({ column: index % columns, row: Math.floor(index / columns), width: 1, height: 1 });
const defaults: DashboardLayout = {
  clock: single(0),
  date: single(1),
  shortcuts: single(2),
  views: single(4),
  likes: single(5),
  copies: single(6),
  lastPosts: single(8),
  topPosts: single(9),
};

const validPlacement = (value: unknown): value is WidgetPlacement => {
  if (!value || typeof value !== "object") return false;
  const { column, row, width, height } = value as WidgetPlacement;
  return [column, row, width, height].every(Number.isInteger) &&
    column >= 0 && row >= 0 && width >= 1 && height >= 1 &&
    column + width <= columns && row + height <= maxRows;
};

function overlaps(a: WidgetPlacement, b: WidgetPlacement) {
  return a.column < b.column + b.width && a.column + a.width > b.column &&
    a.row < b.row + b.height && a.row + a.height > b.row;
}

function occupied(layout: DashboardLayout, placement: WidgetPlacement) {
  return dashboardWidgetOptions.some(({ id }) => layout[id] && overlaps(layout[id], placement));
}

function firstSpace(layout: DashboardLayout, width: number, height: number) {
  for (let row = 0; row <= maxRows - height; row++) {
    for (let column = 0; column <= columns - width; column++) {
      const placement = { column, row, width, height };
      if (!occupied(layout, placement)) return placement;
    }
  }
  return null;
}

function arrange(layout: DashboardLayout, id: DashboardWidgetId, placement: WidgetPlacement): DashboardLayout | null {
  const next = { ...layout };
  delete next[id];
  const displaced = dashboardWidgetOptions.filter(({ id: other }) => next[other] && overlaps(next[other], placement));
  for (const { id: other } of displaced) delete next[other];
  next[id] = placement;
  for (const { id: other } of displaced) {
    const original = layout[other]!;
    const space = firstSpace(next, original.width, original.height);
    if (!space) return null;
    next[other] = space;
  }
  return next;
}

export function loadDashboardLayout(): DashboardLayout {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "null");
    if (!saved || typeof saved !== "object" || Array.isArray(saved)) return defaults;
    const result: DashboardLayout = {};
    for (const option of dashboardWidgetOptions) {
      const value = (saved as Record<string, unknown>)[option.id];
      const placement = typeof value === "number" && Number.isInteger(value) && value >= 0 && value < columns * maxRows
        ? single(value)
        : value;
      if (validPlacement(placement) && !occupied(result, placement)) {
        const normalized = { ...placement };
        if (!normalized.design || !["classic", "glass", "minimal", "analog"].includes(normalized.design) || (option.id !== "clock" && normalized.design === "analog")) delete normalized.design;
        result[option.id] = normalized;
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

function AnalogClock({ now }: { now: Date }) {
  const hour = ((now.getHours() % 12) + now.getMinutes() / 60) * 30;
  const minute = (now.getMinutes() + now.getSeconds() / 60) * 6;
  const second = now.getSeconds() * 6;
  return <svg className="dashboard-analog-clock" viewBox="0 0 120 120" role="img" aria-label={`Round clock: ${now.toLocaleTimeString()}`}>
    <circle className="clock-dial" cx="60" cy="60" r="55" />
    {Array.from({ length: 12 }, (_, index) => <line key={index} className={index % 3 === 0 ? "clock-mark-major" : "clock-mark"} x1="60" y1="12" x2="60" y2={index % 3 === 0 ? "22" : "18"} transform={`rotate(${index * 30} 60 60)`} />)}
    <line className="clock-hand-hour" x1="60" y1="65" x2="60" y2="35" transform={`rotate(${hour} 60 60)`} />
    <line className="clock-hand-minute" x1="60" y1="67" x2="60" y2="25" transform={`rotate(${minute} 60 60)`} />
    <line className="clock-hand-second" x1="60" y1="70" x2="60" y2="19" transform={`rotate(${second} 60 60)`} />
    <circle className="clock-pin" cx="60" cy="60" r="4" />
  </svg>;
}

function WidgetContent({ id, design, stats, now, onNavigate }: {
  id: DashboardWidgetId;
  design: WidgetDesign;
  stats: Stats;
  now: Date;
  onNavigate?: (section: "Projects" | "Notes" | "Statistics") => void;
}) {
  const posts = stats.history.at(-1)?.posts ?? [];
  if (id === "clock" && design === "analog") return <AnalogClock now={now} />;
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

function WidgetCard({ id, design = "classic", stats, now, onNavigate }: {
  id: DashboardWidgetId;
  design?: WidgetDesign;
  stats: Stats;
  now: Date;
  onNavigate?: (section: "Projects" | "Notes" | "Statistics") => void;
}) {
  return <div className={`dashboard-widget-card design-${design}`}><h3>{dashboardWidgetOptions.find((option) => option.id === id)?.label}</h3><WidgetContent id={id} design={design} stats={stats} now={now} onNavigate={onNavigate} /></div>;
}

export function DashboardWidgets({ layout, stats, onNavigate, onEdit }: {
  layout: DashboardLayout;
  stats: Stats;
  onNavigate: (section: "Projects" | "Notes" | "Statistics") => void;
  onEdit: () => void;
}) {
  const now = useNow();
  const placed = dashboardWidgetOptions
    .filter(({ id }) => layout[id])
    .sort((a, b) => layout[a.id]!.row - layout[b.id]!.row || layout[a.id]!.column - layout[b.id]!.column);
  return <section className="dashboard-widgets" aria-label="Dashboard widgets">
    <div className="dashboard-widgets-heading"><h2>My widgets</h2><button className="secondary" onClick={onEdit}>Edit dashboard</button></div>
    {placed.length ? <div className="dashboard-widget-grid">{placed.map(({ id }) => {
      const placement = layout[id]!;
      return <div key={id} className="dashboard-widget" style={{ gridColumn: `${placement.column + 1} / span ${placement.width}`, gridRow: `${placement.row + 1} / span ${placement.height}` }}>
        <WidgetCard id={id} design={placement.design} stats={stats} now={now} onNavigate={onNavigate} />
      </div>;
    })}</div> : <p className="field-hint">No widgets yet. Add some with Edit dashboard.</p>}
  </section>;
}

export function DashboardWidgetEditor({ layout, stats, onConfirm, onCancel }: {
  layout: DashboardLayout;
  stats: Stats;
  onConfirm: (layout: DashboardLayout) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState<DashboardLayout>(() => ({ ...layout }));
  const [selected, setSelected] = useState<DashboardWidgetId | null>(null);
  const [pendingDesigns, setPendingDesigns] = useState<Partial<Record<DashboardWidgetId, WidgetDesign>>>({});
  const [dropTarget, setDropTarget] = useState<number | null>(null);
  const [hint, setHint] = useState("Drag a widget into the grid, or choose one and click a cell.");
  const resizeStart = useRef<{
    id: DashboardWidgetId;
    pointerId: number;
    x: number;
    y: number;
    stepX: number;
    stepY: number;
    placement: WidgetPlacement;
    layout: DashboardLayout;
    lastWidth: number;
    lastHeight: number;
  } | null>(null);
  const now = useNow();
  const rows = Math.min(maxRows, Math.max(4, ...Object.values(draft).filter((value): value is WidgetPlacement => !!value).map((value) => value.row + value.height + 1)));
  const at = (index: number) => dashboardWidgetOptions.find(({ id }) => {
    const placement = draft[id];
    return placement && overlaps(placement, single(index));
  })?.id;
  const designFor = (id: DashboardWidgetId): WidgetDesign => draft[id]?.design ?? pendingDesigns[id] ?? "classic";
  function chooseDesign(id: DashboardWidgetId, design: WidgetDesign) {
    setPendingDesigns((current) => ({ ...current, [id]: design }));
    if (draft[id]) setDraft((current) => {
      const placement = { ...current[id]! };
      if (design === "classic") delete placement.design;
      else placement.design = design;
      return { ...current, [id]: placement };
    });
    setHint(`${dashboardWidgetOptions.find((option) => option.id === id)?.label} design: ${design === "analog" ? "Round clock" : design}. Confirm to save.`);
  }
  function place(id: DashboardWidgetId, index: number) {
    const chosenDesign = pendingDesigns[id];
    const previous = draft[id] ?? { ...single(index), ...(chosenDesign && chosenDesign !== "classic" ? { design: chosenDesign } : {}) };
    const placement = { ...previous, column: Math.min(index % columns, columns - previous.width), row: Math.min(Math.floor(index / columns), maxRows - previous.height) };
    const next = arrange(draft, id, placement);
    if (!next) {
      setHint("No room for that widget. Remove or shrink another widget first.");
      return;
    }
    setDraft(next);
    setSelected(null);
    setDropTarget(null);
    setHint(`${dashboardWidgetOptions.find((option) => option.id === id)?.label} placed. Confirm to save your dashboard.`);
  }
  function resize(id: DashboardWidgetId, dimension: "width" | "height", value: number) {
    const current = draft[id];
    if (!current) return;
    const limit = dimension === "width" ? columns - current.column : Math.min(4, maxRows - current.row);
    const placement = { ...current, [dimension]: Math.max(1, Math.min(limit, value)) };
    if (placement[dimension] === current[dimension]) return;
    const next = arrange(draft, id, placement);
    if (!next) {
      setHint("No room for that size. Remove or shrink another widget first.");
      return;
    }
    setDraft(next);
    setHint(`${dashboardWidgetOptions.find((option) => option.id === id)?.label} is now ${placement.width} × ${placement.height}. Confirm to save.`);
  }
  function startResize(event: PointerEvent<HTMLButtonElement>, id: DashboardWidgetId) {
    const placement = draft[id];
    const grid = event.currentTarget.closest<HTMLElement>(".dashboard-editor-grid");
    if (!placement || !grid) return;
    event.currentTarget.focus();
    event.preventDefault();
    event.stopPropagation();
    const gap = Number.parseFloat(getComputedStyle(grid).columnGap) || 14;
    resizeStart.current = {
      id,
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      stepX: (grid.clientWidth - gap * (columns - 1)) / columns + gap,
      stepY: 200 + gap,
      placement,
      layout: draft,
      lastWidth: placement.width,
      lastHeight: placement.height,
    };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  }
  function moveResize(event: PointerEvent<HTMLButtonElement>) {
    const start = resizeStart.current;
    if (!start || start.pointerId !== event.pointerId) return;
    const width = Math.max(1, Math.min(columns - start.placement.column, start.placement.width + Math.round((event.clientX - start.x) / start.stepX)));
    const height = Math.max(1, Math.min(4, maxRows - start.placement.row, start.placement.height + Math.round((event.clientY - start.y) / start.stepY)));
    if (width === start.lastWidth && height === start.lastHeight) return;
    start.lastWidth = width;
    start.lastHeight = height;
    const next = arrange(start.layout, start.id, { ...start.placement, width, height });
    if (!next) {
      setHint("No room for that size. Remove or shrink another widget first.");
      return;
    }
    setDraft(next);
    setHint(`${dashboardWidgetOptions.find((option) => option.id === start.id)?.label} is now ${width} × ${height}. Confirm to save.`);
  }
  function endResize(event: PointerEvent<HTMLButtonElement>) {
    if (resizeStart.current?.pointerId !== event.pointerId) return;
    resizeStart.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }
  function keyResize(event: KeyboardEvent<HTMLButtonElement>, id: DashboardWidgetId) {
    const current = draft[id];
    if (!current) return;
    const change = {
      ArrowRight: ["width", 1],
      ArrowLeft: ["width", -1],
      ArrowDown: ["height", 1],
      ArrowUp: ["height", -1],
    }[event.key] as ["width" | "height", number] | undefined;
    if (!change) return;
    event.preventDefault();
    event.stopPropagation();
    resize(id, change[0], current[change[0]] + change[1]);
  }
  function drop(event: DragEvent<HTMLDivElement>, index: number) {
    event.preventDefault();
    setDropTarget(null);
    const id = event.dataTransfer.getData("text/plain");
    if (dashboardWidgetOptions.some((option) => option.id === id)) place(id as DashboardWidgetId, index);
  }
  function keyPlace(event: KeyboardEvent<HTMLDivElement>, index: number) {
    if (event.target !== event.currentTarget) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    if (selected) place(selected, index);
  }
  return <div className="dashboard-editor">
    <div className="dashboard-widget-picker">
      <div className="dashboard-widget-picker-heading"><span className="dashboard-widget-picker-symbol"><Plus size={20} aria-hidden="true" /></span><div><h2>Add widgets</h2><p>Drag a tile onto the dashboard, or select it and choose a cell.</p></div></div>
      <div className="dashboard-widget-palette" aria-label="Available widgets">{dashboardWidgetOptions.map(({ id, label }) => {
        const Icon = widgetVisuals[id].icon;
        return <button key={id} className={`dashboard-palette-card ${selected === id ? "selected" : ""}`} aria-label={label} aria-pressed={selected === id} draggable onDragStart={(event) => { event.dataTransfer.effectAllowed = "move"; event.dataTransfer.setData("text/plain", id); }} onDragEnd={() => setDropTarget(null)} onClick={() => { setSelected(id); setHint(`Choose a grid cell for ${label}.`); }}><span className="dashboard-palette-icon"><Icon size={23} aria-hidden="true" /></span><strong>{label}</strong><small>{widgetVisuals[id].description}</small><span className="dashboard-palette-state" aria-hidden="true">{draft[id] ? "On dashboard" : "Add widget"}</span></button>;
      })}</div>
      {selected && <div className="dashboard-design-picker" role="group" aria-label={`${dashboardWidgetOptions.find((option) => option.id === selected)?.label} designs`}>
        <div><h3>Choose a look</h3><p>{dashboardWidgetOptions.find((option) => option.id === selected)?.label} · Changes preview instantly</p></div>
        <div className="dashboard-design-options">{(selected === "clock" ? clockDesigns : generalDesigns).map(({ id: design, label }) => {
          const Icon = widgetVisuals[selected].icon;
          return <button key={design} className={`dashboard-design-option ${designFor(selected) === design ? "selected" : ""}`} aria-label={`${dashboardWidgetOptions.find((option) => option.id === selected)?.label} design: ${label}`} aria-pressed={designFor(selected) === design} onClick={() => chooseDesign(selected, design)}><span className={`dashboard-design-sample design-${design}`}>{design === "analog" ? <AnalogClock now={now} /> : <Icon size={24} aria-hidden="true" />}</span><span>{label}</span></button>;
        })}</div>
      </div>}
    </div>
    <div className="dashboard-layout-heading"><h2>Your layout</h2><span>Drag to move · pull the curved corner to resize · × to remove</span></div>
    <p className="field-hint" role="status">{hint}</p>
    <div className="dashboard-editor-grid" role="grid" aria-label="Dashboard layout" style={{ gridTemplateRows: `repeat(${rows}, 200px)` }}>
      {Array.from({ length: rows * columns }, (_, index) => {
        if (at(index)) return null;
        return <div key={index} role="gridcell" tabIndex={0} className={`dashboard-editor-cell ${selected ? "can-place" : ""} ${dropTarget === index ? "drop-target" : ""}`} style={{ gridColumn: index % columns + 1, gridRow: Math.floor(index / columns) + 1 }} aria-label={`Row ${Math.floor(index / columns) + 1}, column ${index % columns + 1}: Empty`} onClick={() => { if (selected) place(selected, index); }} onKeyDown={(event) => keyPlace(event, index)} onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = "move"; setDropTarget(index); }} onDragLeave={() => setDropTarget((current) => current === index ? null : current)} onDrop={(event) => drop(event, index)}><span className="dashboard-empty-cell">+ Add widget</span></div>;
      })}
      {dashboardWidgetOptions.filter(({ id }) => draft[id]).map(({ id, label }) => {
        const placement = draft[id]!;
        const index = placement.row * columns + placement.column;
        return <div key={id} role="gridcell" tabIndex={0} draggable className={`dashboard-editor-widget ${dropTarget === index ? "drop-target" : ""}`} style={{ gridColumn: `${placement.column + 1} / span ${placement.width}`, gridRow: `${placement.row + 1} / span ${placement.height}` }} aria-label={`Row ${placement.row + 1}, column ${placement.column + 1}: ${label}, ${placement.width} by ${placement.height}`} onClick={() => setSelected(id)} onKeyDown={(event) => { if (event.target === event.currentTarget && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); setSelected(id); } }} onDragStart={(event) => { event.dataTransfer.effectAllowed = "move"; event.dataTransfer.setData("text/plain", id); }} onDragEnd={() => setDropTarget(null)} onDragOver={(event) => { event.preventDefault(); event.dataTransfer.dropEffect = "move"; setDropTarget(index); }} onDragLeave={() => setDropTarget((current) => current === index ? null : current)} onDrop={(event) => drop(event, index)}>
          <WidgetCard id={id} design={placement.design} stats={stats} now={now} />
          <button type="button" className="dashboard-widget-remove" aria-label={`Remove ${label}`} onClick={(event) => { event.stopPropagation(); setDraft((current) => { const next = { ...current }; delete next[id]; return next; }); if (selected === id) setSelected(null); }}>×</button>
          <button type="button" className="dashboard-widget-resize" aria-label={`Drag to resize ${label}`} title="Drag corner to resize · arrow keys also work" draggable={false} onClick={(event) => event.stopPropagation()} onDragStart={(event) => event.preventDefault()} onPointerDown={(event) => startResize(event, id)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize} onKeyDown={(event) => keyResize(event, id)}><svg viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" aria-hidden="true"><path d="M6 26C17 26 26 17 26 6" /></svg></button>
        </div>;
      })}
    </div>
    <div className="dashboard-editor-actions"><button className="primary" onClick={() => onConfirm(draft)}>Confirm dashboard</button><button className="secondary" onClick={onCancel}>Cancel</button></div>
  </div>;
}
