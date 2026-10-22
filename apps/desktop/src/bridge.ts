import { invoke, isTauri } from "@tauri-apps/api/core";

export interface WorkspaceInfo {
  name: string;
  root: string;
  schemaVersion: number;
  managedFileCount: number;
}
export const desktopAvailable = isTauri();
export interface ProjectFields {
  name: string;
  description: string;
  tags: string[];
  vehicles: string[];
  referenceLinks: string[];
  deadline: string | null;
}
export interface Project extends ProjectFields {
  id: string;
  createdAt: string;
  updatedAt: string;
  archived: boolean;
  assetCount: number;
}
export interface Asset {
  id: string;
  projectId: string;
  name: string;
  hash: string;
  size: number;
  importedAt: string;
  preview: string | null;
}
export interface Catalogue {
  capturedAt: string;
  source: string;
  completeness: string;
  vehicles: Vehicle[];
}
export interface ImportReport {
  imported: Asset[];
  skipped: string[];
  failed: string[];
}
export const listProjects = () => invoke<Project[]>("list_projects");
export const saveProject = (id: string | null, fields: ProjectFields) =>
  invoke<Project>("save_project", { id, fields });
export const archiveProject = (id: string, archived: boolean) =>
  invoke<Project>("archive_project", { id, archived });
export const listAssets = (projectId: string | null) =>
  invoke<Asset[]>("list_assets", { projectId });
export const importAssets = (
  projectId: string,
  folderId: string | null = null,
) => invoke<ImportReport | null>("import_assets", { projectId, folderId });
export const saveAssetCopy = (id: string) =>
  invoke<boolean>("save_asset_copy", { id });
export const vehicleCatalogue = () => invoke<Catalogue>("vehicle_catalogue");
export function readWorkspace(): Promise<WorkspaceInfo> {
  return invoke("workspace_info");
}
export function renameWorkspace(name: string): Promise<WorkspaceInfo> {
  return invoke("rename_workspace", { name });
}
export function errorMessage(error: unknown): string {
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  )
    return error.message;
  return typeof error === "string"
    ? error
    : "The workspace could not be opened. Retry or check folder permissions.";
}

export interface Note {
  id: string;
  projectId: string | null;
  title: string;
  document: import("@tiptap/core").JSONContent;
  markdown: string;
  revision: number;
  archived: boolean;
  createdAt: string;
  updatedAt: string;
}
export const listNotes = (projectId: string | null) =>
  invoke<Note[]>("list_notes", { projectId });
export const createNote = (projectId: string | null) =>
  invoke<Note>("create_note", { projectId });
export const saveNote = (
  note: Note,
  title: string,
  document: Note["document"],
  markdown: string,
) =>
  invoke<Note>("save_note", {
    id: note.id,
    revision: note.revision,
    title,
    document,
    markdown,
  });
export const archiveNote = (note: Note, archived: boolean) =>
  invoke<Note>("archive_note", {
    id: note.id,
    revision: note.revision,
    archived,
  });
export const attachNoteImage = (id: string) =>
  invoke<string | null>("attach_note_image", { id });
export const noteImage = (id: string, hash: string) =>
  invoke<string>("note_image", { id, hash });
export const exportProject = (
  id: string,
  embedded: boolean,
  includeNotes: boolean,
) => invoke<boolean>("export_project", { id, embedded, includeNotes });
export const createBackup = () => invoke<boolean>("create_backup");
export interface ArchivePreview {
  token: string;
  preview: {
    title: string;
    assets: number;
    notes: number;
    bytes: number;
    kind: "project" | "workspace";
  };
}
export const inspectArchive = (restore: boolean) =>
  invoke<ArchivePreview | null>("inspect_archive", { restore });
export const applyArchive = (token: string, restore: boolean) =>
  invoke<string>("apply_archive", { token, restore });

export interface Vehicle {
  id: string;
  name: string;
  team: string;
  source: string;
  liverySupport: string;
  bodyStatus: string | null;
  windowStatus: string | null;
  notes: string[];
  slots: { key: string; status: string; kind: string }[];
}
export interface AssetFolder {
  id: string;
  projectId: string;
  parentId: string | null;
  name: string;
  kind: "folder" | "livery";
  vehicleId: string | null;
  coverHash: string | null;
  preview: string | null;
}
export interface Placement {
  assetId: string;
  folderId: string | null;
  slot: string | null;
}
export interface AssetVersion {
  id: string;
  assetId: string;
  hash: string;
  name: string;
  createdAt: string;
}
export interface AssetBoard {
  assets: Asset[];
  folders: AssetFolder[];
  placements: Placement[];
}
export interface LiveryDraft {
  token: string;
  files: { index: number; name: string; suggestedSlot: string | null }[];
}
export const assetBoard = (projectId: string) =>
  invoke<AssetBoard>("asset_board", { projectId });
export const saveFolder = (
  projectId: string,
  id: string | null,
  parentId: string | null,
  name: string,
) => invoke<AssetFolder>("save_folder", { projectId, id, parentId, name });
export const moveAsset = (id: string, folderId: string | null) =>
  invoke<void>("move_asset", { id, folderId });
export const assetVersions = (id: string) =>
  invoke<AssetVersion[]>("asset_versions", { id });
export const restoreAssetVersion = (id: string, versionId: string) =>
  invoke<void>("restore_asset_version", { id, versionId });
export const setFolderCover = (id: string) =>
  invoke<boolean>("set_folder_cover", { id });
export const prepareLivery = (projectId: string, token: string | null = null) =>
  invoke<LiveryDraft | null>("prepare_livery", { projectId, token });
export const commitLivery = (
  token: string,
  parentId: string | null,
  existingId: string | null,
  name: string,
  vehicleId: string,
  assignments: { index: number; slot: string | null }[],
  coverIndex: number | null,
) =>
  invoke<string>("commit_livery", {
    token,
    parentId,
    existingId,
    name,
    vehicleId,
    assignments,
    coverIndex,
  });
export const cancelLivery = (token: string) =>
  invoke<void>("cancel_livery", { token });

export const assignAssetSlot = (id: string, slot: string | null) =>
  invoke<void>("assign_asset_slot", { id, slot });
export const copyAssetItem = (
  projectId: string,
  id: string,
  isFolder: boolean,
  folderId: string | null,
) => invoke<void>("copy_asset_item", { projectId, id, isFolder, folderId });
