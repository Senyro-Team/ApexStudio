import { readBundle } from "./bundle.js";
import { storage } from "./storage.js";
import { nativeConnection } from "./native.js";
const $ = (id) => document.getElementById(id);
let native = null,
  timer = null;
async function message(kind) {
  const result = await chrome.runtime.sendMessage({ kind });
  if (result?.error) throw Error(result.error);
  return result;
}
async function update() {
  try {
    const state = await message("status");
    $("step").textContent = state.step || "ERLCX";
    $("status").textContent = state.message || "No active handoff.";
    $("actions").hidden = !state.session;
    $("resume").disabled = state.state !== "paused";
    $("cancel").disabled = ["review", "cancelled"].includes(state.state);
    $("open").onclick = () => {
      if (state.tabId) void chrome.tabs.update(state.tabId, { active: true });
    };
  } catch (e) {
    $("status").textContent = e.message;
  }
}
$("resume").onclick = () =>
  void message("resume")
    .then(update)
    .catch((e) => ($("status").textContent = e.message));
$("cancel").onclick = () =>
  void message("cancel")
    .then(update)
    .catch((e) => ($("status").textContent = e.message));
async function init() {
  const params = new URLSearchParams(location.hash.slice(1));
  history.replaceState(null, "", location.pathname);
  if (!params.size) {
    await update();
    timer = setInterval(update, 1000);
    return;
  }
  native = nativeConnection();
  try {
    if (params.has("check")) {
      await native.request({ kind: "check", token: params.get("check") });
      $("step").textContent = "Connected";
      $("status").textContent =
        "Apex Studio and this Chrome profile are connected.";
      return;
    }
    const claimed = await native.request({
      kind: "claim",
      token: params.get("token"),
    });
    const active = await storage("read", undefined, "automatic");
    if (active && ["running", "paused"].includes(active.state)) {
      // A valid new app token supersedes only an expired or disconnected run.
      await message("cancel");
      const started = Date.now();
      while (["running", "paused"].includes((await message("status")).state)) {
        if (Date.now() - started > 20000)
          throw Error(
            "The previous assistant is still stopping. Cancel this handoff in Apex Studio and try again.",
          );
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    }
    if (
      !Number.isSafeInteger(claimed.size) ||
      claimed.size <= 0 ||
      claimed.size > 201 * 1024 * 1024
    )
      throw Error("Invalid bundle size.");
    const bytes = new Uint8Array(claimed.size);
    let offset = 0;
    while (offset < bytes.length) {
      $("status").textContent =
        `Receiving post… ${Math.floor((offset / bytes.length) * 100)}%`;
      const chunk = await native.request({
        kind: "chunk",
        session: claimed.session,
        offset,
      });
      const data = Uint8Array.from(atob(chunk.data), (c) => c.charCodeAt(0));
      if (
        !data.length ||
        data.length > 192 * 1024 ||
        offset + data.length > bytes.length ||
        chunk.offset !== offset + data.length
      )
        throw Error("Invalid bundle chunk.");
      bytes.set(data, offset);
      offset = chunk.offset;
    }
    const bundle = await readBundle(bytes.buffer);
    await storage("write", bundle);
    const tab = await chrome.tabs.create({
      url: "https://erlcx.com/upload",
      active: true,
    });
    await storage(
      "write",
      {
        session: claimed.session,
        tabId: tab.id,
        exportId: bundle.manifest.exportId,
        state: "running",
        step: "",
        message: "Opening ERLCX…",
      },
      "automatic",
    );
    await message("start");
    await update();
    timer = setInterval(update, 1000);
  } catch (e) {
    $("step").textContent = "Connection paused";
    $("status").textContent = e.message;
  } finally {
    native?.close();
    native = null;
  }
}
window.addEventListener("unload", () => {
  if (timer) clearInterval(timer);
  native?.close();
});
void init();
