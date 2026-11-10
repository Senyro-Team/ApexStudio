import { useEffect, useMemo, useRef, useState, type PointerEvent } from "react";
import { ArrowDownToLine, ImagePlus, Layers3, Save, Shapes, Trash2, Type, Upload } from "lucide-react";
import {
  deleteOverlayTemplate,
  importOverlayPreviews,
  listOverlayTemplates,
  overlayImageData,
  saveOverlayTemplate,
  type Asset,
  type MediaLibraryItem,
  type OverlayPreviewOutput,
  type OverlayTemplate,
} from "./bridge";
import { errorMessage } from "./bridge";

type LayerKind = "text" | "image" | "shape";
type OverlayLayer = {
  id: string;
  kind: LayerKind;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  opacity: number;
  rotation: number;
  text?: string;
  fontSize?: number;
  fontFamily?: string;
  align?: CanvasTextAlign;
  color?: string;
  src?: string;
  shape?: "rectangle" | "ellipse" | "line";
  fill?: string;
  stroke?: string;
  strokeWidth?: number;
};
type SourceImage = { name: string; src: string; hash?: string };

const MAX_LAYERS = 50;
const MAX_TEMPLATE_IMAGE_BYTES = 2 * 1024 * 1024;
const id = () => crypto.randomUUID();

function readFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("Could not read this image."));
    reader.onerror = () => reject(new Error("Could not read this image."));
    reader.readAsDataURL(file);
  });
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("An image layer could not be loaded."));
    image.src = src;
  });
}

function drawLayer(ctx: CanvasRenderingContext2D, layer: OverlayLayer) {
  ctx.save();
  ctx.globalAlpha = Math.max(0, Math.min(1, layer.opacity / 100));
  const cx = layer.x + layer.width / 2;
  const cy = layer.y + layer.height / 2;
  ctx.translate(cx, cy);
  ctx.rotate((layer.rotation * Math.PI) / 180);
  const x = -layer.width / 2;
  const y = -layer.height / 2;
  if (layer.kind === "shape") {
    ctx.fillStyle = layer.fill || "#5758e8";
    ctx.strokeStyle = layer.stroke || "#ffffff";
    ctx.lineWidth = layer.strokeWidth || 0;
    if (layer.shape === "line") {
      ctx.beginPath();
      ctx.moveTo(x, y + layer.height);
      ctx.lineTo(x + layer.width, y);
      ctx.stroke();
    } else if (layer.shape === "ellipse") {
      ctx.beginPath();
      ctx.ellipse(0, 0, Math.abs(layer.width) / 2, Math.abs(layer.height) / 2, 0, 0, Math.PI * 2);
      if (layer.fill) ctx.fill();
      if (layer.strokeWidth) ctx.stroke();
    } else {
      if (layer.fill) ctx.fillRect(x, y, layer.width, layer.height);
      if (layer.strokeWidth) ctx.strokeRect(x, y, layer.width, layer.height);
    }
  } else if (layer.kind === "text") {
    const size = Math.max(8, layer.fontSize || 72);
    ctx.fillStyle = layer.color || "#ffffff";
    ctx.font = `700 ${size}px ${layer.fontFamily || "Arial"}`;
    ctx.textAlign = layer.align || "left";
    ctx.textBaseline = "middle";
    const textX = layer.align === "center" ? 0 : layer.align === "right" ? layer.width / 2 : x;
    const lines = (layer.text || "Text").split("\n");
    const lineHeight = size * 1.16;
    const startY = -((lines.length - 1) * lineHeight) / 2;
    lines.forEach((line, index) => ctx.fillText(line, textX, startY + index * lineHeight, layer.width));
  } else if (layer.src) {
    const image = imageCache.get(layer.src);
    if (image?.complete) {
      const scale = Math.min(layer.width / image.naturalWidth, layer.height / image.naturalHeight);
      const width = image.naturalWidth * scale;
      const height = image.naturalHeight * scale;
      ctx.drawImage(image, -width / 2, -height / 2, width, height);
    }
  }
  ctx.restore();
}

const imageCache = new Map<string, HTMLImageElement>();

function numericValue(raw: string, minimum: number, maximum: number): number | null {
  if (!raw.trim()) return null;
  const value = Number(raw);
  return Number.isFinite(value) ? Math.max(minimum, Math.min(maximum, value)) : null;
}

