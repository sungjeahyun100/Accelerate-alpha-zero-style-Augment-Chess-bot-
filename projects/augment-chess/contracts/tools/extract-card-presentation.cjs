"use strict";

// Recreate the reviewed v7 presentation projection from the SHA-pinned client.
// Usage: node projects/augment-chess/contracts/tools/extract-card-presentation.cjs <frozen-source-dir> [--check]
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { FrozenClientSource } = require("../../oracle/game-adapter/src/frozen-client-source");
const { executionProfileForSha, metadataDigest } = require("../../oracle/game-adapter/src/reviewed-initializers");

const base = require("../catalog/card-definitions-20260928.json");
const outputPath = path.join(__dirname, "..", "catalog", "card-presentation-20260928.json");
const fields = ["id", "name", "phase", "stars", "text", "art", "effect", "help", "helpIcons", "helpItems"];
const presentationOnly = new Set(base.presentationFieldsExcluded);

function main() {
  const [sourceDirectory, option] = process.argv.slice(2);
  if (!sourceDirectory || (option && option !== "--check") || process.argv.length > 4) {
    throw new Error("Usage: node projects/augment-chess/contracts/tools/extract-card-presentation.cjs <frozen-source-dir> [--check]");
  }
  const executionProfile = executionProfileForSha(base.sourceMainSha256);
  const executionProfileSha256 = metadataDigest(executionProfile);
  const expectedCatalogVersion = metadataDigest({ contractVersion: "augment-v7-execution-catalog-v1",
    sourcePublicCatalogHash: executionProfile.sourcePublicCatalogHash, executionProfileSha256 });
  assert.equal(base.rulesVersion, executionProfile.rulesVersion, "presentation base rules version drift");
  assert.equal(base.catalogVersion, expectedCatalogVersion, "presentation base execution catalog identity drift");
  assert.equal(base.sourcePublicCatalogHash, executionProfile.sourcePublicCatalogHash, "presentation base official catalog hash drift");
  assert.deepStrictEqual(base.executionProfile, { version: executionProfile.profileVersion,
    sha256: executionProfileSha256, manifest: "execution-profile-20260928.json" }, "presentation base execution profile drift");
  const source = new FrozenClientSource(path.resolve(sourceDirectory), {
    expectedClientSha256: base.sourceMainSha256,
    expectedExecutionProfileVersion: executionProfile.profileVersion,
  });
  assert.equal(source.executionProfileSha256, executionProfileSha256, "presentation source execution profile drift");
  const cards = JSON.parse(source.createRuntime().evaluate("JSON.stringify(CARD_DEFS)"));
  assert(Array.isArray(cards) && cards.length === 257, "pinned CARD_DEFS count drift");
  assert.equal(base.definitions.length, cards.length, "base definitions count drift");
  const definitions = cards.map((card, index) => {
    const nonPresentation = Object.fromEntries(
      Object.entries(card).filter(([field]) => !presentationOnly.has(field))
    );
    assert.deepStrictEqual(nonPresentation, base.definitions[index], `source definition drift: ${card.id}`);
    for (const field of fields) assert(Object.hasOwn(card, field), `missing ${field}: ${card.id}`);
    for (const field of fields.filter(field => !["stars", "helpIcons", "helpItems"].includes(field))) {
      assert.equal(typeof card[field], "string", `invalid ${field}: ${card.id}`);
    }
    assert(card.stars === null || Number.isFinite(card.stars), `invalid stars: ${card.id}`);
    assert(Array.isArray(card.helpIcons) && card.helpIcons.every(icon => typeof icon === "string"), `invalid helpIcons: ${card.id}`);
    assert(Array.isArray(card.helpItems) && card.helpItems.every(item =>
      Array.isArray(item.icons) && item.icons.every(icon => typeof icon === "string") && typeof item.text === "string"),
    `invalid helpItems: ${card.id}`);
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
    definitions,
    sourcePublicCatalogHash: executionProfile.sourcePublicCatalogHash,
    executionProfile: { ...base.executionProfile },
  };
  const rendered = `${JSON.stringify(catalog, null, 2)}\n`;
  if (option === "--check") {
    assert.equal(fs.readFileSync(outputPath, "utf8").replace(/\r\n/g, "\n"), rendered, "card presentation projection drift");
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
