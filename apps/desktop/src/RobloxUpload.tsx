import { useEffect, useRef, useState } from "react";
import {
  robloxLivery,
  uploadRobloxTexture,
  errorMessage,
  type LiveryUploads,
  type PublishingLivery,
} from "./bridge";
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
export default function RobloxUpload({
  folderId,
  include = false,
  onDone,
  onClose,
  onBusy,
}: {
  folderId: string;
  include?: boolean;
  onDone: (l: LiveryUploads) => void;
  onClose: () => void;
  onBusy: (busy: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null),
    cancel = useRef(false);
  const [livery, setLivery] = useState<LiveryUploads | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  useEffect(() => {
    dialog.current?.showModal();
    let active = true;
    robloxLivery(folderId)
      .then((l) => {
        if (active) setLivery(l);
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [folderId]);
  useEffect(() => {
    onBusy(busy);
    return () => onBusy(false);
  }, [busy, onBusy]);
  async function upload() {
    if (!livery) return;
    setBusy(true);
    cancel.current = false;
    setError("");
    setNotice("");
    try {
      const hashes = [
        ...new Set(
          livery.textures
            .filter(
              (t) =>
                !t.upload ||
                t.upload.state === "failed" ||
                t.upload.state === "processing" ||
                !!t.upload.operation,
            )
            .map((t) => t.hash),
        ),
      ];
      let latest = livery;
      for (const hash of hashes) {
        if (cancel.current) break;
        await uploadRobloxTexture(folderId, hash);
        latest = await robloxLivery(folderId);
        setLivery(latest);
        if (latest.textures.some((t) => t.upload?.state === "uncertain")) break;
      }
      if (include && readyForPublishing(latest) && !cancel.current) {
        onDone(latest);
        return;
      }
      setNotice(
        cancel.current
          ? "Stopped. Completed uploads are saved."
          : readyForPublishing(latest)
            ? "Decal IDs saved."
            : "Review the upload results. Check status for pending operations.",
      );
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const pending = livery?.textures.some(
    (t) => t.upload?.state === "processing" || t.upload?.operation,
  );
  return (
    <dialog
      ref={dialog}
      className="roblox-dialog"
      aria-labelledby="roblox-upload-title"
      onCancel={(e) => {
        if (busy) e.preventDefault();
        else onClose();
      }}
    >
      <h2 id="roblox-upload-title">
        {include ? "Upload and include livery" : "Upload to Roblox"}
      </h2>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      {!livery && !error && <p role="status">Loading livery…</p>}
      {livery && (
        <>
          <p className="roblox-upload-name">{livery.name}</p>
          <p className="field-hint">
            {livery.accountId
              ? `Roblox account ${livery.accountId}`
              : "Configure your Roblox account and API key in Settings first."}
          </p>
          {include && (
            <p>
              Missing decal IDs must be uploaded before this livery can be
              included in the ERLCX post.
            </p>
          )}
          {!livery.textures.length ? (
            <p>Assign texture sides in this livery before uploading.</p>
          ) : (
            <ul className="roblox-textures">
              {livery.textures.map((t) => (
                <li key={t.assetId}>
                  <div>
                    <strong>{t.side}</strong>
                    <span>{t.name}</span>
                  </div>
                  <div>
                    <span>
                      {t.upload?.assetId
                        ? `Decal ${t.upload.assetId}`
                        : t.upload?.state === "uploading"
                          ? "Interrupted — check Creator Dashboard"
                          : (t.upload?.state ?? "Not uploaded")}
                    </span>
                    {t.upload?.assetId && (
                      <small>Moderation: {t.upload.moderation}</small>
                    )}
                    {t.upload?.message && (
                      <small className="error">{t.upload.message}</small>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          )}
          <p className="field-hint">
            PNG textures up to 8000 × 8000 pixels and 20 MB. Only assigned
            textures are uploaded. Existing IDs for unchanged files are reused.
            Covers and unassigned images are excluded.
          </p>
        </>
      )}
      <div className="button-row">
        {include && livery && readyForPublishing(livery) ? (
          <button
            className="primary"
            disabled={busy}
            onClick={() => onDone(livery)}
          >
            Include livery
          </button>
        ) : (
          <button
            className="primary"
            disabled={
              busy ||
              !livery?.accountId ||
              !livery.textures.length ||
              !livery.textures.some(
                (t) =>
                  !t.upload ||
                  ["failed", "processing"].includes(t.upload.state) ||
                  t.upload.operation,
              )
            }
            onClick={() => void upload()}
          >
            {busy
              ? "Uploading / checking…"
              : include
                ? "Upload and include"
                : pending
                  ? "Upload missing / check status"
                  : "Upload missing textures"}
          </button>
        )}
        {busy ? (
          <button
            className="secondary"
            onClick={() => {
              cancel.current = true;
              setNotice("Stopping after the current request…");
            }}
          >
            Stop after current file
          </button>
        ) : (
          <button className="text-button" onClick={onClose}>
            {include ? "Skip this livery" : "Close"}
          </button>
        )}
      </div>
    </dialog>
  );
}
