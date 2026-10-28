// Serialized into the ERLCX isolated world. Deliberately has no publication action.
export function wizardState(bundle, advance = false) {
  const normal = (s) => (s ?? "").replace(/\s+/g, " ").trim();
  const visible = (e) =>
    e &&
    !e.closest("[hidden]") &&
    getComputedStyle(e).display !== "none" &&
    getComputedStyle(e).visibility !== "hidden";
  const main = document.querySelector("main");
  const blocked = (reason) => ({ step: "", ready: false, reason });
  if (
    location.origin !== "https://erlcx.com" ||
    !/^\/upload\/?$/.test(location.pathname) ||
    window.top !== window.self
  )
    return blocked("Open the ERLCX upload wizard and sign in.");
  if (!main) return blocked("Upload form unavailable. Sign in to ERLCX.");
  if (
    [
      ...document.querySelectorAll(
        'iframe[title*="challenge" i],iframe[src*="captcha" i],.g-recaptcha,[data-hcaptcha-widget-id]',
      ),
    ].some(visible)
  )
    return blocked("Complete the verification yourself before resuming.");
  const fields = (selector) =>
      [...main.querySelectorAll(selector)].filter(visible),
    buttons = () => fields("button"),
    button = (name) => {
      const b = buttons().filter((b) => normal(b.textContent) === name);
      return b.length === 1 ? b[0] : null;
    };
  let step = "",
    ready = false,
    waiting = false,
    reason = "";
  if (main.querySelector("#guidelines") || button("Publish Post")) {
    return {
      step: "Submit",
      ready: false,
      reason:
        "Review the final page yourself. No agreement or publication controls were used.",
    };
  }
  if (main.querySelector("#title")) {
    step = "Details";
    const title = main.querySelector("#title"),
      description = fields("textarea").find((e) =>
        e.placeholder.startsWith("Write your post description"),
      ),
      tags = fields('button[aria-label^="Remove tag "]').map((b) =>
        b.getAttribute("aria-label").slice(11).toLowerCase(),
      );
    ready =
      title.value === bundle.title &&
      description?.value === bundle.description &&
      tags.length === bundle.tags.length &&
      bundle.tags.every((t) => tags.includes(t.toLowerCase()));
  } else if (main.querySelector('input[type="file"]')) {
    step = "Media";
    const text = normal(main.textContent),
      match = text.match(/Images\s*\*?\s*\(\s*(\d+)\s*\/\s*10/),
      count = match ? Number(match[1]) : -1;
    const uploaded = fields("img").filter(
      (i) =>
        i.getAttribute("alt")?.match(/preview|uploaded|image/i) &&
        i.complete &&
        i.naturalWidth > 0,
    );
    waiting =
      /uploading|processing|please wait/i.test(text) ||
      fields('[role="progressbar"],[aria-busy="true"]').length > 0;
    ready =
      count === bundle.media.length && !waiting && uploaded.length >= count;
    reason = waiting
      ? "Waiting for preview uploads."
      : "Verify that every preview finished uploading in the original order.";
    if (
      bundle.youtubeUrl &&
      main.querySelector("#youtubeUrl")?.value !== bundle.youtubeUrl
    )
      ready = false;
  } else if (
    fields('input[placeholder="Vehicle name (e.g., Crown Victoria)"]').length ||
    button("Add Livery Code")
  ) {
    step = "Codes";
    const v = fields(
      'input[placeholder="Vehicle name (e.g., Crown Victoria)"]',
    );
    const ids = fields('input[placeholder="Decal ID"]');
    ready =
      v.length === bundle.liveries.length &&
      v.every((e, i) => e.value === bundle.liveries[i].vehicle) &&
      ids.length === bundle.liveries.reduce((n, l) => n + l.sides.length, 0) &&
      ids.every(
        (e, i) =>
          e.value ===
          bundle.liveries.flatMap((l) => l.sides.map((s) => s.decalId))[i],
      );
    ready =
      ready &&
      v.every((field, i) => {
        let root = field.parentElement;
        while (
          root &&
          root !== main &&
          !(
            [...root.querySelectorAll("button")].some(
              (b) => normal(b.textContent) === "Add Side",
            ) &&
            root.querySelectorAll(
              'input[placeholder="Vehicle name (e.g., Crown Victoria)"]',
            ).length === 1
          )
        )
          root = root.parentElement;
        if (!root || root === main) return false;
        const l = bundle.liveries[i],
          sides = [
            ...root.querySelectorAll(
              'input[placeholder="Side (e.g. Left, Right)"]',
            ),
          ].filter(visible),
          opacity = [...root.querySelectorAll('input[type="number"]')].filter(
            visible,
          ),
          direct = [...root.querySelectorAll('input[type="color"]')].filter(
            visible,
          ),
          triggers = [...root.querySelectorAll("button")].filter(
            (b) => visible(b) && /^#[a-f0-9]{6}$/i.test(normal(b.textContent)),
          ),
          colors = direct.length
            ? direct.map((e) => e.value.toLowerCase())
            : triggers.map((b) => normal(b.textContent).toLowerCase());
        return (
          sides.length === l.sides.length &&
          sides.every((s, j) => s.value === l.sides[j].side) &&
          opacity.length === 1 &&
          Number(opacity[0].value) === l.opacity &&
          colors.length === (l.secondaryColor ? 2 : 1) &&
          colors[0] === l.primaryColor.toLowerCase() &&
          (!l.secondaryColor || colors[1] === l.secondaryColor.toLowerCase())
        );
      });
  } else if (main.querySelector("#robloxGamepassUrl")) {
    step = "Monetization";
    const urls = fields(
      'input[type="url"],input[id*="gamepass" i],input[id*="discord" i]',
    );
    ready =
      urls.every((e) => !e.value.trim()) &&
      fields('input[type="number"]').every(
        (e) => !e.value || Number(e.value) === 0,
      ) &&
      !fields('input[type="checkbox"],button[role="switch"]').some(
        (e) => e.checked || e.getAttribute("aria-checked") === "true",
      );
    reason =
      "Remove paid or gated access settings to use the free livery workflow.";
  } else {
    const categories = buttons().filter((b) =>
      /^(Liveries|Uniforms|ELS|Map Templates|UGC)\s*Add/i.test(
        normal(b.textContent),
      ),
    );
    if (categories.length) {
      step = "Categories";
      ready =
        categories.filter((b) => b.classList.contains("border-primary"))
          .length === 1 &&
        categories
          .find((b) => /^Liveries\s*Add/i.test(normal(b.textContent)))
          ?.classList.contains("border-primary");
    }
  }
  if (!step)
    return blocked("Unknown ERLCX layout. Review the page before resuming.");
  const errors = fields('[role="alert"],[aria-invalid="true"]');
  if (errors.length) {
    ready = false;
    reason = "ERLCX reports a validation error. Review the current step.";
  }
  if (advance) {
    const next = button("Continue");
    if (!ready || !next || next.disabled)
      return {
        step,
        ready: false,
        waiting,
        reason: reason || "The current step is incomplete.",
      };
    next.click();
    return { step, ready: true, advanced: true };
  }
  return { step, ready, waiting, reason };
}
export function permitsAdvance(step) {
  return ["Categories", "Details", "Media", "Codes", "Monetization"].includes(
    step,
  );
}

// Chrome can report a complete document before ERLCX has hydrated its wizard.
// Only the unrecognised-layout state is retried; security and login stops remain immediate.
export async function waitForWizard(read, wait, attempts = 40) {
  for (let attempt = 0; ; attempt++) {
    const state = await read();
    if (
      state.step ||
      state.reason !==
        "Unknown ERLCX layout. Review the page before resuming." ||
      attempt >= attempts - 1
    )
      return state;
    await wait(250);
  }
}
