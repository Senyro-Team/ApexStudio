import PerformanceDiagnostics from "./PerformanceDiagnostics";
import ChromeSettings from "./ChromeSettings";
import RobloxSettings from "./RobloxSettings";
import Statistics, { StatisticsSettings, useStatistics } from "./Statistics";
import { DashboardWidgets, DashboardWidgetEditor, loadDashboardLayout, saveDashboardLayout, type DashboardLayout } from "./DashboardWidgets";
import { invoke } from "@tauri-apps/api/core";
import {
  applyTheme,
  loadCustomThemes,
  loadThemeOverrides,
  savedTheme,
  saveCustomThemes,
  saveTheme,
  saveThemeOverrides,
  themeColors,
  themeFavicon,
  themes,
  type Theme,
  type ThemeColors,
  type ThemeCustomization,
  type ThemeMode,
} from "./themes";
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
  ChartNoAxesCombined,
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
  { name: "Statistics", icon: ChartNoAxesCombined },
  { name: "Settings", icon: Settings },
] as const;
type Section = (typeof sections)[number]["name"];

const settingsCategories = [
  ["appearance", "Appearance"],
  ["dashboard", "Dashboard"],
  ["performance", "Performance"],
  ["roblox", "Roblox"],
  ["chrome", "Chrome"],
  ["statistics", "Statistics"],
  ["backups", "Backups"],
  ["workspace", "Workspace"],
  ["storage", "Storage"],
] as const;

function highlightSettingsCategory(id: string) {
  requestAnimationFrame(() => {
    const heading = document
      .getElementById(id)
      ?.querySelector<HTMLElement>("h2, summary");
    if (!heading) return;
    heading.classList.remove("settings-category-highlight");
    void heading.offsetWidth;
    heading.classList.add("settings-category-highlight");
  });
}

