import { createHash, webcrypto } from "node:crypto";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { fillCurrentStep } from "../../erlcx-extension/adapter.js";
import { validateManifest, readBundle } from "../../erlcx-extension/bundle.js";
const manifest = () => ({
  version: 1,
  kind: "apex-publish",
  targetOrigin: "https://erlcx.com",
  exportId: "test-export",
  generatedAt: "2026-10-22T12:00:00Z",
  title: "Patrol test",
  description: "Public description",
  tags: ["Patrol"],
  youtubeUrl: null,
  media: [
    {
      path: `media/${"a".repeat(64)}.png`,
      hash: "a".repeat(64),
      name: "one.png",
      size: 24,
    },
    {
      path: `media/${"b".repeat(64)}.png`,
      hash: "b".repeat(64),
      name: "two.png",
      size: 24,
    },
  ],
  liveries: [
    {
      vehicle: "Vehicle",
      primaryColor: "#FFFFFF",
      secondaryColor: null,
      opacity: 100,
      sides: [{ side: "Left 1", decalId: "12345" }],
    },
  ],
});
let click;
beforeEach(() => {
  vi.stubGlobal("location", new URL("https://erlcx.com/upload"));
  document.body.innerHTML = "<main></main>";
  delete globalThis.__apexFillState;
  delete globalThis.__apexMedia;
  click = vi.fn((e) => {
    if (/^(Continue|Save Draft|Publish Post)$/.test(e.target.textContent))
      throw Error("Forbidden navigation or submission");
  });
  document.addEventListener("click", click);
});
afterEach(() => {
  document.removeEventListener("click", click);
  vi.unstubAllGlobals();
});
function fixture(html) {
  document.querySelector("main").innerHTML =
    html +
    "<button>Continue</button><button>Save Draft</button><button hidden>Publish Post</button>";
}
it("fills React-compatible Details fields, retains existing text, and does not duplicate tags", async () => {
  fixture(
    '<input id="title"><textarea placeholder="Write your post description here..."></textarea><div id="tags"></div><input placeholder="Type a tag and press Enter...">',
  );
  const input = document.querySelector("input[placeholder]");
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const b = document.createElement("button");
      b.setAttribute("aria-label", `Remove tag ${input.value}`);
      document.querySelector("#tags").append(b);
      input.value = "";
    }
  });
  const d = manifest();
  const result = await fillCurrentStep(d);
  expect(result.step).toBe("Details");
  expect(document.querySelector("#title").value).toBe(d.title);
  await fillCurrentStep(d);
  expect(document.querySelectorAll('[aria-label^="Remove tag"]')).toHaveLength(
    1,
  );
  document.querySelector("#title").value = "User edit";
  const conflict = await fillCurrentStep(d);
  expect(document.querySelector("#title").value).toBe("User edit");
  expect(conflict.attention.join()).toContain("left unchanged");
  expect(click).not.toHaveBeenCalled();
});
it("selects Liveries once and stops at the review and monetization boundaries", async () => {
  fixture(
    '<button class="border-border/60">LiveriesAdd liveries codes to your post</button>',
  );
  const b = document.querySelector("button");
  b.addEventListener("click", () => b.classList.add("border-primary"));
  expect((await fillCurrentStep(manifest())).step).toBe("Categories");
  await fillCurrentStep(manifest());
  expect(click).toHaveBeenCalledTimes(1);
  fixture('<input id="robloxGamepassUrl" value="">');
  expect((await fillCurrentStep(manifest())).step).toBe("Monetization");
  expect(document.querySelector("input").value).toBe("");
  fixture('<input id="guidelines" type="checkbox">');
  expect((await fillCurrentStep(manifest())).step).toBe("Submit");
  expect(document.querySelector("input").checked).toBe(false);
});
it("fills dynamic livery/side rows and colour defaults without clicking submission controls", async () => {
  fixture('<div id="rows"></div><button id="add">Add Livery Code</button>');
  document.querySelector("#add").addEventListener("click", () => {
    const row = document.createElement("section");
    row.innerHTML =
      '<input placeholder="Vehicle name (e.g., Crown Victoria)"><input type="color" value="#ffffff"><input type="number" value="100"><div class="sides"></div><button>Add Side</button>';
    row.querySelector("button").addEventListener("click", () => {
      const side = document.createElement("div");
      side.innerHTML =
        '<input placeholder="Side (e.g. Left, Right)"><input placeholder="Decal ID">';
      row.querySelector(".sides").append(side);
    });
    document.querySelector("#rows").append(row);
  });
  const d = manifest();
  d.liveries[0].primaryColor = "#123456";
  const result = await fillCurrentStep(d);
  expect(result.step).toBe("Codes");
  expect(document.querySelector('input[placeholder="Decal ID"]').value).toBe(
    "12345",
  );
  expect(document.querySelector("input[type=color]").value).toBe("#123456");
  await fillCurrentStep(d);
  expect(document.querySelectorAll("section")).toHaveLength(1);
  expect(
    document.querySelectorAll('input[placeholder="Decal ID"]'),
  ).toHaveLength(1);
});
it("leaves existing media untouched and rejects wrong origins and unknown layouts", async () => {
  fixture('<p>Images * ( 1 / 10 )</p><input type="file" multiple>');
  expect((await fillCurrentStep(manifest())).attention.join()).toContain(
    "existing images left unchanged",
  );
  vi.stubGlobal("location", new URL("https://example.com/upload"));
  expect((await fillCurrentStep(manifest())).filled).toHaveLength(0);
  vi.stubGlobal("location", new URL("https://erlcx.com/upload"));
  fixture("<p>Unrecognized form</p>");
  document.querySelector("button:last-child").remove();
  expect((await fillCurrentStep(manifest())).step).toBe("Unknown");
});
it("rejects malicious metadata, missing media, oversized and invalid archives", async () => {
  const d = manifest();
  expect(validateManifest(d)).toBe(d);
  expect(() =>
    validateManifest({ ...d, targetOrigin: "https://evil.example" }),
  ).toThrow();
  expect(() => validateManifest({ ...d, privateNotes: "secret" })).toThrow();
  expect(() =>
    validateManifest({
      ...d,
      media: [{ ...d.media[0], path: "../secret" }, d.media[1]],
    }),
  ).toThrow();
  expect(() =>
    validateManifest({
      ...d,
      liveries: [
        {
          ...d.liveries[0],
          sides: [{ side: "Left 1", decalId: "javascript:alert(1)" }],
        },
      ],
    }),
  ).toThrow();
  await expect(readBundle(new Uint8Array(22).buffer)).rejects.toThrow();
});

