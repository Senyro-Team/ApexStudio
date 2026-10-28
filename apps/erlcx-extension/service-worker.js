import { storage } from "./storage.js";
import { nativeConnection } from "./native.js";
import {
  fillCurrentStep,
  stageImageChunk,
  clearStagedMedia,
} from "./adapter.js";
import { wizardState, permitsAdvance } from "./automation.js";
let running = false,
  stop = false,
  resume = false,
  current = null,
  connection = null;
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function runScript(tabId, func, args) {
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    world: "ISOLATED",
    func,
    args,
  });
  if (results.length !== 1) throw Error("Unexpected ERLCX frame result.");
  return results[0].result;
}
async function persist() {
  await storage("write", current, "automatic");
}
async function report(state, step, message) {
  current = { ...current, state, step, message };
  await persist();
  const response = await connection.request({
    kind: "status",
    session: current.session,
    state,
    step,
    message,
  });
  if (response.command === "cancel") stop = true;
  if (response.command === "resume") resume = true;
  return response;
}
async function stage(tabId, bundle) {
  await runScript(tabId, clearStagedMedia, []);
  for (const image of bundle.media) {
    const bytes = new Uint8Array(await image.blob.arrayBuffer());
    for (let i = 0; i < bytes.length; i += 192 * 1024) {
      if (stop) return;
      const chunk = bytes.subarray(i, i + 192 * 1024);
      let binary = "";
      for (let j = 0; j < chunk.length; j += 8192)
        binary += String.fromCharCode(...chunk.subarray(j, j + 8192));
      await runScript(tabId, stageImageChunk, [
        bundle.manifest.exportId,
        { name: image.name, hash: image.hash },
        btoa(binary),
        i + chunk.length === bytes.length,
      ]);
    }
  }
}
async function execute() {
  if (running) return;
  running = true;
  stop = false;
  connection = nativeConnection();
  try {
    current = await storage("read", undefined, "automatic");
    if (!current) throw Error("No browser handoff is available.");
    const bundle = await storage("read");
    if (!bundle || bundle.manifest.exportId !== current.exportId)
      throw Error(
        "The active publishing bundle changed. Start again from Apex Studio.",
      );
    const loadingStarted = Date.now();
    while ((await chrome.tabs.get(current.tabId)).status !== "complete") {
      if (Date.now() - loadingStarted > 20000)
        throw Error(
          "ERLCX is still loading. Resume when the upload page is ready.",
        );
      await delay(250);
    }
    let previous = "",
      awaiting = false;
    while (!stop) {
      if (current.state === "paused") {
        await report("paused", current.step, current.message);
        if (!resume) {
          await delay(1500);
          continue;
        }
        resume = false;
        current.state = "running";
        previous = "";
        awaiting = false;
      }
      await report("running", current.step, "Checking ERLCX…");
      if (stop) break;
      let info = await runScript(current.tabId, wizardState, [
        bundle.manifest,
        false,
      ]);
      if (info.step === "Submit") {
        await report("review", "Submit", info.reason);
        return;
      }
      if (!info.step) {
        await report("paused", "", info.reason);
        continue;
      }
      if (awaiting) {
        if (info.step === previous) {
          await report(
            "paused",
            info.step,
            "The wizard did not advance. Review the page before resuming.",
          );
          continue;
        }
        awaiting = false;
      }
      let result = await runScript(current.tabId, fillCurrentStep, [
        bundle.manifest,
        { automatic: true, expectedStep: info.step },
      ]);
      if (
        result.step === "Media" &&
        result.attention.includes(
          "Previews: import and stage the bundle images first.",
        )
      ) {
        await stage(current.tabId, bundle);
        if (stop) break;
        result = await runScript(current.tabId, fillCurrentStep, [
          bundle.manifest,
          { automatic: true, expectedStep: "Media" },
        ]);
      }
      const attention = result.attention.filter(
        (a) =>
          !(
            (result.step === "Media" &&
              a ===
                "Check image upload completion and order before continuing.") ||
            (result.step === "Categories" &&
              a ===
                "Check Post as before continuing. Apex does not change the author.") ||
            (result.step === "Monetization" &&
              a ===
                "Review monetization yourself. Apex does not change paid settings.")
          ),
      );
      if (attention.length) {
        await report("paused", info.step, attention.join(" "));
        continue;
      }
      let checked = await runScript(current.tabId, wizardState, [
        bundle.manifest,
        false,
      ]);
      if (info.step === "Media") {
        const started = Date.now();
        while (!checked.ready && Date.now() - started < 90000) {
          await report("running", "Media", "Waiting for preview uploads…");
          if (stop) break;
          await delay(1000);
          checked = await runScript(current.tabId, wizardState, [
            bundle.manifest,
            false,
          ]);
          if (checked.step !== "Media") break;
        }
      }
      if (stop) break;
      if (
        !checked.ready ||
        checked.step !== info.step ||
        !permitsAdvance(info.step)
      ) {
        await report(
          "paused",
          info.step,
          checked.reason ||
            "The filled fields could not be verified. Review the page.",
        );
        continue;
      }
      // Reconcile native cancellation immediately before the only permitted navigation action.
      await report("running", info.step, "Step complete.");
      if (stop) break;
      const advanced = await runScript(current.tabId, wizardState, [
        bundle.manifest,
        true,
      ]);
      if (!advanced.advanced) {
        await report(
          "paused",
          info.step,
          advanced.reason || "Continue is unavailable.",
        );
        continue;
      }
      previous = info.step;
      awaiting = true;
      await delay(400);
    }
    await report(
      "cancelled",
      current?.step || "",
      "Browser handoff cancelled. The post was not submitted.",
    );
  } catch (e) {
    if (current) {
      current = {
        ...current,
        state: stop || e.cancelled ? "cancelled" : "paused",
        message: e.message,
      };
      await persist();
      try {
        await connection.request({
          kind: "status",
          session: current.session,
          state: "paused",
          step: current.step || "",
          message: e.message,
        });
      } catch {
        /* App restart invalidates the session. */
      }
    }
  } finally {
    connection?.close();
    connection = null;
    running = false;
  }
}
chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (
    sender.id !== chrome.runtime.id ||
    !sender.url?.startsWith(chrome.runtime.getURL("handoff.html"))
  ) {
    respond({ error: "Unapproved sender." });
    return false;
  }
  void (async () => {
    if (message.kind === "status") {
      let record = await storage("read", undefined, "automatic");
      if (record && !running && record.state === "running") {
        record = {
          ...record,
          state: "paused",
          message: "Assistant restarted. Review the current page and resume.",
        };
        await storage("write", record, "automatic");
      }
      respond(record || {});
      return;
    }
    if (message.kind === "start") {
      if (running) throw Error("A browser handoff is already running.");
      const record = await storage("read", undefined, "automatic");
      if (!record) throw Error("No approved handoff.");
      void execute();
      respond({ ok: true });
      return;
    }
    if (message.kind === "resume") {
      resume = true;
      if (!running) {
        current = await storage("read", undefined, "automatic");
        if (current) {
          current.state = "paused";
          await persist();
          void execute();
        }
      }
      respond({ ok: true });
      return;
    }
    if (message.kind === "cancel") {
      stop = true;
      if (!running) {
        const record = await storage("read", undefined, "automatic");
        if (record) {
          await storage(
            "write",
            { ...record, state: "cancelled" },
            "automatic",
          );
        }
      }
      respond({ ok: true });
      return;
    }
    throw Error("Unknown assistant action.");
  })().catch((e) => respond({ error: e.message }));
  return true;
});