export default function App() {
  const [section, setSection] = useState<Section>("Dashboard");
  const [dashboardLayout, setDashboardLayout] = useState<DashboardLayout>(loadDashboardLayout);
  const [dashboardEditing, setDashboardEditing] = useState(false);
  const [customThemes, setCustomThemes] = useState<Theme[]>(loadCustomThemes);
  const [themeOverrides, setThemeOverrides] = useState(loadThemeOverrides);
  const [themeId, setThemeId] = useState(() => savedTheme(customThemes));
  const [themeDraft, setThemeDraft] = useState<{
    id: string;
    customization: ThemeCustomization;
  } | null>(null);
  const [themeStatus, setThemeStatus] = useState("");
  const [newThemeName, setNewThemeName] = useState("");
  const [newThemeColor, setNewThemeColor] = useState("#8c91ff");
  const [newThemeMode, setNewThemeMode] = useState<ThemeMode>("black");
  const [themeError, setThemeError] = useState("");
  const baseTheme =
    [...themes, ...customThemes].find((option) => option.id === themeId) ??
    themes[0];
  const savedCustomization = baseTheme.id.startsWith("custom-")
    ? { mode: baseTheme.mode, colors: themeColors(baseTheme) }
    : (themeOverrides[baseTheme.id] ?? {
        mode: baseTheme.mode,
        colors: themeColors(baseTheme),
      });
  const activeCustomization =
    themeDraft?.id === baseTheme.id
      ? themeDraft.customization
      : savedCustomization;
  const activeColors = activeCustomization.colors;
  useEffect(() => {
    let active = true;
    applyTheme(baseTheme, activeCustomization);
    saveTheme(baseTheme.id);
    void themeFavicon(activeColors.logo)
      .then((url) => {
        if (active)
          document
            .querySelector<HTMLLinkElement>('link[rel="icon"]')
            ?.setAttribute("href", url);
      })
      .catch(() => {});
    if (desktopAvailable) {
      void invoke("set_app_theme_icon", { color: activeColors.logo })
        .then(() => {
          if (active) setThemeError("");
        })
        .catch(() => {
          if (active) setThemeError("The desktop icon could not be updated.");
        });
    }
    return () => {
      active = false;
    };
  }, [
    baseTheme.id,
    activeCustomization.mode,
    activeColors.button,
    activeColors.line,
    activeColors.text,
    activeColors.logo,
  ]);
  function selectTheme(id: string) {
    setThemeDraft(null);
    setThemeStatus("");
    setThemeId(id);
  }
  function previewTheme(update: Partial<ThemeColors> | { mode: ThemeMode }) {
    const customization =
      "mode" in update
        ? { ...activeCustomization, mode: update.mode }
        : { ...activeCustomization, colors: { ...activeColors, ...update } };
    setThemeDraft({ id: baseTheme.id, customization });
    setThemeStatus("");
  }
  function saveActiveTheme() {
    if (themeDraft?.id !== baseTheme.id) return;
    if (baseTheme.id.startsWith("custom-")) {
      const next = customThemes.map((option) =>
        option.id === baseTheme.id
          ? { ...option, ...activeCustomization, color: activeColors.logo }
          : option,
      );
      saveCustomThemes(next);
      setCustomThemes(next);
    } else {
      const next = { ...themeOverrides, [baseTheme.id]: activeCustomization };
      saveThemeOverrides(next);
      setThemeOverrides(next);
    }
    setThemeDraft(null);
    setThemeStatus("Theme saved.");
  }
  function restorePreset() {
    const next = { ...themeOverrides };
    delete next[baseTheme.id];
    saveThemeOverrides(next);
    setThemeOverrides(next);
    setThemeDraft(null);
    setThemeStatus("Preset restored.");
  }
  const renderTheme = (option: Theme) => (
    <button
      key={option.id}
      type="button"
      className={`theme-option ${baseTheme.id === option.id ? "selected" : ""}`}
      aria-pressed={baseTheme.id === option.id}
      onClick={() => selectTheme(option.id)}
    >
      <span
        className="theme-mark"
        style={{
          backgroundColor:
            option.id === baseTheme.id
              ? activeColors.logo
              : (themeOverrides[option.id]?.colors.logo ??
                themeColors(option).logo),
        }}
        aria-hidden="true"
      />
      <span>{option.name}</span>
      <span
        className="theme-swatch"
        style={{
          backgroundColor:
            option.id === baseTheme.id
              ? activeColors.button
              : (themeOverrides[option.id]?.colors.button ??
                themeColors(option).button),
        }}
      />
    </button>
  );
  function createCustomTheme(event: FormEvent) {
    event.preventDefault();
    const name = newThemeName.trim();
    if (!name) return;
    const option: Theme = {
      id: `custom-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`,
      name,
      color: newThemeColor,
      mode: newThemeMode,
    };
    const next = [...customThemes, option];
    saveCustomThemes(next);
    setCustomThemes(next);
    setThemeDraft(null);
    setThemeId(option.id);
    setNewThemeName("");
  }
  function deleteCustomTheme(id: string) {
    const next = customThemes.filter((option) => option.id !== id);
    saveCustomThemes(next);
    setCustomThemes(next);
    if (themeId === id) selectTheme("violet");
  }
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
  const stats = useStatistics(workspaceGeneration);
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
          <span
            className="brand-mark"
            style={{ backgroundColor: activeColors.logo }}
            aria-hidden="true"
          />
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
          {section === "Dashboard" && <DashboardWidgets layout={dashboardLayout} stats={stats} onNavigate={setSection} />}
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
          {section === "Statistics" && <Statistics stats={stats} onSettings={() => setSection("Settings")} />}
          {section === "Settings" && (
            <div className="settings-content">
              <nav className="settings-nav" aria-label="Settings categories">
                {settingsCategories.map(([id, label]) => (
                  <a
                    key={id}
                    href={`#settings-${id}`}
                    onClick={() => highlightSettingsCategory(`settings-${id}`)}
                  >
                    {label}
                  </a>
                ))}
              </nav>
              <section className="theme-settings" id="settings-appearance">
                <h2>Appearance</h2>
                <p>Choose a theme for the app and its logo.</p>
                <h3>Color themes</h3>
                <div className="theme-options" role="group" aria-label="Theme">
                  {themes
                    .filter(
                      (option) =>
                        (themeOverrides[option.id]?.mode ?? option.mode) ===
                        "tinted",
                    )
                    .map(renderTheme)}
                </div>
                <h3>Black themes</h3>
                <p>Black backgrounds with colored buttons and lines.</p>
                <div
                  className="theme-options"
                  role="group"
                  aria-label="Black themes"
                >
                  {themes
                    .filter(
                      (option) =>
                        (themeOverrides[option.id]?.mode ?? option.mode) ===
                        "black",
                    )
                    .map(renderTheme)}
                </div>
                {customThemes.length > 0 && (
                  <>
                    <h3>My themes</h3>
                    <div
                      className="theme-options"
                      role="group"
                      aria-label="My themes"
                    >
                      {customThemes.map((option) => (
                        <div className="theme-entry" key={option.id}>
                          {renderTheme(option)}
                          <button
                            type="button"
                            className="text-button"
                            onClick={() => deleteCustomTheme(option.id)}
                            aria-label={`Delete ${option.name}`}
                          >
                            Delete
                          </button>
                        </div>
                      ))}
                    </div>
                  </>
                )}
                <form
                  className="custom-theme-form"
                  onSubmit={createCustomTheme}
                >
                  <h3>Create custom theme</h3>
                  <label htmlFor="custom-theme-name">Name</label>
                  <input
                    id="custom-theme-name"
                    maxLength={40}
                    required
                    value={newThemeName}
                    onChange={(event) => setNewThemeName(event.target.value)}
                  />
                  <label htmlFor="custom-theme-color">Accent color</label>
                  <input
                    id="custom-theme-color"
                    type="color"
                    value={newThemeColor}
                    onChange={(event) => setNewThemeColor(event.target.value)}
                  />
                  <label htmlFor="custom-theme-mode">Background</label>
                  <select
                    id="custom-theme-mode"
                    value={newThemeMode}
                    onChange={(event) =>
                      setNewThemeMode(event.target.value as ThemeMode)
                    }
                  >
                    <option value="black">
                      Black — colored buttons and lines
                    </option>
                    <option value="tinted">Tinted — colored surfaces</option>
                  </select>
                  <button className="primary" type="submit">
                    Create theme
                  </button>
                </form>
                <div className="theme-editor">
                  <h3>Edit active theme: {baseTheme.name}</h3>
                  <p>
                    Changes appear immediately. Save to keep them, or discard
                    the preview.
                  </p>
                  <div
                    className="theme-live-preview"
                    aria-label="Live theme preview"
                  >
                    <div className="preview-heading">
                      <span
                        className="brand-mark"
                        style={{ backgroundColor: activeColors.logo }}
                        aria-hidden="true"
                      />
                      <strong>Apex Studio</strong>
                    </div>
                    <p>Preview text in the selected color.</p>
                    <div className="preview-line" />
                    <span className="primary preview-button">
                      Sample button
                    </span>
                  </div>
                  <div className="theme-editor-fields">
                    {(
                      [
                        ["button", "Button color"],
                        ["line", "Line color"],
                        ["text", "Text color"],
                        ["logo", "Logo color"],
                      ] as const
                    ).map(([key, label]) => (
                      <label key={key}>
                        <span>{label}</span>
                        <input
                          type="color"
                          aria-label={label}
                          value={activeColors[key]}
                          onChange={(event) =>
                            previewTheme({ [key]: event.target.value })
                          }
                        />
                      </label>
                    ))}
                  </div>
                  <label htmlFor="active-theme-mode">Background</label>
                  <select
                    id="active-theme-mode"
                    value={activeCustomization.mode}
                    onChange={(event) =>
                      previewTheme({ mode: event.target.value as ThemeMode })
                    }
                  >
                    <option value="black">
                      Black — colored buttons and lines
                    </option>
                    <option value="tinted">Tinted — colored surfaces</option>
                  </select>
                  <div className="theme-editor-actions">
                    <button
                      type="button"
                      className="primary"
                      disabled={themeDraft?.id !== baseTheme.id}
                      onClick={saveActiveTheme}
                    >
                      Save changes
                    </button>
                    <button
                      type="button"
                      className="text-button"
                      disabled={themeDraft?.id !== baseTheme.id}
                      onClick={() => {
                        setThemeDraft(null);
                        setThemeStatus("Changes discarded.");
                      }}
                    >
                      Discard preview
                    </button>
                    {!baseTheme.id.startsWith("custom-") &&
                      themeOverrides[baseTheme.id] && (
                        <button
                          type="button"
                          className="text-button"
                          onClick={restorePreset}
                        >
                          Restore preset
                        </button>
                      )}
                  </div>
                  {themeStatus && <p role="status">{themeStatus}</p>}
                </div>
                {themeError && (
                  <p className="field-hint" role="status">
                    {themeError}
                  </p>
                )}
              </section>
              <section id="settings-dashboard" className="dashboard-settings">
                <h2>Dashboard</h2>
                <p>Choose and arrange the widgets on your dashboard.</p>
                {!dashboardEditing ? (
                  <button className="secondary" onClick={() => setDashboardEditing(true)}>Edit dashboard</button>
                ) : (
                  <DashboardWidgetEditor
                    layout={dashboardLayout}
                    stats={stats}
                    onConfirm={(layout) => {
                      saveDashboardLayout(layout);
                      setDashboardLayout(layout);
                      setDashboardEditing(false);
                      setSection("Dashboard");
                    }}
                    onCancel={() => setDashboardEditing(false)}
                  />
                )}
              </section>
              <PerformanceDiagnostics />
              <RobloxSettings onBusy={setRobloxLock} />
              <ChromeSettings onBusy={setChromeLock} />
              <StatisticsSettings stats={stats} />
              <BackupTools
                onBusy={setBackupLock}
                onRestore={() => {
                  setWorkspaceGeneration((v) => v + 1);
                  void load();
                }}
              />
              <section id="settings-workspace">
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
              <section className="storage-details" id="settings-storage">
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
