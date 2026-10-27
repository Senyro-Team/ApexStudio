import { expect, it } from "vitest";
import { liveryCodesMarkdown } from "./liveryCodesMarkdown";
import type { LiveryUploads } from "./bridge";
const texture = (side: string, id: string | null, moderation = "approved") => ({
  assetId: side,
  name: "image.png",
  side,
  hash: "hash",
  upload: id
    ? {
        accountId: "123",
        hash: "hash",
        state: "uploaded",
        operation: null,
        assetId: id,
        moderation,
        message: "",
      }
    : null,
});
const livery: LiveryUploads = {
  folderId: "a",
  accountId: "123",
  name: "EMS",
  vehicle: "Ambulance",
  textures: [
    texture("Left 10", "110"),
    texture("Left 2", "102"),
    texture("Back 1", null),
    texture("Top 1", "300", "rejected"),
  ],
};
it("keeps partial liveries explicit and sorts sides numerically without mutating the source", () => {
  const result = liveryCodesMarkdown([{ projectName: "FDNY", livery }]);
  expect(result.codes).toBe(2);
  expect(result.missing).toBe(2);
  expect(result.markdown).toBe(
    "## FDNY — EMS\n\n**Vehicle:** Ambulance\n\n- **Back 1:** Not uploaded\n- **Left 2:** `102`\n- **Left 10:** `110`\n- **Top 1:** Rejected by Roblox",
  );
  expect(livery.textures[0].side).toBe("Left 10");
});
it("escapes user names and never exports processing or wrong-account IDs", () => {
  const wrong = texture("Left 1", "999");
  wrong.upload!.accountId = "other";
  const processing = texture("Front 1", "111");
  processing.upload!.state = "processing";
  const result = liveryCodesMarkdown([
    {
      projectName: "# Test\nNew",
      livery: { ...livery, name: "[EMS] *2*", textures: [wrong, processing] },
    },
  ]);
  expect(result.codes).toBe(0);
  expect(result.markdown).not.toContain("999");
  expect(result.markdown).not.toContain("111");
  expect(result.markdown).toContain("## \\# Test New — \\[EMS\\] \\*2\\*");
});