async function paintCanvas(canvas: HTMLCanvasElement, width: number, height: number, background: string | null, layers: OverlayLayer[], selectedLayerId = "") {
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Canvas rendering is unavailable.");
  canvas.width = width;
  canvas.height = height;
  ctx.clearRect(0, 0, width, height);
  ctx.fillStyle = "#17181d";
  ctx.fillRect(0, 0, width, height);
  if (background) {
    const image = await loadImage(background);
    const scale = Math.max(width / image.naturalWidth, height / image.naturalHeight);
    const drawWidth = image.naturalWidth * scale;
    const drawHeight = image.naturalHeight * scale;
    ctx.drawImage(image, (width - drawWidth) / 2, (height - drawHeight) / 2, drawWidth, drawHeight);
  }
  const pending = layers.filter((layer) => layer.kind === "image" && layer.src && !imageCache.get(layer.src)?.complete);
  await Promise.all(pending.map(async (layer) => {
    const image = await loadImage(layer.src!);
    imageCache.set(layer.src!, image);
  }));
  layers.forEach((layer) => drawLayer(ctx, layer));
  const selected = layers.find((layer) => layer.id === selectedLayerId);
  if (selected) {
    ctx.save();
    ctx.translate(selected.x + selected.width / 2, selected.y + selected.height / 2);
    ctx.rotate((selected.rotation * Math.PI) / 180);
    ctx.setLineDash([12, 8]);
    ctx.lineWidth = 4;
    ctx.strokeStyle = "#817bff";
    ctx.strokeRect(-selected.width / 2, -selected.height / 2, selected.width, selected.height);
    ctx.restore();
  }
}

