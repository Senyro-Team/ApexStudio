const step = document.getElementById("step");
const status = document.getElementById("status");
let started = false;

function showCheckError(message) {
  step.textContent = "Connection failed";
  status.textContent = message;
}

window.apexHandoffStarted = () => {
  started = true;
  clearTimeout(startupTimer);
};

const startupTimer = setTimeout(() => {
  if (started) return;
  step.textContent = "Extension failed to start";
  status.textContent =
    "The handoff page did not start. Check the extension files and reload Apex Studio’s extension.";
}, 8000);

const checkParams = new URLSearchParams(location.hash.slice(1));
if (checkParams.has("check")) {
  started = true;
  clearTimeout(startupTimer);
  window.apexConnectionCheckActive = true;
  const token = checkParams.get("check");
  try {
    history.replaceState(null, "", location.pathname);
    step.textContent = "Connecting to Apex Studio…";
    status.textContent = "Opening the native messaging connection…";
    if (typeof chrome.runtime.connectNative !== "function") {
      throw Error(
        "Native messaging is unavailable. Check the extension permissions and reload the extension.",
      );
    }
    const port = chrome.runtime.connectNative("com.apexstudio.erlcx");
    let settled = false;
    const timeout = setTimeout(() => {
      if (settled) return;
      settled = true;
      showCheckError(
        "Apex Studio did not answer the browser connection check.",
      );
      port.disconnect();
    }, 20000);
    port.onMessage.addListener((response) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (response?.error) {
        showCheckError(response.error);
      } else if (response?.ok === true) {
        step.textContent = "Connected";
        status.textContent =
          "Apex Studio and this Chrome profile are connected.";
      } else {
        showCheckError("Apex Studio returned an invalid connection response.");
      }
      port.disconnect();
    });
    port.onDisconnect.addListener(() => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      const error = chrome.runtime.lastError?.message;
      showCheckError(
        error
          ? `Could not start the Apex Studio native host: ${error}`
          : "The connection to the Apex Studio native host closed before it replied.",
      );
    });
    port.postMessage({ kind: "check", token });
  } catch (error) {
    showCheckError(error instanceof Error ? error.message : String(error));
  }
} else {
  const moduleScript = document.createElement("script");
  moduleScript.id = "handoff-module";
  moduleScript.type = "module";
  moduleScript.src = "handoff.js";
  moduleScript.addEventListener("error", () => {
    step.textContent = "Extension failed to load";
    status.textContent =
      "Apex Studio’s handoff script could not load. Reload the extension and try again.";
  });
  document.body.append(moduleScript);
}
