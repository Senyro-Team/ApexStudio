import { zipStored } from "./zip-stored.mjs";
import { mkdir, copyFile, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
const root = resolve(import.meta.dirname, ".."),
  source = resolve(root, "apps/erlcx-extension"),
  destination = resolve(root, "dist/erlcx-extension");
await mkdir(destination, { recursive: true });
const files = [
  "manifest.json",
  "popup.html",
  "popup.css",
  "popup.js",
  "adapter.js",
  "bundle.js",
  "icon32.png",
  "icon128.png",
  "README.md",
  "BRANDING.md",
  "Archivo.ttf",
  "OFL.txt",
];
for (const file of files)
  await copyFile(resolve(source, file), resolve(destination, file));
console.log(`Unpacked extension: ${destination}`);

const archive = resolve(root, "dist/apex-erlcx-extension-0.1.0.zip");
await writeFile(
  archive,
  zipStored(
    await Promise.all(
      files.map(async (name) => [name, await readFile(resolve(source, name))]),
    ),
  ),
);
console.log(`Extension ZIP: ${archive}`);
