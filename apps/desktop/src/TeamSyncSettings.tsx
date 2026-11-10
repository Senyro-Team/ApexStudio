import { useEffect, useRef, useState, useSyncExternalStore, type FormEvent } from "react";
import { autoSyncEnabled, setAutoSyncEnabled, subscribeAutoSync } from "./teamSyncPreferences";
import { Check, CircleAlert, Copy, ImagePlus, Link2, LogOut, RefreshCw, Server, Trash2, UserPlus, Users } from "lucide-react";
import AppSelect from "./AppSelect";
import { readTeamSyncLog, recordTeamSyncError, recordTeamSyncReport, syncSummary } from "./teamSyncLog";
import {
  desktopAvailable,
  syncAcceptInvitation,
  syncCheckServer,
  syncConnectionStatus,
  syncCreateInvitation,
  syncDisconnect,
  syncInitialSetup,
  syncSignIn,
  syncProjects,
  syncListMembers,
  syncRemoveMember,
  syncServiceAvailability,
  type SyncConnectionStatus,
  type SyncInvitation,
  type SyncInvitationRole,
  type SyncTeamMember,
  type SyncServiceAvailability,
} from "./bridge";

const SERVER_URL_KEY = "apex-studio.team-sync.server-url";
const ACTIVE_TEAM_KEY = "apex-studio.team-sync.active-team";

function savedServerUrl() {
  try {
    return window.localStorage.getItem(SERVER_URL_KEY) ?? "";
  } catch {
    return "";
  }
}

