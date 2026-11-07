import { useEffect, useState } from "react";
import AppSelect from "./AppSelect";
import {
  desktopAvailable,
  errorMessage,
  refreshStatistics,
  setStatisticsProfile,
  statisticsHistory,
  statisticsProfile,
  statisticsSales,
  statisticsBrowserStatus,
  startStatisticsBrowser,
  type SalesSnapshot,
  type StatisticsPost,
  type StatisticsSnapshot,
} from "./bridge";

type Metric = "views" | "likes" | "copies" | "favourites";
const metrics: { key: Metric; label: string }[] = [
  { key: "views", label: "Views" },
  { key: "likes", label: "Likes" },
  { key: "copies", label: "Copies" },
  { key: "favourites", label: "Favourites" },
];
const number = (v: number | null) =>
  v === null ? "—" : new Intl.NumberFormat().format(v);
const total = (posts: StatisticsPost[], key: Metric): number | null =>
  posts.some((p) => p[key] === null)
    ? null
    : posts.reduce((sum, p) => sum + (p[key] ?? 0), 0);

export function useStatistics(workspaceGeneration: number) {
  const [profile, setProfile] = useState<string | null>(null);
  const [history, setHistory] = useState<StatisticsSnapshot[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [sales, setSales] = useState<SalesSnapshot | null>(null);
  const [salesBusy, setSalesBusy] = useState(false);
  useEffect(() => {
    if (!desktopAvailable) return;
    let active = true;
    setProfile(null);
    setHistory([]);
    setSales(null);
    Promise.all([statisticsProfile(), statisticsHistory(), statisticsSales()])
      .then(([id, data, salesData]) => {
        if (active) {
          setProfile(id);
          setHistory(data);
          setSales(salesData);
        }
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [workspaceGeneration]);
  useEffect(() => {
    if (!salesBusy) return;
    let active = true;
    const poll = async () => {
      try {
        const status = await statisticsBrowserStatus();
        if (!active || status.state === "running") return;
        if (status.state === "complete") setSales(await statisticsSales());
        else if (status.state === "failed") setError(status.message);
        if (active) setSalesBusy(false);
      } catch (e) {
        if (active) {
          setError(errorMessage(e));
          setSalesBusy(false);
        }
      }
    };
    const timer = setInterval(() => void poll(), 1200);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [salesBusy]);
  async function saveProfile(id: string) {
    setBusy(true);
    setError("");
    try {
      await setStatisticsProfile(id.trim());
      setProfile(id.trim());
      setHistory([]);
      setSales(null);
      setHistory(await statisticsHistory());
      setSales(await statisticsSales());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function refresh() {
    setBusy(true);
    setError("");
    try {
      setHistory(await refreshStatistics());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function refreshSales() {
    setError("");
    try {
      await startStatisticsBrowser();
      setSalesBusy(true);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return {
    profile,
    history,
    error,
    busy,
    sales,
    salesBusy,
    saveProfile,
    refresh,
    refreshSales,
  };
}
type Stats = ReturnType<typeof useStatistics>;

export function StatisticsSettings({ stats }: { stats: Stats }) {
  const [id, setId] = useState(stats.profile ?? "");
  useEffect(() => setId(stats.profile ?? ""), [stats.profile]);
  return (
    <section className="statistics-settings" id="settings-statistics">
      <h2>ERLCX statistics</h2>
      <label htmlFor="statistics-profile">Roblox user ID</label>
      <input
        id="statistics-profile"
        inputMode="numeric"
        value={id}
        onChange={(e) => setId(e.target.value)}
        placeholder="e.g. 2919579206"
      />
      <p>Statistics include the selected profile's public posts.</p>
      <button
        className="secondary"
        disabled={
          !desktopAvailable ||
          stats.busy ||
          stats.salesBusy ||
          !id.trim() ||
          id.trim() === stats.profile
        }
        onClick={() => void stats.saveProfile(id)}
      >
        Save user ID
      </button>
      {stats.error && (
        <p role="alert" className="error">
          {stats.error}
        </p>
      )}
    </section>
  );
}

function Cards({ snapshot }: { snapshot: StatisticsSnapshot }) {
  return (
    <div className="statistics-cards">
      {metrics.map((m) => (
        <div className="statistics-card" key={m.key}>
          <span>{m.label}</span>
          <strong>{number(total(snapshot.posts, m.key))}</strong>
        </div>
      ))}
    </div>
  );
}

export function StatisticsOverview({
  stats,
  onOpen,
}: {
  stats: Stats;
  onOpen: () => void;
}) {
  const latest = stats.history.at(-1);
  if (!stats.profile || !latest) return null;
  return (
    <section className="content-section statistics-overview">
      <div className="statistics-heading">
        <h2>ERLCX statistics</h2>
        <button className="text-button" onClick={onOpen}>
          View statistics →
        </button>
      </div>
      <Cards snapshot={latest} />
      <p className="field-hint">
        Public posts · Updated {new Date(latest.collectedAt).toLocaleString()}
      </p>
    </section>
  );
}

export default function Statistics({
  stats,
  onSettings,
}: {
  stats: Stats;
  onSettings: () => void;
}) {
  const [metric, setMetric] = useState<Metric>("views");
  const latest = stats.history.at(-1);
  const points = stats.history
    .map((s) => ({ time: s.collectedAt, value: total(s.posts, metric) }))
    .filter((p): p is { time: string; value: number } => p.value !== null);
  const max = Math.max(1, ...points.map((p) => p.value));
  const firstTime = points.length ? Date.parse(points[0].time) : 0;
  const lastTime = points.length
    ? Date.parse(points[points.length - 1].time)
    : 0;
  const xPosition = (time: string, index: number) =>
    lastTime > firstTime
      ? 20 + ((Date.parse(time) - firstTime) / (lastTime - firstTime)) * 560
      : 20 + (index * 560) / Math.max(points.length - 1, 1);
  const sorted = [...(latest?.posts ?? [])].sort(
    (a, b) => (b[metric] ?? -1) - (a[metric] ?? -1),
  );
  const top = sorted.filter((p) => p[metric] !== null).slice(0, 5);
  const topMax = Math.max(1, ...top.map((p) => p[metric] ?? 0));
  return (
    <div className="statistics-page">
      <div className="statistics-heading">
        <p>
          {stats.profile
            ? `Roblox user ${stats.profile}`
            : "Choose a Roblox user ID in Settings."}
        </p>
        {stats.profile ? (
          <button
            className="primary"
            disabled={stats.busy || stats.salesBusy}
            onClick={() => void stats.refresh()}
          >
            {stats.busy ? "Refreshing…" : "Refresh"}
          </button>
        ) : (
          <button className="secondary" onClick={onSettings}>
            Open settings
          </button>
        )}
      </div>
      {stats.error && (
        <p role="alert" className="error">
          {stats.error}
        </p>
      )}
      {stats.profile && !latest && <p>No saved observations. Refresh to load public post statistics.</p>}
      {stats.profile && (
        <section className="content-section">
          <div className="statistics-heading">
            <h2>Sales</h2>
            <button
              className="secondary"
              disabled={stats.salesBusy || stats.busy}
              onClick={() => void stats.refreshSales()}
            >
              {stats.salesBusy ? "Connecting…" : "Refresh sales in Chrome"}
            </button>
          </div>
          {stats.sales && (
            <>
              <p className="field-hint">
                Creator dashboard · Last 30 days · Updated{" "}
                {new Date(stats.sales.collectedAt).toLocaleString()}
              </p>
              <div className="statistics-cards">
                <div className="statistics-card">
                  <span>Sales</span>
                  <strong>{number(stats.sales.data.sales)}</strong>
                </div>
                <div className="statistics-card">
                  <span>All-time sales</span>
                  <strong>{number(stats.sales.data.allTimeSales)}</strong>
                </div>
                <div className="statistics-card">
                  <span>Net revenue</span>
                  <strong>R$ {number(stats.sales.data.netRobux)}</strong>
                </div>
              </div>
              {stats.sales.data.series.length > 0 && <details><summary>Sales by day</summary><div className="statistics-ranking">{stats.sales.data.series.map(point => <div key={point.date}><div className="statistics-heading"><span>{point.date}</span><span>{number(point.sales)} sales · R$ {number(point.netRobux)} net</span></div><div className="statistics-bar-track" aria-hidden="true"><div style={{width:`${point.sales / Math.max(1,...stats.sales!.data.series.map(p=>p.sales))*100}%`}} /></div></div>)}</div></details>}
            </>
          )}
        </section>
      )}
      {latest && (
        <>
          <Cards snapshot={latest} />
          <p className="field-hint">
            Public posts · {latest.posts.length} posts · Updated{" "}
            {new Date(latest.collectedAt).toLocaleString()}
          </p>
          <section className="content-section">
            <div className="statistics-heading">
              <h2>History</h2>
              <label>
                Metric{" "}
                <AppSelect
                  value={metric}
                  onChange={(e) => setMetric(e.target.value as Metric)}
                >
                  {metrics.map((m) => (
                    <option key={m.key} value={m.key}>
                      {m.label}
                    </option>
                  ))}
                </AppSelect>
              </label>
            </div>
            {points.length < 2 ? (
              <p>Refresh again later to compare snapshots.</p>
            ) : (
              <>
                <svg
                  className="statistics-chart"
                  viewBox="0 0 600 180"
                  role="img"
                  aria-label={`${metrics.find((m) => m.key === metric)?.label} across ${points.length} observations`}
                >
                  <polyline
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="3"
                    points={points
                      .map(
                        (p, i) =>
                          `${xPosition(p.time, i)},${160 - (p.value / max) * 140}`,
                      )
                      .join(" ")}
                  />
                  {points.map((p, i) => (
                    <circle
                      key={p.time + i}
                      cx={xPosition(p.time, i)}
                      cy={160 - (p.value / max) * 140}
                      r="4"
                      fill="currentColor"
                    >
                      <title>
                        {new Date(p.time).toLocaleString()}: {number(p.value)}
                      </title>
                    </circle>
                  ))}
                </svg>
                <p className="field-hint">
                  Each point is a saved observation. Changes between points may
                  include posts becoming public or unavailable.
                </p>
                <details>
                  <summary>View observations</summary>
                  <ul>
                    {points.map((p, i) => (
                      <li key={p.time + i}>
                        {new Date(p.time).toLocaleString()} · {number(p.value)}
                      </li>
                    ))}
                  </ul>
                </details>
              </>
            )}
          </section>
          {top.length > 0 && (
            <section className="content-section">
              <h2>
                Top posts by{" "}
                {metrics.find((m) => m.key === metric)?.label.toLowerCase()}
              </h2>
              <div className="statistics-ranking">
                {top.map((p) => (
                  <div key={p.bundleId}>
                    <div className="statistics-heading">
                      <span>{p.title}</span>
                      <strong>{number(p[metric])}</strong>
                    </div>
                    <div className="statistics-bar-track" aria-hidden="true">
                      <div
                        style={{
                          width: `${((p[metric] ?? 0) / topMax) * 100}%`,
                        }}
                      />
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}
          <section className="content-section">
            <h2>Posts</h2>
            <div className="statistics-table-wrap">
              <table className="statistics-table">
                <thead>
                  <tr>
                    <th>Post</th>
                    {metrics.map((m) => (
                      <th key={m.key}>{m.label}</th>
                    ))}
                    <th>Rating</th>
                  </tr>
                </thead>
                <tbody>
                  {sorted.map((p) => (
                    <tr key={p.bundleId}>
                      <td>{p.title}</td>
                      {metrics.map((m) => (
                        <td key={m.key}>{number(p[m.key])}</td>
                      ))}
                      <td>
                        {p.ratingCount
                          ? `${p.ratingAverage?.toFixed(1) ?? "—"} (${number(p.ratingCount)})`
                          : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
