#!/usr/bin/env node
"use strict";
const source = require("../../game-adapter/src/frozen-client-source");
module.exports = source;

if (require.main === module) {
  const [command = "verify", root = source.cacheRoot()] = process.argv.slice(2);
  (async () => {
    if (command === "freeze") console.log(JSON.stringify(await source.freeze(root), null, 2));
    else if (command === "verify") console.log(JSON.stringify(source.verify(root), null, 2));
    else if (command === "inspect") {
      const loaded = source.loadMain(root);
      console.log(JSON.stringify(loaded.evaluate("({cards: CARDS$1.length, catalogHash: SEPTEMBER26_CATALOG_HASH, initialBoard: typeof createInitialBoard, draft: typeof startDraft})"), null, 2));
    } else throw new Error("Usage: frozen-site.js [freeze|verify|inspect] [baseline-directory]");
  })().catch(error => { console.error(error.stack); process.exitCode = 1; });
}
