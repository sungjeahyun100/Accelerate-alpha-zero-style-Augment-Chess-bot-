#!/usr/bin/env node
"use strict";

// The public site's source remains outside Git. Loading never performs network I/O.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const crypto = require("node:crypto");
const SITE = "https://augmentchess.org";
const PARSER_URL = "https://unpkg.com/acorn@8.15.0/dist/acorn.js";
const PARSER_SHA256 = "fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c";
const sha256 = value => crypto.createHash("sha256").update(value).digest("hex");
function cacheRoot() {
  const parent = process.env.RUNNER_TEMP || process.env.APPDATA;
  if (!parent) throw new Error("APPDATA or RUNNER_TEMP is required; pass an explicit baseline directory on other hosts.");
  return path.join(parent, "Accelerate", "cache", "site-baseline");
}
async function freeze(root = cacheRoot()) {
  const manifestPath = path.join(root, "baseline.json");
  if (fs.existsSync(manifestPath)) return verify(root);
  fs.mkdirSync(root, { recursive: true });
  async function get(url) {
    const response = await fetch(url, { signal: AbortSignal.timeout(30000) });
    if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
    return Buffer.from(await response.arrayBuffer());
  }
  const index = await get(`${SITE}/`);
  const asset = index.toString("utf8").match(/\/assets\/(main-[A-Za-z0-9_-]+\.js)/)?.[1];
  if (!asset) throw new Error("Site main asset was not found.");
  const files = [];
  for (const [name, url, provided] of [["index.html", `${SITE}/`, index], [asset, `${SITE}/assets/${asset}`], ["aiWorker.raw.js", `${SITE}/assets/aiWorker.js`], ["acorn-8.15.0.js", PARSER_URL]]) {
    const data = provided || await get(url);
    if (url === PARSER_URL && sha256(data) !== PARSER_SHA256) throw new Error("Pinned parser integrity failure.");
    const target = path.join(root, name);
    if (fs.existsSync(target) && !fs.readFileSync(target).equals(data)) throw new Error(`Refusing to replace frozen ${name}.`);
    fs.writeFileSync(target, data);
    files.push({ name, url, sha256: sha256(data), bytes: data.length });
  }
  const manifest = { schemaVersion: 1, frozenAt: new Date().toISOString(), site: SITE, files };
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n", { flag: "wx" });
  return manifest;
}
function verify(root = cacheRoot()) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, "baseline.json"), "utf8"));
  if (manifest.schemaVersion !== 1 || manifest.site !== SITE || !Array.isArray(manifest.files)) throw new Error("Invalid baseline manifest.");
  for (const file of manifest.files) {
    if (!/^(index\.html|main-[A-Za-z0-9_-]+\.js|aiWorker\.raw\.js|acorn-8\.15\.0\.js)$/.test(file.name)) throw new Error("Unexpected baseline path.");
    const data = fs.readFileSync(path.join(root, file.name));
    if (data.length !== file.bytes || sha256(data) !== file.sha256) throw new Error(`Frozen source integrity failure: ${file.name}`);
  }
  return manifest;
}
function browserShell() {
  const noop = () => {};
  const node = new Proxy(function () { return node; }, {
    get(_target, key) {
      if (key === Symbol.iterator) return function* () {};
      if (key === Symbol.toPrimitive) return () => "";
      if (key === "then") return undefined;
      if (key === "length") return 0;
      if (key === "getItem") return () => null;
      if (["querySelectorAll", "getElementsByClassName"].includes(key)) return () => [];
      if (["matches", "contains"].includes(key)) return () => false;
      return node;
    }, apply() { return node; }, construct() { return node; }
  });
  const context = { URL, URLSearchParams, TextEncoder, TextDecoder, AbortController, AbortSignal, structuredClone,
    console, document: node, navigator: node, localStorage: node, sessionStorage: node,
    location: { href: `${SITE}/`, pathname: "/", search: "", hostname: "augmentchess.org", protocol: "https:" },
    Worker: node, Audio: node, Image: node, MutationObserver: node, ResizeObserver: node, IntersectionObserver: node,
    performance: { now: () => 0 }, requestAnimationFrame: noop, cancelAnimationFrame: noop,
    setTimeout: noop, setInterval: noop, clearTimeout: noop, clearInterval: noop,
    crypto: crypto.webcrypto, addEventListener: noop,
    matchMedia: () => ({ matches: false, addEventListener: noop }),
    fetch() { throw new Error("Network is disabled in the offline oracle."); }
  };
  context.window = context;
  return vm.createContext(context);
}
function loadMain(root = cacheRoot()) {
  const manifest = verify(root);
  const main = manifest.files.find(file => /^main-/.test(file.name));
  const parser = path.join(root, "acorn-8.15.0.js");
  if (!fs.existsSync(parser)) throw new Error("The offline parser is missing; bootstrap the pinned parser explicitly.");
  if (sha256(fs.readFileSync(parser)) !== PARSER_SHA256) throw new Error("Pinned parser integrity failure.");
  const acorn = require(parser);
  const raw = fs.readFileSync(path.join(root, main.name), "utf8");
  const ast = acorn.parse(raw, { ecmaVersion: "latest", sourceType: "module" });
  const imports = ast.body.filter(node => node.type === "ImportDeclaration").flatMap(node => node.specifiers.map(specifier => specifier.local.name));
  const declarations = ast.body.filter(node => ["FunctionDeclaration", "VariableDeclaration", "ClassDeclaration"].includes(node.type));
  const source = imports.map(name => `var ${name} = "";`).join("\n") + "\n" + declarations.map(node => raw.slice(node.start, node.end).replace(/import\.meta\.url/g, JSON.stringify(main.url))).join("\n");
  const context = browserShell();
  vm.runInContext(source, context, { timeout: 15000, filename: main.name });
  return { context, manifest, evaluate(source, timeout = 10000) { return vm.runInContext(source, context, { timeout }); } };
}
function loadWorker(root = cacheRoot()) {
  const manifest = verify(root);
  const source = fs.readFileSync(path.join(root, "aiWorker.raw.js"), "utf8");
  const end = source.lastIndexOf("})();");
  if (end < 0) throw new Error("Worker format changed.");
  const context = vm.createContext({ console, structuredClone, self: {}, addEventListener() {} });
  vm.runInContext(source.slice(0, end) + "globalThis.oracle = {generateActions,applyAction,cloneState,setWorkerBoardDimensions};\n" + source.slice(end), context, { timeout: 10000 });
  return { context, manifest, api: context.oracle };
}
module.exports = { cacheRoot, freeze, verify, loadMain, loadWorker, sha256 };
if (require.main === module) {
  const [command = "verify", root = cacheRoot()] = process.argv.slice(2);
  (async () => {
    if (command === "freeze") console.log(JSON.stringify(await freeze(root), null, 2));
    else if (command === "verify") console.log(JSON.stringify(verify(root), null, 2));
    else if (command === "inspect") { const loaded = loadMain(root); console.log(JSON.stringify(loaded.evaluate("({cards: CARDS$1.length, catalogHash: SEPTEMBER26_CATALOG_HASH, initialBoard: typeof createInitialBoard, draft: typeof startDraft})"), null, 2)); }
    else throw new Error("Usage: frozen-site.js [freeze|verify|inspect] [baseline-directory]");
  })().catch(error => { console.error(error.stack); process.exitCode = 1; });
}
