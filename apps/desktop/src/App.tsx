import PerformanceDiagnostics from "./PerformanceDiagnostics";
import ChromeSettings from "./ChromeSettings";
import RobloxSettings from "./RobloxSettings";
import { lazy, Suspense, useEffect, useState, type FormEvent } from "react";
import ProjectsArea from "./Projects";
const ERLCX = lazy(() => import("./ERLCX"));
const Roblox = lazy(() => import("./Roblox"));
const Notes = lazy(() => import("./Notes"));
import { BackupTools } from "./Packages";
import {
  House,
  FolderOpen,
  NotebookPen,
  Images,
  Upload,
  Settings,
  ArrowRight,
  PanelLeftClose,
  PanelLeftOpen,
  HardDrive,
  ArrowUpRight,
} from "lucide-react";
import {
  setEditLock,
  type RobloxFocus,
  desktopAvailable,
  errorMessage,
  readWorkspace,
  renameWorkspace,
  type WorkspaceInfo,
} from "./bridge";

const sections = [
  { name: "Dashboard", icon: House },
  { name: "Projects", icon: FolderOpen },
  { name: "Notes", icon: NotebookPen },
  { name: "Assets", icon: Images },
  { name: "Roblox", icon: Upload },
  { name: "ERLCX", icon: ArrowUpRight },
  { name: "Settings", icon: Settings },
] as const;
type Section = (typeof sections)[number]["name"];

