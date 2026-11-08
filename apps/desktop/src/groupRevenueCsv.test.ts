import { expect, it } from "vitest";
import { readCsvFiles } from "./GroupRevenue";

const group = {
  groupId: "16538500",
  name: "APX",
  experienceIds: ["84560840200510"],
  passMappings: {},
  selectedPassIds: [],
};

it("imports Roblox's current group CSV headers, decimal-zero Robux, and transaction IDs without buyer IDs", async () => {
  const csv = [
    "Id,Buyer User Id,Date and Time,Location,Location Id,Universe Id,Universe,Asset Id,Asset Name,Asset Type,Hold Status,Revenue,Price",
    "9910001,private-buyer,2026-11-07T01:02:00.000Z,Website,,84560840200510,APX,2053361604,Fleet Pack,Game Pass,Released,700.0,1000.0",
    "9910002,private-buyer,2026-11-05T22:02:00.000Z,Game,84560840200510,84560840200510,APX,2037608486,Support Pass,Game Pass,Held,140.0,200.0",
  ].join("\n");
  const file = { name: "sales.csv", size: csv.length, text: async () => csv } as File;

  const preview = await readCsvFiles([file], group);

  expect(preview.rows).toHaveLength(2);
  expect(preview.rows.map(({ revenue, price }) => [revenue, price])).toEqual([[700, 1000], [140, 200]]);
  expect(preview.rows[0].externalTransactionId).toBe("9910001");
  expect(preview.rows[0].saleLocation).toBe("Website");
  expect(preview.rows.every((row) => !("buyerUserId" in row))).toBe(true);
  expect(preview.warnings).toEqual([]);
});

it("accepts trailing decimal zeroes but rejects fractional Robux rather than rounding", async () => {
  const { amount } = await import("./GroupRevenue");
  expect(amount("700.000", "Revenue")).toBe(700);
  expect(() => amount("700.5", "Revenue")).toThrow(/whole number/);
});
