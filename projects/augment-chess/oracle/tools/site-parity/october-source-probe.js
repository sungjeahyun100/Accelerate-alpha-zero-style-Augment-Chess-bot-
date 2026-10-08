#!/usr/bin/env node
"use strict";

// A bounded execution probe for the October client. It calls the original
// reset/draft functions; it is not an adopted GameAdapter or visibility oracle.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const profile = require("../../../contracts/catalog/execution-profile-20261007-probe.json");
const sha256 = value => crypto.createHash("sha256").update(value).digest("hex");
const DECLARATIONS = new Set(["FunctionDeclaration", "VariableDeclaration", "ClassDeclaration"]);

function browserShell() {
  const noop = () => {};
  const node = new Proxy(function () { return node; }, {
    get(_target, key) {
      if (key === Symbol.iterator) return function* () {};
      if (key === Symbol.toPrimitive) return () => "";
      if (key === "then") return undefined;
      if (key === "length") return 0;
      if (key === "getItem") return () => null;
      if (key === "querySelectorAll") return () => [];
      return node;
    }, apply() { return node; }, construct() { return node; },
  });
  const context = { console, URL, URLSearchParams, TextEncoder, TextDecoder,
    structuredClone, crypto: crypto.webcrypto, document: node, navigator: node,
    localStorage: node, sessionStorage: node, Worker: node, Image: node, Audio: node,
    MutationObserver: node, ResizeObserver: node, IntersectionObserver: node,
    location: { href: profile.sourceUrl, pathname: "/", search: "", hostname: "augmentchess.org", protocol: "https:" },
    performance: { now: () => 0 }, addEventListener: noop,
    setTimeout: noop, setInterval: noop, clearTimeout: noop, clearInterval: noop,
    requestAnimationFrame: noop, cancelAnimationFrame: noop,
    matchMedia: () => ({ matches: false, addEventListener: noop }),
    fetch() { throw new Error("Network disabled in October source probe."); },
  };
  context.window = context;
  return vm.createContext(context);
}

function bootstrapSource() {
  const adapterPath = path.resolve(__dirname, "../../game-adapter/src/game-adapter.js");
  const raw = fs.readFileSync(adapterPath, "utf8");
  // This is the existing headless adapter's bootstrap, checked by digest.
  // The probe does not silently invent replacements for source rule helpers.
  const start = raw.indexOf("const PRESENTATION_HOOKS =");
  const end = raw.indexOf("const BOOTSTRAP =", start);
  const after = raw.indexOf("\n`;", end);
  assert.ok(start >= 0 && end > start && after > end, "Headless bootstrap boundary changed.");
  const loader = vm.createContext({});
  vm.runInContext(raw.slice(start, after + 3), loader, { timeout: 1000 });
  const bootstrap = vm.runInContext("BOOTSTRAP", loader, { timeout: 1000 });
  assert.equal(sha256(bootstrap), profile.bootstrapSha256, "Headless bootstrap changed; review October probe again.");
  return bootstrap;
}

function loadSource(sourcePath, parserPath) {
  assert.ok(path.isAbsolute(sourcePath) && path.isAbsolute(parserPath), "Source and parser paths must be absolute.");
  const sourceBytes = fs.readFileSync(sourcePath);
  assert.equal(sha256(sourceBytes), profile.sourceMainSha256, "October source SHA-256 mismatch.");
  const parserBytes = fs.readFileSync(parserPath);
  assert.equal(sha256(parserBytes), profile.parserSha256, "October parser SHA-256 mismatch.");
  const acorn = require(parserPath);
  const raw = sourceBytes.toString("utf8");
  const ast = acorn.parse(raw, { ecmaVersion: "latest", sourceType: "module" });
  const declarations = ast.body.filter(node => DECLARATIONS.has(node.type));
  const imports = ast.body.filter(node => node.type === "ImportDeclaration");
  assert.equal(declarations.length, profile.declarationCount, "October declaration count changed.");
  assert.equal(imports.length, profile.importCount, "October import count changed.");
  assert.equal(ast.body.length - declarations.length - imports.length, profile.excludedTopLevelCount,
    "October top-level statement partition changed.");
  const importBindings = imports.flatMap(node => node.specifiers.map(specifier => specifier.local.name));
  const executable = importBindings.map(name => `var ${name} = "";`).join("\n") + "\n" +
    declarations.map(node => raw.slice(node.start, node.end).replace(/import\.meta\.url/g, JSON.stringify(profile.sourceUrl))).join("\n");
  const context = browserShell();
  new vm.Script(executable, { filename: path.basename(sourcePath) }).runInContext(context, { timeout: 15000 });
  context.__maxMicrotasks = 256;
  vm.runInContext(bootstrapSource(), context, { timeout: 15000 });
  return context;
}