export default function App() {
  const [section, setSection] = useState<Section>("Dashboard");
  const [robloxFocus, setRobloxFocus] = useState<RobloxFocus>({});
  const openRoblox = (focus: RobloxFocus) => {
    setRobloxFocus(focus);
    setSection("Roblox");
  };
  const [collapsed, setCollapsed] = useState(false);
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);
  const [loading, setLoading] = useState(desktopAvailable);
  const [error, setError] = useState("");
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [navigationLocked, setNavigationLocked] = useState(false);
  const [robloxLock, setRobloxLock] = useState(false),
    [chromeLock, setChromeLock] = useState(false),
    [backupLock, setBackupLock] = useState(false);
  useEffect(() => {
    if (section === "Settings")
      setNavigationLocked(robloxLock || chromeLock || backupLock);
  }, [section, robloxLock, chromeLock, backupLock]);
  useEffect(() => {
    if (desktopAvailable) void setEditLock(navigationLocked).catch(() => {});
  }, [navigationLocked]);
  const [workspaceGeneration, setWorkspaceGeneration] = useState(0);
  async function load() {
    setLoading(true);
    setError("");
    try {
      const info = await readWorkspace();
      setWorkspace(info);
      setName(info.name);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    if (desktopAvailable) void load();
  }, []);
  useEffect(() => {
    if (desktopAvailable && section === "Settings") void load();
  }, [section]);
  useEffect(() => {
    if (!desktopAvailable) return;
    let cancelled = false;
    let stop: (() => void) | undefined;
    void import("@tauri-apps/api/window")
      .then(async ({ getCurrentWindow }) => {
        const unlisten = await getCurrentWindow().onCloseRequested((event) => {
          if (navigationLocked) {
            event.preventDefault();
            setError(
              "Save or discard your changes and wait for file operations to finish before closing.",
            );
          }
        });
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [navigationLocked]);
  useEffect(() => {
    if (!desktopAvailable) return;
    let stop: (() => void) | undefined,
      active = true;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen("close-blocked", () =>
          setError("Save or discard your changes before closing Apex Studio."),
        ),
      )
      .then((fn) => {
        if (active) stop = fn;
        else fn();
      })
      .catch(() => {});
    return () => {
      active = false;
      stop?.();
    };
  }, []);
  async function save(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setSaved(false);
    setError("");
    try {
      const info = await renameWorkspace(name);
      setWorkspace(info);
      setName(info.name);
      setSaved(true);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }
  return (
    <div className={`app ${collapsed ? "compact" : ""}`}>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <aside className="sidebar">
        <div className="brand">
          <img src="/brand/apex.png" alt="" />
          <span>
            Apex <span className="muted">Studio</span>
          </span>
        </div>
        <nav aria-label="Main navigation">
          {sections.map(({ name: item, icon: Icon }) => (
            <button
              key={item}
              className={`nav-item ${section === item ? "selected" : ""} ${item === "Settings" ? "settings-link" : ""}`}
              aria-label={item}
              disabled={navigationLocked && item !== section}
              aria-current={section === item ? "page" : undefined}
              title={collapsed ? item : undefined}
              onClick={() => {
                setSection(item);
                setSaved(false);
              }}
            >
              <Icon size={21} aria-hidden="true" />
              <span>{item}</span>
            </button>
          ))}
        </nav>
        <button
          className="collapse"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          onClick={() => setCollapsed(!collapsed)}
        >
          {collapsed ? (
            <PanelLeftOpen size={19} />
          ) : (
            <PanelLeftClose size={19} />
          )}
        </button>
      </aside>
      <div className="workspace-shell">
        <header className="topbar">
          <span className="workspace-name">
            {loading
              ? "Opening workspace…"
              : (workspace?.name ?? "Apex Studio")}
          </span>
          <span className="local-status">
            <HardDrive size={15} aria-hidden="true" />
            {desktopAvailable ? "Local workspace" : "Browser preview"}
          </span>
        </header>
        <main id="main" tabIndex={-1}>
          {!desktopAvailable && (
            <p className="preview-notice">
              Browser preview · Workspace storage is available in the desktop
              app.
            </p>
          )}
          {error && (
            <div className="error" role="alert">
              <p>{error}</p>
              {!workspace && desktopAvailable && (
                <button onClick={() => void load()}>Retry</button>
              )}
            </div>
          )}
          <ProjectsArea
            key={workspaceGeneration}
            onOpenRoblox={openRoblox}
            section={section}
            onNavigate={setSection}
            onEditingChange={setNavigationLocked}
          />
          {!["Dashboard", "Projects", "Assets", "ERLCX"].includes(section) && (
            <header className="page-heading">
              <h1>{section}</h1>
            </header>
          )}
          {section === "ERLCX" && (
            <Suspense fallback={<p role="status">Opening ERLCX…</p>}>
              <ERLCX onBusy={setNavigationLocked} onOpenRoblox={openRoblox} />
            </Suspense>
          )}
          {section === "Roblox" && (
            <Suspense fallback={<p role="status">Loading uploads…</p>}>
              <Roblox
                focus={robloxFocus}
                onSettings={() => setSection("Settings")}
              />
            </Suspense>
          )}
          {section === "Notes" && (
            <Suspense fallback={<p role="status">Loading editor…</p>}>
              <Notes
                key={workspaceGeneration}
                onEditingChange={setNavigationLocked}
              />
            </Suspense>
          )}
          {section === "Settings" && (
            <div className="settings-content">
              <PerformanceDiagnostics />
              <RobloxSettings onBusy={setRobloxLock} />
              <ChromeSettings onBusy={setChromeLock} />
              <BackupTools
                onBusy={setBackupLock}
                onRestore={() => {
                  setWorkspaceGeneration((v) => v + 1);
                  void load();
                }}
              />
              <section>
                <h2>Workspace</h2>
                <form onSubmit={(event) => void save(event)}>
                  <label htmlFor="workspace-name">Workspace name</label>
                  <input
                    id="workspace-name"
                    value={name}
                    onChange={(event) => {
                      setName(event.target.value);
                      setSaved(false);
                    }}
                    maxLength={80}
                    required
                    disabled={!workspace || saving}
                  />
                  <p className="field-hint">Stored only on this device.</p>
                  <button
                    className="primary"
                    disabled={!workspace || saving || loading || !name.trim()}
                  >
                    {saving ? "Saving…" : "Save name"}
                  </button>
                  <span className="save-status" role="status">
                    {saved ? "Workspace name saved." : ""}
                  </span>
                </form>
              </section>
              <section className="storage-details">
                <h2>Storage</h2>
                <dl>
                  <dt>Workspace folder</dt>
                  <dd>
                    {workspace?.root ??
                      "Open the desktop app to see your workspace folder."}
                  </dd>
                  <dt>Database schema</dt>
                  <dd>{workspace?.schemaVersion ?? "Not connected"}</dd>
                  <dt>Managed files</dt>
                  <dd>{workspace?.managedFileCount ?? "Not connected"}</dd>
                </dl>
              </section>
            </div>
          )}
          <footer className="page-footer">
            <span>0.1.0</span>
            <button
              className="text-button"
              disabled={navigationLocked}
              onClick={() =>
                setSection(section === "Settings" ? "Dashboard" : "Settings")
              }
            >
              {section === "Settings" ? "Dashboard" : "Workspace settings"}
              <ArrowUpRight size={14} />
            </button>
          </footer>
        </main>
      </div>
    </div>
  );
}
