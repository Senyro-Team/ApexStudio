import PostLiveryPicker from "./PostLiveryPicker";
import AppSelect from "./AppSelect";
import OverlayStudio from "./OverlayStudio";
import { readyForPublishing, publishingLivery } from "./RobloxUpload";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowDown, ArrowUp, BookmarkPlus, Copy, Plus, Trash2, Upload } from "lucide-react";
import SlotEditor from "./SlotEditor";
import {
  browserStatus,
  prepareBrowser,
  controlBrowser,
  type BrowserStatus,
  postDraft,
  savePost,
  postSources,
  pickPostImages,
  pinPostImages,
  releasePostImages,
  exportPost,
  type PostSourceStatus,
  robloxLivery,
  publishingDraft,
  publishingPreviews,
  savePublishing,
  pickPublishingImages,
  exportPublishing,
  importOverlayPreviews,
  assetBoard,
  mediaLibrary,
  listMarkdownPresets,
  saveMarkdownPreset,
  deleteMarkdownPreset,
  syncImageLibrary,
  syncUploadLibraryImages,
  syncServiceAvailability,
  syncListInviteCampaigns,
  syncCreateInviteCampaign,
  listAssets,
  pickMediaLibraryImage,
  addMediaLibraryAsset,
  errorMessage,
  type PublishingData,
  type PublishingDraft,
  type PublishingPreview,
  type PublishingLivery,
  type AssetBoard,
  type Catalogue,
  type Project,
  type MarkdownBlock,
  type MediaLibraryItem,
  type Asset,
  type InviteCampaign,
  type MarkdownPreset,
  type OverlayPreviewOutput,
} from "./bridge";
const steps = ["Liveries", "Details", "Previews", "Markdown", "Review"] as const;
const PENDING_INVITE_REQUESTS_KEY = "apex-studio.pending-invite-requests.v1";

type PendingInviteRequest = { id: string; scope: string; payloadHash: string };

function readPendingInviteRequests(): PendingInviteRequest[] {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(PENDING_INVITE_REQUESTS_KEY) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter((entry): entry is PendingInviteRequest =>
      !!entry && typeof entry === "object"
      && typeof entry.id === "string"
      && typeof entry.scope === "string"
      && typeof entry.payloadHash === "string",
    );
  } catch { return []; }
}

async function hashInvitePayload(payload: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(payload));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function ensurePendingInviteRequest(scope: string, payload: string): Promise<PendingInviteRequest> {
  const payloadHash = await hashInvitePayload(payload);
  const requests = readPendingInviteRequests();
  const existing = requests.find((request) => request.scope === scope && request.payloadHash === payloadHash);
  if (existing) return existing;
  const request = { id: crypto.randomUUID(), scope, payloadHash };
  requests.push(request);
  window.localStorage.setItem(PENDING_INVITE_REQUESTS_KEY, JSON.stringify(requests));
  return request;
}

function clearPendingInviteRequest(id: string) {
  const requests = readPendingInviteRequests().filter((request) => request.id !== id);
  window.localStorage.setItem(PENDING_INVITE_REQUESTS_KEY, JSON.stringify(requests));
}

function safeHttpsUrl(value: string): string | null {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && !url.username && !url.password ? url.toString() : null;
  } catch { return null; }
}

function markdownLabel(value: string): string {
  return value.replace(/[\\\[\]\r\n]/g, (character) => {
    if (character === "\r" || character === "\n") return " ";
    return `\\${character}`;
  });
}

function markdownDestination(value: string): string {
  return `<${value}>`;
}

function renderInlineMarkdown(value: string): ReactNode[] {
  const parts = value.split(/(\[[^\]]+\]\([^)]+\)|\*\*.+?\*\*|\*.+?\*|`[^`]+`)/g);
  return parts.map((part, index) => {
    const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
    if (link) {
      const href = safeHttpsUrl(link[2]);
      return href ? <a key={index} href={href} target="_blank" rel="noopener noreferrer">{link[1]}</a> : <span key={index}>{part}</span>;
    }
    if (part.startsWith("**") && part.endsWith("**")) return <strong key={index}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("*") && part.endsWith("*")) return <em key={index}>{part.slice(1, -1)}</em>;
    if (part.startsWith("`") && part.endsWith("`")) return <code key={index}>{part.slice(1, -1)}</code>;
    return <span key={index}>{part}</span>;
  });
}

function renderTextMarkdown(value: string): ReactNode[] {
  const result: ReactNode[] = [];
  let paragraph: string[] = [];
  let list: string[] = [];
  let orderedList: string[] = [];
  let code: string[] | null = null;
  const flushParagraph = () => {
    if (paragraph.length) result.push(<p key={`p-${result.length}`}>{paragraph.map(renderInlineMarkdown).map((line, index) => <span key={index}>{index > 0 && <br />}{line}</span>)}</p>);
    paragraph = [];
  };
  const flushList = () => {
    if (list.length) result.push(<ul key={`ul-${result.length}`}>{list.map((line, index) => <li key={index}>{renderInlineMarkdown(line)}</li>)}</ul>);
    list = [];
  };
  const flushOrderedList = () => {
    if (orderedList.length) result.push(<ol key={`ol-${result.length}`}>{orderedList.map((line, index) => <li key={index}>{renderInlineMarkdown(line)}</li>)}</ol>);
    orderedList = [];
  };
  for (const line of value.split(/\r?\n/)) {
    if (line.trim().startsWith("```")) {
      flushParagraph(); flushList(); flushOrderedList();
      if (code) { result.push(<pre key={`code-${result.length}`}><code>{code.join("\n")}</code></pre>); code = null; }
      else code = [];
      continue;
    }
    if (code) { code.push(line); continue; }
    if (!line.trim()) { flushParagraph(); flushList(); flushOrderedList(); continue; }
    const heading = line.match(/^(#{1,3})\s+(.+)$/);
    if (heading) {
      flushParagraph(); flushList(); flushOrderedList();
      const content = renderInlineMarkdown(heading[2]);
      const key = `h-${result.length}`;
      if (heading[1].length === 1) result.push(<h1 key={key}>{content}</h1>);
      else if (heading[1].length === 2) result.push(<h2 key={key}>{content}</h2>);
      else result.push(<h3 key={key}>{content}</h3>);
      continue;
    }
    const item = line.match(/^\s*[-*+]\s+(.+)$/);
    if (item) { flushParagraph(); flushOrderedList(); list.push(item[1]); continue; }
    const numberedItem = line.match(/^\s*\d+\.\s+(.+)$/);
    if (numberedItem) { flushParagraph(); flushList(); orderedList.push(numberedItem[1]); continue; }
    flushList(); flushOrderedList();
    const quote = line.match(/^\s*&gt;\s*(.*)$/) ?? line.match(/^\s*>\s*(.*)$/);
    if (quote) { flushParagraph(); result.push(<blockquote key={`q-${result.length}`}>{renderInlineMarkdown(quote[1])}</blockquote>); continue; }
    paragraph.push(line);
  }
  if (code) result.push(<pre key={`code-${result.length}`}><code>{code.join("\n")}</code></pre>);
  flushParagraph(); flushList(); flushOrderedList();
  return result;
}

