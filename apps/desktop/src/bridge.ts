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

export interface PublishingImage {
  hash: string;
  name: string;
}
export interface PublishingLivery {
  source?: { folderId: string; accountId: string; hashes: string[] };
  vehicle: string;
  primaryColor: string;
  secondaryColor: string | null;
  opacity: number;
  sides: { side: string; decalId: string }[];
}
export interface PublishingData {
  title: string;
  description: string;
  tags: string[];
  youtubeUrl: string | null;
  media: PublishingImage[];
  liveries: PublishingLivery[];
}
export interface PublishingDraft {
  data: PublishingData;
  revision: number;
}
export interface PublishingPreview extends PublishingImage {
  size: number;
  preview: string | null;
}
export const publishingDraft = (projectId: string) =>
  invoke<PublishingDraft>("publishing_draft", { projectId });
export const publishingPreviews = (images: PublishingImage[]) =>
  invoke<PublishingPreview[]>("publishing_previews", { images });
export const savePublishing = (
  projectId: string,
  revision: number,
  data: PublishingData,
) => invoke<PublishingDraft>("save_publishing", { projectId, revision, data });
export const pickPublishingImages = (projectId: string) =>
  invoke<PublishingPreview[] | null>("pick_publishing_images", { projectId });
export const exportPublishing = (projectId: string) =>
  invoke<boolean>("export_publishing", { projectId });

export interface RobloxSettings {
  configured: boolean;
  accountId: string | null;
}
export interface DecalUpload {
  accountId: string;
  hash: string;
  state: string;
  operation: string | null;
  assetId: string | null;
  moderation: string;
  message: string;
}
export interface LiveryUploads {
  folderId: string;
  name: string;
  vehicle: string;
  accountId: string;
  textures: {
    assetId: string;
    name: string;
    hash: string;
    side: string;
    upload: DecalUpload | null;
  }[];
}
export const robloxSettings = () => invoke<RobloxSettings>("roblox_settings");
export const saveRobloxSettings = (accountId: string, apiKey: string) =>
  invoke<void>("save_roblox_settings", { accountId, apiKey });
export const testRobloxConnection = () =>
  invoke<void>("test_roblox_connection");
export const clearRobloxSettings = () => invoke<void>("clear_roblox_settings");
export const robloxLivery = (folderId: string) =>
  invoke<LiveryUploads>("roblox_livery", { folderId });

export interface LiveryFolderDraft {
  name: string;
  draft: LiveryDraft | null;
  warning: string | null;
  error: string | null;
}
export const prepareLiveryFolders = (projectId: string) =>
  invoke<LiveryFolderDraft[] | null>("prepare_livery_folders", { projectId });

export interface RobloxFocus {
  projectId?: string;
  folderIds?: string[];
}
export interface RobloxLiverySummary {
  id: string;
  projectId: string;
  projectName: string;
  name: string;
  vehicleId: string | null;
  textures: number;
  uploaded: number;
}
export interface UploadBatch {
  id: string;
  accountId: string;
  state: string;
  createdAt: string;
}
export interface UploadJob {
  id: string;
  accountId: string;
  hash: string;
  name: string;
  state: string;
  pass: number;
  nextAt: number;
  message: string;
  operation: string | null;
  firstId: string | null;
  secondId: string | null;
  moderation: string;
  generation: number;
}
export interface UploadItem {
  batchId: string;
  jobId: string;
  folderId: string;
  side: string;
}
export interface UploadSnapshot {
  revision: number;
  queue: { batches: UploadBatch[]; jobs: UploadJob[]; items: UploadItem[] };
  concurrency: number;
  activeTransfers: number;
  nextAllowedAt: number;
}
export interface UploadReview {
  accountId: string;
  liveries: number;
  textures: number;
  uniqueTextures: number;
  reused: number;
  queued: number;
  creates: number;
}
export const robloxLiveries = () =>
  invoke<RobloxLiverySummary[]>("roblox_liveries");
export const uploadSnapshot = () => invoke<UploadSnapshot>("upload_snapshot");
export const previewUploadBatch = (folderIds: string[], force: boolean) =>
  invoke<UploadReview>("preview_upload_batch", { folderIds, force });
export const startUploadBatch = (
  requestId: string,
  folderIds: string[],
  force: boolean,
) => invoke<string>("start_upload_batch", { requestId, folderIds, force });
export const controlUploadBatch = (
  batchId: string | null,
  action: "pause" | "resume" | "cancel" | "retry",
) => invoke<void>("control_upload_batch", { batchId, action });
export const setEditLock = (locked: boolean) =>
  invoke<void>("set_edit_lock", { locked });
export interface PreparationProgress {
  total: number;
  ready: number;
  errors: string[];
}
export const liveryPreparations = (tokens: string[]) =>
  invoke<{ token: string; progress: PreparationProgress }[]>(
    "livery_preparations",
    { tokens },
  );
export interface TrashPreview {
  name: string;
  files: number;
  folders: number;
}
export interface TrashItem extends TrashPreview {
  id: string;
  projectId: string;
  projectName: string;
  rootId: string;
  kind: "folder" | "file";
  deletedAt: string;
}
export const trashPreview = (id: string, folder: boolean) =>
  invoke<TrashPreview>("trash_preview", { id, folder });
export const trashItem = (id: string, folder: boolean) =>
  invoke<string>("trash_item", { id, folder });
export const listTrash = () => invoke<TrashItem[]>("list_trash");
export const restoreTrash = (id: string) =>
  invoke<string>("restore_trash", { id });
export const purgeTrash = (id: string) => invoke<void>("purge_trash", { id });
