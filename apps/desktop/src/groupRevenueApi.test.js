import { webcrypto } from "node:crypto";
import { expect, it, vi } from "vitest";
import { readGroupRevenue } from "../../erlcx-extension/group-revenue.js";

const response = (data, status = 200) => ({ ok: status >= 200 && status < 300, status, json: async () => data });

it("reads only group catalog and sales GETs and forwards a sanitized pass/sale batch", async () => {
  vi.stubGlobal("crypto", webcrypto);
  const fetch = vi.fn()
    .mockResolvedValueOnce(response({ data: [{ id: 11 }], nextPageCursor: null }))
    .mockResolvedValueOnce(response({ data: [{ id: 200, name: "Fleet Pass" }], nextPageCursor: null }))
    .mockResolvedValueOnce(response({ data: [{ id: 900, created: "2026-11-07T01:02:00Z", details: { id: 200, universeId: 11 }, currency: { amount: 70 }, price: 100, isPending: false, buyerUserId: "private" }], nextPageCursor: null }))
    .mockResolvedValueOnce(response({ revenue: { month: 70 } }));
  vi.stubGlobal("fetch", fetch);
  const native = { request: vi.fn(async () => ({ ok: true })) };

  await readGroupRevenue("16538500", "session", native);

  expect(fetch.mock.calls.every(([url, init]) => String(url).startsWith("https://") && init.method === "GET" && init.credentials === "include")).toBe(true);
  expect(native.request).toHaveBeenCalledTimes(2);
  const batch = native.request.mock.calls[0][0];
  expect(batch.passes).toEqual([{ groupId: "16538500", assetId: "200", universeId: "11", name: "Fleet Pass", source: "catalog" }]);
  expect(batch.sales[0]).toMatchObject({ universeId: "11", assetId: "200", price: 100, revenue: 70, source: "api", holdStatus: "Released" });
  expect(JSON.stringify(batch)).not.toContain("private");
  expect(native.request.mock.calls[1][0]).toMatchObject({ kind: "group_revenue_complete", summary: { "revenue.month": 70, returnedTransactions: 1 } });
});

it("rejects Roblox throttling before reporting a successful refresh", async () => {
  const fetch = vi.fn().mockResolvedValueOnce(response({}, 429));
  vi.stubGlobal("fetch", fetch);
  const native = { request: vi.fn(async () => ({ ok: true })) };

  await expect(readGroupRevenue("16538500", "session", native)).rejects.toMatchObject({ code: "rate_limited" });
  expect(native.request).not.toHaveBeenCalled();
});