function probe(sourcePath, parserPath, style) {
  assert.ok(["normal", "chaos", "grand"].includes(style), "Unsupported probe style.");
  const context = loadSource(sourcePath, parserPath);
  // Deterministic host RNG is a replay aid, not a proof of chance distribution.
  let random = 0x6d2b79f5;
  context.__sourceRandom = () => {
    random ^= random << 13; random ^= random >>> 17; random ^= random << 5;
    return (random >>> 0) / 0x100000000;
  };
  context.__style = style;
  vm.runInContext("Math.random=__sourceRandom;selectedGameStyle=__style;localPlayMode='local';playMode='local';resetGame(false,[]);beginInitialGameFlow();", context, { timeout: 15000 });
  const before = JSON.parse(vm.runInContext("JSON.stringify({mode:state.mode,gameStyle:state.gameStyle,turn:state.turn,boardRows:state.board.length,boardColumns:state.board.map(row=>row.length),draftPhase:state.draft?.phase,draftColor:state.draft?.color,choices:isGrandDraftState()?grandAvailableCardsForColor(state.draft.color).map(card=>({id:card.id,instanceId:card.instanceId})):isChaosDraftState()?chaosDraftBundles().map(bundle=>({index:bundle.index,cardIds:bundle.cards.map(card=>card.id)})):(state.draft?.choices||[]).map(card=>({id:card.id,instanceId:card.instanceId}))})", context));
  assert.equal(before.mode, "draft");
  assert.equal(before.boardRows, 8);
  assert.ok(before.boardColumns.every(size => size === 8));
  assert.ok(before.choices.length > 0, "Source produced no legal draft choices.");
  const applied = JSON.parse(vm.runInContext("JSON.stringify((()=>{const color=state.draft.color,phase=state.draft.phase,grand=isGrandDraftState(),chaos=isChaosDraftState();let ok,selectedCardId;if(chaos){const bundle=chaosDraftBundles()[0];selectedCardId=bundle.cards.map(card=>card.id).join(',');ok=finishChaosDraftBundle({index:bundle.index},{auto:true});}else{const card=grand?grandAvailableCardsForColor(color)[0]:state.draft.choices[0];selectedCardId=card.id;ok=finishDraft(card,null,{auto:true});}if(ok){if(grand)completeGrandDraftStep(color,state.draft.pickIndex);else completeDraftStep(color,phase);}return {ok,mode:state.mode,turn:state.turn,draftPhase:state.draft?.phase,draftColor:state.draft?.color,selectedCardId};})())", context, { timeout: 15000 }));
  assert.equal(applied.ok, true, "Source rejected its own offered card.");
  assert.ok(applied.draftColor !== before.draftColor || applied.draftPhase !== before.draftPhase || applied.mode !== before.mode,
    "Source draft action caused no phase or actor transition.");
  let play = null;
  if (style === "normal") {
    // The first offer is followed by the opponent's opening offer. Bound the
    // source loop so a changed draft protocol cannot hang this probe.
    for (let step = 0; step < 16 && vm.runInContext("state.mode==='draft'", context); step++) {
      const accepted = vm.runInContext("(()=>{const color=state.draft.color,phase=state.draft.phase,card=state.draft.choices[0];if(!card)return false;const ok=finishDraft(card,null,{auto:true});if(ok)completeDraftStep(color,phase);return ok;})()", context, { timeout: 15000 });
      assert.equal(accepted, true, "Source failed to complete its own opening draft.");
    }
    assert.equal(vm.runInContext("state.mode", context), "play", "Opening draft did not reach play within 16 decisions.");
    const visibleBefore = JSON.parse(vm.runInContext("JSON.stringify(Object.fromEntries(['white','black'].map(viewer=>[viewer,state.board.flatMap((row,r)=>row.map((piece,c)=>piece&&pieceVisibleToColorAt(piece,r,c,viewer,state.board)?{row:r,col:c,type:piece.type,color:piece.color}:null)).filter(Boolean)])))", context, { timeout: 15000 }));
    const actions = JSON.parse(vm.runInContext("JSON.stringify(collectValidAiActions(state.turn,{includeCards:true,exhaustiveCards:false}))", context, { timeout: 15000 }));
    assert.ok(actions.length > 0 && actions.length <= 100000, "Source legal action count is outside probe budget.");
    const move = actions.find(action => action.type === "move");
    assert.ok(move, "Source supplied no opening move.");
    context.__action = move;
    const outcome = JSON.parse(vm.runInContext("JSON.stringify(applyAiAction(__action))", context, { timeout: 15000 }));
    assert.equal(outcome.ok, true, "Source rejected its own generated move.");
    const after = JSON.parse(vm.runInContext("JSON.stringify({mode:state.mode,turn:state.turn,moveCount:state.moveCount,from:__action.from,to:{row:__action.move.row,col:__action.move.col},pieceAtDestination:state.board[__action.move.row]?.[__action.move.col]?.type||null})", context));
    assert.equal(after.moveCount, 1, "Source opening move did not advance the move count.");
    assert.ok(after.pieceAtDestination, "Source opening move left its destination empty.");
    const visibleAfter = JSON.parse(vm.runInContext("JSON.stringify(Object.fromEntries(['white','black'].map(viewer=>[viewer,state.board.flatMap((row,r)=>row.map((piece,c)=>piece&&pieceVisibleToColorAt(piece,r,c,viewer,state.board)?{row:r,col:c,type:piece.type,color:piece.color}:null)).filter(Boolean)])))", context, { timeout: 15000 }));
    play = { legalActionCount: actions.length, selectedAction: move, outcome, after,
      visiblePieceCountBefore: Object.fromEntries(Object.entries(visibleBefore).map(([viewer,pieces])=>[viewer,pieces.length])),
      visiblePieceCountAfter: Object.fromEntries(Object.entries(visibleAfter).map(([viewer,pieces])=>[viewer,pieces.length])) };
  }
  return { profileVersion: profile.profileVersion, rulesVersion: profile.rulesVersion,
    sourceMainSha256: profile.sourceMainSha256, sourcePublicCatalogHash: profile.sourcePublicCatalogHash,
    style, before, applied, play };
}

if (require.main === module) {
  try {
    const [sourcePath, parserPath, style = "normal"] = process.argv.slice(2);
    if (!sourcePath || !parserPath) throw new Error("Usage: october-source-probe.js ABSOLUTE_MAIN_PATH ABSOLUTE_ACORN_PATH [normal|chaos|grand]");
    console.log(JSON.stringify(probe(sourcePath, parserPath, style), null, 2));
  } catch (error) { console.error(error.stack || error); process.exitCode = 1; }
}
module.exports = { probe, loadSource };
