import type { LiveryUploads, PublishingLivery } from "./bridge";
export function readyForPublishing(l: LiveryUploads) {
  return (
    l.textures.length > 0 &&
    l.textures.every(
      (t) =>
        t.upload?.assetId &&
        t.upload.state === "uploaded" &&
        t.upload.moderation !== "rejected",
    )
  );
}
export function publishingLivery(l: LiveryUploads): PublishingLivery {
  return {
    source: {
      folderId: l.folderId,
      accountId: l.accountId,
      hashes: l.textures.map((t) => t.hash),
    },
    vehicle: l.vehicle,
    primaryColor: "#FFFFFF",
    secondaryColor: null,
    opacity: 100,
    sides: l.textures.map((t) => ({
      side: t.side,
      decalId: t.upload?.assetId ?? "",
    })),
  };
}
