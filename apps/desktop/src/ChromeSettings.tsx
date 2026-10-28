import { useEffect, useState } from "react";
import {
  chromeSettings,
  saveChromeSettings,
  checkBrowser,
  browserStatus,
  errorMessage,
  type ChromeConfig,
  type BrowserStatus,
} from "./bridge";
const empty: ChromeConfig = {
  chromePath: "",
  profile: "Default",
  extensionId: "",
};
export default function ChromeSettings({
  onBusy,
}: {
  onBusy: (v: boolean) => void;
}) {
  const [config, setConfig] = useState(empty),
    [saved, setSaved] = useState(empty),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [status, setStatus] = useState<BrowserStatus | null>(null);
  useEffect(() => {
    void chromeSettings()
      .then((c) => {
        setConfig(c);
        setSaved(c);
      })
      .catch((e) => setError(errorMessage(e)));
    let active = true;
    const poll = () => {
      void browserStatus()
        .then((s) => {
          if (active) setStatus(s);
        })
        .catch(() => {});
    };
    poll();
    const t = setInterval(poll, 1500);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, []);
  const dirty = JSON.stringify(config) !== JSON.stringify(saved);
  useEffect(() => {
    onBusy(busy || dirty);
    return () => onBusy(false);
  }, [busy, dirty, onBusy]);
  async function act(check: boolean) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      if (check) {
        await checkBrowser();
        setNotice("Complete the connection check in the opened Chrome tab.");
      } else {
        await saveChromeSettings(config);
        setSaved(config);
        setNotice("Chrome settings saved.");
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="roblox-settings">
      <h2>ERLCX · Chrome</h2>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void act(false);
        }}
      >
        <label>
          Chrome executable
          <input
            value={config.chromePath}
            placeholder="C:\Program Files\Google\Chrome\Application\chrome.exe"
            disabled={busy}
            onChange={(e) =>
              setConfig({ ...config, chromePath: e.target.value })
            }
          />
        </label>
        <label>
          Profile directory
          <input
            value={config.profile}
            placeholder="Default or Profile 1"
            disabled={busy}
            onChange={(e) => setConfig({ ...config, profile: e.target.value })}
          />
        </label>
        <label>
          Installed extension ID
          <input
            value={config.extensionId}
            maxLength={32}
            spellCheck={false}
            disabled={busy}
            onChange={(e) =>
              setConfig({ ...config, extensionId: e.target.value.trim() })
            }
          />
        </label>
        <div className="button-row">
          <button className="primary" disabled={busy || !dirty}>
            Save Chrome connection
          </button>
          {dirty && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => setConfig(saved)}
            >
              Discard changes
            </button>
          )}
          <button
            type="button"
            className="secondary"
            disabled={busy || dirty || !saved.extensionId}
            onClick={() => void act(true)}
          >
            Check connection
          </button>
        </div>
      </form>
      <p role="status">
        {status?.connected
          ? "Chrome connected"
          : "Chrome connection not verified"}
      </p>
    </section>
  );
}
