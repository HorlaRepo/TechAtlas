import { existsSync, readFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dashboardDirectory = fileURLToPath(new URL("../apps/dashboard/dist/", import.meta.url));
const manifestPath = join(dashboardDirectory, ".vite/manifest.json");
const limits = {
  initialJavascript: 250_000,
  css: 50_000,
  lazyJavascriptChunk: 400_000,
};

if (!existsSync(manifestPath)) {
  throw new Error("dashboard manifest is missing; run the production dashboard build first");
}

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const entry = Object.values(manifest).find((item) => item.isEntry);
if (!entry) {
  throw new Error("dashboard manifest does not contain an entry asset");
}

function gzipBytes(file) {
  return gzipSync(readFileSync(join(dashboardDirectory, file))).length;
}

function staticFiles(item, seen = new Set()) {
  if (seen.has(item.file)) return seen;
  seen.add(item.file);
  for (const imported of item.imports ?? []) {
    const dependency = manifest[imported];
    if (dependency) staticFiles(dependency, seen);
  }
  return seen;
}

const initialFiles = staticFiles(entry);
const initialJavascript = [...initialFiles].reduce((total, file) => total + gzipBytes(file), 0);
const cssFiles = new Set(Object.values(manifest).flatMap((item) => item.css ?? []));
const css = [...cssFiles].reduce((total, file) => total + gzipBytes(file), 0);
const lazyChunks = Object.values(manifest)
  .filter((item) => item.file.endsWith(".js") && !initialFiles.has(item.file))
  .map((item) => ({ file: item.file, bytes: gzipBytes(item.file) }));

if (initialJavascript > limits.initialJavascript) {
  throw new Error(`initial JavaScript gzip budget exceeded: ${initialJavascript} bytes (limit ${limits.initialJavascript} bytes)`);
}
if (css > limits.css) {
  throw new Error(`CSS gzip budget exceeded: ${css} bytes (limit ${limits.css} bytes)`);
}
for (const chunk of lazyChunks) {
  if (chunk.bytes > limits.lazyJavascriptChunk) {
    throw new Error(`lazy JavaScript chunk gzip budget exceeded: ${chunk.file} is ${chunk.bytes} bytes (limit ${limits.lazyJavascriptChunk} bytes)`);
  }
}

console.log(JSON.stringify({ dashboard_gzip_bytes: { initial_javascript: initialJavascript, css, lazy_chunks: lazyChunks }, limits }));
