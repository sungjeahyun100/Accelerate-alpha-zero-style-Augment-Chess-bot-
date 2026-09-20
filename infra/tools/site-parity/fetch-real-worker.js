#!/usr/bin/env node
// Downloads the live https://augmentchess.org/assets/aiWorker.js and writes an exported copy
// to .cache/real-worker.js (exports generateActions/applyAction/cloneState/setWorkerBoardDimensions/
// searchBestAction/evaluateState). Only the public worker asset is fetched.
// Usage: node fetch-real-worker.js [--force]   (skips download if cache exists unless --force)
const fs = require("fs"), path = require("path"), crypto = require("crypto");
const CACHE = path.join(__dirname, ".cache");
const RAW = path.join(CACHE, "aiWorker.raw.js"), OUT = path.join(CACHE, "real-worker.js");
async function main() {
  fs.mkdirSync(CACHE, { recursive: true });
  if (!process.argv.includes("--force") && fs.existsSync(OUT)) { console.log("cached:", OUT); return; }
  const res = await fetch("https://augmentchess.org/assets/aiWorker.js");
  if (!res.ok) throw new Error("HTTP " + res.status);
  const src = await res.text();
  fs.writeFileSync(RAW, src);
  const i = src.lastIndexOf("})();");
  if (i < 0) throw new Error("worker IIFE terminator not found; site format changed");
  const exp = "globalThis.__aiWorkerReal = { searchBestAction, generateActions, applyAction, evaluateState, cloneState, setWorkerBoardDimensions };\n  if (typeof module !== \"undefined\") module.exports = globalThis.__aiWorkerReal;\n";
  fs.writeFileSync(OUT, src.slice(0, i) + exp + src.slice(i));
  console.log("wrote", OUT, src.length, "bytes sha256", crypto.createHash("sha256").update(src).digest("hex"));
}
main().catch((e) => { console.error("fetch failed:", e.message); process.exit(1); });
