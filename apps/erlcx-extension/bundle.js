const MAX_IMAGE = 20 * 1024 * 1024,
  MAX_TOTAL = MAX_IMAGE * 10 + 256 * 1024;
const text = new TextDecoder("utf-8", { fatal: true });
const assert = (ok, message) => {
  if (!ok) throw Error(message);
};
const keys = (o, allowed) => {
  assert(
    o && typeof o === "object" && !Array.isArray(o),
    "Invalid bundle object.",
  );
  assert(
    Object.keys(o).every((k) => allowed.includes(k)),
    "Unexpected fields in publishing bundle.",
  );
};
export function validateManifest(m) {
  keys(m, [
    "version",
    "kind",
    "targetOrigin",
    "exportId",
    "generatedAt",
    "title",
    "description",
    "tags",
    "youtubeUrl",
    "liveries",
    "media",
  ]);
  assert(
    m.version === 1 &&
      m.kind === "apex-publish" &&
      m.targetOrigin === "https://erlcx.com",
    "Unsupported bundle version or destination.",
  );
  assert(
    typeof m.exportId === "string" &&
      /^[a-zA-Z0-9-]{1,100}$/.test(m.exportId) &&
      typeof m.generatedAt === "string" &&
      Number.isFinite(Date.parse(m.generatedAt)),
    "Invalid export identity.",
  );
  assert(
    typeof m.title === "string" &&
      m.title.trim().length >= 5 &&
      m.title.length <= 50 &&
      typeof m.description === "string" &&
      m.description.trim() &&
      m.description.length <= 2000,
    "Invalid title or description.",
  );
  assert(
    Array.isArray(m.tags) &&
      m.tags.length <= 10 &&
      m.tags.every(
        (t) => typeof t === "string" && t.trim() && t.length <= 30,
      ) &&
      new Set(m.tags.map((t) => t.toLowerCase())).size === m.tags.length,
    "Invalid tags.",
  );
  if (m.youtubeUrl !== null) {
    let u;
    try {
      u = new URL(m.youtubeUrl);
    } catch {
      throw Error("Invalid YouTube URL.");
    }
    assert(
      u.protocol === "https:" &&
        ["www.youtube.com", "youtube.com", "youtu.be"].includes(u.hostname) &&
        !u.username &&
        !u.password,
      "Invalid YouTube URL.",
    );
  }
  assert(
    Array.isArray(m.media) && m.media.length >= 2 && m.media.length <= 10,
    "Choose 2-10 preview images.",
  );
  const hashes = new Set();
  for (const p of m.media) {
    keys(p, ["path", "hash", "name", "size"]);
    assert(
      typeof p.hash === "string" &&
        /^[a-f0-9]{64}$/.test(p.hash) &&
        p.path === `media/${p.hash}.png` &&
        !hashes.has(p.hash),
      "Invalid or duplicate media reference.",
    );
    hashes.add(p.hash);
    assert(
      typeof p.name === "string" &&
        p.name.length > 0 &&
        p.name.length <= 255 &&
        !/[\\/\u0000-\u001f]/.test(p.name) &&
        Number.isInteger(p.size) &&
        p.size > 0 &&
        p.size <= MAX_IMAGE,
      "Invalid image name or size.",
    );
  }
  assert(
    Array.isArray(m.liveries) &&
      m.liveries.length >= 1 &&
      m.liveries.length <= 20,
    "Invalid livery count.",
  );
  for (const l of m.liveries) {
    keys(l, ["vehicle", "primaryColor", "secondaryColor", "opacity", "sides"]);
    assert(
      typeof l.vehicle === "string" &&
        l.vehicle.trim() &&
        l.vehicle.length <= 120 &&
        /^#[0-9a-f]{6}$/i.test(l.primaryColor) &&
        (l.secondaryColor === null ||
          /^#[0-9a-f]{6}$/i.test(l.secondaryColor)) &&
        Number.isInteger(l.opacity) &&
        l.opacity >= 0 &&
        l.opacity <= 100,
      "Invalid livery fields.",
    );
    assert(
      Array.isArray(l.sides) && l.sides.length >= 1 && l.sides.length <= 100,
      "Invalid side count.",
    );
    const seen = new Set();
    for (const s of l.sides) {
      keys(s, ["side", "decalId"]);
      assert(
        typeof s.side === "string" &&
          s.side.trim() &&
          s.side.length <= 60 &&
          !/[\u0000-\u001f]/.test(s.side) &&
          !seen.has(s.side) &&
          typeof s.decalId === "string" &&
          /^[1-9][0-9]{0,19}$/.test(s.decalId),
        "Invalid or duplicate livery side.",
      );
      seen.add(s.side);
    }
  }
  return m;
}
// Apex exports ZIP_STORED: PNGs are already compressed. Reject other codecs and ZIP64.
export async function readBundle(buffer) {
  const bytes = new Uint8Array(buffer);
  assert(
    bytes.length <= MAX_TOTAL && bytes.length >= 22,
    "Publishing bundle is too large or empty.",
  );
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let end = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 65557); i--)
    if (view.getUint32(i, true) === 0x06054b50) {
      end = i;
      break;
    }
  assert(
    end >= 0 && end + 22 + view.getUint16(end + 20, true) === bytes.length,
    "Invalid ZIP directory.",
  );
  const count = view.getUint16(end + 10, true),
    centralSize = view.getUint32(end + 12, true),
    start = view.getUint32(end + 16, true);
  assert(
    view.getUint16(end + 4, true) === 0 &&
      view.getUint16(end + 6, true) === 0 &&
      view.getUint16(end + 8, true) === count &&
      count >= 3 &&
      count <= 11 &&
      start + centralSize === end,
    "Unsupported ZIP layout.",
  );
  const files = new Map();
  let cursor = start,
    total = 0;
  for (let i = 0; i < count; i++) {
    assert(
      cursor + 46 <= end && view.getUint32(cursor, true) === 0x02014b50,
      "Invalid ZIP entry.",
    );
    const flags = view.getUint16(cursor + 8, true),
      method = view.getUint16(cursor + 10, true),
      compressed = view.getUint32(cursor + 20, true),
      size = view.getUint32(cursor + 24, true),
      nameLen = view.getUint16(cursor + 28, true),
      extra = view.getUint16(cursor + 30, true),
      comment = view.getUint16(cursor + 32, true),
      offset = view.getUint32(cursor + 42, true);
    assert(
      cursor + 46 + nameLen + extra + comment <= end,
      "Invalid ZIP entry bounds.",
    );
    const name = text.decode(
      bytes.subarray(cursor + 46, cursor + 46 + nameLen),
    );
    assert(
      (name === "public.json" || /^media\/[a-f0-9]{64}\.png$/.test(name)) &&
        !files.has(name) &&
        !(flags & 1) &&
        method === 0 &&
        size === compressed &&
        size <= (name === "public.json" ? 256 * 1024 : MAX_IMAGE),
      "Unsupported, duplicate or oversized bundle file.",
    );
    total += size;
    assert(
      total <= MAX_TOTAL &&
        offset + 30 <= start &&
        view.getUint32(offset, true) === 0x04034b50,
      "Invalid bundle size or local header.",
    );
    const ln = view.getUint16(offset + 26, true),
      le = view.getUint16(offset + 28, true),
      dataStart = offset + 30 + ln + le;
    assert(
      dataStart + size <= start &&
        text.decode(bytes.subarray(offset + 30, offset + 30 + ln)) === name &&
        view.getUint16(offset + 8, true) === 0,
      "Invalid local file entry.",
    );
    files.set(name, bytes.slice(dataStart, dataStart + size));
    cursor += 46 + nameLen + extra + comment;
  }
  assert(
    cursor === end && files.has("public.json"),
    "Bundle manifest is missing.",
  );
  const manifest = validateManifest(
    JSON.parse(text.decode(files.get("public.json"))),
  );
  assert(files.size === manifest.media.length + 1, "Unexpected bundle files.");
  const media = [];
  for (const image of manifest.media) {
    const data = files.get(image.path);
    assert(
      data &&
        data.length === image.size &&
        [137, 80, 78, 71, 13, 10, 26, 10].every((b, i) => data[i] === b),
      "Preview image is missing or invalid.",
    );
    assert(
      data.length >= 24 && text.decode(data.subarray(12, 16)) === "IHDR",
      "Invalid PNG header.",
    );
    const dimensions = new DataView(
      data.buffer,
      data.byteOffset,
      data.byteLength,
    );
    assert(
      dimensions.getUint32(16) > 0 &&
        dimensions.getUint32(16) <= 8000 &&
        dimensions.getUint32(20) > 0 &&
        dimensions.getUint32(20) <= 8000,
      "Preview dimensions exceed 8000 pixels.",
    );
    const hash = Array.from(
      new Uint8Array(await crypto.subtle.digest("SHA-256", data)),
      (b) => b.toString(16).padStart(2, "0"),
    ).join("");
    assert(hash === image.hash, "Preview image integrity check failed.");
    media.push({
      name: image.name,
      hash: image.hash,
      blob: new Blob([data], { type: "image/png" }),
    });
  }
  return { manifest, media };
}