export default function Publishing({
  project,
  draftId,
  projects = [],
  catalogue,
  onEditingChange,
  onOpenRoblox,
}: {
  onOpenRoblox?: (folderId: string) => void;
  project?: Project;
  draftId?: string;
  projects?: Project[];
  catalogue: Catalogue | null;
  onEditingChange: (dirty: boolean) => void;
}) {
  const [browser, setBrowser] = useState<BrowserStatus | null>(null);
  useEffect(() => {
    if (!draftId) return;
    let active = true;
    const poll = () => {
      void browserStatus()
        .then((s) => {
          if (active) setBrowser(s);
        })
        .catch(() => {});
    };
    poll();
    const timer = setInterval(poll, 1500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [draftId]);
  const [mediaProject, setMediaProject] = useState(project?.id ?? "");
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [sources, setSources] = useState<PostSourceStatus[]>([]),
    [reviewed, setReviewed] = useState(false);
  const [pendingFolder, setPendingFolder] = useState<string | null>(null);
  const [saved, setSaved] = useState<PublishingDraft | null>(null),
    [data, setData] = useState<PublishingData | null>(null),
    [step, setStep] = useState<(typeof steps)[number]>("Liveries"),
    [busy, setBusy] = useState(false),
    [dirty, setDirty] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [images, setImages] = useState<PublishingPreview[]>([]),
    [board, setBoard] = useState<AssetBoard>({
      assets: [],
      folders: [],
      placements: [],
    }),
    [source, setSource] = useState(""),
    [activeLivery, setActiveLivery] = useState(0),
    [showAssets, setShowAssets] = useState(false),
    [vehicleOpen, setVehicleOpen] = useState(false),
    [vehicleIndex, setVehicleIndex] = useState(0);
  const [libraryItems, setLibraryItems] = useState<MediaLibraryItem[]>([]);
  const [libraryChoice, setLibraryChoice] = useState("");
  const [projectImages, setProjectImages] = useState<Asset[]>([]);
  const [projectImageChoice, setProjectImageChoice] = useState("");
  const [inviteCampaigns, setInviteCampaigns] = useState<InviteCampaign[]>([]);
  const [inviteCampaignsLoading, setInviteCampaignsLoading] = useState(false);
  const [inviteCampaignsError, setInviteCampaignsError] = useState("");
  const [markdownPresets, setMarkdownPresets] = useState<MarkdownPreset[]>([]);
  const [markdownPresetChoice, setMarkdownPresetChoice] = useState("");
  const [savingMarkdownBlockId, setSavingMarkdownBlockId] = useState("");
  const [markdownPresetName, setMarkdownPresetName] = useState("");
  const [imageHostingAvailable, setImageHostingAvailable] = useState(false);
  const [inviteTrackingAvailable, setInviteTrackingAvailable] = useState(false);
  const [inviteCampaignBusy, setInviteCampaignBusy] = useState(false);
  const inviteRequestId = useRef<{ id: string; payload: string; scope: string } | null>(null);
  const [inviteTitle, setInviteTitle] = useState("");
  const [inviteCategory, setInviteCategory] = useState<InviteCampaign["category"]>("livery");
  const [inviteChannelId, setInviteChannelId] = useState("");
  const [inviteNotes, setInviteNotes] = useState("");
  const [inviteMaxAge, setInviteMaxAge] = useState(0);
  const [inviteMaxUses, setInviteMaxUses] = useState(0);
  useEffect(() => {
    let active = true;
    void listMarkdownPresets().then((presets) => { if (active) setMarkdownPresets(presets); }).catch(() => {});
    const serverUrl = window.localStorage.getItem("apex-studio.team-sync.server-url") ?? "";
    const teamId = window.localStorage.getItem("apex-studio.team-sync.active-team") ?? "";
    void mediaLibrary().then((library) => {
      if (!active) return;
      setLibraryItems((current) => {
        const localRemoteIds = new Set(library.map((item) => item.providerImageId).filter(Boolean));
        const remoteItems = current.filter((item) => item.isRemote && !localRemoteIds.has(item.providerImageId));
        return [...remoteItems, ...library];
      });
    }).catch(() => {});
    void listAssets(null).then((assets) => {
      if (active) setProjectImages(assets.filter((asset) => asset.name.toLowerCase().endsWith(".png")));
    }).catch(() => {});
    if (serverUrl && teamId) {
      void syncServiceAvailability(serverUrl, teamId).then(async (availability) => {
        if (!active) return;
        const inviteTrackingReady = availability.inviteTracking
          && availability.inviteTrackingReachable !== false;
        setInviteTrackingAvailable(inviteTrackingReady);
        setImageHostingAvailable(availability.imageHosting
          && availability.imageHostingReachable !== false);
        if (inviteTrackingReady) {
          setInviteCampaignsLoading(true);
          try {
            const campaigns = await syncListInviteCampaigns(serverUrl, teamId);
            if (active) {
              setInviteCampaigns(campaigns);
              setInviteCampaignsError("");
            }
          } catch (cause) {
            if (active) setInviteCampaignsError(errorMessage(cause));
          } finally {
            if (active) setInviteCampaignsLoading(false);
          }
        }
      }).catch(() => { /* Optional services do not gate local editing. */ });
      void syncImageLibrary(serverUrl, teamId).then((shared) => {
        if (!active) return;
        setLibraryItems((current) => {
          const localItems = current.filter((item) => !item.isRemote);
          const localRemoteIds = new Set(localItems.map((item) => item.providerImageId).filter(Boolean));
          const remoteItems: MediaLibraryItem[] = shared.images
            .filter((image) => !localRemoteIds.has(image.id))
            .map((image) => ({ id: `remote-${image.id}`, hash: null, name: image.name, altText: image.altText, imageUrl: image.imageUrl, defaultLinkUrl: image.defaultLinkUrl, folder: "", tags: [], overlayVersionGroup: null, overlayVersion: null, overlayTarget: null, ownerScope: image.ownerScope, teamId: image.ownerScope === "team" ? teamId : null, providerImageId: image.id, createdAt: image.createdAt, updatedAt: image.createdAt, preview: null, isRemote: true }));
          return [...remoteItems, ...localItems];
        });
      }).catch(() => { /* The local library stays available without the server. */ });
    }
    return () => { active = false; };
  }, []);
  useEffect(() => {
    let active = true;
    setError("");
    Promise.all([
      draftId ? postDraft(draftId) : publishingDraft(project!.id),
      project
        ? assetBoard(project.id)
        : Promise.resolve({ assets: [], folders: [], placements: [] }),
    ])
      .then(([d, b]) => {
        if (active) {
          setSaved(d);
          setData(d.data);
          setBoard(b);
        }
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [project?.id, draftId, loadAttempt]);
  useEffect(() => {
    if (!draftId) return;
    void postSources(draftId)
      .then(setSources)
      .catch((e) => setError(errorMessage(e)));
  }, [draftId, saved?.revision]);
  useEffect(
    () => () => {
      if (draftId) void releasePostImages(draftId).catch(() => {});
    },
    [draftId],
  );
  useEffect(() => {
    if (!draftId || !data?.media.length) return;
    const renew = () => {
      void pinPostImages(
        draftId,
        data.media.map((m) => m.hash),
      ).catch((e) => setError(errorMessage(e)));
    };
    renew();
    const timer = setInterval(renew, 300000);
    return () => clearInterval(timer);
  }, [draftId, data?.media]);
  useEffect(() => {
    if (!draftId) return;
    if (!mediaProject) {
      setBoard({ assets: [], folders: [], placements: [] });
      return;
    }
    void assetBoard(mediaProject)
      .then(setBoard)
      .catch((e) => setError(errorMessage(e)));
  }, [draftId, mediaProject]);
  useEffect(() => {
    onEditingChange(dirty || busy);
    return () => onEditingChange(false);
  }, [dirty, busy, pendingFolder, onEditingChange]);
  useEffect(() => {
    let active = true;
    if (!data?.media.length) {
      setImages([]);
      return;
    }
    publishingPreviews(data.media)
      .then((v) => {
        if (active) setImages(v);
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [data?.media]);
  const locked = busy || !!project?.archived;
  function edit(patch: Partial<PublishingData>) {
    if (!data) return;
    const next = { ...data, ...patch };
    if (JSON.stringify(next) === JSON.stringify(data)) return;
    setReviewed(false);
    setData(next);
    setDirty(JSON.stringify(next) !== JSON.stringify(saved?.data));
    setNotice("");
  }
  const markdownBlocks = data?.markdownBlocks ?? [];
  function updateMarkdown(blocks: MarkdownBlock[]) { edit({ markdownBlocks: blocks }); }
  async function saveReusableBlock(block: MarkdownBlock) {
    if (!markdownPresetName.trim()) return;
    setError("");
    try {
      const preset = await saveMarkdownPreset(markdownPresetName.trim(), block);
      setMarkdownPresets((current) => [preset, ...current.filter((item) => item.id !== preset.id)]);
      setMarkdownPresetChoice(preset.id);
      setSavingMarkdownBlockId("");
      setMarkdownPresetName("");
      setNotice("Reusable Markdown block saved in this workspace.");
    } catch (cause) { setError(errorMessage(cause)); }
  }
  function insertReusableBlock() {
    const preset = markdownPresets.find((item) => item.id === markdownPresetChoice);
    if (!preset || markdownBlocks.length >= 100) return;
    updateMarkdown([...markdownBlocks, { ...preset.block, id: crypto.randomUUID(), mediaHash: null }]);
    setNotice("Reusable block added to this post.");
  }
  async function removeReusableBlock() {
    const preset = markdownPresets.find((item) => item.id === markdownPresetChoice);
    if (!preset || !window.confirm(`Delete reusable block “${preset.name}”? Existing posts will not change.`)) return;
    try {
      await deleteMarkdownPreset(preset.id);
      setMarkdownPresets((current) => current.filter((item) => item.id !== preset.id));
      setMarkdownPresetChoice("");
    } catch (cause) { setError(errorMessage(cause)); }
  }
  function addMarkdown(kind: MarkdownBlock["kind"]) {
    const block: MarkdownBlock = { id: crypto.randomUUID(), kind, text: kind === "invite" ? "Join our Discord" : "", imageUrl: "", altText: "", linkUrl: "", inviteUrl: "", mediaHash: null };
    updateMarkdown([...markdownBlocks, block]);
  }
  function addInviteUrl(label: string, inviteUrl: string) {
    if (!inviteUrl || markdownBlocks.length >= 100) return;
    updateMarkdown([...markdownBlocks, { id: crypto.randomUUID(), kind: "invite", text: label, imageUrl: "", altText: "", linkUrl: "", inviteUrl, mediaHash: null }]);
  }
  async function createTrackedInvite() {
    const serverUrl = window.localStorage.getItem("apex-studio.team-sync.server-url") ?? "";
    const teamId = window.localStorage.getItem("apex-studio.team-sync.active-team") ?? "";
    if (!serverUrl || !teamId || !inviteTrackingAvailable) return;
    if (!window.confirm("Create this tracked Discord invite for the selected team?")) return;
    setInviteCampaignBusy(true);
    setError("");
    try {
      const input = {
        title: inviteTitle.trim(), category: inviteCategory, notes: inviteNotes.trim(),
        channelId: inviteChannelId.trim(), maxAgeDays: inviteMaxAge, maxUses: inviteMaxUses,
      };
      const payload = JSON.stringify(input);
      const scope = JSON.stringify([serverUrl, teamId, draftId ?? project?.id ?? "global"]);
      const persisted = await ensurePendingInviteRequest(scope, payload);
      if (inviteRequestId.current?.payload !== payload || inviteRequestId.current.scope !== scope) {
        inviteRequestId.current = { ...persisted, payload };
      }
      const campaign = await syncCreateInviteCampaign(serverUrl, teamId, inviteRequestId.current.id, input);
      inviteRequestId.current = null;
      setInviteCampaigns((current) => [campaign, ...current.filter((item) => item.id !== campaign.id)]);
      setInviteCampaignsError("");
      if (campaign.inviteUrl) addInviteUrl(campaign.title, campaign.inviteUrl);
      setInviteTitle(""); setInviteNotes("");
      try {
        clearPendingInviteRequest(persisted.id);
        setNotice("Tracked invite created and added to the post.");
      } catch {
        setNotice("Tracked invite created and added. Its retry key remains saved locally.");
      }
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setInviteCampaignBusy(false); }
  }
  async function refreshTrackedInvites() {
    const serverUrl = window.localStorage.getItem("apex-studio.team-sync.server-url") ?? "";
    const teamId = window.localStorage.getItem("apex-studio.team-sync.active-team") ?? "";
    if (!serverUrl || !teamId || !inviteTrackingAvailable || inviteCampaignsLoading) return;
    setInviteCampaignsLoading(true);
    setInviteCampaignsError("");
    try {
      const campaigns = await syncListInviteCampaigns(serverUrl, teamId);
      if (window.localStorage.getItem("apex-studio.team-sync.server-url") === serverUrl
        && window.localStorage.getItem("apex-studio.team-sync.active-team") === teamId) {
        setInviteCampaigns(campaigns);
      }
    } catch (cause) {
      setInviteCampaignsError(errorMessage(cause));
    } finally {
      setInviteCampaignsLoading(false);
    }
  }
  function moveMarkdown(index: number, delta: number) {
    const target = index + delta;
    if (target < 0 || target >= markdownBlocks.length) return;
    const next = [...markdownBlocks];
    [next[index], next[target]] = [next[target], next[index]];
    updateMarkdown(next);
  }
  function markdownText() {
    return markdownBlocks.map((block) => {
      if (block.kind === "text") return block.text;
      if (block.kind === "invite") {
        const url = safeHttpsUrl(block.inviteUrl);
        return url ? `[${markdownLabel(block.text || "Discord invite")}](${markdownDestination(url)})` : "";
      }
      const imageUrl = safeHttpsUrl(block.imageUrl);
      if (!imageUrl) return "";
      const image = `![${markdownLabel(block.altText)}](${markdownDestination(imageUrl)})`;
      const linkUrl = safeHttpsUrl(block.linkUrl);
      return linkUrl ? `[${image}](${markdownDestination(linkUrl)})` : image;
    }).filter(Boolean).join("\n\n");
  }
  async function copyMarkdown() {
    const value = markdownText();
    if (!value) { setError("Add a public image URL or an invite link before copying."); return; }
    try { await navigator.clipboard.writeText(value); setNotice("Markdown copied."); }
    catch (e) { setError(errorMessage(e)); }
  }
  function addLibraryImage() {
    const item = libraryItems.find((candidate) => candidate.id === libraryChoice);
    if (!item) return;
    updateMarkdown([...markdownBlocks, { id: crypto.randomUUID(), kind: "image", text: "", imageUrl: item.imageUrl, altText: item.altText || item.name, linkUrl: item.defaultLinkUrl ?? "", inviteUrl: "", mediaHash: item.hash }]);
    setLibraryChoice("");
  }
  async function addLocalLibraryImage() {
    try {
      const item = await pickMediaLibraryImage();
      if (!item) return;
      setLibraryItems((items) => [item, ...items.filter((existing) => existing.id !== item.id)]);
      setLibraryChoice(item.id);
      if (!item.imageUrl) setNotice("Image added to the library. Add its public URL after uploading it where you choose.");
    } catch (e) { setError(errorMessage(e)); }
  }
  async function addProjectLibraryImage() {
    const asset = projectImages.find((item) => item.id === projectImageChoice);
    if (!asset) return;
    try {
      const item = await addMediaLibraryAsset(asset.projectId, asset.id);
      setLibraryItems((items) => [item, ...items.filter((existing) => existing.id !== item.id)]);
      setMarkdownImage(item);
      setProjectImageChoice("");
    } catch (e) { setError(errorMessage(e)); }
  }
  async function uploadMarkdownImages() {
    const serverUrl = window.localStorage.getItem("apex-studio.team-sync.server-url") ?? "";
    const teamId = window.localStorage.getItem("apex-studio.team-sync.active-team") ?? "";
    if (!serverUrl || !teamId || !imageHostingAvailable) {
      setError("Image Hosting is not available. Add a public image URL manually or configure Image Hosting in Server.");
      return;
    }
    const missingHashes = new Set(markdownBlocks
      .filter((block) => block.kind === "image" && !block.imageUrl.trim() && block.mediaHash)
      .map((block) => block.mediaHash as string));
    const pending = libraryItems.filter((item) =>
      item.hash && missingHashes.has(item.hash) && !item.providerImageId && !item.isRemote,
    ).slice(0, 10);
    if (!pending.length) return;
    if (!window.confirm(`Upload ${pending.length} local image${pending.length === 1 ? "" : "s"} to your personal Image Hosting space? Their public URLs will be saved into this draft. Continue?`)) return;

    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await syncUploadLibraryImages(serverUrl, teamId, pending.map((item) => item.id), "personal");
      const byClientId = new Map<string, typeof result.images[number]>();
      for (const image of result.images) {
        const local = image.clientId
          ? pending.find((item) => item.id === image.clientId)
          : pending.filter((item) => item.name === image.name).length === 1
            ? pending.find((item) => item.name === image.name)
            : undefined;
        if (local) byClientId.set(local.id, image);
      }
      setLibraryItems((current) => current.map((item) => {
        const uploaded = byClientId.get(item.id);
        return uploaded ? {
          ...item,
          imageUrl: uploaded.imageUrl,
          providerImageId: uploaded.id,
          ownerScope: "personal",
          teamId: null,
        } : item;
      }));
      const updatedBlocks = markdownBlocks.map((block) => {
        if (block.kind !== "image" || block.imageUrl.trim() || !block.mediaHash) return block;
        const local = pending.find((item) => item.hash === block.mediaHash);
        const uploaded = local ? byClientId.get(local.id) : undefined;
        return uploaded ? { ...block, imageUrl: uploaded.imageUrl, altText: block.altText || uploaded.altText } : block;
      });
      updateMarkdown(updatedBlocks);
      const errors = result.errors;
      const unmapped = result.images.length - byClientId.size;
      setNotice(`${byClientId.size} image${byClientId.size === 1 ? "" : "s"} uploaded or reused. URLs were added to this draft.${errors.length + unmapped ? ` ${errors.length + unmapped} image${errors.length + unmapped === 1 ? "" : "s"} need attention.` : ""}`);
      const issues = result.errors.map((entry) => `${entry.name || "Image"}: ${entry.error || "Upload failed"}`);
      if (unmapped) issues.push(`${unmapped} uploaded image${unmapped === 1 ? " could not" : "s could not"} be matched to a local file. Refresh the Image Library and add its public URL manually.`);
      if (issues.length) setError(issues.join("\n"));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }
  function setMarkdownImage(item: MediaLibraryItem) {
    updateMarkdown([...markdownBlocks, { id: crypto.randomUUID(), kind: "image", text: "", imageUrl: item.imageUrl, altText: item.altText || item.name, linkUrl: item.defaultLinkUrl ?? "", inviteUrl: "", mediaHash: item.hash }]);
  }
  function changeLivery(patch: Partial<PublishingLivery>) {
    if (!data) return;
    edit({
      liveries: data.liveries.map((l, i) =>
        i === activeLivery
          ? {
              ...l,
              ...patch,
              ...(patch.sides || patch.vehicle ? { source: undefined } : {}),
            }
          : l,
      ),
    });
  }
  async function addImages() {
    setBusy(true);
    setError("");
    try {
      const selected = await (draftId
        ? pickPostImages(draftId)
        : pickPublishingImages(project!.id));
      if (selected && data) {
        const merged = [...data.media];
        for (const image of selected)
          if (!merged.some((m) => m.hash === image.hash))
            merged.push({ hash: image.hash, name: image.name });
        if (merged.length > 10)
          throw Error(
            "Use up to ten preview images. Remove an image before adding more.",
          );
        edit({ media: merged });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function addOverlayPreviews(output: OverlayPreviewOutput[]) {
    if (!data) return;
    const selected = await importOverlayPreviews(
      draftId ?? null,
      project?.id ?? null,
      output,
    );
    const merged = [...data.media];
    for (const image of selected) {
      if (!merged.some((item) => item.hash === image.hash)) {
        if (merged.length >= 10) throw new Error("This post already has ten preview images.");
        merged.push({ hash: image.hash, name: image.name });
      }
    }
    edit({ media: merged });
  }
  async function save() {
    if (!saved || !data) return;
    setBusy(true);
    setError("");
    try {
      const next = await (draftId
        ? savePost(draftId, saved.revision, data)
        : savePublishing(project!.id, saved.revision, data));
      setSaved(next);
      setData(next.data);
      setDirty(false);
      setNotice("Publishing draft saved.");
      if (draftId) {
        try {
          await releasePostImages(draftId);
        } catch (e) {
          setError(errorMessage(e));
        }
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function exportBundle() {
    setBusy(true);
    setError("");
    try {
      if (
        await (draftId
          ? exportPost(draftId, saved!.revision, reviewed)
          : exportPublishing(project!.id))
      )
        setNotice("Publishing bundle exported.");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  function includeLivery(next: PublishingLivery) {
    if (!data) return;
    const existing = next.source
      ? data.liveries.findIndex(
          (l) => l.source?.folderId === next.source!.folderId,
        )
      : -1;
    if (existing >= 0) {
      edit({
        liveries: data.liveries.map((l, i) =>
          i === existing
            ? {
                ...next,
                primaryColor: l.primaryColor,
                secondaryColor: l.secondaryColor,
                opacity: l.opacity,
              }
            : l,
        ),
      });
      setActiveLivery(existing);
    } else {
      edit({ liveries: [...data.liveries, next] });
      setActiveLivery(data.liveries.length);
    }
    setPendingFolder(null);
  }
  async function addLivery() {
    if (!source || !data) return;
    setBusy(true);
    setError("");
    try {
      const l = await robloxLivery(source);
      if (readyForPublishing(l)) includeLivery(publishingLivery(l));
      else setPendingFolder(source);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  if (error && !data)
    return (
      <div role="alert" className="error">
        <p>{error}</p>
        <button
          className="secondary"
          type="button"
          onClick={() => setLoadAttempt((n) => n + 1)}
        >
          Retry loading draft
        </button>
      </div>
    );
  if (!data || !saved) return <p role="status">Loading publishing draft…</p>;
  const current = data.liveries[activeLivery];
  const vehicleMatches = current?.vehicle.trim()
    ? catalogue?.vehicles
        .filter((vehicle) =>
          vehicle.name.toLowerCase().includes(current.vehicle.toLowerCase()),
        )
        .slice(0, 8) ?? []
    : [];
  const requirements = [
    ...(data.title.trim().length < 5 ? ["Title: at least 5 characters."] : []),
    ...(!data.description.trim() ? ["Description is required."] : []),
    ...(data.media.length < 2 ? ["Previews: choose at least two images."] : []),
    ...(!data.liveries.length ? ["Add at least one livery."] : []),
    ...data.liveries.flatMap((l, i) =>
      !l.vehicle.trim() ||
      !l.sides.length ||
      l.sides.some((s) => !s.side || !/^[1-9][0-9]{0,19}$/.test(s.decalId))
        ? [`Livery ${i + 1}: enter a vehicle, sides and numeric decal IDs.`]
        : [],
    ),
  ];
  return (
    <section className="publishing-editor" aria-label="Publishing draft">
      {pendingFolder && (
        <div className="queue-prompt">
          <p>This livery needs Roblox decal IDs.</p>
          <button
            disabled={dirty || busy}
            onClick={() => onOpenRoblox?.(pendingFolder)}
          >
            Open Roblox uploads
          </button>
          <button onClick={() => setPendingFolder(null)}>Skip livery</button>
          {dirty && <p>Save your draft before opening uploads.</p>}
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <nav className="publishing-steps" aria-label="Publishing sections">
        {steps.map((s, index) => (
          <button
            key={s}
            className="publishing-step"
            aria-current={step === s ? "step" : undefined}
            onClick={() => setStep(s)}
            disabled={busy}
          >
            <span className="publishing-step-number" aria-hidden="true">{index + 1}</span>
            <span>{s}</span>
          </button>
        ))}
      </nav>
      {step === "Details" && (
        <div className="publishing-fields">
          <label>
            Title
            <input
              aria-label="Post title"
              maxLength={50}
              value={data.title}
              disabled={locked}
              onChange={(e) => edit({ title: e.target.value })}
            />
          </label>
          <span className="field-hint">{data.title.length}/50</span>
          <label>
            Description
            <textarea
              aria-label="Post description"
              rows={8}
              maxLength={2000}
              value={data.description}
              disabled={locked}
              onChange={(e) => edit({ description: e.target.value })}
            />
          </label>
          <span className="field-hint">
            {data.description.length}/2000 · Markdown supported
          </span>
          <label>
            Tags
            <input
              aria-label="Post tags"
              value={data.tags.join(", ")}
              disabled={locked}
              onChange={(e) =>
                edit({ tags: e.target.value.split(",").map((t) => t.trim()) })
              }
              onBlur={() => edit({ tags: data.tags.filter(Boolean) })}
            />
          </label>
          <p className="field-hint">
            Separate tags with commas. Up to 10 tags.
          </p>
          <label>
            YouTube URL
            <input
              aria-label="YouTube URL"
              type="url"
              value={data.youtubeUrl ?? ""}
              disabled={locked}
              onChange={(e) => edit({ youtubeUrl: e.target.value || null })}
            />
          </label>
        </div>
      )}
      {step === "Previews" && (
        <section>
          <div className="publishing-section-actions">
            <h2>Preview images</h2>
            <div className="button-row">
              <button
                className="secondary"
                disabled={locked || data.media.length >= 10}
                onClick={() => void addImages()}
              >
                Add images
              </button>
              {board.assets.some((a) => /\.png$/i.test(a.name)) && (
                <button
                  className="text-button"
                  disabled={locked}
                  onClick={() => setShowAssets((v) => !v)}
                >
                  Choose project images
                </button>
              )}
            </div>
          </div>
          <p className="field-hint">
            2–10 PNG images. The first image is the post cover.
          </p>
          {draftId && (
            <label>
              Image project
              <AppSelect
                value={mediaProject}
                disabled={locked}
                onChange={(e) => setMediaProject(e.target.value)}
              >
                <option value="">Choose project</option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </AppSelect>
            </label>
          )}
          {showAssets && (
            <div className="publishing-asset-choices">
              {board.assets
                .filter((a) => /\.png$/i.test(a.name))
                .map((a) => (
                  <button
                    key={a.id}
                    className="secondary"
                    disabled={
                      locked ||
                      data.media.length >= 10 ||
                      data.media.some((m) => m.hash === a.hash)
                    }
                    onClick={() => {
                      void (async () => {
                        try {
                          if (draftId) await pinPostImages(draftId, [a.hash]);
                          edit({
                            media: [
                              ...data.media,
                              { hash: a.hash, name: a.name },
                            ],
                          });
                        } catch (e) {
                          setError(errorMessage(e));
                        }
                      })();
                    }}
                  >
                    {a.name}
                  </button>
                ))}
            </div>
          )}
          <ol className="publishing-media-list">
            {data.media.map((m, i) => (
              <li key={m.hash}>
                {images.find((p) => p.hash === m.hash)?.preview && (
                  <img
                    src={images.find((p) => p.hash === m.hash)!.preview!}
                    alt={m.name}
                  />
                )}
                <div>
                  <strong>{m.name}</strong>
                  {i === 0 && <small>Post cover</small>}
                </div>
                <div className="publishing-media-actions">
                  <button
                    className="text-button"
                    aria-label={`Move ${m.name} up`}
                    disabled={locked || i === 0}
                    onClick={() => {
                      const media = [...data.media];
                      [media[i - 1], media[i]] = [media[i], media[i - 1]];
                      edit({ media });
                    }}
                  >
                    Up
                  </button>
                  <button
                    className="text-button"
                    aria-label={`Move ${m.name} down`}
                    disabled={locked || i === data.media.length - 1}
                    onClick={() => {
                      const media = [...data.media];
                      [media[i + 1], media[i]] = [media[i], media[i + 1]];
                      edit({ media });
                    }}
                  >
                    Down
                  </button>
                  <button
                    className="text-button"
                    disabled={locked}
                    onClick={() =>
                      edit({
                        media: data.media.filter((v) => v.hash !== m.hash),
                      })
                    }
                  >
                    Remove
                  </button>
                </div>
              </li>
            ))}
          </ol>
          <details className="publishing-overlay-details">
            <summary>Create overlay previews</summary>
            <OverlayStudio
              postId={draftId}
              projectId={draftId ? undefined : project?.id}
              currentCount={data.media.length}
              disabled={locked}
              projectImages={projectImages}
              libraryItems={libraryItems}
              onExported={addOverlayPreviews}
            />
          </details>
        </section>
      )}
      {step === "Liveries" && (
        <section>
          <div className="publishing-section-actions">
            <h2>Liveries for ERLCX</h2>
            {!draftId && (
              <div className="publishing-add-livery">
                <AppSelect
                  aria-label="Livery source"
                  value={source}
                  disabled={locked}
                  onChange={(e) => setSource(e.target.value)}
                >
                  <option value="">Choose a project livery</option>
                  {board.folders
                    .filter((f) => f.kind === "livery")
                    .map((f) => (
                      <option key={f.id} value={f.id}>
                        {f.name}
                      </option>
                    ))}
                </AppSelect>
                <button
                  className="secondary"
                  disabled={locked || !source || data.liveries.length >= 20}
                  onClick={() => void addLivery()}
                >
                  Include livery
                </button>
              </div>
            )}
          </div>
          <p className="field-hint">
            Upload livery textures to Roblox, then include their decal IDs here.
            Preview images for ERLCX are selected separately.
          </p>
          {draftId && (
            <PostLiveryPicker
              onBusy={setBusy}
              catalogue={catalogue}
              value={data.liveries}
              disabled={locked}
              onChange={(liveries) => {
                edit({ liveries });
                setActiveLivery(0);
              }}
            />
          )}
          {!data.liveries.length && <p>No liveries selected.</p>}
          <details className="publishing-manual">
            <summary>Use existing decal IDs manually</summary>
            <p className="field-hint">
              For textures already uploaded outside Apex Studio.
            </p>
            <button
              className="secondary"
              disabled={locked || data.liveries.length >= 20}
              onClick={() => {
                includeLivery({
                  vehicle: "",
                  primaryColor: "#FFFFFF",
                  secondaryColor: null,
                  opacity: 100,
                  sides: [{ side: "Left 1", decalId: "" }],
                });
              }}
            >
              Add manual livery
            </button>
          </details>
          {data.liveries.length > 0 && (
            <div className="publishing-code-layout">
              <div className="publishing-livery-list">
                {data.liveries.map((l, i) => (
                  <button
                    key={i}
                    className="secondary"
                    aria-pressed={activeLivery === i}
                    onClick={() => setActiveLivery(i)}
                  >
                    {l.vehicle || `Livery ${i + 1}`}
                  </button>
                ))}
              </div>
              {current && (
                <div className="publishing-fields">
                  {current.source && (
                    <div className="publishing-source">
                      <span>Roblox decal IDs linked to this livery.</span>
                      <button
                        className="secondary"
                        disabled={locked}
                        onClick={() => {
                          setSource(current.source!.folderId);
                          void (async () => {
                            setBusy(true);
                            try {
                              const l = await robloxLivery(
                                current.source!.folderId,
                              );
                              if (readyForPublishing(l))
                                includeLivery(publishingLivery(l));
                              else setPendingFolder(current.source!.folderId);
                            } catch (e) {
                              setError(errorMessage(e));
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      >
                        Refresh Roblox IDs
                      </button>
                    </div>
                  )}
                  <div className="publishing-vehicle-field">
                    <label htmlFor="publishing-vehicle-input">Vehicle</label>
                    <span className="publishing-vehicle-picker">
                      <input
                        id="publishing-vehicle-input"
                        role="combobox"
                        aria-label="Publishing vehicle"
                        aria-autocomplete="list"
                        aria-expanded={vehicleOpen && vehicleMatches.length > 0}
                        aria-controls="publishing-vehicle-options"
                        aria-activedescendant={
                          vehicleOpen && vehicleMatches[vehicleIndex]
                            ? `publishing-vehicle-${vehicleMatches[vehicleIndex].id}`
                            : undefined
                        }
                        readOnly={!!current.source}
                        maxLength={120}
                        value={current.vehicle}
                        disabled={locked}
                        onFocus={() => setVehicleOpen(true)}
                        onBlur={() => setVehicleOpen(false)}
                        onChange={(e) => {
                          changeLivery({ vehicle: e.target.value });
                          setVehicleIndex(0);
                          setVehicleOpen(true);
                        }}
                        onKeyDown={(event) => {
                          if (event.key === "Escape") setVehicleOpen(false);
                          if (event.key === "ArrowDown" && vehicleMatches.length) {
                            event.preventDefault();
                            setVehicleOpen(true);
                            setVehicleIndex((index) => Math.min(index + 1, vehicleMatches.length - 1));
                          }
                          if (event.key === "ArrowUp" && vehicleMatches.length) {
                            event.preventDefault();
                            setVehicleIndex((index) => Math.max(0, index - 1));
                          }
                          if (event.key === "Enter" && vehicleOpen && vehicleMatches[vehicleIndex]) {
                            event.preventDefault();
                            changeLivery({ vehicle: vehicleMatches[vehicleIndex].name });
                            setVehicleOpen(false);
                          }
                        }}
                      />
                      {vehicleOpen && !locked && !current.source && vehicleMatches.length > 0 && (
                        <span id="publishing-vehicle-options" className="publishing-vehicle-options" role="listbox">
                          {vehicleMatches.map((vehicle, index) => (
                            <button
                              key={vehicle.id}
                              id={`publishing-vehicle-${vehicle.id}`}
                              type="button"
                              role="option"
                              aria-selected={index === vehicleIndex}
                              onPointerDown={(event) => event.preventDefault()}
                              onMouseEnter={() => setVehicleIndex(index)}
                              onClick={() => {
                                changeLivery({ vehicle: vehicle.name });
                                setVehicleOpen(false);
                              }}
                            >
                              {vehicle.name}
                            </button>
                          ))}
                        </span>
                      )}
                    </span>
                  </div>
                  <div className="publishing-colours">
                    <label>
                      Primary colour
                      <input
                        aria-label="Primary colour"
                        type="color"
                        value={current.primaryColor}
                        disabled={locked}
                        onChange={(e) =>
                          changeLivery({ primaryColor: e.target.value })
                        }
                      />
                    </label>
                    <label>
                      Opacity
                      <input
                        aria-label="Opacity"
                        type="number"
                        min={0}
                        max={100}
                        value={current.opacity}
                        disabled={locked}
                        onChange={(e) =>
                          changeLivery({ opacity: Number(e.target.value) })
                        }
                      />
                    </label>
                  </div>
                  <label className="check-label">
                    <input
                      type="checkbox"
                      checked={current.secondaryColor !== null}
                      disabled={locked}
                      onChange={(e) =>
                        changeLivery({
                          secondaryColor: e.target.checked ? "#000000" : null,
                        })
                      }
                    />
                    Secondary colour
                  </label>
                  {current.secondaryColor !== null && (
                    <input
                      aria-label="Secondary colour"
                      type="color"
                      value={current.secondaryColor}
                      disabled={locked}
                      onChange={(e) =>
                        changeLivery({ secondaryColor: e.target.value })
                      }
                    />
                  )}
                  <div className="publishing-sides">
                    {current.sides.map((s, i) => (
                      <div className="publishing-side" key={i}>
                        <SlotEditor
                          label={`code row ${i + 1}`}
                          value={s.side || null}
                          disabled={locked || !!current.source}
                          onChange={(side) =>
                            changeLivery({
                              sides: current.sides.map((v, j) =>
                                i === j ? { ...v, side: side ?? "" } : v,
                              ),
                            })
                          }
                        />
                        <input
                          aria-label={`Decal ID for row ${i + 1}`}
                          inputMode="numeric"
                          maxLength={20}
                          placeholder="Decal ID"
                          value={s.decalId}
                          disabled={locked || !!current.source}
                          onChange={(e) =>
                            changeLivery({
                              sides: current.sides.map((v, j) =>
                                j === i ? { ...v, decalId: e.target.value } : v,
                              ),
                            })
                          }
                        />
                        <button
                          className="text-button"
                          aria-label={`Remove code row ${i + 1}`}
                          disabled={locked || !!current.source}
                          onClick={() =>
                            changeLivery({
                              sides: current.sides.filter((_, j) => i !== j),
                            })
                          }
                        >
                          Remove
                        </button>
                      </div>
                    ))}
                  </div>
                  <div className="publishing-section-actions">
                    <button
                      className="secondary"
                      disabled={
                        locked ||
                        !!current.source ||
                        current.sides.length >= 100
                      }
                      onClick={() =>
                        changeLivery({
                          sides: [...current.sides, { side: "", decalId: "" }],
                        })
                      }
                    >
                      Add side
                    </button>
                    <button
                      className="text-button"
                      disabled={locked}
                      onClick={() => {
                        edit({
                          liveries: data.liveries.filter(
                            (_, i) => i !== activeLivery,
                          ),
                        });
                        setActiveLivery(0);
                      }}
                    >
                      Remove livery
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </section>
      )}
      {step === "Markdown" && (
        <section className="publishing-markdown">
          <div className="publishing-markdown-heading">
            <div><h2>Post Markdown</h2><p className="field-hint">Build reusable text, linked images and invite links. Local images need a public URL before they can be copied.</p></div>
            <button className="primary" type="button" disabled={locked} onClick={() => void copyMarkdown()}><Copy size={16} /> Copy Markdown</button>
          </div>
          <div className="publishing-markdown-toolbar">
            <button className="secondary" type="button" disabled={locked || markdownBlocks.length >= 100} onClick={() => addMarkdown("text")}>Add text</button>
            <button className="secondary" type="button" disabled={locked || markdownBlocks.length >= 100} onClick={() => addMarkdown("image")}>Add image URL</button>
            <button className="secondary" type="button" disabled={locked || markdownBlocks.length >= 100} onClick={() => addMarkdown("invite")}>Add invite link</button>
            <button className="secondary" type="button" disabled={locked || markdownBlocks.length >= 100} onClick={() => void addLocalLibraryImage()}>Add local PNG</button>
            {projectImages.length > 0 && <><select aria-label="Choose a project image" value={projectImageChoice} disabled={locked} onChange={(event) => setProjectImageChoice(event.target.value)}><option value="">Choose project image</option>{projectImages.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><button className="secondary" type="button" disabled={locked || !projectImageChoice || markdownBlocks.length >= 100} onClick={() => void addProjectLibraryImage()}>Add project image</button></>}
            {libraryItems.length > 0 && <><select aria-label="Choose an image from Image Library" value={libraryChoice} disabled={locked} onChange={(event) => setLibraryChoice(event.target.value)}><option value="">Choose library image</option>{libraryItems.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><button className="secondary" type="button" disabled={locked || !libraryChoice || markdownBlocks.length >= 100} onClick={addLibraryImage}>Add library image</button></>}
            {imageHostingAvailable && <button className="secondary" type="button" disabled={locked || !markdownBlocks.some((block) => block.kind === "image" && !block.imageUrl.trim() && block.mediaHash && libraryItems.some((item) => item.hash === block.mediaHash && item.hash && !item.providerImageId && !item.isRemote))} onClick={() => void uploadMarkdownImages()}><Upload size={15} /> Upload missing images</button>}
          </div>
          <section className="publishing-reusable-markdown" aria-label="Reusable Markdown blocks">
            <div><strong>Reusable blocks</strong><p>{markdownPresets.length ? "Saved text, image and invite blocks are available in this workspace." : "Save a block below to reuse it in other posts."}</p></div>
            <div className="button-row">
              <select aria-label="Choose reusable Markdown block" value={markdownPresetChoice} disabled={locked || !markdownPresets.length} onChange={(event) => setMarkdownPresetChoice(event.target.value)}><option value="">Choose a saved block</option>{markdownPresets.map((preset) => <option key={preset.id} value={preset.id}>{preset.name} · {preset.block.kind}</option>)}</select>
              <button className="secondary" type="button" disabled={locked || !markdownPresetChoice || markdownBlocks.length >= 100} onClick={insertReusableBlock}>Insert block</button>
              <button className="text-button" type="button" disabled={locked || !markdownPresetChoice} onClick={() => void removeReusableBlock()}>Delete saved block</button>
            </div>
          </section>
          {inviteTrackingAvailable && <section className="publishing-invite-tracker">
            <div className="publishing-markdown-heading"><div><h3>Tracked Discord invites</h3><p className="field-hint">Create an invite for a specific post or add an existing team invite link.</p></div><button type="button" className="secondary" disabled={locked || inviteCampaignsLoading} onClick={() => void refreshTrackedInvites()}>{inviteCampaignsLoading ? "Loading…" : "Refresh campaigns"}</button></div>
            <div className="publishing-invite-form">
              <label>Campaign title<input value={inviteTitle} maxLength={80} disabled={locked || inviteCampaignBusy} onChange={(event) => setInviteTitle(event.target.value)} placeholder="ERLCX release post" /></label>
              <label>Category<select value={inviteCategory} disabled={locked || inviteCampaignBusy} onChange={(event) => setInviteCategory(event.target.value as InviteCampaign["category"])}><option value="livery">Livery</option><option value="social">Social</option><option value="other">Other</option></select></label>
              <label>Discord channel ID<input inputMode="numeric" value={inviteChannelId} disabled={locked || inviteCampaignBusy} onChange={(event) => setInviteChannelId(event.target.value)} placeholder="17–20 digit channel ID" /></label>
              <label>Expiry<select value={inviteMaxAge} disabled={locked || inviteCampaignBusy} onChange={(event) => setInviteMaxAge(Number(event.target.value))}><option value={0}>Never</option><option value={1}>1 day</option><option value={7}>7 days</option><option value={30}>30 days</option><option value={90}>90 days</option><option value={365}>365 days</option></select></label>
              <label>Maximum uses<input type="number" min={0} max={100000} value={inviteMaxUses} disabled={locked || inviteCampaignBusy} onChange={(event) => setInviteMaxUses(Number(event.target.value))} /></label>
              <label>Notes <span className="optional">(optional)</span><input value={inviteNotes} maxLength={280} disabled={locked || inviteCampaignBusy} onChange={(event) => setInviteNotes(event.target.value)} placeholder="Internal context" /></label>
            </div>
            <button className="secondary" type="button" disabled={locked || inviteCampaignBusy || inviteTitle.trim().length < 2 || !/^\d{17,20}$/.test(inviteChannelId)} onClick={() => void createTrackedInvite()}>{inviteCampaignBusy ? "Creating…" : "Create invite and add link"}</button>
            {inviteCampaignsLoading && <p role="status" className="field-hint">Loading tracked campaigns…</p>}
            {inviteCampaignsError && <p role="alert" className="error">Could not load tracked campaigns: {inviteCampaignsError}</p>}
            {!inviteCampaignsLoading && !inviteCampaignsError && inviteCampaigns.length === 0 && <p className="field-hint">No tracked campaigns yet.</p>}
            {inviteCampaigns.length > 0 && <div className="publishing-invite-list">{inviteCampaigns.map((campaign) => <article key={campaign.id}>
              <div><strong>{campaign.title}</strong><span>{campaign.category} · {campaign.trackedJoins} tracked joins{campaign.maxUses > 0 ? ` · ${campaign.totalUses}/${campaign.maxUses} uses` : ` · ${campaign.totalUses} uses`}{!campaign.isActive ? " · inactive" : ""}</span></div>
              <button type="button" className="secondary" disabled={locked || !campaign.inviteUrl || markdownBlocks.length >= 100} onClick={() => campaign.inviteUrl && addInviteUrl(campaign.title, campaign.inviteUrl)}>Add link</button>
            </article>)}</div>}
          </section>}
          {markdownBlocks.map((block, index) => {
            const patchBlock = (patch: Partial<MarkdownBlock>) => updateMarkdown(markdownBlocks.map((current, i) => i === index ? { ...current, ...patch } : current));
            return <article className="publishing-markdown-block" key={block.id}>
              <header><strong>{block.kind === "text" ? "Text" : block.kind === "image" ? "Image" : "Invite link"}</strong><div className="button-row"><button className="text-button" type="button" disabled={locked} onClick={() => { setSavingMarkdownBlockId((current) => current === block.id ? "" : block.id); setMarkdownPresetName(block.text.slice(0, 100)); }}><BookmarkPlus size={15} /> Save to reusable</button><button className="icon-button" aria-label="Move block up" disabled={locked || index === 0} onClick={() => moveMarkdown(index, -1)}><ArrowUp size={16} /></button><button className="icon-button" aria-label="Move block down" disabled={locked || index === markdownBlocks.length - 1} onClick={() => moveMarkdown(index, 1)}><ArrowDown size={16} /></button><button className="icon-button" aria-label="Remove block" disabled={locked} onClick={() => updateMarkdown(markdownBlocks.filter((_, i) => i !== index))}><Trash2 size={16} /></button></div></header>
              {savingMarkdownBlockId === block.id && <div className="publishing-reusable-save"><label>Reusable block name<input autoFocus maxLength={100} value={markdownPresetName} disabled={locked} onChange={(event) => setMarkdownPresetName(event.target.value)} placeholder="Studio footer" /></label><button className="secondary" type="button" disabled={locked || !markdownPresetName.trim()} onClick={() => void saveReusableBlock(block)}>Save block</button><button className="text-button" type="button" disabled={locked} onClick={() => setSavingMarkdownBlockId("")}>Cancel</button></div>}
              {block.kind === "text" ? <label>Markdown text<textarea rows={5} maxLength={20000} value={block.text} disabled={locked} onChange={(event) => patchBlock({ text: event.target.value })} placeholder="Write Markdown text…" /></label> : block.kind === "invite" ? <><label>Link label<input maxLength={300} value={block.text} disabled={locked} onChange={(event) => patchBlock({ text: event.target.value })} /></label><label>Invite URL<input type="url" value={block.inviteUrl} disabled={locked} onChange={(event) => patchBlock({ inviteUrl: event.target.value })} placeholder="https://discord.gg/…" /></label></> : <><label>Alt text<input maxLength={500} value={block.altText} disabled={locked} onChange={(event) => patchBlock({ altText: event.target.value })} /></label><label>Public image URL<input type="url" value={block.imageUrl} disabled={locked} onChange={(event) => patchBlock({ imageUrl: event.target.value })} placeholder="https://…" /></label><label>Click-through URL <span className="optional">(optional)</span><input type="url" value={block.linkUrl} disabled={locked} onChange={(event) => patchBlock({ linkUrl: event.target.value })} placeholder="https://…" /></label>{block.mediaHash && <small>Saved image bytes are retained with this post snapshot.</small>}</>}
              <pre>{block.kind === "text" ? block.text : block.kind === "invite" ? `[${block.text || "Discord invite"}](${block.inviteUrl || "invite-url"})` : block.imageUrl ? `${block.linkUrl ? "[" : ""}![${block.altText}](${block.imageUrl})${block.linkUrl ? `](${block.linkUrl})` : ""}` : "Public image URL required before Markdown can be copied"}</pre>
            </article>;
          })}
          {!markdownBlocks.length && <div className="empty-state"><h3>No Markdown blocks yet</h3><p>Add text, image or invite blocks, then arrange them into a post footer or announcement.</p></div>}
          <section className="publishing-markdown-preview">
            <h3>Post preview</h3>
            {markdownBlocks.length > 0 ? <div className="publishing-markdown-rendered">{markdownBlocks.map((block) => {
              if (block.kind === "text") return <article className="markdown-preview-text" key={block.id}>{renderTextMarkdown(block.text)}</article>;
              if (block.kind === "invite") {
                const href = safeHttpsUrl(block.inviteUrl);
                return <p className="markdown-preview-invite" key={block.id}>{href ? <a href={href} target="_blank" rel="noopener noreferrer">{block.text || "Discord invite"}</a> : <span>{block.text || "Discord invite URL required"}</span>}</p>;
              }
              const src = safeHttpsUrl(block.imageUrl);
              const href = safeHttpsUrl(block.linkUrl);
              if (!src) return <p className="field-hint" key={block.id}>Public HTTPS image URL required for this preview.</p>;
              const image = <img src={src} alt={block.altText} loading="lazy" />;
              return <figure className="markdown-preview-image" key={block.id}>{href ? <a href={href} target="_blank" rel="noopener noreferrer">{image}</a> : image}</figure>;
            })}</div> : <p className="field-hint">Add Markdown blocks to preview the post.</p>}
            {markdownBlocks.length > 0 && <details className="publishing-markdown-source"><summary>Markdown source</summary><pre>{markdownText() || "Add public URLs to make this Markdown copyable."}</pre></details>}
          </section>
        </section>
      )}
      {step === "Review" && (
        <section className="publishing-review">
          <p className="field-hint">
            The assistant stops at the final review. Agreement and publication
            remain your choice.
          </p>
          <h2>{data.title || "Untitled post"}</h2>
          <p className="pre-wrap">{data.description}</p>
          <p>{data.tags.filter(Boolean).join(" · ")}</p>
          <p>
            {data.media.length} preview images · {data.liveries.length}{" "}
            {data.liveries.length === 1 ? "livery" : "liveries"}
          </p>
          <details>
            <summary>
              View livery codes (
              {data.liveries.reduce((n, l) => n + l.sides.length, 0)})
            </summary>
            {data.liveries.map((l, i) => (
              <details key={i}>
                <summary>
                  {l.vehicle || `Livery ${i + 1}`} · {l.sides.length}{" "}
                  {l.sides.length === 1 ? "texture" : "textures"}
                </summary>
                {l.sides.map((s, j) => (
                  <p key={j}>
                    {s.side || "Unassigned"} — {s.decalId || "No decal ID"}
                  </p>
                ))}
              </details>
            ))}
          </details>
          {draftId &&
            sources
              .filter((s) => s.state !== "current")
              .map((s) => (
                <p key={s.folderId}>
                  {s.vehicle}: source {s.state}. Saved codes are unchanged.
                </p>
              ))}
          {draftId && sources.some((s) => s.state !== "current") && (
            <label className="post-check">
              <input
                type="checkbox"
                checked={reviewed}
                onChange={(e) => setReviewed(e.target.checked)}
              />
              I reviewed the saved codes and preview images and want to use this
              snapshot.
            </label>
          )}
          {data.liveries
            .filter((l) => l.source && l.sides.some((s) => !s.decalId))
            .map((l) => (
              <button
                key={l.source!.folderId}
                className="secondary"
                disabled={dirty || busy}
                onClick={() => onOpenRoblox?.(l.source!.folderId)}
              >
                Open Roblox uploads: {l.vehicle}
              </button>
            ))}
          {requirements.length > 0 && (
            <ul aria-label="Publishing requirements">
              {requirements.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          )}
          {draftId && (
            <div className="browser-handoff">
              <button
                className="primary"
                disabled={
                  busy ||
                  dirty ||
                  !!requirements.length ||
                  !browser?.connected ||
                  (sources.some((s) => s.state !== "current") && !reviewed) ||
                  ["running", "paused"].includes(browser?.state ?? "")
                }
                onClick={() => {
                  setBusy(true);
                  setError("");
                  void prepareBrowser(draftId, saved.revision, reviewed)
                    .then(() => browserStatus())
                    .then(setBrowser)
                    .catch((e) => setError(errorMessage(e)))
                    .finally(() => setBusy(false));
                }}
              >
                Prepare on ERLCX
              </button>
              {!browser?.connected && (
                <p>Verify the browser connection in Settings.</p>
              )}
              {browser?.postId === draftId && (
                <>
                  <p role="status">
                    {browser.step} {browser.message}
                  </p>
                  {browser.state === "paused" && (
                    <button
                      className="secondary"
                      onClick={() =>
                        void controlBrowser("resume").catch((e) =>
                          setError(errorMessage(e)),
                        )
                      }
                    >
                      Resume assistant
                    </button>
                  )}
                  {["running", "paused"].includes(browser.state) && (
                    <button
                      className="secondary"
                      onClick={() =>
                        void controlBrowser("cancel").catch((e) =>
                          setError(errorMessage(e)),
                        )
                      }
                    >
                      Cancel assistant
                    </button>
                  )}
                </>
              )}
            </div>
          )}
          <button
            className={draftId ? "secondary" : "primary"}
            disabled={
              busy ||
              dirty ||
              requirements.length > 0 ||
              (!!draftId &&
                sources.some((s) => s.state !== "current") &&
                !reviewed)
            }
            onClick={() => void exportBundle()}
          >
            Export for ERLCX extension
          </button>
          {dirty && (
            <p className="field-hint">Save changes before exporting.</p>
          )}
        </section>
      )}
      <footer className="publishing-footer">
        <button
          className="primary"
          disabled={locked || !dirty}
          onClick={() => void save()}
        >
          Save draft
        </button>
        {dirty && (
          <button
            className="text-button"
            disabled={busy}
            onClick={() => {
              setData(saved.data);
              setDirty(false);
              setError("");
              if (draftId)
                void releasePostImages(draftId).catch((e) =>
                  setError(errorMessage(e)),
                );
            }}
          >
            Discard changes
          </button>
        )}
      </footer>
    </section>
  );
}