export default function OverlayStudio({
  postId,
  projectId,
  currentCount,
  disabled,
  projectImages,
  libraryItems,
  onExported,
}: {
  postId?: string;
  projectId?: string;
  currentCount: number;
  disabled: boolean;
  projectImages: Asset[];
  libraryItems: MediaLibraryItem[];
  onExported: (images: OverlayPreviewOutput[]) => Promise<void>;
}) {
  const [height, setHeight] = useState(1080);
  const [layers, setLayers] = useState<OverlayLayer[]>([]);
  const [selected, setSelected] = useState("");
  const [templates, setTemplates] = useState<OverlayTemplate[]>([]);
  const [templateId, setTemplateId] = useState("");
  const [templateName, setTemplateName] = useState("Untitled overlay");
  const [sources, setSources] = useState<SourceImage[]>([]);
  const [projectChoice, setProjectChoice] = useState("");
  const [libraryChoice, setLibraryChoice] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [redraw, setRedraw] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<{ id: string; pointerId: number; x: number; y: number; layerX: number; layerY: number } | null>(null);
  const imageLayerInput = useRef<HTMLInputElement>(null);
  const sourceObjectUrls = useRef(new Set<string>());
  const maxOutputCount = Math.max(0, 10 - currentCount);
  const remainingToAdd = Math.max(0, maxOutputCount - sources.length);
  const currentLayer = useMemo(() => layers.find((layer) => layer.id === selected) ?? null, [layers, selected]);

  useEffect(() => {
    let active = true;
    void listOverlayTemplates().then((rows) => { if (active) setTemplates(rows); }).catch(() => {});
    return () => { active = false; };
  }, []);

  useEffect(() => () => {
    sourceObjectUrls.current.forEach((url) => URL.revokeObjectURL(url));
    sourceObjectUrls.current.clear();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    void paintCanvas(canvas, 1920, height, sources[0]?.src ?? null, layers, selected)
      .then(() => setRedraw((value) => value + 1))
      .catch((reason: unknown) => setError(errorMessage(reason)));
  }, [height, layers, selected, sources]);

  function pointerPosition(event: PointerEvent<HTMLCanvasElement>) {
    const rect = event.currentTarget.getBoundingClientRect();
    return { x: (event.clientX - rect.left) * (1920 / rect.width), y: (event.clientY - rect.top) * (height / rect.height) };
  }

  function beginLayerDrag(event: PointerEvent<HTMLCanvasElement>) {
    if (disabled || busy || !layers.length) return;
    const point = pointerPosition(event);
    const hit = [...layers].reverse().find((layer) => {
      const dx = point.x - (layer.x + layer.width / 2);
      const dy = point.y - (layer.y + layer.height / 2);
      const angle = (-layer.rotation * Math.PI) / 180;
      const localX = dx * Math.cos(angle) - dy * Math.sin(angle);
      const localY = dx * Math.sin(angle) + dy * Math.cos(angle);
      return Math.abs(localX) <= layer.width / 2 && Math.abs(localY) <= layer.height / 2;
    });
    if (!hit) { setSelected(""); return; }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    setSelected(hit.id);
    dragRef.current = { id: hit.id, pointerId: event.pointerId, x: point.x, y: point.y, layerX: hit.x, layerY: hit.y };
  }

  function moveLayer(event: PointerEvent<HTMLCanvasElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const point = pointerPosition(event);
    const nextX = Math.max(-3840, Math.min(3840, drag.layerX + point.x - drag.x));
    const nextY = Math.max(-2400, Math.min(2400, drag.layerY + point.y - drag.y));
    setLayers((current) => current.map((layer) => layer.id === drag.id ? { ...layer, x: Math.round(nextX), y: Math.round(nextY) } : layer));
  }

  function endLayerDrag(event: PointerEvent<HTMLCanvasElement>) {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }

  function updateLayer(patch: Partial<OverlayLayer>) {
    setLayers((current) => current.map((layer) => layer.id === selected ? { ...layer, ...patch } : layer));
  }

  function addLayer(kind: LayerKind) {
    if (layers.length >= MAX_LAYERS) return;
    const next: OverlayLayer = {
      id: id(), kind, name: kind === "text" ? "Text" : kind === "image" ? "Image" : "Shape",
      x: 100, y: 80 + layers.length * 20, width: kind === "text" ? 900 : 360, height: kind === "text" ? 180 : 240,
      opacity: 100, rotation: 0,
      ...(kind === "text" ? { text: "Your title", fontSize: 96, fontFamily: "Arial", align: "left" as CanvasTextAlign, color: "#ffffff" } : {}),
      ...(kind === "shape" ? { shape: "rectangle" as const, fill: "#5758e8", stroke: "#ffffff", strokeWidth: 0 } : {}),
    };
    setLayers((current) => [...current, next]);
    setSelected(next.id);
    if (kind === "image") imageLayerInput.current?.click();
  }

  async function addLayerImage(file: File | undefined) {
    if (!file) return;
    if (!file.type.startsWith("image/")) { setError("Choose an image file."); return; }
    if (file.size > MAX_TEMPLATE_IMAGE_BYTES) { setError("Overlay images must be 2 MB or smaller so templates stay portable in workspace backups."); return; }
    try {
      const src = await readFile(file);
      const layer = layers.find((item) => item.id === selected);
      if (!layer) return;
      const image = await loadImage(src);
      imageCache.set(src, image);
      updateLayer({ name: file.name.replace(/\.[^.]+$/, "") || "Image", src, width: Math.min(500, image.naturalWidth), height: Math.min(300, image.naturalHeight) });
      setError("");
    } catch (reason) { setError(errorMessage(reason)); }
  }

  async function chooseSources(files: FileList | null) {
    if (!files) return;
    const selectedFiles = Array.from(files).slice(0, remainingToAdd);
    if (selectedFiles.some((file) => file.size > 40 * 1024 * 1024)) { setError("Source images must be 40 MB or smaller."); return; }
    if (Array.from(files).length > remainingToAdd) setNotice(`Only ${remainingToAdd} more preview${remainingToAdd === 1 ? "" : "s"} fit in this post.`);
    setBusy(true);
    setError("");
    try {
      const next = selectedFiles.map((file) => {
        const src = URL.createObjectURL(file);
        sourceObjectUrls.current.add(src);
        return { name: file.name.replace(/\.[^.]+$/, "") || "Preview", src };
      });
      setSources((current) => [...current, ...next]);
      if (next.length) setNotice(`${next.length} source image${next.length === 1 ? "" : "s"} ready for export.`);
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function addManagedSource(item: { id: string; name: string; hash: string }) {
    if (remainingToAdd < 1) { setError("This post has no available preview slots."); return; }
    if (sources.some((source) => source.hash === item.hash)) { setError("That image is already in this overlay batch."); return; }
    setBusy(true); setError(""); setNotice("");
    try {
      const src = await overlayImageData(item.hash, 1920, height);
      setSources((current) => [...current, { name: item.name.replace(/\.[^.]+$/, "") || "Preview", src, hash: item.hash }]);
      setNotice("Managed image added to the overlay batch.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function saveTemplate() {
    if (!templateName.trim()) { setError("Enter a name for this overlay template."); return; }
    setBusy(true); setError(""); setNotice("");
    try {
      const saved = await saveOverlayTemplate({ id: templateId || undefined, name: templateName.trim(), width: 1920, height, layersJson: JSON.stringify(layers) });
      const next = await listOverlayTemplates();
      setTemplates(next); setTemplateId(saved.id); setNotice("Overlay template saved in this workspace.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  function loadTemplate(idToLoad: string) {
    setTemplateId(idToLoad);
    const template = templates.find((item) => item.id === idToLoad);
    if (!template) { setLayers([]); setTemplateName("Untitled overlay"); return; }
    try {
      const restored = JSON.parse(template.layersJson) as OverlayLayer[];
      setLayers(restored); setSelected(restored[0]?.id ?? ""); setHeight(template.height); setTemplateName(template.name); setError("");
    } catch { setError("This overlay template could not be opened."); }
  }

  async function removeTemplate() {
    if (!templateId) return;
    const template = templates.find((item) => item.id === templateId);
    if (!template || !window.confirm(`Delete the “${template.name}” overlay template?`)) return;
    setBusy(true); setError("");
    try {
      await deleteOverlayTemplate(templateId);
      setTemplates(await listOverlayTemplates()); setTemplateId(""); setTemplateName("Untitled overlay");
      setNotice("Overlay template deleted.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function exportBatch() {
    if (!sources.length) { setError("Choose at least one source image for the batch."); return; }
    if (sources.length > maxOutputCount) { setError(`Remove images so the post stays within the 10-preview limit.`); return; }
    setBusy(true); setError(""); setNotice("");
    try {
      const output: OverlayPreviewOutput[] = [];
      for (let index = 0; index < sources.length; index += 1) {
        const source = sources[index];
        const canvas = document.createElement("canvas");
        await paintCanvas(canvas, 1920, height, source.src, layers);
        output.push({ name: `${source.name}-${height === 1080 ? "erlcx" : "execlab"}.png`, dataUrl: canvas.toDataURL("image/png") });
      }
      await onExported(output);
      sources.forEach((source) => {
        if (source.src.startsWith("blob:")) {
          URL.revokeObjectURL(source.src);
          sourceObjectUrls.current.delete(source.src);
        }
      });
      setSources([]);
      setNotice(`${output.length} overlay preview${output.length === 1 ? "" : "s"} added to this post.`);
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  return (
    <section className="overlay-studio" aria-label="Overlay Studio">
      <header className="overlay-studio-header">
        <div><h3>Overlay Studio</h3><p>Create reusable overlays and drag layers on the preview to position them.</p></div>
        <label className="overlay-format-select">Format<select value={height} disabled={disabled || busy} onChange={(event) => setHeight(Number(event.target.value))}><option value={1080}>ERLCX · 16:9</option><option value={1200}>ExecLab · 16:10</option></select></label>
      </header>
      <div className="overlay-template-tools">
        <label>Saved template<select value={templateId} disabled={disabled || busy} onChange={(event) => loadTemplate(event.target.value)}><option value="">New overlay</option>{templates.map((template) => <option key={template.id} value={template.id}>{template.name} · {template.height === 1080 ? "16:9" : "16:10"}</option>)}</select></label>
        <label>Template name<input value={templateName} maxLength={120} disabled={disabled || busy} onChange={(event) => setTemplateName(event.target.value)} /></label>
        <button className="secondary" type="button" disabled={disabled || busy || !templateName.trim()} onClick={() => void saveTemplate()}><Save size={15} /> Save template</button>
        {templateId && <button className="text-button" type="button" disabled={disabled || busy} onClick={() => void removeTemplate()}><Trash2 size={15} /> Delete template</button>}
      </div>
      <div className="overlay-studio-layout">
        <div className="overlay-canvas-column">
          <div className="overlay-canvas-frame">
                <canvas ref={canvasRef} className={`overlay-canvas${dragRef.current ? " is-dragging" : ""}`} aria-label="Overlay composition preview. Drag a visible layer to reposition it." onPointerDown={beginLayerDrag} onPointerMove={moveLayer} onPointerUp={endLayerDrag} onPointerCancel={endLayerDrag} />
            {redraw === 0 && !sources.length && <span className="overlay-canvas-empty">Choose source images to preview the overlay</span>}
          </div>
          <div className="overlay-source-tools">
            <label className="secondary overlay-file-button"><Upload size={15} /> Choose local images<input type="file" accept="image/*" multiple disabled={disabled || busy || remainingToAdd === 0} onChange={(event) => { void chooseSources(event.target.files); event.currentTarget.value = ""; }} /></label>
            {projectImages.length > 0 && <div><select aria-label="Choose a project image for overlay" value={projectChoice} disabled={disabled || busy || remainingToAdd === 0} onChange={(event) => setProjectChoice(event.target.value)}><option value="">Choose project image</option>{projectImages.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><button className="secondary" type="button" disabled={disabled || busy || !projectChoice || remainingToAdd === 0} onClick={() => { const asset = projectImages.find((item) => item.id === projectChoice); if (asset) void addManagedSource(asset); setProjectChoice(""); }}>Add project image</button></div>}
            {libraryItems.some((item) => item.hash) && <div><select aria-label="Choose an Image Library image for overlay" value={libraryChoice} disabled={disabled || busy || remainingToAdd === 0} onChange={(event) => setLibraryChoice(event.target.value)}><option value="">Choose library image</option>{libraryItems.filter((item) => item.hash).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><button className="secondary" type="button" disabled={disabled || busy || !libraryChoice || remainingToAdd === 0} onClick={() => { const item = libraryItems.find((candidate) => candidate.id === libraryChoice); if (item?.hash) void addManagedSource({ id: item.id, name: item.name, hash: item.hash }); setLibraryChoice(""); }}>Add library image</button></div>}
          </div>
          <div className="overlay-canvas-actions">
            <span>{sources.length} selected · {remainingToAdd} preview slots available</span>
            <button className="primary" type="button" disabled={disabled || busy || !sources.length || remainingToAdd < 0} onClick={() => void exportBatch()}><ArrowDownToLine size={16} /> {busy ? "Preparing…" : "Apply to previews"}</button>
          </div>
          {sources.length > 0 && <div className="overlay-source-list">{sources.map((source, index) => <div key={`${source.name}-${index}`}><img src={source.src} alt="" /><span>{source.name}</span><button className="text-button" type="button" disabled={busy} aria-label={`Remove ${source.name}`} onClick={() => { if (source.src.startsWith("blob:")) { URL.revokeObjectURL(source.src); sourceObjectUrls.current.delete(source.src); } setSources((items) => items.filter((_, i) => i !== index)); }}>Remove</button></div>)}</div>}
        </div>
        <aside className="overlay-layer-panel" aria-label="Overlay layers">
          <div className="overlay-layer-heading"><strong><Layers3 size={16} /> Layers</strong><span>{layers.length}/{MAX_LAYERS}</span></div>
          <div className="overlay-layer-add">
            <button className="secondary" type="button" disabled={disabled || busy || layers.length >= MAX_LAYERS} onClick={() => addLayer("text")}><Type size={15} /> Text</button>
            <button className="secondary" type="button" disabled={disabled || busy || layers.length >= MAX_LAYERS} onClick={() => addLayer("shape")}><Shapes size={15} /> Shape</button>
            <button className="secondary" type="button" disabled={disabled || busy || layers.length >= MAX_LAYERS} onClick={() => addLayer("image")}><ImagePlus size={15} /> Image</button>
          </div>
          <input ref={imageLayerInput} type="file" accept="image/*" hidden onChange={(event) => { void addLayerImage(event.target.files?.[0]); event.currentTarget.value = ""; }} />
          <div className="overlay-layer-list">{layers.map((layer, index) => <button className={layer.id === selected ? "overlay-layer-item is-selected" : "overlay-layer-item"} type="button" key={layer.id} onClick={() => setSelected(layer.id)}><span>{layer.kind === "text" ? <Type size={14} /> : layer.kind === "image" ? <ImagePlus size={14} /> : <Shapes size={14} />}{layer.name || layer.kind}</span><small>{String(index + 1).padStart(2, "0")}</small></button>)}</div>
          {currentLayer && <div className="overlay-layer-properties">
            <label>Layer name<input maxLength={120} value={currentLayer.name} disabled={disabled || busy} onChange={(event) => updateLayer({ name: event.target.value })} /></label>
            {currentLayer.kind === "text" && <>
              <label>Text<textarea rows={3} maxLength={5000} value={currentLayer.text ?? ""} disabled={disabled || busy} onChange={(event) => updateLayer({ text: event.target.value })} /></label>
              <div className="overlay-property-pair"><label>Font size<input type="number" min={8} max={800} value={currentLayer.fontSize ?? 72} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, 8, 800); if (value !== null) updateLayer({ fontSize: value }); }} /></label><label>Alignment<select value={currentLayer.align ?? "left"} disabled={disabled || busy} onChange={(event) => updateLayer({ align: event.target.value as CanvasTextAlign })}><option value="left">Left</option><option value="center">Center</option><option value="right">Right</option></select></label></div>
              <label>Font<input value={currentLayer.fontFamily ?? "Arial"} disabled={disabled || busy} onChange={(event) => updateLayer({ fontFamily: event.target.value })} /></label>
              <label>Text color<input type="color" value={currentLayer.color ?? "#ffffff"} disabled={disabled || busy} onChange={(event) => updateLayer({ color: event.target.value })} /></label>
            </>}
            {currentLayer.kind === "shape" && <>
              <label>Shape<select value={currentLayer.shape ?? "rectangle"} disabled={disabled || busy} onChange={(event) => updateLayer({ shape: event.target.value as OverlayLayer["shape"] })}><option value="rectangle">Rectangle</option><option value="ellipse">Ellipse</option><option value="line">Line</option></select></label>
              <div className="overlay-property-pair"><label>Fill<input type="color" value={currentLayer.fill ?? "#5758e8"} disabled={disabled || busy} onChange={(event) => updateLayer({ fill: event.target.value })} /></label><label>Stroke<input type="color" value={currentLayer.stroke ?? "#ffffff"} disabled={disabled || busy} onChange={(event) => updateLayer({ stroke: event.target.value })} /></label></div>
              <label>Stroke width<input type="number" min={0} max={80} value={currentLayer.strokeWidth ?? 0} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, 0, 80); if (value !== null) updateLayer({ strokeWidth: value }); }} /></label>
            </>}
            <div className="overlay-property-pair"><label>X<input type="number" min={-3840} max={3840} value={currentLayer.x} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, -3840, 3840); if (value !== null) updateLayer({ x: value }); }} /></label><label>Y<input type="number" min={-2400} max={2400} value={currentLayer.y} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, -2400, 2400); if (value !== null) updateLayer({ y: value }); }} /></label></div>
            <div className="overlay-property-pair"><label>Width<input type="number" min={1} max={3840} value={currentLayer.width} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, 1, 3840); if (value !== null) updateLayer({ width: value }); }} /></label><label>Height<input type="number" min={1} max={2400} value={currentLayer.height} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, 1, 2400); if (value !== null) updateLayer({ height: value }); }} /></label></div>
            <label>Rotation<input type="number" min={-180} max={180} value={currentLayer.rotation} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, -180, 180); if (value !== null) updateLayer({ rotation: value }); }} /></label>
            <label>Opacity<input type="range" min={0} max={100} value={currentLayer.opacity} disabled={disabled || busy} onChange={(event) => { const value = numericValue(event.target.value, 0, 100); if (value !== null) updateLayer({ opacity: value }); }} /><small>{currentLayer.opacity}%</small></label>
            <button className="text-button" type="button" disabled={disabled || busy} onClick={() => { setLayers((items) => items.filter((item) => item.id !== selected)); setSelected(""); }}><Trash2 size={14} /> Remove layer</button>
          </div>}
          {!currentLayer && <p className="overlay-layer-empty">Select a layer to edit its position and appearance.</p>}
        </aside>
      </div>
      {error && <p className="error" role="alert">{error}</p>}
      {notice && <p className="field-hint" role="status">{notice}</p>}
    </section>
  );
}
