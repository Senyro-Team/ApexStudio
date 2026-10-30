import { useEffect, useState } from "react";
import {
  robloxSettings,
  saveRobloxSettings,
  testRobloxConnection,
  clearRobloxSettings,
  errorMessage,
  type RobloxSettings as Settings,
} from "./bridge";
export default function RobloxSettings({
  onBusy,
}: {
  onBusy: (busy: boolean) => void;
}) {
  const [settings, setSettings] = useState<Settings | null>(null),
    [account, setAccount] = useState(""),
    [key, setKey] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  useEffect(() => {
    let active = true;
    robloxSettings()
      .then((s) => {
        if (active) {
          setSettings(s);
          setAccount(s.accountId ?? "");
        }
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, []);
  useEffect(() => {
    onBusy(busy || !!key || account !== (settings?.accountId ?? ""));
    return () => onBusy(false);
  }, [busy, key, account, settings, onBusy]);
  async function act(action: () => Promise<void>, message: string) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
      setNotice(message);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="roblox-settings" id="settings-roblox">
      <h2>Roblox</h2>
      <p className="field-hint">
        Personal-account decal uploads. Credentials are protected by Windows
        Credential Manager and excluded from backups and exports.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void act(async () => {
            await saveRobloxSettings(account, key);
            setKey("");
            setSettings(await robloxSettings());
          }, "Roblox connection verified and saved.");
        }}
      >
        <label htmlFor="roblox-account">Account ID</label>
        <input
          id="roblox-account"
          value={account}
          onChange={(e) => setAccount(e.target.value)}
          inputMode="numeric"
          autoComplete="off"
          maxLength={20}
          disabled={busy}
        />
        <label htmlFor="roblox-key">
          {settings?.configured ? "Replace API key" : "API key"}
        </label>
        <input
          id="roblox-key"
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          autoComplete="new-password"
          spellCheck={false}
          maxLength={2000}
          disabled={busy}
        />
        <p className="field-hint">
          Grant Assets read and write access for this account. The saved key is
          never displayed.
        </p>
        <div className="button-row">
          <button
            className="primary"
            disabled={
              busy || !key.trim() || !/^[1-9][0-9]{0,19}$/.test(account.trim())
            }
          >
            {busy ? "Connecting…" : "Verify and save"}
          </button>
          <button
            type="button"
            className="secondary"
            disabled={
              busy ||
              !settings?.configured ||
              !!key ||
              account !== settings.accountId
            }
            onClick={() =>
              void act(testRobloxConnection, "Roblox connection verified.")
            }
          >
            Test saved connection
          </button>
          {settings?.configured && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await clearRobloxSettings();
                  setSettings({ configured: false, accountId: null });
                  setKey("");
                  setAccount("");
                }, "Roblox credentials removed.")
              }
            >
              Remove credentials
            </button>
          )}
          {(key || account !== (settings?.accountId ?? "")) && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => {
                setKey("");
                setAccount(settings?.accountId ?? "");
              }}
            >
              Discard changes
            </button>
          )}
        </div>
      </form>
    </section>
  );
}
