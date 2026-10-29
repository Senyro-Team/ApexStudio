import { afterEach, expect, it, vi } from "vitest";
import { readCreatorStatistics } from "../../erlcx-extension/creator-statistics.js";
afterEach(() => vi.unstubAllGlobals());
it("transfers aggregates only and checks the signed-in account first", async () => {
  vi.stubGlobal("location", new URL("https://erlcx.com/creator/dashboard"));
  const fetch = vi
    .fn()
    .mockResolvedValueOnce({
      ok: true,
      json: async () => ({ profile: { sub: "123" } }),
    })
    .mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        currency: "robux",
        summary: {
          purchases: 2,
          allTimePurchases: 3,
          netRobux: 70,
          grossRobux: 100,
        },
        series: [{ date: "2026-10-25", sales: 2, netRobux: 70 }],
        items: [{ buyerUsername: "private", transactionId: "private" }],
      }),
    });
  vi.stubGlobal("fetch", fetch);
  const data = await readCreatorStatistics("123");
  expect(data).toEqual({
    sales: 2,
    allTimeSales: 3,
    netRobux: 70,
    grossRobux: 100,
    series: [{ date: "2026-10-25", sales: 2, netRobux: 70 }],
  });
  expect(JSON.stringify(data)).not.toContain("private");
  expect(fetch).toHaveBeenCalledTimes(2);
  expect(fetch.mock.calls.every(([url]) => !url.includes("/item/"))).toBe(true);
});
it("refuses a different signed-in account before requesting earnings", async () => {
  vi.stubGlobal("location", new URL("https://erlcx.com/creator/dashboard"));
  const fetch = vi
    .fn()
    .mockResolvedValue({
      ok: true,
      json: async () => ({ profile: { sub: "wrong" } }),
    });
  vi.stubGlobal("fetch", fetch);
  await expect(readCreatorStatistics("123")).rejects.toThrow("does not match");
  expect(fetch).toHaveBeenCalledOnce();
});
