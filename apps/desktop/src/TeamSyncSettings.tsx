import { useEffect, useRef, useState, type FormEvent } from "react";
import { Check, CircleAlert, LogOut, RefreshCw, Server } from "lucide-react";
import {
  desktopAvailable,
  syncAcceptInvitation,
  syncCheckServer,
  syncConnectionStatus,
  syncDisconnect,
  syncInitialSetup,
  syncSignIn,
  type SyncConnectionStatus,
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
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [invitationToken, setInvitationToken] = useState("");
  const [inviteName, setInviteName] = useState("");
  const [inviteEmail, setInviteEmail] = useState("");
  const [invitePassword, setInvitePassword] = useState("");
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
        setConnection(result);
        setServerUrl(result.server.origin);
        initialUrl.current = result.server.origin;
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
    try {
      window.localStorage.setItem(ACTIVE_TEAM_KEY, id);
    } catch {
      // The selected team remains available for this session.
    }
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

  return (
    <section className="team-sync-settings" id="settings-team-sync">
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
                  <select
                    id="team-sync-team"
                    value={selectedTeam?.id ?? ""}
                    onChange={(event) => changeActiveTeam(event.target.value)}
                  >
                    {connectedUser.teams.map((team) => (
                      <option key={team.id} value={team.id}>{team.name}</option>
                    ))}
                  </select>
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
              <label htmlFor="team-sync-invite-token">Invitation link or token</label>
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