function storedZip(entries) {
  const local = [],
    central = [];
  let offset = 0;
  for (const [name, bytes] of entries) {
    const n = new TextEncoder().encode(name),
      header = new Uint8Array(30 + n.length),
      h = new DataView(header.buffer);
    h.setUint32(0, 0x04034b50, true);
    h.setUint16(4, 20, true);
    h.setUint32(18, bytes.length, true);
    h.setUint32(22, bytes.length, true);
    h.setUint16(26, n.length, true);
    header.set(n, 30);
    local.push(header, bytes);
    const c = new Uint8Array(46 + n.length),
      v = new DataView(c.buffer);
    v.setUint32(0, 0x02014b50, true);
    v.setUint16(4, 20, true);
    v.setUint16(6, 20, true);
    v.setUint32(20, bytes.length, true);
    v.setUint32(24, bytes.length, true);
    v.setUint16(28, n.length, true);
    v.setUint32(42, offset, true);
    c.set(n, 46);
    central.push(c);
    offset += header.length + bytes.length;
  }
  const size = central.reduce((n, b) => n + b.length, 0),
    end = new Uint8Array(22),
    e = new DataView(end.buffer);
  e.setUint32(0, 0x06054b50, true);
  e.setUint16(8, entries.length, true);
  e.setUint16(10, entries.length, true);
  e.setUint32(12, size, true);
  e.setUint32(16, offset, true);
  const zip = new Uint8Array(offset + size + 22);
  let i = 0;
  for (const part of [...local, ...central, end]) {
    zip.set(part, i);
    i += part.length;
  }
  return zip;
}
it("reads a complete stored publishing ZIP and detects tampered or duplicate entries", async () => {
  vi.stubGlobal("crypto", webcrypto);
  const a = new Uint8Array(readFileSync("../erlcx-extension/icon32.png")),
    b = new Uint8Array(readFileSync("../erlcx-extension/icon128.png"));
  const m = manifest();
  m.media = [a, b].map((data, i) => {
    const hash = createHash("sha256").update(data).digest("hex");
    return {
      hash,
      path: `media/${hash}.png`,
      name: `image${i}.png`,
      size: data.length,
    };
  });
  const metadata = new TextEncoder().encode(JSON.stringify(m));
  const entries = [
    ["public.json", metadata],
    [m.media[0].path, a],
    [m.media[1].path, b],
  ];
  const bundle = await readBundle(storedZip(entries).buffer);
  expect(bundle.manifest.title).toBe(m.title);
  expect(bundle.media).toHaveLength(2);
  const corrupt = a.slice();
  corrupt[corrupt.length - 1] ^= 1;
  await expect(
    readBundle(
      storedZip([
        ["public.json", metadata],
        [m.media[0].path, corrupt],
        [m.media[1].path, b],
      ]).buffer,
    ),
  ).rejects.toThrow(/integrity/);
  await expect(
    readBundle(storedZip([...entries, ["public.json", metadata]]).buffer),
  ).rejects.toThrow(/duplicate/);
  await expect(
    readBundle(
      storedZip([
        ["public.json", metadata],
        ["../secret", a],
        [m.media[1].path, b],
      ]).buffer,
    ),
  ).rejects.toThrow();
});
it("attaches staged media only once and leaves final agreement untouched", async () => {
  fixture('<p>Images * ( 0 / 10 )</p><input type="file" multiple>');
  const files = [];
  const input = document.querySelector("input");
  Object.defineProperty(input, "files", {
    configurable: true,
    get: () => files,
    set: (value) => {
      files.push(...value);
    },
  });
  vi.stubGlobal(
    "DataTransfer",
    class {
      constructor() {
        this.files = [];
        this.items = { add: (file) => this.files.push(file) };
      }
    },
  );
  globalThis.__apexMedia = new Map([
    [
      "test-export",
      { files: [new File(["one"], "one.png"), new File(["two"], "two.png")] },
    ],
  ]);
  const changed = vi.fn();
  input.addEventListener("change", changed);
  expect((await fillCurrentStep(manifest())).filled).toContain(
    "2 preview images",
  );
  await fillCurrentStep(manifest());
  expect(changed).toHaveBeenCalledTimes(1);
  expect(files).toHaveLength(2);
  expect(click).not.toHaveBeenCalled();
});
it("undo restores matching fields but preserves edits made after filling", async () => {
  fixture(
    '<input id="title"><textarea placeholder="Write your post description here..."></textarea>',
  );
  const m = manifest();
  m.tags = [];
  await fillCurrentStep(m);
  document.querySelector("#title").value = "New user edit";
  const result = await fillCurrentStep(m, { undo: true });
  expect(document.querySelector("textarea").value).toBe("");
  expect(document.querySelector("#title").value).toBe("New user edit");
  expect(result.skipped.join()).toContain("changed");
});

