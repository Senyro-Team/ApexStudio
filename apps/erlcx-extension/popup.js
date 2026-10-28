import { storage } from "./storage.js";
import { readBundle } from "./bundle.js";
import {
  fillCurrentStep,
  stageImageChunk,
  clearStagedMedia,
} from "./adapter.js";
const $ = (id) => document.getElementById(id);
$("assistant").addEventListener("click", () => {
  void chrome.tabs.create({ url: chrome.runtime.getURL("handoff.html") });
});
let draft = null,
  urls = [];
function report(message) {
  $("status").textContent = message;
}
function render() {
  for (const url of urls) URL.revokeObjectURL(url);
  urls = [];
  $("images").replaceChildren();
  $("codes").replaceChildren();
  $("draft").hidden = !draft;
  if (!draft) return;
  const m = draft.manifest;
  $("title").textContent = m.title;
  $("description").textContent = m.description;
  $("tags").textContent = m.tags.join(" · ");
  $("summary").textContent =
    `${m.media.length} previews · ${m.liveries.length} ${m.liveries.length === 1 ? "livery" : "liveries"}`;
  for (const image of draft.media) {
    const img = document.createElement("img"),
      url = URL.createObjectURL(image.blob);
    urls.push(url);
    img.src = url;
    img.alt = image.name;
    img.loading = "lazy";
    $("images").append(img);
  }
  for (const livery of m.liveries) {
    const p = document.createElement("p");
    p.textContent = `${livery.vehicle}: ${livery.sides.map((s) => `${s.side} / ${s.decalId}`).join(", ")}`;
    $("codes").append(p);
  }
}
async function activeTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (
    !tab?.id ||
    !tab.url ||
    new URL(tab.url).origin !== "https://erlcx.com" ||
    !/^\/upload\/?$/.test(new URL(tab.url).pathname)
  )
    throw Error("Open https://erlcx.com/upload in the active tab.");
  return tab.id;
}
async function run(tabId, func, args) {
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    world: "ISOLATED",
    func,
    args,
  });
  if (results.length !== 1) throw Error("Unexpected frame result.");
  return results[0].result;
}
function showResult(result) {
  $("report").replaceChildren();
  const h = document.createElement("h2");
  h.textContent = result.step;
  $("report").append(h);
  for (const [label, items] of [
    ["Filled", result.filled],
    ["Unchanged", result.skipped],
    ["Review", result.attention],
  ])
    for (const item of items) {
      const p = document.createElement("p");
      p.textContent = `${label}: ${item}`;
      $("report").append(p);
    }
}
function busy(value) {
  for (const id of ["fill", "undo", "clear", "bundle", "replace"])
    $(id).disabled = value;
}
$("bundle").addEventListener("change", async (e) => {
  const file = e.target.files?.[0];
  if (!file) return;
  busy(true);
  report("Checking bundle…");
  try {
    const automatic = await storage("read", undefined, "automatic");
    if (automatic && ["running", "paused"].includes(automatic.state))
      throw Error("Cancel the ERLCX assistant before importing another post.");
    if (file.size > 201 * 1024 * 1024) throw Error("Bundle exceeds 201 MB.");
    const checked = await readBundle(await file.arrayBuffer());
    await storage("write", checked);
    draft = checked;
    $("replace").checked = false;
    render();
    report("Bundle imported. Review the public content before filling.");
  } catch (error) {
    report(error.message);
  } finally {
    busy(false);
    e.target.value = "";
  }
});
$("fill").addEventListener("click", async () => {
  if (!draft) return;
  busy(true);
  try {
    const automatic = await storage("read", undefined, "automatic");
    if (automatic && ["running", "paused"].includes(automatic.state))
      throw Error(
        "Use the ERLCX assistant page, or cancel it before filling manually.",
      );
    const tabId = await activeTab();
    report("Filling current step…");
    const result = await run(tabId, fillCurrentStep, [
      draft.manifest,
      { replace: $("replace").checked },
    ]);
    if (
      result.step === "Media" &&
      result.attention.includes(
        "Previews: import and stage the bundle images first.",
      )
    ) {
      await run(tabId, clearStagedMedia, []);
      for (const image of draft.media) {
        const bytes = new Uint8Array(await image.blob.arrayBuffer());
        for (let i = 0; i < bytes.length; i += 192 * 1024) {
          const chunk = bytes.subarray(i, i + 192 * 1024);
          let binary = "";
          for (let j = 0; j < chunk.length; j += 8192)
            binary += String.fromCharCode(...chunk.subarray(j, j + 8192));
          await run(tabId, stageImageChunk, [
            draft.manifest.exportId,
            { name: image.name, hash: image.hash },
            btoa(binary),
            i + chunk.length === bytes.length,
          ]);
        }
      }
      showResult(
        await run(tabId, fillCurrentStep, [
          draft.manifest,
          { replace: $("replace").checked, expectedStep: "Media" },
        ]),
      );
    } else showResult(result);
    report("Review the current step before continuing on ERLCX.");
  } catch (error) {
    report(error.message);
  } finally {
    busy(false);
  }
});
$("undo").addEventListener("click", async () => {
  if (!draft) return;
  busy(true);
  try {
    const automatic = await storage("read", undefined, "automatic");
    if (automatic && ["running", "paused"].includes(automatic.state))
      throw Error("Cancel the ERLCX assistant before changing this bundle.");
    showResult(
      await run(await activeTab(), fillCurrentStep, [
        draft.manifest,
        { undo: true },
      ]),
    );
  } catch (e) {
    report(e.message);
  } finally {
    busy(false);
  }
});
$("clear").addEventListener("click", async () => {
  busy(true);
  try {
    const automatic = await storage("read", undefined, "automatic");
    if (automatic && ["running", "paused"].includes(automatic.state))
      throw Error("Cancel the ERLCX assistant before clearing this bundle.");
    await storage("clear");
    try {
      await run(await activeTab(), clearStagedMedia, [true]);
    } catch {}
    draft = null;
    render();
    $("report").replaceChildren();
    report(
      "Draft cleared from this extension. Existing ERLCX fields are unchanged.",
    );
  } catch (e) {
    report(e.message);
  } finally {
    busy(false);
  }
});
try {
  draft = await storage("read");
  render();
  if (draft) report("Saved bundle loaded.");
} catch (e) {
  report(e.message);
}
