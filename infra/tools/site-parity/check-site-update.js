#!/usr/bin/env node
// Detects a new deployment of https://augmentchess.org/ by comparing the main bundle asset name and the
// aiWorker.js SHA-256 with last-seen.json. Fetches only public assets (index.html, main-*.js, aiWorker.js).
// Exit code: 0 = CHANGED or UNCHANGED, 2 = network error.
// Usage: node check-site-update.js [--save]   (--save rewrites last-seen.json with the fresh values)
const fs = require("fs"), path = require("path"), crypto = require("crypto");
const SITE = "https://augmentchess.org";
const SEEN = path.join(__dirname, "last-seen.json");
async function get(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(url + " -> HTTP " + res.status);
  return Buffer.from(await res.arrayBuffer());
}
(async () => {
  let fresh;
  try {
    const html = (await get(SITE + "/")).toString("utf8");
    const m = html.match(/\/assets\/(main-[A-Za-z0-9_-]+\.js)/);
    if (!m) throw new Error("main bundle not found in index.html");
    const worker = await get(SITE + "/assets/aiWorker.js");
    const main = await get(SITE + "/assets/" + m[1]);
    const v = main.toString("utf8").match(/UPDATE_LOG_VERSION\s*=\s*"([^"]+)"/);
    fresh = { mainBundle: m[1], mainBundleBytes: main.length, aiWorkerSha256: crypto.createHash("sha256").update(worker).digest("hex"), aiWorkerBytes: worker.length, updateLogVersion: v ? v[1] : null, checkedAt: new Date().toISOString() };
  } catch (e) { console.error("NETWORK/PARSE ERROR:", e.message); process.exit(2); }
  let old = null;
  try { old = JSON.parse(fs.readFileSync(SEEN, "utf8")); } catch {}
  const changed = !old || old.mainBundle !== fresh.mainBundle || old.aiWorkerSha256 !== fresh.aiWorkerSha256;
  if (!changed) { console.log("UNCHANGED", fresh.mainBundle, "worker", fresh.aiWorkerSha256.slice(0, 12), "log", fresh.updateLogVersion); }
  else {
    console.log("CHANGED" + (old ? "" : " (no previous last-seen.json)"));
    console.log("  main bundle :", old?.mainBundle, "->", fresh.mainBundle, `(${fresh.mainBundleBytes} bytes)`);
    console.log("  aiWorker    :", old?.aiWorkerSha256?.slice(0, 12), "->", fresh.aiWorkerSha256.slice(0, 12), `(${fresh.aiWorkerBytes} bytes)`);
    console.log("  update log  :", old?.updateLogVersion, "->", fresh.updateLogVersion);
    console.log("  next: node fetch-real-worker.js --force && run parity tests; then node check-site-update.js --save");
  }
  if (!old || process.argv.includes("--save")) { fs.writeFileSync(SEEN, JSON.stringify(fresh, null, 2) + "\n"); console.log("saved", SEEN); }
})();
