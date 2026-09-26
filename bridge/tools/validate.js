#!/usr/bin/env node
// DRAFT (bridge-draft-0). Dependency-free checker for bridge/examples against bridge/schemas.
//
//   node bridge/tools/validate.js            # validate every entry in bridge/examples/manifest.json
//   node bridge/tools/validate.js --verbose  # also print each result
//
// Exit code 0 = every "valid" example passes AND every "invalid" example is rejected AND every
// example file is listed in the manifest. Non-zero otherwise.
//
// This is NOT a full JSON Schema implementation. It supports only the keywords the draft schemas
// use and THROWS on any other keyword, so a schema edit that needs more cannot silently pass:
//   $schema $id $defs $ref title description $comment examples, type (string|array), enum, const,
//   properties, required, additionalProperties (boolean|schema), items, minItems, maxItems,
//   minimum, minLength, oneOf, anyOf, allOf
"use strict";
const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "..");
const SCHEMA_DIR = path.join(ROOT, "schemas");
const EX_DIR = path.join(ROOT, "examples");
const VERBOSE = process.argv.includes("--verbose");

const ANNOTATIONS = new Set(["$schema", "$id", "$defs", "title", "description", "$comment", "examples"]);
const KNOWN = new Set([...ANNOTATIONS, "$ref", "type", "enum", "const", "properties", "required",
  "additionalProperties", "items", "minItems", "maxItems", "minimum", "minLength", "oneOf", "anyOf", "allOf"]);

const docs = {};
for (const f of fs.readdirSync(SCHEMA_DIR).filter((n) => n.endsWith(".json"))) {
  docs[f] = JSON.parse(fs.readFileSync(path.join(SCHEMA_DIR, f), "utf8"));
}

function typeOf(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (typeof v === "number") return Number.isInteger(v) ? "integer" : "number";
  return typeof v; // string | boolean | object
}
const typeOk = (v, t) => { const a = typeOf(v); return a === t || (t === "number" && a === "integer"); };
const deepEq = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function resolveRef(ref, docName) {
  const [file, frag = ""] = ref.split("#");
  const name = file === "" ? docName : file;
  const doc = docs[name];
  if (!doc) throw new Error("unresolved $ref (no such schema file): " + ref + " from " + docName);
  let node = doc;
  for (const seg of frag.split("/").filter(Boolean)) {
    node = node && node[seg.replace(/~1/g, "/").replace(/~0/g, "~")];
    if (node === undefined) throw new Error("unresolved $ref fragment: " + ref + " from " + docName);
  }
  return { schema: node, docName: name };
}

// returns an array of error strings (empty = valid)
function validate(schema, data, at, docName) {
  if (schema === true) return [];
  if (schema === false) return [at + ": not allowed"];
  for (const k of Object.keys(schema)) {
    if (!KNOWN.has(k)) throw new Error("unsupported schema keyword '" + k + "' at " + at + " in " + docName);
  }
  const errs = [];
  if (schema.$ref !== undefined) {
    const r = resolveRef(schema.$ref, docName);
    errs.push(...validate(r.schema, data, at, r.docName));
  }
  if (schema.type !== undefined) {
    const ts = Array.isArray(schema.type) ? schema.type : [schema.type];
    if (!ts.some((t) => typeOk(data, t))) errs.push(at + ": expected type " + ts.join("|") + ", got " + typeOf(data));
  }
  if (schema.const !== undefined && !deepEq(schema.const, data)) errs.push(at + ": expected const " + JSON.stringify(schema.const) + ", got " + JSON.stringify(data));
  if (schema.enum !== undefined && !schema.enum.some((e) => deepEq(e, data))) errs.push(at + ": " + JSON.stringify(data) + " not in enum " + JSON.stringify(schema.enum));
  if (typeof data === "number") {
    if (schema.minimum !== undefined && data < schema.minimum) errs.push(at + ": " + data + " < minimum " + schema.minimum);
  }
  if (typeof data === "string") {
    if (schema.minLength !== undefined && data.length < schema.minLength) errs.push(at + ": string shorter than " + schema.minLength);
  }
  if (Array.isArray(data)) {
    if (schema.minItems !== undefined && data.length < schema.minItems) errs.push(at + ": fewer than " + schema.minItems + " items");
    if (schema.maxItems !== undefined && data.length > schema.maxItems) errs.push(at + ": more than " + schema.maxItems + " items");
    if (schema.items !== undefined) data.forEach((x, i) => errs.push(...validate(schema.items, x, at + "[" + i + "]", docName)));
  }
  if (data !== null && typeof data === "object" && !Array.isArray(data)) {
    const props = schema.properties || {};
    for (const r of schema.required || []) if (!(r in data)) errs.push(at + ": missing required property '" + r + "'");
    for (const k of Object.keys(data)) {
      if (k in props) errs.push(...validate(props[k], data[k], at + "." + k, docName));
      else if (schema.additionalProperties === false) errs.push(at + ": unexpected property '" + k + "'");
      else if (schema.additionalProperties && typeof schema.additionalProperties === "object") errs.push(...validate(schema.additionalProperties, data[k], at + "." + k, docName));
    }
  }
  if (schema.allOf) for (const s of schema.allOf) errs.push(...validate(s, data, at, docName));
  if (schema.anyOf) {
    const results = schema.anyOf.map((s) => validate(s, data, at, docName));
    if (!results.some((e) => e.length === 0)) errs.push(at + ": matches none of anyOf", ...best(results, schema.anyOf, data, docName));
  }
  if (schema.oneOf) {
    const results = schema.oneOf.map((s) => validate(s, data, at, docName));
    const n = results.filter((e) => e.length === 0).length;
    if (n === 0) errs.push(at + ": matches none of oneOf", ...best(results, schema.oneOf, data, docName));
    else if (n > 1) errs.push(at + ": matches " + n + " branches of oneOf (must be exactly 1)");
  }
  return errs;
}
// for failed unions show only the branch(es) with the fewest errors
function best(results, branches, data, docName) {
  let idx = results.map((_, i) => i);
  if (data && typeof data === "object" && typeof data.type === "string" && branches) {
    // prefer the branch whose `type` const matches the data, so the real problem is shown
    const match = idx.filter((i) => {
      let b = branches[i];
      if (b && b.$ref) b = resolveRef(b.$ref, docName).schema;
      return b && b.properties && b.properties.type && b.properties.type.const === data.type;
    });
    if (match.length) idx = match;
  }
  const min = Math.min(...idx.map((i) => results[i].length));
  return idx.filter((i) => results[i].length === min).slice(0, 2).map((i) => "  | " + results[i].slice(0, 4).join("\n  | "));
}

