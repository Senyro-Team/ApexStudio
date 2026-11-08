const PAGE_SIZE = 100;
const REQUEST_TIMEOUT_MS = 20000;
const MAX_PAGES = 1000;

async function getJson(url) {
  const response = await fetch(url, {
    method: "GET",
    credentials: "include",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  if (response.status === 401 || response.status === 403) {
    const error = Error("Roblox login expired or this account cannot view the group sales.");
    error.code = "unauthorized";
    throw error;
  }
  if (response.status === 429) {
    const error = Error("Roblox temporarily rate-limited the request. Try again later.");
    error.code = "rate_limited";
    throw error;
  }
  if (!response.ok) throw Error(`Roblox API request failed (HTTP ${response.status}).`);
  try { return await response.json(); }
  catch { throw Error("Roblox returned a response that was not valid JSON."); }
}

function rowsOf(payload) {
  if (Array.isArray(payload?.data)) return payload.data;
  if (Array.isArray(payload?.gamePasses)) return payload.gamePasses;
  if (Array.isArray(payload?.passes)) return payload.passes;
  return null;
}

function nextCursor(payload) {
  return payload?.nextPageCursor ?? payload?.nextPageToken ?? payload?.nextCursor ?? null;
}

async function pages(urlForCursor) {
  const all = [];
  let cursor = null;
  const seen = new Set();
  for (let page = 0; page < MAX_PAGES; page++) {
    const url = urlForCursor(cursor);
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
    let payload;
    try {
      const response = await fetch(url, {
        method: "GET", credentials: "include", cache: "no-store",
        headers: { Accept: "application/json" }, signal: controller.signal,
      });
      if (response.status === 401 || response.status === 403) {
        const error = Error("Roblox login expired or this account cannot view the group sales."); error.code = "unauthorized"; throw error;
      }
      if (response.status === 429) {
        const error = Error("Roblox temporarily rate-limited the request. Try again later."); error.code = "rate_limited"; throw error;
      }
      if (!response.ok) throw Error(`Roblox API request failed (HTTP ${response.status}).`);
      payload = await response.json();
    } finally { clearTimeout(timeout); }
    const items = rowsOf(payload);
    if (!items) throw Error("Roblox returned an unknown paginated response format.");
    all.push(...items);
    const next = nextCursor(payload);
    if (!next) return all;
    if (seen.has(next)) throw Error("Roblox repeated a page cursor; refresh stopped without changing saved data.");
    seen.add(next);
    cursor = next;
  }
  throw Error("Roblox response exceeded the page safety limit.");
}

const numberString = (value) => {
  if (typeof value === "string" && /^\d{1,20}$/.test(value)) return value;
  if (Number.isSafeInteger(value) && value > 0) return String(value);
  return null;
};
const amount = (...values) => {
  for (const value of values) {
    if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) return value;
    if (typeof value === "string" && /^\d+(?:\.0+)?$/.test(value)) {
      const parsed = Number(value);
      if (Number.isSafeInteger(parsed)) return parsed;
    }
  }
  return null;
};
const sha256 = async (value) => {
  const bytes = new TextEncoder().encode(value);
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
};

export async function readGroupRevenue(groupId, session, native) {
  const experiences = await pages((cursor) => {
    const url = new URL(`https://games.roblox.com/v2/groups/${encodeURIComponent(groupId)}/games`);
    url.searchParams.set("accessFilter", "All");
    url.searchParams.set("limit", String(PAGE_SIZE));
    if (cursor) url.searchParams.set("cursor", cursor);
    return url.toString();
  });
  const ids = [...new Set(experiences.map((game) => numberString(game.id ?? game.universeId)).filter(Boolean))];
  if (!ids.length) throw Error("Roblox returned no group experiences; the previous catalog was kept.");

  const passes = [];
  for (const universeId of ids) {
    const items = await pages((cursor) => {
      const url = new URL(`https://apis.roblox.com/game-passes/v1/universes/${encodeURIComponent(universeId)}/game-passes/creator`);
      url.searchParams.set("limit", String(PAGE_SIZE));
      if (cursor) url.searchParams.set("cursor", cursor);
      return url.toString();
    });
    for (const pass of items) {
      const assetId = numberString(pass.id ?? pass.gamePassId ?? pass.assetId);
      const name = typeof pass.name === "string" ? pass.name.trim() : "";
      if (!assetId || !name) throw Error("Roblox returned an incomplete Game Pass entry; the previous catalog was kept.");
      passes.push({ groupId, assetId, universeId, name, source: "catalog" });
    }
  }

  const transactions = await pages((cursor) => {
    const url = new URL(`https://economy.roblox.com/v2/groups/${encodeURIComponent(groupId)}/transactions`);
    url.searchParams.set("transactionType", "Sale");
    url.searchParams.set("limit", String(PAGE_SIZE));
    if (cursor) url.searchParams.set("cursor", cursor);
    return url.toString();
  });
  let groupSummary = {};
  try {
    const summary = await getJson(`https://economy.roblox.com/v1/groups/${encodeURIComponent(groupId)}/revenue/summary/Month`);
    const collect = (value, prefix = "") => {
      if (!value || typeof value !== "object" || Array.isArray(value)) return;
      for (const [key, field] of Object.entries(value)) {
        const name = `${prefix}${key}`.slice(0, 80);
        const numeric = typeof field === "number" ? field : typeof field === "string" && /^\d+(?:\.0+)?$/.test(field) ? Number(field) : null;
        if (numeric !== null && Number.isSafeInteger(numeric) && numeric >= 0) groupSummary[name] = numeric;
        else if (field && typeof field === "object" && !Array.isArray(field)) collect(field, `${name}.`);
      }
    };
    collect(summary);
  } catch { /* This comparison endpoint is optional; a failure must not block the transaction feed. */ }
  const passById = new Map(passes.map((pass) => [pass.assetId, pass]));
  const sales = [];
  let unsupported = 0;
  for (const item of transactions) {
    const details = item.details ?? {};
    const id = numberString(details.id ?? item.assetId ?? item.gamePassId);
    const universeId = numberString(item.universeId ?? details.universeId ?? details.placeId ?? passById.get(id)?.universeId);
    const created = item.created ?? item.createdAt ?? item.date;
    const time = typeof created === "string" ? new Date(created) : null;
    const revenue = amount(item.revenue, item.currency?.amount, item.amount);
    const price = amount(item.price, item.priceInRobux, details.price, details.priceInRobux);
    if (!id || !universeId || !passById.has(id) || !time || !Number.isFinite(time.getTime()) || revenue === null || price === null) {
      unsupported++;
      continue;
    }
    const hold = item.holdStatus ?? (typeof item.isPending === "boolean" ? (item.isPending ? "Held" : "Released") : null);
    const holdStatus = /^held|pending$/i.test(hold) ? "Held" : /^cancelled|canceled$/i.test(hold) ? "Cancelled" : /^released$/i.test(hold) ? "Released" : null;
    if (!holdStatus) { unsupported++; continue; }
    const saleLocation = typeof item.saleLocation === "string" ? item.saleLocation : typeof item.location === "string" ? item.location : "";
    const externalTransactionId = typeof item.id === "string" || Number.isSafeInteger(item.id) ? String(item.id) : null;
    const identity = externalTransactionId ? `id:${externalTransactionId}` : JSON.stringify([universeId,id,time.toISOString(),saleLocation,holdStatus,price,revenue]);
    sales.push({
      saleFingerprint: await sha256(identity), universeId, assetId: id, saleAt: time.toISOString(),
      saleLocation, holdStatus, price, revenue, importBatchId: "api-refresh",
      externalTransactionId, source: "api", dedupeConfidence: externalTransactionId ? "transaction_id" : "exact_fields",
    });
  }

  const batchSize = 10;
  for (let i = 0; i < passes.length || i < sales.length; i += batchSize) {
    await native.request({
      kind: "group_revenue_batch", session,
      passes: i < passes.length ? passes.slice(i, i + batchSize) : [],
      sales: i < sales.length ? sales.slice(i, i + batchSize) : [],
    });
  }
  const dates = sales.map((sale) => sale.saleAt.slice(0, 10)).sort();
  await native.request({
    kind: "group_revenue_complete", session,
    coverageFrom: dates[0] ?? null, coverageTo: dates.at(-1) ?? null,
    summary: { ...groupSummary, returnedTransactions: transactions.length, importedTransactions: sales.length, unsupportedTransactions: unsupported, experiences: ids.length, gamePasses: passes.length },
  });
}