function errorText(reason: unknown) {
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object" && "message" in reason) {
    const message = (reason as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return "The request could not be completed. Check the server address and try again.";
}

export default function TeamSyncSettings({
  onBusy,
}: {
  onBusy: (busy: boolean) => void;
}) {
  const initialUrl = useRef(savedServerUrl());
  const [serverUrl, setServerUrl] = useState(initialUrl.current);
  const [connection, setConnection] = useState<SyncConnectionStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [syncBusy, setSyncBusy] = useState(false);
  const [members, setMembers] = useState<SyncTeamMember[]>([]);
  const [membersBusy, setMembersBusy] = useState(false);
  const [membersError, setMembersError] = useState("");
  const [removingMember, setRemovingMember] = useState("");
  const [membersRefresh, setMembersRefresh] = useState(0);
  const [serviceAvailability, setServiceAvailability] = useState<SyncServiceAvailability | null>(null);
  const [servicesBusy, setServicesBusy] = useState(false);
  const [servicesError, setServicesError] = useState("");
  const [syncLog, setSyncLog] = useState(() => readTeamSyncLog(initialUrl.current, ""));
  const automaticSync = useSyncExternalStore(subscribeAutoSync, autoSyncEnabled, () => true);
  useEffect(() => {
    const failed = (event: Event) => setError(String((event as CustomEvent).detail));
    window.addEventListener("apex-project-sync-error", failed);
    return () => window.removeEventListener("apex-project-sync-error", failed);
  }, []);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [invitationToken, setInvitationToken] = useState("");
  const [inviteName, setInviteName] = useState("");
  const [inviteEmail, setInviteEmail] = useState("");
  const [invitePassword, setInvitePassword] = useState("");
  const [inviteRole, setInviteRole] = useState<SyncInvitationRole>("designer");
  const [createdInvitation, setCreatedInvitation] = useState<SyncInvitation | null>(null);
  const [setupToken, setSetupToken] = useState("");
  const [teamName, setTeamName] = useState("");
  const [ownerName, setOwnerName] = useState("");
  const [ownerEmail, setOwnerEmail] = useState("");
  const [ownerPassword, setOwnerPassword] = useState("");
  const [activeTeam, setActiveTeam] = useState(() => {
    try {
      return window.localStorage.getItem(ACTIVE_TEAM_KEY) ?? "";
    } catch {
      return "";
    }
  });

  const hasEnteredSecret = Boolean(
    password || invitationToken || invitePassword || setupToken || ownerPassword,
  );
  useEffect(() => {
    onBusy(busy || hasEnteredSecret);
    return () => onBusy(false);
  }, [busy, hasEnteredSecret, onBusy]);

  useEffect(() => {
    const url = initialUrl.current.trim();
    if (!desktopAvailable || !url) return;
    let active = true;
    void (async () => {
      try {
        const result = await syncConnectionStatus(url);
        if (!active) return;
        acceptConnection(result);
      } catch (reason) {
        if (active) setError(errorText(reason));
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  async function run(action: () => Promise<SyncConnectionStatus | void>, message?: string) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await action();
      if (result) acceptConnection(result);
      if (message) setNotice(message);
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
      setPassword("");
      setInvitePassword("");
      setInvitationToken("");
      setSetupToken("");
      setOwnerPassword("");
    }
  }

  function acceptConnection(result: SyncConnectionStatus) {
    setConnection(result);
    setServerUrl(result.server.origin);
    initialUrl.current = result.server.origin;
    try {
      window.localStorage.setItem(SERVER_URL_KEY, result.server.origin);
    } catch {
      setError("The server address could not be saved on this device.");
    }
    const teams = result.user?.teams ?? [];
    const current = teams.some((team) => team.id === activeTeam)
      ? activeTeam
      : (teams[0]?.id ?? "");
    if (current !== activeTeam) {
      setCreatedInvitation(null);
      setInviteRole("designer");
    }
    setActiveTeam(current);
    try {
      if (current) window.localStorage.setItem(ACTIVE_TEAM_KEY, current);
      else window.localStorage.removeItem(ACTIVE_TEAM_KEY);
    } catch {
      // The selected team remains available for this session.
    }
  }

  function changeActiveTeam(id: string) {
    setActiveTeam(id);
    setCreatedInvitation(null);
    setInviteRole("designer");
    try {
      window.localStorage.setItem(ACTIVE_TEAM_KEY, id);
    } catch {
      // The selected team remains available for this session.
    }
    window.dispatchEvent(new Event("apex-project-sync-request"));
  }

  function checkServer(event: FormEvent) {
    event.preventDefault();
    void run(async () => {
      const server = await syncCheckServer(serverUrl);
      const result = await syncConnectionStatus(server.origin);
      return result;
    }, "Server connection verified.");
  }

  const connectedUser = connection?.user;
  const selectedTeam = connectedUser?.teams.find((team) => team.id === activeTeam);
  const canInvite = selectedTeam?.role === "owner" || selectedTeam?.role === "admin";
  const serviceState = (
    configured: boolean | undefined,
    reachable: boolean | null | undefined,
    unconfigured: string,
  ) => {
    if (servicesBusy) return "Checking…";
    if (servicesError) return "Status unavailable";
    if (configured === undefined) return "Not checked";
    if (!configured) return unconfigured;
    if (reachable === false) return "Unreachable";
    return reachable === true ? "Online" : "Configured";
  };

  useEffect(() => {
    if (!connectedUser || !selectedTeam) {
      setServiceAvailability(null);
      setServicesError("");
      return;
    }
    let active = true;
    setServicesBusy(true);
    setServicesError("");
    void syncServiceAvailability(serverUrl, selectedTeam.id)
      .then((availability) => { if (active) setServiceAvailability(availability); })
      .catch((reason) => { if (active) setServicesError(errorText(reason)); })
      .finally(() => { if (active) setServicesBusy(false); });
    return () => { active = false; };
  }, [connectedUser?.id, selectedTeam?.id, serverUrl]);

  function canRemoveMember(member: SyncTeamMember) {
    if (!selectedTeam || member.role === "owner" || member.id === connectedUser?.id) return false;
    return selectedTeam.role === "owner"
      || (selectedTeam.role === "admin" && member.role !== "admin");
  }

  async function removeMember(member: SyncTeamMember) {
    if (!selectedTeam || !canRemoveMember(member)) return;
    const confirmed = window.confirm(
      `Remove ${member.displayName} from ${selectedTeam.name}? Their active sessions will be signed out and they will lose access to this team's projects.`,
    );
    if (!confirmed) return;
    setRemovingMember(member.id);
    setError("");
    setNotice("");
    try {
      await syncRemoveMember(serverUrl, selectedTeam.id, member.id);
      setMembers((current) => current.filter((row) => row.id !== member.id));
      setNotice(`${member.displayName} was removed from ${selectedTeam.name}.`);
    } catch (reason) {
      setMembersError(errorText(reason));
    } finally {
      setRemovingMember("");
    }
  }

  useEffect(() => {
    setSyncLog(readTeamSyncLog(serverUrl, selectedTeam?.id ?? ""));
    const refresh = (event: Event) => {
      const detail = (event as CustomEvent<{ serverUrl: string; teamId: string }>).detail;
      if (detail?.serverUrl === serverUrl && detail.teamId === selectedTeam?.id) {
        setSyncLog(readTeamSyncLog(serverUrl, detail.teamId));
      }
    };
    window.addEventListener("apex-team-sync-log", refresh);
    return () => window.removeEventListener("apex-team-sync-log", refresh);
  }, [serverUrl, selectedTeam?.id]);

  useEffect(() => {
    if (!connectedUser || !selectedTeam) {
      setMembers([]);
      setMembersError("");
      setMembersBusy(false);
      return;
    }
    let active = true;
    setMembersBusy(true);
    setMembersError("");
    void syncListMembers(serverUrl, selectedTeam.id)
      .then((rows) => {
        if (active) setMembers(rows);
      })
      .catch((reason) => {
        if (active) setMembersError(errorText(reason));
      })
      .finally(() => {
        if (active) setMembersBusy(false);
      });
    return () => {
      active = false;
    };
  }, [connectedUser?.id, selectedTeam?.id, serverUrl, membersRefresh]);

  async function createInvitation(event: FormEvent) {
    event.preventDefault();
    if (!selectedTeam) return;
    setBusy(true);
    setError("");
    setNotice("");
    setCreatedInvitation(null);
    try {
      const invitation = await syncCreateInvitation(serverUrl, selectedTeam.id, inviteRole);
      setCreatedInvitation(invitation);
      setNotice("Invitation code created.");
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  }

  async function copyInvitationCode() {
    if (!createdInvitation) return;
    try {
      await navigator.clipboard.writeText(createdInvitation.invitationToken);
      setNotice("Invitation code copied.");
    } catch {
      setError("Could not copy the code. Select and copy it manually.");
    }
  }

  return (
    <section className="team-sync-settings" id="server-team-sync">
      <h2>Team sync</h2>
      <p className="field-hint">
        Connect this workspace to a team server. The address is saved on this
        device; your session is protected by Windows Credential Manager.
      </p>

      {!desktopAvailable && (
        <p className="team-sync-message" role="status">
          Connect to a team server from the Apex Studio desktop app.
        </p>
      )}

      <form className="team-sync-server-form" onSubmit={checkServer}>
        <label htmlFor="team-sync-url">Server address</label>
        <div className="team-sync-url-row">
          <input
            id="team-sync-url"
            type="url"
            inputMode="url"
            autoComplete="url"
            placeholder="https://studio-sync.example.org"
            value={serverUrl}
            onChange={(event) => {
              setServerUrl(event.target.value);
              setConnection(null);
              setError("");
              setNotice("");
            }}
            disabled={!desktopAvailable || busy}
            required
          />
          <button
            className="secondary"
            type="submit"
            disabled={!desktopAvailable || busy || !serverUrl.trim()}
          >
            {busy ? <RefreshCw size={16} className="team-sync-spinning" /> : <Server size={16} />}
            {busy ? "Checking…" : "Check server"}
          </button>
        </div>
        <p className="field-hint">
          Use the HTTPS address provided by your team. HTTP works only for a
          server running on this computer.
        </p>
      </form>

      {error && (
        <p className="team-sync-message team-sync-error" role="alert">
          <CircleAlert size={17} aria-hidden="true" />
          {error}
        </p>
      )}
      {notice && (
        <p className="team-sync-message team-sync-success" role="status">
          <Check size={17} aria-hidden="true" />
          {notice}
        </p>
      )}

      {connection && (
        <div className="team-sync-server-status" aria-live="polite">
          <div>
            <span className="team-sync-status-dot" aria-hidden="true" />
            <strong>{connection.server.product}</strong>
            <span>Reachable</span>
          </div>
          <small>
            API {connection.server.apiVersion} · Server {connection.server.serverVersion}
          </small>
        </div>
      )}

      {connectedUser ? (
        <div className="team-sync-account">
          <div className="team-sync-account-heading">
            <div>
              <h3>{connectedUser.displayName}</h3>
              <p>{connectedUser.email}</p>
            </div>
            <button
              className="text-button"
              type="button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await syncDisconnect();
                  setConnection(null);
                  setActiveTeam("");
                  setCreatedInvitation(null);
                  try {
                    window.localStorage.removeItem(ACTIVE_TEAM_KEY);
                  } catch {
                    // The session is still removed from secure storage.
                  }
                }, "Signed out from this team server.")
              }
            >
              <LogOut size={16} />
              Sign out
            </button>
          </div>
          {connectedUser.teams.length > 0 ? (
            <div className="team-sync-selected-team">
              <label htmlFor="team-sync-team">Team</label>
              {connectedUser.teams.length === 1 ? (
                <div id="team-sync-team" className="team-sync-team-value">
                  <strong>{connectedUser.teams[0].name}</strong>
                  <span>{connectedUser.teams[0].role.replace("_", " ")}</span>
                </div>
              ) : (
                <>
                  <AppSelect
                    id="team-sync-team"
                    value={selectedTeam?.id ?? ""}
                    onChange={(event) => changeActiveTeam(event.target.value)}
                  >
                    {connectedUser.teams.map((team) => (
                      <option key={team.id} value={team.id}>{team.name}</option>
                    ))}
                  </AppSelect>
                  {selectedTeam && (
                    <p className="field-hint">
                      Your role: {selectedTeam.role.replace("_", " ")}
                    </p>
                  )}
                </>
              )}
            </div>
          ) : (
            <p className="team-sync-message">
              This account is not a member of a team on this server.
            </p>
          )}
          {selectedTeam && <section className="server-services" aria-labelledby="server-services-title">
            <div className="server-services-heading"><div><h3 id="server-services-title">Optional services</h3><p className="field-hint">Each service can be enabled independently by this team’s server.</p></div><button className="icon-button" aria-label="Refresh service availability" type="button" disabled={servicesBusy} onClick={() => { setServicesError(""); setServicesBusy(true); void syncServiceAvailability(serverUrl, selectedTeam.id).then(setServiceAvailability).catch((reason) => setServicesError(errorText(reason))).finally(() => setServicesBusy(false)); }}><RefreshCw size={16} className={servicesBusy ? "team-sync-spinning" : undefined} /></button></div>
            {servicesError && <p className="team-sync-message team-sync-error" role="alert">{servicesError}</p>}
            <div className="server-service-grid">
              <article><Server size={18} aria-hidden="true" /><div><strong>Project sync</strong><span>{serviceState(serviceAvailability?.projectSync, true, "Unavailable")}</span><p>Team project synchronization remains active.</p></div></article>
              <article><ImagePlus size={18} aria-hidden="true" /><div><strong>Image Hosting</strong><span>{serviceState(serviceAvailability?.imageHosting, serviceAvailability?.imageHostingReachable, "Not configured")}</span><p>{serviceAvailability?.imageHosting && serviceAvailability.imageHostingReachable !== false && !servicesError ? "Upload selected images from Assets → Image Library. Uploaded URLs are public." : "Images stay local; you can still paste URLs from any image host."}</p></div></article>
              <article><Link2 size={18} aria-hidden="true" /><div><strong>Invite Tracking</strong><span>{serviceState(serviceAvailability?.inviteTracking, serviceAvailability?.inviteTrackingReachable, "Not configured")}</span><p>{serviceAvailability?.inviteTracking && serviceAvailability.inviteTrackingReachable !== false && !servicesError ? "Create tracked invite campaigns from the ERLCX post composer." : "Existing Discord invite URLs can be added manually to ERLCX Markdown."}</p></div></article>
            </div>
          </section>}
          {selectedTeam && <section className="project-sync-options" aria-label="Team project synchronization">
            <h3>Project synchronization</h3>
            <label className="check-label">
              <input type="checkbox" checked={automaticSync} onChange={(event) => {
                try {
                  setAutoSyncEnabled(event.target.checked);
                  setNotice(event.target.checked ? "Automatic synchronization enabled." : "Automatic synchronization disabled. Use Sync projects now to synchronize manually.");
                } catch { setError("The automatic sync setting could not be saved on this device."); }
              }} />
              Automatically synchronize projects
            </label>
            <p className="field-hint">Sync saved changes shortly after editing and check for team updates every 2 seconds.</p>
            <p className="field-hint">Shared projects download automatically. Choose “Synchronize with team server” when creating a project or in its Details to share it.</p>
            <button className="secondary" type="button" disabled={busy || syncBusy} onClick={() => {
              setSyncBusy(true); setError(""); setNotice("");
              void syncProjects(serverUrl, selectedTeam.id).then((report) => {
                recordTeamSyncReport(serverUrl, selectedTeam.id, "manual", report);
                setNotice(syncSummary(report));
                if (report.errors.length) setError(report.errors.join("\n"));
                window.dispatchEvent(new CustomEvent("apex-projects-synced", {detail: report}));
              }).catch((reason: unknown) => {
                const message = errorText(reason);
                recordTeamSyncError(serverUrl, selectedTeam.id, "manual", message);
                setError(message);
              }).finally(() => setSyncBusy(false));
            }}>{syncBusy ? "Synchronizing…" : "Sync projects now"}</button>
            <div className="team-sync-log" aria-label="Synchronization log">
              <h4>Sync log</h4>
              {syncLog.length ? <ol>{syncLog.map((item, index) => (
                <li key={`${item.at}-${index}`}>
                  <div className="team-sync-log-heading">
                    <time dateTime={item.at}>{new Date(item.at).toLocaleString()}</time>
                    <span>{item.source === "manual" ? "Manual" : "Automatic"}</span>
                  </div>
                  <p>{item.summary}</p>
                  {item.error && <p className="team-sync-log-error">{item.error}</p>}
                  {item.entries?.some((entry) => entry.status !== "up_to_date") && <ul>{item.entries.filter((entry) => entry.status !== "up_to_date").map((entry) => (
                    <li key={entry.projectId}>{entry.projectName}: {entry.message}</li>
                  ))}</ul>}
                </li>
              ))}</ol> : <p className="field-hint">No sync attempts recorded yet.</p>}
            </div>
          </section>}
          {selectedTeam && (
            <section className="team-sync-members" aria-labelledby="team-sync-members-title">
              <div className="team-sync-members-heading">
                <div>
                  <h3 id="team-sync-members-title">Team members</h3>
                  <p className="field-hint">People who have joined {selectedTeam.name}.</p>
                </div>
                <button
                  className="secondary"
                  type="button"
                  disabled={membersBusy}
                  onClick={() => setMembersRefresh((revision) => revision + 1)}
                >
                  <RefreshCw size={16} className={membersBusy ? "team-sync-spinning" : undefined} />
                  {membersBusy ? "Refreshing…" : "Refresh"}
                </button>
              </div>
              {membersError ? (
                <p className="team-sync-message team-sync-error" role="alert">
                  <CircleAlert size={17} aria-hidden="true" />
                  {membersError}
                </p>
              ) : membersBusy && !members.length ? (
                <p className="field-hint" role="status">Loading team members…</p>
              ) : members.length ? (
                <ul className="team-sync-member-list">
                  {members.map((member) => (
                    <li key={member.id}>
                      <div>
                        <strong>{member.displayName}</strong>
                        <span>{member.email}</span>
                      </div>
                      <span className="team-sync-member-role">{member.role.replace("_", " ")}</span>
                      {canRemoveMember(member) && (
                        <button
                          className="danger-quiet"
                          type="button"
                          aria-label={`Remove ${member.displayName}`}
                          disabled={removingMember === member.id || Boolean(removingMember)}
                          onClick={() => void removeMember(member)}
                        >
                          <Trash2 size={15} aria-hidden="true" />
                          {removingMember === member.id ? "Removing…" : "Remove"}
                        </button>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="field-hint" role="status">
                  <Users size={16} aria-hidden="true" /> No members found for this team.
                </p>
              )}
            </section>
          )}
          {selectedTeam && canInvite && (
            <section className="team-sync-invitations" aria-labelledby="team-sync-invite-title">
              <div className="team-sync-invite-heading">
                <span aria-hidden="true"><UserPlus size={21} /></span>
                <div><h3 id="team-sync-invite-title">Invite a member</h3>
                  <p>Create a private, single-use code. It expires after 72 hours.</p></div>
              </div>
              <form onSubmit={(event) => void createInvitation(event)}>
                <label htmlFor="team-sync-invite-role">Team role</label>
                <AppSelect
                  id="team-sync-invite-role"
                  value={inviteRole}
                  onChange={(event) => {
                    setInviteRole(event.target.value as SyncInvitationRole);
                    setCreatedInvitation(null);
                  }}
                  disabled={busy}
                >
                  {selectedTeam.role === "owner" && <option value="admin">Admin</option>}
                  <option value="designer">Designer</option>
                  <option value="read_only">Read-only</option>
                </AppSelect>
                <p className="team-sync-role-hint">{inviteRole === "admin" ? "Can manage members and shared projects." : inviteRole === "designer" ? "Can work on shared projects." : "Can view shared projects without editing."}</p>
                <button className="primary" disabled={busy}>
                  <UserPlus size={17} aria-hidden="true" /> {busy ? "Creating code…" : "Create invitation code"}
                </button>
              </form>
              {createdInvitation && (
                <div className="team-sync-invitation-code" role="status">
                  <code>{createdInvitation.invitationToken}</code>
                  <button className="secondary" type="button" onClick={() => void copyInvitationCode()}>
                    <Copy size={16} />
                    Copy code
                  </button>
                  <p className="field-hint">
                    Share it only with the intended person. It grants {createdInvitation.role.replace("_", " ")} access and can be used once.
                  </p>
                </div>
              )}
            </section>
          )}
        </div>
      ) : connection ? (
        <div className="team-sync-access">
          {connection.sessionExpired && (
            <p className="field-hint" role="status">
              Your saved session expired. Sign in again to reconnect.
            </p>
          )}
          {connection.savedSessionForDifferentServer && (
            <p className="field-hint" role="status">
              The saved session belongs to another server. Sign in here to replace it.
            </p>
          )}
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void run(() => syncSignIn(serverUrl, email, password));
            }}
          >
            <h3>Sign in</h3>
            <label htmlFor="team-sync-email">Email</label>
            <input
              id="team-sync-email"
              type="email"
              autoComplete="username"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              disabled={busy}
              required
            />
            <label htmlFor="team-sync-password">Password</label>
            <input
              id="team-sync-password"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              disabled={busy}
              minLength={12}
              maxLength={128}
              required
            />
            <div className="button-row">
              <button className="primary" disabled={busy}>
                {busy ? "Signing in…" : "Sign in"}
              </button>
            </div>
          </form>

          <details className="team-sync-details">
            <summary>Accept an invitation</summary>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void run(() => syncAcceptInvitation(
                  serverUrl,
                  invitationToken,
                  inviteEmail,
                  inviteName,
                  invitePassword,
                ));
              }}
            >
              <label htmlFor="team-sync-invite-token">Invitation code</label>
              <input
                id="team-sync-invite-token"
                value={invitationToken}
                onChange={(event) => {
                  const value = event.target.value.trim();
                  const token = value.includes("token=")
                    ? new URLSearchParams(value.split("?")[1] ?? "").get("token") ?? value
                    : value;
                  setInvitationToken(token);
                }}
                autoComplete="off"
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-invite-name">Display name</label>
              <input
                id="team-sync-invite-name"
                value={inviteName}
                onChange={(event) => setInviteName(event.target.value)}
                autoComplete="name"
                maxLength={80}
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-invite-email">Email</label>
              <input
                id="team-sync-invite-email"
                type="email"
                value={inviteEmail}
                onChange={(event) => setInviteEmail(event.target.value)}
                autoComplete="email"
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-invite-password">Create password</label>
              <input
                id="team-sync-invite-password"
                type="password"
                value={invitePassword}
                onChange={(event) => setInvitePassword(event.target.value)}
                autoComplete="new-password"
                minLength={12}
                maxLength={128}
                disabled={busy}
                required
              />
              <button className="secondary" disabled={busy}>Accept invitation</button>
            </form>
          </details>

          <details className="team-sync-details">
            <summary>Set up a new server</summary>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void run(() => syncInitialSetup(
                  serverUrl,
                  setupToken,
                  teamName,
                  ownerEmail,
                  ownerName,
                  ownerPassword,
                ));
              }}
            >
              <p className="field-hint">
                Use the one-time setup key provided by the server administrator. This works only before the first team is created.
              </p>
              <label htmlFor="team-sync-setup-token">One-time setup key</label>
              <input
                id="team-sync-setup-token"
                type="password"
                value={setupToken}
                onChange={(event) => setSetupToken(event.target.value)}
                autoComplete="off"
                minLength={32}
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-name">Team name</label>
              <input
                id="team-sync-name"
                value={teamName}
                onChange={(event) => setTeamName(event.target.value)}
                maxLength={80}
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-owner-name">Your name</label>
              <input
                id="team-sync-owner-name"
                value={ownerName}
                onChange={(event) => setOwnerName(event.target.value)}
                autoComplete="name"
                maxLength={80}
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-owner-email">Owner email</label>
              <input
                id="team-sync-owner-email"
                type="email"
                value={ownerEmail}
                onChange={(event) => setOwnerEmail(event.target.value)}
                autoComplete="email"
                disabled={busy}
                required
              />
              <label htmlFor="team-sync-owner-password">Create password</label>
              <input
                id="team-sync-owner-password"
                type="password"
                value={ownerPassword}
                onChange={(event) => setOwnerPassword(event.target.value)}
                autoComplete="new-password"
                minLength={12}
                maxLength={128}
                disabled={busy}
                required
              />
              <button className="primary" disabled={busy}>
                {busy ? "Creating team…" : "Create first team"}
              </button>
            </form>
          </details>
        </div>
      ) : null}
    </section>
  );
}
