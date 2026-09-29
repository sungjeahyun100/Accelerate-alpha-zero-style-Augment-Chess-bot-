"use strict";

// Recreate the reviewed v7 presentation projection from the SHA-pinned client.
// Usage: node bridge/tools/extract-card-presentation.cjs <frozen-source-dir> [--check]
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { FrozenClientSource } = require("../../packages/game-adapter/src/frozen-client-source");

const base = require("../catalog/card-definitions-20260928.json");
const outputPath = path.join(__dirname, "..", "catalog", "card-presentation-20260928.json");
const fields = ["id", "name", "phase", "stars", "text", "art", "effect"];
const presentationOnly = new Set(["name", "text", "art"]);

function main() {
  const [sourceDirectory, option] = process.argv.slice(2);
  if (!sourceDirectory || (option && option !== "--check") || process.argv.length > 4) {
    throw new Error("Usage: node bridge/tools/extract-card-presentation.cjs <frozen-source-dir> [--check]");
  }
  const source = new FrozenClientSource(path.resolve(sourceDirectory), {
    expectedClientSha256: base.sourceMainSha256
  });
  const cards = JSON.parse(source.createRuntime().evaluate("JSON.stringify(CARD_DEFS)"));
  assert(Array.isArray(cards) && cards.length === 257, "pinned CARD_DEFS count drift");
  assert.equal(base.definitions.length, cards.length, "base definitions count drift");
  const definitions = cards.map((card, index) => {
    const nonPresentation = Object.fromEntries(
      Object.entries(card).filter(([field]) => !presentationOnly.has(field))
    );
    assert.deepStrictEqual(nonPresentation, base.definitions[index], `source definition drift: ${card.id}`);
    for (const field of fields) assert(Object.hasOwn(card, field), `missing ${field}: ${card.id}`);
    for (const field of fields.filter(field => field !== "stars")) {
      assert.equal(typeof card[field], "string", `invalid ${field}: ${card.id}`);
    }
    assert(card.stars === null || Number.isFinite(card.stars), `invalid stars: ${card.id}`);
    return Object.fromEntries(fields.map(field => [field, card[field]]));
  });
  assert.equal(new Set(definitions.map(card => card.id)).size, 257, "duplicate source card IDs");
  const catalog = {
    schemaVersion: 1,
    rulesVersion: base.rulesVersion,
    catalogVersion: base.catalogVersion,
    sourceMainSha256: base.sourceMainSha256,
    sourceExpression: "JSON.stringify(CARD_DEFS)",
    sourceDefinitionCount: 257,
    definitions
  };
  const rendered = `${JSON.stringify(catalog, null, 2)}\n`;
  if (option === "--check") {
    assert.equal(fs.readFileSync(outputPath, "utf8"), rendered, "card presentation projection drift");
  } else {
    fs.writeFileSync(outputPath, rendered);
  }
  process.stdout.write(`${option === "--check" ? "verified" : "wrote"} ${outputPath}\n`);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.stack || error}\n`);
    process.exitCode = 1;
  }
}
