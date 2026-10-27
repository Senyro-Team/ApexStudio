import type { LiveryUploads } from "./bridge";

const markdownText = (text: string) =>
  text.replace(/[\r\n]+/g, " ").replace(/[\\`*_{}\[\]<>#+.!|~()-]/g, "\\$&");

/** Export only the selected, matching-account decal result; never a pass-one/image ID. */
export function liveryCodesMarkdown(
  liveries: { projectName: string; livery: LiveryUploads }[],
) {
  let codes = 0;
  let missing = 0;
  const markdown = liveries
    .map(({ projectName, livery }) => {
      const lines = [...livery.textures]
        .sort((a, b) => a.side.localeCompare(b.side, "en", { numeric: true }))
        .map((texture) => {
          const upload = texture.upload;
          const usable =
            upload?.state === "uploaded" &&
            upload.moderation !== "rejected" &&
            upload.accountId === livery.accountId &&
            upload.hash === texture.hash &&
            /^[1-9]\d{0,19}$/.test(upload.assetId ?? "");
          if (usable) codes++;
          else missing++;
          const value = usable
            ? `\`${upload!.assetId}\``
            : upload?.moderation === "rejected"
              ? "Rejected by Roblox"
              : "Not uploaded";
          return `- **${markdownText(texture.side)}:** ${value}`;
        });
      return `## ${markdownText(projectName)} — ${markdownText(livery.name)}\n\n**Vehicle:** ${markdownText(livery.vehicle)}\n\n${lines.length ? lines.join("\n") : "No assigned textures."}`;
    })
    .join("\n\n");
  return { markdown, codes, missing };
}
