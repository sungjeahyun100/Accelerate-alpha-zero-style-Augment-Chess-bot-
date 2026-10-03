"use strict";

const crypto = require("node:crypto");

// 원본 main의 SHA와 UTF-16 AST 경계를 함께 고정한다. 브라우저 startup을
// 실행하지 않으며, v7의 검토된 순수 초기화와 제외 문장을 모두 장부에 남긴다.
const V6_SHA = "abfe01a035813875772d8eeaf8e300a1df0348888ff48778d4a1789b76ae492f";
const V7_SHA = "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c";
const DECLARATIONS = new Set(["FunctionDeclaration", "VariableDeclaration", "ClassDeclaration"]);
const sha256 = value => crypto.createHash("sha256").update(value).digest("hex");

function canonicalMetadata(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalMetadata).join(",") + "]";
  if (value && typeof value === "object") {
    return "{" + Object.keys(value).sort().map(key => JSON.stringify(key) + ":" + canonicalMetadata(value[key])).join(",") + "}";
  }
  if (typeof value === "number" && !Number.isSafeInteger(value)) throw new TypeError("Execution metadata requires safe integer counters.");
  if (value === null || ["string", "number", "boolean"].includes(typeof value)) return JSON.stringify(value);
  throw new TypeError("Execution metadata must be JSON data.");
}

const metadataDigest = value => sha256(canonicalMetadata(value));
function freezeMetadata(value) {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.values(value).forEach(freezeMetadata);
    Object.freeze(value);
  }
  return value;
}

const V6_PROFILE = freezeMetadata({
  schemaVersion: 1,
  manifestVersion: "augment-v6-execution-profile-v1",
  profileVersion: "accelerate-headless-semantic-v6",
  sourceMainSha256: V6_SHA,
  initializers: [],
});
const V7_PROFILE = freezeMetadata(require("../../../contracts/catalog/execution-profile-20260928.json"));

function executionProfileForSha(clientSha256) {
  if (clientSha256 === V6_SHA) return V6_PROFILE;
  if (clientSha256 === V7_SHA) return V7_PROFILE;
  throw new Error("No reviewed headless initialization profile for frozen client SHA-256 " + clientSha256 + ".");
}

function boundNames(pattern, output = []) {
  if (!pattern) return output;
  if (pattern.type === "Identifier") output.push(pattern.name);
  else if (pattern.type === "ObjectPattern") {
    for (const property of pattern.properties) boundNames(property.type === "RestElement" ? property.argument : property.value, output);
  } else if (pattern.type === "ArrayPattern") {
    for (const element of pattern.elements) boundNames(element, output);
  } else if (pattern.type === "RestElement") boundNames(pattern.argument, output);
  else if (pattern.type === "AssignmentPattern") boundNames(pattern.left, output);
  else throw new Error("Unsupported source binding pattern " + pattern.type + ".");
  return output;
}

function visitAst(node, visit, parent = null, key = null) {
  if (!node || typeof node !== "object") return;
  visit(node, parent, key);
  for (const [childKey, value] of Object.entries(node)) {
    if (["start", "end", "loc"].includes(childKey)) continue;
    if (Array.isArray(value)) {
      for (const item of value) visitAst(item, visit, node, childKey);
    } else if (value && typeof value === "object") visitAst(value, visit, node, childKey);
  }
}

function dependencies(node, bindings) {
  const names = new Set();
  visitAst(node, (entry, parent, key) => {
    if (entry.type !== "Identifier" || !bindings.has(entry.name)) return;
    if (parent?.type === "MemberExpression" && key === "property" && !parent.computed ||
        ["Property", "MethodDefinition", "PropertyDefinition"].includes(parent?.type) && key === "key" && !parent.computed ||
        ["FunctionDeclaration", "FunctionExpression", "ClassDeclaration", "ClassExpression", "VariableDeclarator"].includes(parent?.type) && key === "id") return;
    names.add(entry.name);
  });
  return [...names].sort();
}

