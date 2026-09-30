#!/usr/bin/env node
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const { loadMain, cacheRoot } = require("./frozen-site");
const catalog = require("../../../contracts/catalog/site-20260927.json");

// Rules data only: UI text/art are excluded explicitly from semantic state parity.
function extract(root = cacheRoot()) {
  const main = loadMain(root);
  const source = main.manifest.files.find(file => /^main-/.test(file.name));
  if (!catalog.source.files.some(file => file.sha256 === source.sha256)) throw new Error("Unadopted rules baseline.");
  const definitions = JSON.parse(main.evaluate("JSON.stringify(CARD_DEFS.map(({name,text,art,...rules})=>rules))"));
  const names = ["DRAFT_BALANCE_MAX_SCORE_GAP", "DRAFT_BALANCE_EXTRA_ATTEMPTS", "GRAND_DRAFT_VERSION", "TURN_EXCLUSIVE_CARD_KEYS", "LATEST_MERCHANT_GUILD_EXCLUSIVE_CARD_IDS", "LATEST_MUTUALLY_EXCLUSIVE_DRAFT_CARD_GROUPS", "LATEST_WHITE_BOX_RESULT_EXCLUSIVE_CARD_IDS", "EXCLUSIVE_OPENING_CARD_IDS", "DELETED_CARD_IDS", "HIDDEN_CARD_IDS"];
  const constants = {};
  for (const name of names) {
    try { constants[name] = JSON.parse(main.evaluate(`JSON.stringify(${name} instanceof Set?[...${name}]:${name})`)); }
    catch (error) { if (name === "HIDDEN_CARD_IDS" && /not defined/.test(error.message)) continue; throw error; }
  }
  return { schemaVersion: 1, rulesVersion: catalog.rulesVersion, catalogVersion: catalog.catalogVersion, sourceMainSha256: source.sha256, presentationFieldsExcluded: ["name", "text", "art"], publicCatalogCardCount: catalog.cards.length, definitions, constants };
}
module.exports = { extract };
if (require.main === module) {
  const value = extract(process.argv[2] || cacheRoot());
  const target = path.resolve(__dirname, "../../../contracts/catalog/card-definitions-20260927.json");
  fs.writeFileSync(target, JSON.stringify(value, null, 2) + "\n");
  console.log(JSON.stringify({ target, definitions: value.definitions.length, bytes: fs.statSync(target).size }));
}
