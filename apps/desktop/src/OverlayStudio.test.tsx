import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import OverlayStudio from "./OverlayStudio";
import {
  listOverlayTemplates,
  overlayImageData,
  saveOverlayTemplate,
  type OverlayPreviewOutput,
  type OverlayTemplate,
} from "./bridge";

vi.mock("./bridge", () => ({
  deleteOverlayTemplate: vi.fn(),
  importOverlayPreviews: vi.fn(),
  listOverlayTemplates: vi.fn(),
  overlayImageData: vi.fn(),
  saveOverlayTemplate: vi.fn(),
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "Failed",
}));

const existing: OverlayTemplate = {
  id: "existing-template",
  name: "Agency Banner",
  width: 1920,
  height: 1080,
  layersJson: "[]",
  createdAt: "2026-11-01T00:00:00Z",
  updatedAt: "2026-11-01T00:00:00Z",
};
const imported: OverlayTemplate = {
  ...existing,
  id: "imported-template",
  createdAt: "2026-11-10T00:00:00Z",
  updatedAt: "2026-11-10T00:00:00Z",
};

beforeEach(() => {
  vi.clearAllMocks();
  let listCalls = 0;
  vi.mocked(listOverlayTemplates).mockImplementation(async () => {
    listCalls += 1;
    return listCalls === 1 ? [existing] : [existing, imported];
  });
  vi.mocked(saveOverlayTemplate).mockResolvedValue(imported);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    clearRect: vi.fn(),
    fillRect: vi.fn(),
    drawImage: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("imports a same-named overlay as a new template without replacing the existing one", async () => {
  const user = userEvent.setup();
  render(
    <OverlayStudio
      postId="test-post"
      projectId={undefined}
      currentCount={2}
      disabled={false}
      projectImages={[]}
      libraryItems={[]}
      onExported={vi.fn()}
    />,
  );

  await screen.findByRole("option", { name: "Agency Banner · 16:9" });
  const template = {
    format: "apex-studio-overlay",
    version: 1,
    name: "Agency Banner",
    width: 1920,
    height: 1080,
    layers: [],
  };
  const file = new File([JSON.stringify(template)], "agency.apexoverlay.json", {
    type: "application/json",
  });
  Object.defineProperty(file, "text", { value: async () => JSON.stringify(template) });
  const input = document.querySelector<HTMLInputElement>(
    'input[type="file"][accept*=".apexoverlay"]',
  );
  expect(input).not.toBeNull();
  expect(input!.style.display).toBe("none");
  await user.upload(input!, file);

  await waitFor(() => expect(saveOverlayTemplate).toHaveBeenCalledTimes(1));
  const savedTemplate = vi.mocked(saveOverlayTemplate).mock.calls[0][0];
  expect(savedTemplate).not.toHaveProperty("id");
  expect(savedTemplate).toMatchObject({ name: "Agency Banner", width: 1920, height: 1080 });
  await waitFor(() => {
    expect(screen.getAllByRole("option", { name: "Agency Banner · 16:9" })).toHaveLength(2);
  });
});

it("exports all selected source images at the chosen ExecLab dimensions", async () => {
  const user = userEvent.setup();
  const onExported = vi.fn(async (_images: OverlayPreviewOutput[]) => undefined);
  vi.mocked(overlayImageData)
    .mockResolvedValueOnce("data:image/png;base64,Zmlyc3Q=")
    .mockResolvedValueOnce("data:image/png;base64,c2Vjb25k");
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,b3V0cHV0");
  vi.stubGlobal("Image", class {
    complete = true;
    naturalWidth = 1920;
    naturalHeight = 1080;
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(_value: string) { queueMicrotask(() => this.onload?.()); }
  });
  render(
    <OverlayStudio
      postId="test-post"
      projectId={undefined}
      currentCount={2}
      disabled={false}
      projectImages={[
        { id: "asset-first", projectId: "project", name: "First.png", hash: "a".repeat(64), size: 10, importedAt: "now", preview: null },
        { id: "asset-second", projectId: "project", name: "Second.png", hash: "b".repeat(64), size: 10, importedAt: "now", preview: null },
      ]}
      libraryItems={[]}
      onExported={onExported}
    />,
  );

  await user.selectOptions(screen.getByLabelText("Format"), "1200");
  const projectImage = screen.getByLabelText("Choose a project image for overlay");
  await user.selectOptions(projectImage, "asset-first");
  await user.click(screen.getByRole("button", { name: "Add project image" }));
  await user.selectOptions(projectImage, "asset-second");
  await user.click(screen.getByRole("button", { name: "Add project image" }));
  await user.click(screen.getByRole("button", { name: "Apply to previews" }));

  await waitFor(() => expect(onExported).toHaveBeenCalledTimes(1));
  const exported = onExported.mock.calls[0][0];
  expect(exported).toHaveLength(2);
  expect(exported.map((image) => image.name)).toEqual(["First-execlab.png", "Second-execlab.png"]);
  expect(exported.map((image) => [image.width, image.height])).toEqual([[1920, 1200], [1920, 1200]]);
  expect(exported.map((image) => image.sourceHash)).toEqual(["a".repeat(64), "b".repeat(64)]);
  expect(exported.every((image) => JSON.parse(image.compositionJson).target === "execlab")).toBe(true);
});

it("exports selected source images at the ERLCX 16:9 dimensions", async () => {
  const user = userEvent.setup();
  const onExported = vi.fn(async (_images: OverlayPreviewOutput[]) => undefined);
  vi.mocked(overlayImageData).mockResolvedValue("data:image/png;base64,aW1hZ2U=");
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,b3V0cHV0");
  vi.stubGlobal("Image", class {
    complete = true;
    naturalWidth = 1920;
    naturalHeight = 1080;
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(_value: string) { queueMicrotask(() => this.onload?.()); }
  });
  render(
    <OverlayStudio
      postId="test-post"
      projectId="project"
      currentCount={1}
      disabled={false}
      projectImages={[
        { id: "asset-erlcx", projectId: "project", name: "Patrol.png", hash: "c".repeat(64), size: 10, importedAt: "now", preview: null },
      ]}
      libraryItems={[]}
      onExported={onExported}
    />,
  );

  const projectImage = screen.getByLabelText("Choose a project image for overlay");
  await user.selectOptions(projectImage, "asset-erlcx");
  await user.click(screen.getByRole("button", { name: "Add project image" }));
  await user.click(screen.getByRole("button", { name: "Apply to previews" }));

  await waitFor(() => expect(onExported).toHaveBeenCalledTimes(1));
  const exported = onExported.mock.calls[0][0];
  expect(exported).toHaveLength(1);
  expect(exported[0]).toMatchObject({
    name: "Patrol-erlcx.png",
    sourceHash: "c".repeat(64),
    width: 1920,
    height: 1080,
  });
  expect(JSON.parse(exported[0].compositionJson).target).toBe("erlcx");
});

it("exports multiple locally selected images into ERLCX previews", async () => {
  const user = userEvent.setup();
  const onExported = vi.fn(async (_images: OverlayPreviewOutput[]) => undefined);
  const createObjectUrl = vi.fn()
    .mockReturnValueOnce("blob:local-preview-1")
    .mockReturnValueOnce("blob:local-preview-2");
  Object.defineProperty(URL, "createObjectURL", { configurable: true, value: createObjectUrl });
  const revokeObjectUrl = vi.fn();
  Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revokeObjectUrl });
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,b3V0cHV0");
  vi.stubGlobal("Image", class {
    complete = true;
    naturalWidth = 1920;
    naturalHeight = 1080;
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(_value: string) { queueMicrotask(() => this.onload?.()); }
  });
  render(
    <OverlayStudio
      postId="test-post"
      projectId={undefined}
      currentCount={2}
      disabled={false}
      projectImages={[]}
      libraryItems={[]}
      onExported={onExported}
    />,
  );

  const fileInput = document.querySelector<HTMLInputElement>('input[type="file"][accept="image/*"][multiple]');
  expect(fileInput).not.toBeNull();
  const firstFile = new File(["first image"], "Apex-Banner.png", { type: "image/png" });
  const secondFile = new File(["second image"], "Support-Banner.png", { type: "image/png" });
  for (const [file, contents] of [[firstFile, "first image"], [secondFile, "second image"]] as const) {
    Object.defineProperty(file, "arrayBuffer", {
      value: async () => new TextEncoder().encode(contents).buffer,
    });
  }
  await user.upload(fileInput!, [firstFile, secondFile]);
  await screen.findByText("2 source images ready for export.");
  await user.click(screen.getByRole("button", { name: "Apply to previews" }));

  await waitFor(() => expect(onExported).toHaveBeenCalledTimes(1));
  const exported = onExported.mock.calls[0][0];
  expect(createObjectUrl).toHaveBeenCalledTimes(2);
  expect(exported).toHaveLength(2);
  expect(exported.map((image) => image.name)).toEqual([
    "Apex-Banner-erlcx.png",
    "Support-Banner-erlcx.png",
  ]);
  expect(exported.map((image) => [image.width, image.height])).toEqual([[1920, 1080], [1920, 1080]]);
  expect(exported.every((image) => /^[a-f0-9]{64}$/.test(image.sourceHash))).toBe(true);
  expect(exported[0].sourceHash).not.toBe(exported[1].sourceHash);
  expect(exported.every((image) => JSON.parse(image.compositionJson).target === "erlcx")).toBe(true);
  expect(revokeObjectUrl).toHaveBeenCalledTimes(2);
});
