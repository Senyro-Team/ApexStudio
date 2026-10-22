// Serialized by chrome.scripting into the isolated world; keep this function self-contained.
export async function fillCurrentStep(bundle, options = {}) {
  const result = { step: "Unknown", filled: [], skipped: [], attention: [] };
  const normal = (s) => (s ?? "").replace(/\s+/g, " ").trim();
  if (
    location.origin !== "https://erlcx.com" ||
    !/^\/upload\/?$/.test(location.pathname) ||
    window.top !== window.self
  ) {
    result.attention.push("Open https://erlcx.com/upload in the active tab.");
    return result;
  }
  const main = document.querySelector("main");
  if (!main) {
    result.attention.push(
      "Upload form not found. Sign in and open the upload wizard.",
    );
    return result;
  }
  if (globalThis.__apexFillRunning) {
    result.attention.push("A fill operation is already running on this page.");
    return result;
  }
  globalThis.__apexFillRunning = true;
  try {
    const visible = (e) =>
      e &&
      !e.closest("[hidden]") &&
      getComputedStyle(e).display !== "none" &&
      getComputedStyle(e).visibility !== "hidden";
    const inputs = (selector, root = main) =>
      [...root.querySelectorAll(selector)].filter(visible);
    const button = (name, root = main) => {
      const matches = inputs("button", root).filter(
        (b) => normal(b.textContent) === name,
      );
      return matches.length === 1 ? matches[0] : null;
    };
    const pause = () => new Promise((resolve) => setTimeout(resolve, 30));
    const state = (globalThis.__apexFillState ??= new Map());
    let record = state.get(bundle.exportId);
    if (!record) {
      record = { changes: [], tags: [], media: false };
      state.set(bundle.exportId, record);
    }
    if (options.undo) {
      result.step = "Undo";
      for (const change of [...record.changes].reverse()) {
        if (
          change.element.isConnected &&
          change.element.value === change.after
        ) {
          const proto =
            change.element instanceof HTMLTextAreaElement
              ? HTMLTextAreaElement.prototype
              : HTMLInputElement.prototype;
          Object.getOwnPropertyDescriptor(proto, "value").set.call(
            change.element,
            change.before,
          );
          change.element.dispatchEvent(new Event("input", { bubbles: true }));
          change.element.dispatchEvent(new Event("change", { bubbles: true }));
          result.filled.push(change.name);
        } else
          result.skipped.push(
            change.name + ": changed or no longer on this step",
          );
      }
      for (const tag of record.tags) {
        const e = inputs('button[aria-label^="Remove tag "]').find(
          (e) => e.getAttribute("aria-label") === "Remove tag " + tag,
        );
        if (e) {
          e.click();
          await pause();
        }
      }
      state.delete(bundle.exportId);
      if (record.media)
        result.attention.push(
          "Remove attached preview images manually if needed.",
        );
      return result;
    }
    async function setField(e, value, name, defaults = []) {
      if (!e || e.disabled || e.readOnly) {
        result.attention.push(name + ": field unavailable");
        return false;
      }
      const before = e.value,
        after =
          e.type === "color"
            ? String(value ?? "").toLowerCase()
            : String(value ?? "");
      if (before === after) {
        result.skipped.push(name + ": already matches");
        return true;
      }
      if (before && !defaults.includes(before) && !options.replace) {
        result.attention.push(name + ": existing value left unchanged");
        return false;
      }
      const descriptor = Object.getOwnPropertyDescriptor(
        e instanceof HTMLTextAreaElement
          ? HTMLTextAreaElement.prototype
          : HTMLInputElement.prototype,
        "value",
      );
      descriptor.set.call(e, after);
      e.dispatchEvent(new Event("input", { bubbles: true }));
      e.dispatchEvent(new Event("change", { bubbles: true }));
      await pause();
      if (!e.isConnected || e.value !== after) {
        result.attention.push(name + ": the site did not retain the value");
        return false;
      }
      record.changes.push({ element: e, before, after, name });
      result.filled.push(name);
      return true;
    }
    const title = main.querySelector("#title"),
      upload = main.querySelector('input[type="file"]'),
      vehicles = () =>
        inputs('input[placeholder="Vehicle name (e.g., Crown Victoria)"]');
    const liveryButtons = inputs("button").filter(
      (b) =>
        /^LiveriesAdd liveries codes to your post$/i.test(
          normal(b.textContent),
        ) ||
        /^Liveries Add liveries codes to your post$/i.test(
          normal(b.textContent),
        ),
    );
    if (title) {
      result.step = "Details";
      if (options.expectedStep && options.expectedStep !== result.step) {
        result.attention.push(
          "The wizard step changed. Invoke Fill current step again.",
        );
        return result;
      }
      await setField(title, bundle.title, "Title");
      const description = inputs("textarea").filter((e) =>
        e.placeholder.startsWith("Write your post description"),
      );
      await setField(
        description.length === 1 ? description[0] : null,
        bundle.description,
        "Description",
      );
      const tagInput = main.querySelector(
        'input[placeholder="Type a tag and press Enter..."]',
      );
      if (!tagInput && bundle.tags.length)
        result.attention.push("Tags: field unavailable");
      if (tagInput) {
        for (const tag of bundle.tags) {
          const existing = inputs('button[aria-label^="Remove tag "]').map(
            (e) => e.getAttribute("aria-label").slice(11).toLowerCase(),
          );
          if (existing.includes(tag.toLowerCase())) {
            result.skipped.push("Tag " + tag + ": already present");
            continue;
          }
          if (existing.length >= 10) {
            result.attention.push("Tags: the site already has ten tags");
            break;
          }
          if (tagInput.value && !options.replace) {
            result.attention.push("Tags: unfinished text left unchanged");
            break;
          }
          await setField(tagInput, tag, "Tag " + tag);
          tagInput.dispatchEvent(
            new KeyboardEvent("keydown", {
              key: "Enter",
              code: "Enter",
              keyCode: 13,
              which: 13,
              bubbles: true,
            }),
          );
          await pause();
          if (
            inputs('button[aria-label^="Remove tag "]').some(
              (e) =>
                e.getAttribute("aria-label").slice(11).toLowerCase() ===
                tag.toLowerCase(),
            )
          ) {
            record.tags.push(tag);
          } else result.attention.push("Tag " + tag + ": confirm manually");
        }
      }
    } else if (upload) {
      result.step = "Media";
      if (options.expectedStep && options.expectedStep !== result.step) {
        result.attention.push(
          "The wizard step changed. Invoke Fill current step again.",
        );
        return result;
      }
      if (bundle.youtubeUrl)
        await setField(
          main.querySelector("#youtubeUrl"),
          bundle.youtubeUrl,
          "YouTube URL",
        );
      if (record.media) {
        result.skipped.push("Previews: already attached for this bundle");
        return result;
      }
      const imageCount = normal(main.textContent).match(
        /Images\s*\*?\s*\(\s*(\d+)\s*\/\s*10/,
      );
      if ((imageCount && Number(imageCount[1]) > 0) || upload.files?.length) {
        result.attention.push(
          "Previews: existing images left unchanged. Remove them manually before applying this bundle.",
        );
        return result;
      }
      const staged = globalThis.__apexMedia?.get(bundle.exportId);
      if (!staged || staged.files.length !== bundle.media.length) {
        result.attention.push(
          "Previews: import and stage the bundle images first.",
        );
        return result;
      }
      const transfer = new DataTransfer();
      for (const file of staged.files) transfer.items.add(file);
      upload.files = transfer.files;
      upload.dispatchEvent(new Event("change", { bubbles: true }));
      record.media = true;
      globalThis.__apexMedia.delete(bundle.exportId);
      result.filled.push(`${transfer.files.length} preview images`);
      result.attention.push(
        "Check image upload completion and order before continuing.",
      );
    } else if (vehicles().length || button("Add Livery Code")) {
      result.step = "Codes";
      if (options.expectedStep && options.expectedStep !== result.step) {
        result.attention.push(
          "The wizard step changed. Invoke Fill current step again.",
        );
        return result;
      }
      let list = vehicles();
      if (list.length > bundle.liveries.length) {
        result.attention.push(
          "Existing extra livery rows require manual review.",
        );
        return result;
      }
      while (list.length < bundle.liveries.length) {
        const add = button("Add Livery Code");
        if (!add) {
          result.attention.push("Add Livery Code control unavailable");
          return result;
        }
        const before = list.length;
        add.click();
        await pause();
        list = vehicles();
        if (list.length <= before) {
          result.attention.push("The site did not add a livery row.");
          return result;
        }
      }
      for (let i = 0; i < bundle.liveries.length; i++) {
        const l = bundle.liveries[i],
          field = vehicles()[i];
        if (!(await setField(field, l.vehicle, `Vehicle ${i + 1}`))) continue;
        let root = field.parentElement;
        while (
          root &&
          root !== main &&
          !(
            button("Add Side", root) &&
            root.querySelectorAll(
              'input[placeholder="Vehicle name (e.g., Crown Victoria)"]',
            ).length === 1
          )
        )
          root = root.parentElement;
        if (!root || root === main) {
          result.attention.push(`Livery ${i + 1}: unsupported row layout`);
          continue;
        }
        async function colour(index, value, name, defaults) {
          const direct = inputs('input[type="color"]', root);
          if (direct[index])
            return setField(direct[index], value, name, defaults);
          const triggers = inputs("button", root).filter((b) =>
            /^#[a-f0-9]{6}$/i.test(normal(b.textContent)),
          );
          const trigger = triggers[index];
          if (!trigger) {
            result.attention.push(name + ": colour picker unavailable");
            return false;
          }
          const before = normal(trigger.textContent).toLowerCase();
          if (before === value.toLowerCase()) {
            result.skipped.push(name + ": already matches");
            return true;
          }
          if (
            !defaults.map((v) => v.toLowerCase()).includes(before) &&
            !options.replace
          ) {
            result.attention.push(name + ": existing value left unchanged");
            return false;
          }
          trigger.click();
          await pause();
          const panels = [
            ...document.querySelectorAll(
              '[data-slot="popover-content"],[role="dialog"]',
            ),
          ].filter(visible);
          const candidates = [
            ...new Set(
              panels.flatMap((p) => [
                ...p.querySelectorAll('input[placeholder="#000000"]'),
              ]),
            ),
          ].filter(visible);
          const success = await setField(
            candidates.length === 1 ? candidates[0] : null,
            value.toLowerCase(),
            name,
            defaults.map((v) => v.toLowerCase()),
          );
          if (trigger.isConnected) trigger.click();
          await pause();
          return success;
        }
        await colour(0, l.primaryColor, `Primary colour ${i + 1}`, ["#ffffff"]);
        const hasSecondary = () =>
          inputs("button", root).some(
            (b) => b.title === "Remove secondary colour",
          ) ||
          inputs('input[type="color"]', root).length > 1 ||
          inputs("button", root).filter((b) =>
            /^#[a-f0-9]{6}$/i.test(normal(b.textContent)),
          ).length > 1;
        if (l.secondaryColor) {
          if (!hasSecondary()) {
            const enable = inputs("button", root).find((b) =>
              /^(Add|Enable) Secondary Colou?r$/i.test(normal(b.textContent)),
            );
            if (enable) {
              enable.click();
              await pause();
            }
          }
          await colour(1, l.secondaryColor, `Secondary colour ${i + 1}`, [
            "#000000",
          ]);
        } else if (hasSecondary())
          result.attention.push(
            `Livery ${i + 1}: secondary colour is enabled on the site; review manually`,
          );
        const opacity = inputs('input[type="number"]', root);
        await setField(
          opacity.length === 1 ? opacity[0] : null,
          l.opacity,
          `Opacity ${i + 1}`,
          ["100"],
        );
        const sideFields = () =>
          inputs('input[placeholder="Side (e.g. Left, Right)"]', root);
        const idFields = () => inputs('input[placeholder="Decal ID"]', root);
        if (sideFields().length > l.sides.length) {
          result.attention.push(
            `Livery ${i + 1}: extra side rows require manual review`,
          );
          continue;
        }
        while (sideFields().length < l.sides.length) {
          const before = sideFields().length,
            add = button("Add Side", root);
          if (!add) break;
          add.click();
          await pause();
          if (sideFields().length <= before) break;
        }
        for (let j = 0; j < l.sides.length; j++) {
          await setField(
            sideFields()[j],
            l.sides[j].side,
            `Side ${i + 1}.${j + 1}`,
          );
          await setField(
            idFields()[j],
            l.sides[j].decalId,
            `Decal ID ${i + 1}.${j + 1}`,
          );
        }
      }
    } else if (main.querySelector("#robloxGamepassUrl")) {
      result.step = "Monetization";
      if (options.expectedStep && options.expectedStep !== result.step) {
        result.attention.push(
          "The wizard step changed. Invoke Fill current step again.",
        );
        return result;
      }
      result.attention.push(
        "Review monetization yourself. Apex does not change paid settings.",
      );
    } else if (main.querySelector("#guidelines") || button("Publish Post")) {
      result.step = "Submit";
      if (options.expectedStep && options.expectedStep !== result.step) {
        result.attention.push(
          "The wizard step changed. Invoke Fill current step again.",
        );
        return result;
      }
      result.attention.push(
        "Review the post, agreement and publishing settings yourself. No submission controls were used.",
      );
    } else if (liveryButtons.length === 1) {
      result.step = "Categories";
      if (options.expectedStep && options.expectedStep !== result.step) {
        result.attention.push(
          "The wizard step changed. Invoke Fill current step again.",
        );
        return result;
      }
      if (
        inputs("button").some(
          (b) =>
            /^(Uniforms|ELS|Map Templates|UGC) ?Add/i.test(
              normal(b.textContent),
            ) && b.classList.contains("border-primary"),
        )
      ) {
        result.attention.push(
          "Only the livery-only wizard is supported. Review selected categories manually.",
        );
        return result;
      }
      const choice = liveryButtons[0];
      if (choice.classList.contains("border-primary"))
        result.skipped.push("Liveries: already selected");
      else if (choice.classList.contains("border-border/60")) {
        choice.click();
        result.filled.push("Liveries category");
      } else
        result.attention.push(
          "Category selection state is unknown; choose Liveries manually.",
        );
      result.attention.push(
        "Check Post as before continuing. Apex does not change the author.",
      );
    } else
      result.attention.push(
        "Unsupported wizard layout. No fields were changed.",
      );
    return result;
  } finally {
    globalThis.__apexFillRunning = false;
  }
}
export async function stageImageChunk(exportId, meta, chunk, final) {
  if (
    location.origin !== "https://erlcx.com" ||
    !/^\/upload\/?$/.test(location.pathname)
  )
    throw Error("Wrong destination.");
  if (!document.querySelector('main input[type="file"]'))
    throw Error(
      "The wizard step changed. Return to Media before filling images.",
    );
  const map = (globalThis.__apexMedia ??= new Map());
  if (!map.has(exportId)) map.set(exportId, { files: [], parts: [], size: 0 });
  const stage = map.get(exportId);
  if (chunk !== null) {
    const binary = atob(chunk);
    const part = Uint8Array.from(binary, (c) => c.charCodeAt(0));
    stage.size += part.length;
    if (stage.size > 20 * 1024 * 1024) {
      map.delete(exportId);
      throw Error("Image exceeds the size limit.");
    }
    stage.parts.push(part);
  }
  if (final) {
    const bytes = new Uint8Array(stage.size);
    let offset = 0;
    for (const part of stage.parts) {
      bytes.set(part, offset);
      offset += part.length;
    }
    const hash = Array.from(
      new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
      (b) => b.toString(16).padStart(2, "0"),
    ).join("");
    if (hash !== meta.hash) {
      map.delete(exportId);
      throw Error("Image integrity check failed.");
    }
    stage.files.push(new File([bytes], meta.name, { type: "image/png" }));
    stage.parts = [];
    stage.size = 0;
  }
  return true;
}
export function clearStagedMedia(all = false) {
  delete globalThis.__apexMedia;
  if (all) delete globalThis.__apexFillState;
}
