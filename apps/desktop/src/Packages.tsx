import { useState } from "react";
import {
  applyArchive,
  createBackup,
  inspectArchive,
  exportProject,
  errorMessage,
  desktopAvailable,
  type ArchivePreview,
} from "./bridge";
export function ProjectExport({
  id,
  onBusy,
}: {
  id: string;
  onBusy?: (busy: boolean) => void;
}) {
  const [open, setOpen] = useState(false),
    [embedded, setEmbedded] = useState(true),
    [notes, setNotes] = useState(false),
    [busy, setBusy] = useState(false),
    [status, setStatus] = useState("");
  async function run() {
    setBusy(true);
    onBusy?.(true);
    setStatus("");
    try {
      if (await exportProject(id, embedded, notes)) {
        setStatus("Project exported.");
        setOpen(false);
      }
    } catch (e) {
      setStatus(errorMessage(e));
    } finally {
      setBusy(false);
      onBusy?.(false);
    }
  }
  return (
    <div className="package-control">
      <button
        className="secondary"
        disabled={busy}
        onClick={() => setOpen(!open)}
      >
        Export project
      </button>
      {open && (
        <section className="export-options" aria-label="Export options">
          <label>
            <input
              type="checkbox"
              checked={embedded}
              onChange={(e) => setEmbedded(e.target.checked)}
            />{" "}
            Include files
          </label>
          <label>
            <input
              type="checkbox"
              checked={notes}
              disabled={!embedded}
              onChange={(e) => setNotes(e.target.checked)}
            />{" "}
            Include project notes
          </label>
          <p className="field-hint">
            {embedded
              ? "Only this project and your selected content are included."
              : "Metadata only: project details, without files or notes."}
          </p>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void run()}
          >
            {busy ? "Exporting…" : "Choose destination"}
          </button>
        </section>
      )}
      {status && <p role="status">{status}</p>}
    </div>
  );
}
export function ArchiveImport({
  restore = false,
  disabled = false,
  onComplete,
  onBusy,
}: {
  restore?: boolean;
  disabled?: boolean;
  onComplete: () => void;
  onBusy?: (busy: boolean) => void;
}) {
  const [preview, setPreview] = useState<ArchivePreview | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  function lock(value: boolean) {
    setBusy(value);
    onBusy?.(value);
  }
  async function choose() {
    lock(true);
    setError("");
    setNotice("");
    try {
      setPreview(await inspectArchive(restore));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      lock(false);
    }
  }
  async function apply() {
    if (!preview) return;
    lock(true);
    setError("");
    try {
      const result = await applyArchive(preview.token, restore);
      setPreview(null);
      setNotice(restore ? result : "Project imported as a new project.");
      onComplete();
    } catch (e) {
      setError(errorMessage(e));
      setPreview(null);
    } finally {
      lock(false);
    }
  }
  return (
    <section className="archive-import">
      <button
        className="secondary"
        disabled={!desktopAvailable || busy || disabled}
        onClick={() => void choose()}
      >
        {busy ? "Working…" : restore ? "Restore backup" : "Import project"}
      </button>
      {preview && (
        <div className="confirm-panel">
          <h3>{preview.preview.title}</h3>
          {restore ? (
            <p>
              Replace this workspace with the selected backup? A full backup of
              your current workspace will be saved first.
            </p>
          ) : (
            <p>
              {preview.preview.assets} files · {preview.preview.notes} notes ·{" "}
              {(preview.preview.bytes / 1024 / 1024).toFixed(1)} MB. Imported as
              a new project; existing projects are preserved.
            </p>
          )}
          <div className="button-row">
            <button
              className="primary"
              disabled={busy}
              onClick={() => void apply()}
            >
              {restore
                ? "Back up current workspace and restore"
                : "Import as new project"}
            </button>
            <button
              className="text-button"
              disabled={busy}
              onClick={() => setPreview(null)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
export function BackupTools({
  onRestore,
  onBusy,
}: {
  onRestore: () => void;
  onBusy: (busy: boolean) => void;
}) {
  const [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  async function backup() {
    setBusy(true);
    onBusy(true);
    setMessage("");
    try {
      if (await createBackup()) setMessage("Workspace backup saved.");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }
  return (
    <section className="backup-tools">
      <h2>Backups</h2>
      <p>Backups include your database and managed files.</p>
      <button
        className="secondary"
        disabled={!desktopAvailable || busy}
        onClick={() => void backup()}
      >
        {busy ? "Backing up…" : "Back up workspace"}
      </button>
      <ArchiveImport
        restore
        disabled={busy}
        onComplete={onRestore}
        onBusy={(value) => {
          setBusy(value);
          onBusy(value);
        }}
      />
      {message && <p role="status">{message}</p>}
    </section>
  );
}
