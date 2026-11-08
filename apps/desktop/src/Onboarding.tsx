import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ArrowRight, Check, Chrome, Gamepad2, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

type SetupFeature = "roblox" | "erlcx";

export default function Onboarding({
  onClose,
  onOpenSettings,
  initialSelected,
}: {
  onClose: () => void;
  onOpenSettings: (category: "roblox" | "chrome") => void;
  initialSelected: SetupFeature[];
}) {
  const [selected, setSelected] = useState<SetupFeature[]>(initialSelected);
  const [stage, setStage] = useState<"choose" | "guide">("choose");
  const [active, setActive] = useState<SetupFeature>("roblox");
  const [extensionStatus, setExtensionStatus] = useState("");
  const [extensionBusy, setExtensionBusy] = useState(false);
  const dialog = useRef<HTMLElement>(null);

  useEffect(() => dialog.current?.focus(), []);

  function keepFocusInside(event: KeyboardEvent<HTMLElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab" || !dialog.current) return;
    const controls = dialog.current.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])',
    );
    if (!controls.length) return;
    const first = controls[0];
    const last = controls[controls.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function toggle(feature: SetupFeature) {
    setSelected((current) =>
      current.includes(feature)
        ? current.filter((item) => item !== feature)
        : [...current, feature],
    );
  }

  async function getExtension() {
    setExtensionBusy(true);
    setExtensionStatus("");
    try {
      await invoke("copy_erlcx_extension_to_downloads");
      await invoke("open_erlcx_extension_in_explorer");
      setExtensionStatus("The extension package is now in your Downloads folder.");
    } catch (error) {
      setExtensionStatus(error instanceof Error ? error.message : "Could not open the extension package.");
    } finally {
      setExtensionBusy(false);
    }
  }

  const features = [
    {
      id: "roblox" as const,
      name: "Roblox Upload",
      description: "Upload Roblox textures with your personal account.",
      icon: Gamepad2,
    },
    {
      id: "erlcx" as const,
      name: "ERLCX publishing",
      description: "Prepare your ERLCX submission with the Chrome extension.",
      icon: Chrome,
    },
  ];

  return (
    <div className="onboarding-backdrop" role="presentation">
      <section
        className="onboarding-dialog"
        ref={dialog}
        tabIndex={-1}
        onKeyDown={keepFocusInside}
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
      >
        <button
          className="onboarding-close"
          type="button"
            aria-label="Close setup"
          onClick={onClose}
        >
          <X size={19} aria-hidden="true" />
        </button>
        <div className="onboarding-brand-mark" aria-hidden="true">A</div>
        {stage === "choose" ? (
          <>
            <p className="onboarding-eyebrow">WELCOME TO APEX STUDIO</p>
            <h1 id="onboarding-title">What would you like to set up?</h1>
            <p className="onboarding-intro">
              Choose the features you want to use now. You can skip any step and
              continue later from Settings.
            </p>
            <div className="onboarding-options">
              {features.map(({ id, name, description, icon: Icon }) => {
                const activeOption = selected.includes(id);
                return (
                  <button
                    type="button"
                    className={`onboarding-option ${activeOption ? "selected" : ""}`}
                    aria-pressed={activeOption}
                    key={id}
                    onClick={() => toggle(id)}
                  >
                    <span className="onboarding-option-icon"><Icon size={21} aria-hidden="true" /></span>
                    <span className="onboarding-option-copy">
                      <strong>{name}</strong>
                      <small>{description}</small>
                    </span>
                    <span className="onboarding-check" aria-hidden="true">
                      {activeOption ? <Check size={15} /> : null}
                    </span>
                  </button>
                );
              })}
            </div>
            <div className="onboarding-actions">
              <button className="text-button" type="button" onClick={onClose}>Set up later</button>
              <button
                className="primary"
                type="button"
                disabled={!selected.length}
                onClick={() => {
                  setActive(selected[0]);
                  setStage("guide");
                }}
              >
                Continue <ArrowRight size={16} aria-hidden="true" />
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="onboarding-eyebrow">SETUP · {selected.indexOf(active) + 1} OF {selected.length}</p>
            <h1 id="onboarding-title">{active === "roblox" ? "Connect Roblox uploads" : "Set up the ERLCX extension"}</h1>
            {active === "roblox" ? (
              <>
                <p className="onboarding-intro">
                  Apex Studio verifies your API key and stores it securely in Windows Credential Manager.
                  The key is excluded from backups.
                </p>
                <ol className="onboarding-steps">
                  <li><span>1</span><div><strong>Create an API key in Roblox</strong><small>Grant your personal account Assets read and write access.</small></div></li>
                  <li><span>2</span><div><strong>Have your account ID ready</strong><small>Use the numeric ID of your Roblox account.</small></div></li>
                  <li><span>3</span><div><strong>Verify the connection</strong><small>Apex checks the details before saving the key securely.</small></div></li>
                </ol>
              </>
            ) : (
              <>
                <p className="onboarding-intro">
                  The Apex extension helps prepare your ERLCX submission. Load it into Chrome separately;
                  Apex does not change browser security settings.
                </p>
                <ol className="onboarding-steps">
                  <li><span>1</span><div><strong>Open the extension package</strong><small>Extract it to a permanent folder so Chrome can find it again.</small></div></li>
                  <li><span>2</span><div><strong>Load it in Chrome</strong><small>Open chrome://extensions, turn on Developer mode, then choose “Load unpacked”.</small></div></li>
                  <li><span>3</span><div><strong>Connect it to Apex</strong><small>Enter Chrome's path, profile, and extension ID in Apex Studio, then check the connection.</small></div></li>
                </ol>
                <div className="onboarding-extension-action">
                  <button className="secondary" type="button" disabled={extensionBusy} onClick={() => void getExtension()}>
                    {extensionBusy ? "Preparing…" : "Get the extension package"}
                  </button>
                  {extensionStatus && <p role="status">{extensionStatus}</p>}
                </div>
                <p className="onboarding-note">The Chrome Web Store listing is not available yet, so load the extension manually in Chrome.</p>
              </>
            )}
            <div className="onboarding-actions onboarding-guide-actions">
              <button className="text-button" type="button" onClick={onClose}>Continue later</button>
              <div>
                {selected.length > 1 && (
                  <button className="secondary" type="button" onClick={() => setActive(active === "roblox" ? "erlcx" : "roblox")}>
                    {active === "roblox" ? "Next: ERLCX" : "Back: Roblox"}
                  </button>
                )}
                <button className="primary" type="button" onClick={() => onOpenSettings(active === "roblox" ? "roblox" : "chrome")}>
                  {active === "roblox" ? "Open Roblox settings" : "Open Chrome settings"}
                </button>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