it("fills the live site's portalled hex colour picker and closes it", async () => {
  fixture(
    '<section id="row"><input placeholder="Vehicle name (e.g., Crown Victoria)" value=""><button id="colour">#FFFFFF</button><input type="number" value="100"><input placeholder="Side (e.g. Left, Right)"><input placeholder="Decal ID"><button>Add Side</button></section>',
  );
  const trigger = document.querySelector("#colour");
  trigger.addEventListener("click", () => {
    const existing = document.querySelector('[data-slot="popover-content"]');
    if (existing) {
      existing.remove();
      return;
    }
    const popup = document.createElement("div");
    popup.dataset.slot = "popover-content";
    popup.innerHTML = '<input placeholder="#000000" value="#ffffff">';
    popup.querySelector("input").addEventListener("input", (e) => {
      trigger.textContent = e.target.value.toUpperCase();
    });
    document.body.append(popup);
  });
  const m = manifest();
  m.liveries[0].primaryColor = "#12ab34";
  const result = await fillCurrentStep(m);
  expect(result.attention).toEqual([]);
  expect(trigger.textContent).toBe("#12AB34");
  expect(document.querySelector('[data-slot="popover-content"]')).toBeNull();
});

it("does not fill a different step after a media transfer begins", async () => {
  fixture(
    '<input id="title"><textarea placeholder="Write your post description here..."></textarea>',
  );
  const result = await fillCurrentStep(manifest(), { expectedStep: "Media" });
  expect(document.querySelector("#title").value).toBe("");
  expect(result.filled).toHaveLength(0);
  expect(result.attention.join()).toContain("step changed");
});