// ---- run ----
const manifest = JSON.parse(fs.readFileSync(path.join(EX_DIR, "manifest.json"), "utf8"));
let failed = 0;

// every schema keyword must be supported and every $ref must resolve, even in branches no example reaches
for (const [name, doc] of Object.entries(docs)) {
  (function lint(node, at) {
    if (Array.isArray(node)) return node.forEach((x, i) => lint(x, at + "[" + i + "]"));
    if (!node || typeof node !== "object") return;
    if (typeof node.$ref === "string") resolveRef(node.$ref, name);
    for (const k of Object.keys(node)) if (!KNOWN.has(k)) throw new Error("unsupported schema keyword '" + k + "' at " + at);
    for (const [k, v] of Object.entries(node)) {
      if (k === "properties" || k === "$defs") { for (const [pk, pv] of Object.entries(v)) lint(pv, at + "." + k + "." + pk); continue; }
      if (["const", "enum", "required", "examples"].includes(k)) continue;
      if (v && typeof v === "object") lint(v, at + "." + k);
    }
  })(doc, name);
}

const listed = new Set();
for (const m of manifest) {
  listed.add(m.file);
  const file = path.join(EX_DIR, m.file);
  let data;
  try { data = JSON.parse(fs.readFileSync(file, "utf8")); } catch (e) { console.log("FAIL  " + m.file + ": cannot read/parse: " + e.message); failed++; continue; }
  const [schemaFile, frag] = m.schema.split("#");
  const r = resolveRef((schemaFile || "") + "#" + (frag || ""), schemaFile);
  const errs = validate(r.schema, data, "$", r.docName);
  const ok = m.valid ? errs.length === 0 : errs.length > 0;
  if (!ok) { failed++; console.log("FAIL  " + m.file + (m.valid ? " (expected valid)" : " (expected INVALID but it passed)")); errs.slice(0, 8).forEach((e) => console.log("      " + e)); }
  else if (VERBOSE) console.log("ok    " + m.file + (m.valid ? "" : "  (rejected as intended: " + errs[0] + ")"));
}
// no unlisted example files
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).flatMap((e) => e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)]);
for (const f of walk(EX_DIR)) {
  const rel = path.relative(EX_DIR, f).split(path.sep).join("/");
  if (rel.endsWith(".json") && rel !== "manifest.json" && !listed.has(rel)) { failed++; console.log("FAIL  " + rel + ": example file not listed in manifest.json"); }
}
const nValid = manifest.filter((m) => m.valid).length;
console.log((failed ? "FAILED" : "PASSED") + ": " + nValid + " valid + " + (manifest.length - nValid) + " invalid examples checked against " + Object.keys(docs).length + " schemas" + (failed ? " (" + failed + " problem(s))" : ""));
process.exit(failed ? 1 : 0);