function retainedNodes(ast, clientSha256, raw) {
  const profile = executionProfileForSha(clientSha256);
  // v6의 선언 전용 규칙과 기존 증거의 의미를 변경하지 않는다.
  if (clientSha256 === V6_SHA) return ast.body.filter(node => DECLARATIONS.has(node.type));
  if (typeof raw !== "string" || sha256(raw) !== clientSha256) throw new Error("Reviewed initializer source SHA-256 mismatch.");
  if (profile.schemaVersion !== 1 || profile.manifestVersion !== "augment-v7-execution-profile-v1" ||
      profile.sourceMainSha256 !== clientSha256 || profile.sourcePolicy?.offsetUnit !== "UTF-16" ||
      profile.initializers.length !== 175 || profile.excludedInitializers.length !== 168) {
    throw new Error("Reviewed v7 execution profile manifest mismatch.");
  }
  const fingerprint = node => ({ type: node.type, start: node.start, end: node.end, sha256: sha256(raw.slice(node.start, node.end)) });
  const declarations = ast.body.filter(node => DECLARATIONS.has(node.type));
  if (declarations.length !== profile.declarations.count || metadataDigest(declarations.map(fingerprint)) !== profile.declarations.nodesSha256) {
    throw new Error("Frozen source declaration integrity changed; initializer dependencies require a new review.");
  }
  const bindings = new Map();
  for (const node of declarations) {
    const names = node.type === "VariableDeclaration" ? node.declarations.flatMap(declaration => boundNames(declaration.id)) : boundNames(node.id);
    for (const name of names) {
      if (bindings.has(name)) throw new Error("Duplicate source top-level binding " + name + ".");
      bindings.set(name, node);
    }
  }
  const bindingRecords = [...bindings].map(([name, node]) => ({ name, ...fingerprint(node) }))
    .sort((left, right) => left.name < right.name ? -1 : left.name > right.name ? 1 : 0);
  if (bindings.size !== profile.declarations.bindingCount || metadataDigest(bindingRecords) !== profile.declarations.bindingsSha256) {
    throw new Error("Frozen source top-level binding integrity changed; initializer dependencies require a new review.");
  }
  const actualImports = ast.body.filter(node => node.type === "ImportDeclaration").map(node => ({
    ...fingerprint(node), module: node.source.value, bindings: node.specifiers.map(specifier => specifier.local.name),
  }));
  if (canonicalMetadata(actualImports) !== canonicalMetadata(profile.imports)) throw new Error("Frozen source import stub policy changed.");
  const byStart = new Map(ast.body.map(node => [node.start, node]));
  const selected = new Set(), excluded = new Set();
  const validateNode = (record, collection) => {
    const node = byStart.get(record.start);
    if (!node || node.type !== record.type || node.end !== record.end || fingerprint(node).sha256 !== record.sha256 ||
        DECLARATIONS.has(node.type) || node.type === "ImportDeclaration" || selected.has(node.start) || excluded.has(node.start)) {
      throw new Error("Reviewed frozen client initializer changed at " + record.start + "; source review is required.");
    }
    collection.add(node.start);
    return node;
  };
  for (const record of profile.initializers) {
    const node = validateNode(record, selected);
    if (canonicalMetadata(dependencies(node, bindings)) !== canonicalMetadata(record.dependencies)) {
      throw new Error("Frozen initializer dependency mismatch at " + record.start + ".");
    }
  }
  for (const record of profile.excludedInitializers) validateNode(record, excluded);
  const statementCount = ast.body.filter(node => !DECLARATIONS.has(node.type) && node.type !== "ImportDeclaration").length;
  if (statementCount !== selected.size + excluded.size ||
      metadataDigest(profile.initializers) !== profile.initializersSha256 ||
      metadataDigest(profile.excludedInitializers) !== profile.excludedInitializersSha256) {
    throw new Error("Frozen source initializer partition integrity changed.");
  }
  for (const record of [...profile.pureHelperDeclarations, ...profile.replaySourceDeclarations]) {
    const node = bindings.get(record.name);
    if (!node || canonicalMetadata(fingerprint(node)) !== canonicalMetadata({ type: record.type, start: record.start, end: record.end, sha256: record.sha256 })) {
      throw new Error("Frozen initializer dependency " + record.name + " is missing or changed.");
    }
  }
  if (metadataDigest(profile.replayMetadata) !== profile.replayMetadataSha256) throw new Error("Frozen replay metadata integrity changed.");
  return ast.body.filter(node => DECLARATIONS.has(node.type) || selected.has(node.start));
}

module.exports = { retainedNodes, executionProfileForSha, metadataDigest };
