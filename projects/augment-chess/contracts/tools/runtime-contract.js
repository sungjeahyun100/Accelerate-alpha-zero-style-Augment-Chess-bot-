"use strict";
const crypto = require("node:crypto");
const { validate } = require("./validate");
function freezeMetadata(value) {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freezeMetadata(child);
    Object.freeze(value);
  }
  return value;
}
const BASELINES = Object.freeze({
  "site-20260927": Object.freeze({
    catalog: freezeMetadata(require("../catalog/site-20260927.json")),
    observationPolicy: freezeMetadata(require("../catalog/observation-20260927.json")),
    ORACLE_PROFILE_VERSION: "accelerate-headless-semantic-v6",
    RUNTIME_SCHEMA_NAME: "runtime-v1.schema.json",
    mainSha256: "abfe01a035813875772d8eeaf8e300a1df0348888ff48778d4a1789b76ae492f",
  }),
  "site-20260928": Object.freeze({
    catalog: freezeMetadata(require("../catalog/site-20260928.json")),
    observationPolicy: freezeMetadata(require("../catalog/observation-20260928.json")),
    executionProfile: freezeMetadata(require("../catalog/execution-profile-20260928.json")),
    ORACLE_PROFILE_VERSION: "accelerate-headless-semantic-v7-faithful-init-v1",
    RUNTIME_SCHEMA_NAME: "runtime-site-20260928.schema.json",
    mainSha256: "e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c",
  }),
});
function createRuntimeContract(options = {}) {
  if (!options || typeof options !== "object" || Array.isArray(options) || Object.keys(options).some(key => key !== "baseline")) throw new TypeError("Expected a known runtime baseline selector.");
  const baseline = options.baseline ?? "site-20260927";
  const selected = BASELINES[baseline];
  if (!selected) throw new TypeError(`Unknown runtime baseline ${baseline}.`);
  const { catalog, observationPolicy, executionProfile, ORACLE_PROFILE_VERSION, RUNTIME_SCHEMA_NAME, mainSha256 } = selected;
  if (catalog.rulesVersion !== observationPolicy.rulesVersion ||
      catalog.source.files.filter(file => /^main-/.test(file.name)).length !== 1 ||
      catalog.source.files.find(file => /^main-/.test(file.name)).sha256 !== mainSha256) {
    throw new Error(`Runtime baseline ${baseline} metadata mismatch.`);
  }
const VERSIONS = Object.freeze({ position: "accelerate-position-v1", action: "accelerate-action-v1", observation: "accelerate-observation-v2", result: "accelerate-result-v1", step: "accelerate-step-v1" });
const digest = value => crypto.createHash("sha256").update(canonical(value)).digest("hex");
const executionProfileSha256 = executionProfile ? digest(executionProfile) : null;
if (executionProfile) {
  const expectedCatalog = digest({ contractVersion: "augment-v7-execution-catalog-v1",
    sourcePublicCatalogHash: executionProfile.sourcePublicCatalogHash, executionProfileSha256 });
  if (executionProfile.rulesVersion !== catalog.rulesVersion || executionProfile.sourceMainSha256 !== mainSha256 ||
      executionProfile.profileVersion !== ORACLE_PROFILE_VERSION ||
      catalog.sourcePublicCatalogHash !== executionProfile.sourcePublicCatalogHash ||
      catalog.executionProfile?.version !== ORACLE_PROFILE_VERSION ||
      catalog.executionProfile?.manifest !== "execution-profile-20260928.json" ||
      catalog.executionProfile?.sha256 !== executionProfileSha256 || catalog.catalogVersion !== expectedCatalog) {
    throw new Error(`Runtime baseline ${baseline} execution profile identity mismatch.`);
  }
}
function canonical(value, options = undefined) {
  const diagnostic = options?.diagnostic === true || process.env.ACCELERATE_JSON_BUDGET_DIAGNOSTICS === "1";
  const budget = { nodes: 0, bytes: 0, lastDepth: null };
  const charge = (depth, path, bytes = 8) => {
    const previous = diagnostic ? { nodes: budget.nodes, bytes: budget.bytes, depth: budget.lastDepth } : null;
    budget.nodes++; budget.bytes += bytes;
    const exceeded = depth > 64 ? "depth" : budget.nodes > 100000 ? "nodes" : budget.bytes > 8 * 1024 * 1024 ? "bytes" : null;
    if (exceeded) {
      const error = new TypeError("JSON exceeds depth64/nodes100000/bytes8MiB limits.");
      error.code = "JSON_BUDGET_EXCEEDED";
      if (diagnostic) {
        const segments = path.split("/");
        error.budget = { exceeded, depth, priorDepth: previous.depth,
          nodes: budget.nodes, priorNodes: previous.nodes, bytes: budget.bytes, priorBytes: previous.bytes,
          path, parentPath: segments.length > 1 ? segments.slice(0, -1).join("/") || "$" : null,
          topLevelFieldPath: segments.length > 1 ? `${segments[0]}/${segments[1]}` : "$" };
      }
      throw error;
    }
    if (diagnostic) budget.lastDepth = depth;
  };
  const childPath = (path, key) => diagnostic ? `${path}/${String(key).replace(/~/g, "~0").replace(/\//g, "~1")}` : "";
  function visit(value, depth, path) {
  charge(depth, path);
  if (typeof value === "string") {
    charge(depth, path, Buffer.byteLength(value, "utf8"));
    for (let index = 0; index < value.length; index++) {
      const unit = value.charCodeAt(index);
      if (unit >= 0xd800 && unit <= 0xdbff) { const low = value.charCodeAt(++index); if (!(low >= 0xdc00 && low <= 0xdfff)) throw new TypeError("Lone Unicode surrogate is not JCS data."); }
      else if (unit >= 0xdc00 && unit <= 0xdfff) throw new TypeError("Lone Unicode surrogate is not JCS data.");
    }
    return JSON.stringify(value);
  }
  if (value === null || typeof value === "boolean") return JSON.stringify(value);
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new TypeError("Non-finite JSON number.");
    if (Number.isInteger(value) && !Number.isSafeInteger(value)) throw new TypeError("Unsafe JSON integer.");
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index++) if (!Object.hasOwn(value, index)) throw new TypeError("Sparse arrays are not JSON data.");
    return "[" + value.map((child, index) => visit(child, depth + 1, diagnostic ? childPath(path, index) : "")).join(",") + "]";
  }
  if (value && typeof value === "object" && [Object.prototype, null].includes(Object.getPrototypeOf(value))) return "{" + Object.keys(value).sort().map(key => {
    const fieldPath = diagnostic ? childPath(path, key) : "";
    return visit(key, depth + 1, fieldPath) + ":" + visit(value[key], depth + 1, fieldPath);
  }).join(",") + "}";
  throw new TypeError("Expected JSON data; undefined, sparse arrays, prototypes and executable values are not allowed.");
  }
  return visit(value, 0, "$");
}
// Read-only accounting over the already encoded snapshot. Counts follow the
// canonical charge model; capped entries are lower bounds, not raw values.
function summarizeStateFields(state) {
  const fields = Object.create(null);
  for (const key of Object.keys(state)) {
    const stack = [{ value: state[key], depth: 0 }];
    const size = { nodes: 0, bytes: 0, depth: 0, truncated: false };
    while (stack.length) {
      if (size.nodes >= 200000 || size.bytes >= 32 * 1024 * 1024) { size.truncated = true; break; }
      const { value, depth } = stack.pop();
      size.nodes++; size.bytes += 8; size.depth = Math.max(size.depth, depth);
      if (typeof value === "string") { size.nodes++; size.bytes += Buffer.byteLength(value, "utf8"); }
      else if (Array.isArray(value)) {
        for (let index = value.length - 1; index >= 0; index--) {
          if (stack.length + size.nodes >= 200000) { size.truncated = true; break; }
          stack.push({ value: value[index], depth: depth + 1 });
        }
      } else if (value && typeof value === "object") {
        for (const name of Object.keys(value)) {
          if (stack.length + size.nodes + 2 >= 200000) { size.truncated = true; break; }
          stack.push({ value: value[name], depth: depth + 1 });
          stack.push({ value: name, depth: depth + 1 });
        }
      }
    }
    fields[key] = size;
  }
  return fields;
}
function jsonCopy(value) { return JSON.parse(canonical(value)); }
function deepFreeze(value) { if (value && typeof value === "object") { for (const child of Object.values(value)) deepFreeze(child); Object.freeze(value); } return value; }
function color(value) { if (!["white", "black"].includes(value)) throw new TypeError("Invalid player color."); return value; }
function rng(seed, tape = []) {
  if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) throw new TypeError("Seed must be a uint32.");
  if (!Array.isArray(tape) || tape.some(value => !Number.isFinite(value) || value < 0 || value >= 1)) throw new TypeError("RNG tape values must be in [0,1).");
  canonical(tape);
  return { algorithm: "lcg32-v1", state: seed, cursor: 0, tape: [...tape] };
}
function nextRandom(value) {
  validateRng(value);
  if (value.cursor === Number.MAX_SAFE_INTEGER) throw new TypeError("RNG cursor overflow.");
  const next = { ...value, tape: [...value.tape], state: (Math.imul(value.state, 1664525) + 1013904223) >>> 0, cursor: value.cursor + 1 };
  return { value: value.cursor < value.tape.length ? value.tape[value.cursor] : next.state / 4294967296, rng: next };
}
function validateRng(value) {
  if (!value || value.algorithm !== "lcg32-v1" || !Number.isSafeInteger(value.cursor) || value.cursor < 0) throw new TypeError("Invalid RNG state.");
  if (Object.keys(value).length !== 4 || Object.keys(value).some(key => !["algorithm", "state", "cursor", "tape"].includes(key))) throw new TypeError("Unexpected RNG fields.");
  rng(value.state, value.tape);
}
function position(state, random, history = []) {
  validateRng(random);
  validateState(state, history);
  const value = { protocolVersion: VERSIONS.position, rulesVersion: catalog.rulesVersion, catalogVersion: catalog.catalogVersion, state: jsonCopy(state), rng: jsonCopy(random), history: jsonCopy(history) };
  value.positionId = digest(value);
  return deepFreeze(value);
}
function validatePosition(value) {
  if (!value || value.protocolVersion !== VERSIONS.position || value.rulesVersion !== catalog.rulesVersion || value.catalogVersion !== catalog.catalogVersion) throw new TypeError("Position version mismatch.");
  const keys = ["protocolVersion", "rulesVersion", "catalogVersion", "state", "rng", "history", "positionId"];
  if (Object.keys(value).some(key => !keys.includes(key)) || keys.some(key => !Object.hasOwn(value, key))) throw new TypeError("Unexpected position fields.");
  validateRng(value.rng);
  validateState(value.state, value.history);
  const { positionId, ...content } = value;
  if (typeof positionId !== "string" || digest(content) !== positionId) throw new TypeError("Position identity mismatch.");
  return value;
}
function validateState(state, history) {
  if (!state || typeof state !== "object" || Array.isArray(state) || !Array.isArray(history)) throw new TypeError("Invalid position payload.");
  color(state.turn);
  if (!Array.isArray(state.board) || state.board.length !== 8 || state.board.some(row => !Array.isArray(row) || row.length !== 8 || row.some(cell => cell !== null && (!cell || typeof cell !== "object" || Array.isArray(cell))))) throw new TypeError("Position requires an 8x8 board.");
  if (typeof state.mode !== "string" || !state.mode) throw new TypeError("Position requires the actual site phase.");
  try { canonical(state); }
  catch (error) { if (error.code === "JSON_BUDGET_EXCEEDED") error.jsonRoot = "state"; throw error; }
  try { canonical(history); }
  catch (error) { if (error.code === "JSON_BUDGET_EXCEEDED") error.jsonRoot = "history"; throw error; }
  for (const event of history) validateGameEvent(event);
}
function exactKeys(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).some(key => !keys.includes(key)) || keys.some(key => !Object.hasOwn(value, key))) throw new TypeError(`Invalid ${label} fields.`);
}
function validateGameEvent(event) {
  exactKeys(event, ["protocolVersion", "actor", "action", "turnChanged", "public"], "game event");
  if (event.protocolVersion !== "accelerate-game-event-v1" || typeof event.turnChanged !== "boolean") throw new TypeError("Game event version/turn flag mismatch.");
  color(event.actor); validatePayload(event.action);
  if (event.actor !== event.action.color) throw new TypeError("Game event actor mismatch.");
  exactKeys(event.public, ["white", "black"], "viewer events");
  for (const viewer of ["white", "black"]) {
    validatePublicTransition(event.public[viewer], event.actor);
  }
}
function validatePublicPiece(value) {
  if (value === null) return;
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).some(key => !observationPolicy.piecePublicFields.includes(key))) throw new TypeError("Invalid public piece fields.");
  if (!catalog.pieceTypes.includes(value.type) || !["white", "black", "neutral"].includes(value.color)) throw new TypeError("Invalid public piece identity.");
  for (const [key, field] of Object.entries(value)) {
    if (["type", "color"].includes(key)) continue;
    if (key === "status") { surface("pieceStatus", field); }
    else if (["moved", "shielded", "frozen", "submerged"].includes(key)) { if (typeof field !== "boolean") throw new TypeError("Invalid public piece flag."); }
    else if (key === "logDir") {
      exactKeys(field, ["dr", "dc"], "public log direction");
      if (![field.dr,field.dc].every(value=>Number.isInteger(value)&&value>=-1&&value<=1)) throw new TypeError("Invalid public log direction.");
    } else if (["facing", "windmillMode"].includes(key)) { if (typeof field !== "string" && !Number.isSafeInteger(field)) throw new TypeError("Invalid public orientation."); }
    else if (typeof field !== "number" || !Number.isFinite(field)) throw new TypeError("Invalid public piece counter.");
  }
  if (!Object.hasOwn(value,"status")) throw new TypeError("Public piece needs an explicit source-derived status surface.");
  const errors=validate(observationPolicy.publicPieceSchema,value,"piece",RUNTIME_SCHEMA_NAME);
  if(errors.length)throw new TypeError(`Invalid public piece schema: ${errors.join("; ")}`);
}
function surface(name, value) {
  const errors = validate(observationPolicy.surfaceSchemas[name], value, name, RUNTIME_SCHEMA_NAME);
  if (errors.length) throw new TypeError(`Invalid source public surface: ${errors.join("; ")}`);
}
function validatePublicCard(value) {
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).some(key => !observationPolicy.cardPublicFields.includes(key))) throw new TypeError("Invalid public card fields.");
  if (typeof value.id !== "string" || !value.id || typeof value.instanceId !== "string" || !value.instanceId) throw new TypeError("Invalid public card identity.");
  for (const [key, field] of Object.entries(value)) {
    if (["id","instanceId","effect","phase"].includes(key)) { if (typeof field !== "string" || !field) throw new TypeError("Invalid public card label."); }
    else if (["slot","stars","ratingHalfStars"].includes(key)) { if (typeof field !== "number" || !Number.isFinite(field) || field < 0) throw new TypeError("Invalid public card number."); }
    else if (key === "revealed") { const errors=validate(observationPolicy.cardRevelationSchema,field,"card.revealed",RUNTIME_SCHEMA_NAME);if(errors.length)throw new TypeError(`Invalid revealed public card result: ${errors.join("; ")}`); }
    else if (typeof field !== "boolean") throw new TypeError("Invalid public card flag.");
  }
}
function validateResult(value) {
  exactKeys(value, ["protocolVersion", "status", "winner", "outcome", "reason"], "game result");
  if (value.protocolVersion !== VERSIONS.result || !["ongoing", "terminal", "unfinished"].includes(value.status) || ![null,"white","black"].includes(value.winner) || ![null,"white","black","draw"].includes(value.outcome) || typeof value.reason !== "string" || value.reason.length > 1024) throw new TypeError("Invalid game result.");
  if (value.status === "terminal" ? !value.outcome || (value.outcome === "draw" ? value.winner !== null : value.winner !== value.outcome) : value.winner !== null || value.outcome !== null) throw new TypeError("Inconsistent game result.");
}
function validatePublicTransition(projected, actor = projected?.actor) {
    exactKeys(projected, ["kind", "actor", "nextActor", "phase", "boardChanges", "ownCards", "revealedOpponentCards", "captures", "result"], "public transition");
    if (projected.kind !== "transition" || projected.actor !== actor || typeof projected.phase !== "string" || !Array.isArray(projected.boardChanges) || !Array.isArray(projected.ownCards) || !Array.isArray(projected.revealedOpponentCards)) throw new TypeError("Invalid public transition.");
    color(projected.actor); color(projected.nextActor);
    for (const change of projected.boardChanges) {
      exactKeys(change, ["square", "before", "after"], "board change");
      exactKeys(change.square, ["row", "col"], "public square");
      if (![change.square.row,change.square.col].every(value=>Number.isInteger(value)&&value>=0&&value<8)) throw new TypeError("Invalid public coordinate.");
      validatePublicPiece(change.before); validatePublicPiece(change.after);
    }
    projected.ownCards.forEach(validatePublicCard); projected.revealedOpponentCards.forEach(validatePublicCard);
    exactKeys(projected.captures, ["white", "black"], "public captures");
    for (const list of Object.values(projected.captures)) {
      if (!Array.isArray(list) || list.length > 12) throw new TypeError("Invalid public capture list.");
      for (const piece of list) {
        if (!piece || typeof piece !== "object" || Object.keys(piece).some(key=>!["type","color","logDir","windmillMode"].includes(key)) || !catalog.pieceTypes.includes(piece.type) || !["white","black","neutral"].includes(piece.color)) throw new TypeError("Invalid public capture fields.");
      }
    }
    validateResult(projected.result);
  }
function action(value, payload) {
  validatePosition(value);
  validatePayload(payload);
  const exact = jsonCopy(payload);
  return deepFreeze({ protocolVersion: VERSIONS.action, positionId: value.positionId, actionId: digest(exact), payload: exact });
}
function validatePayload(payload) {
  color(payload?.color);
  if (!catalog.actionTypes.includes(payload?.type)) throw new TypeError("Unknown action type.");
  const string = (value, field) => { if (typeof value !== "string" || !value) throw new TypeError(`Invalid ${field}.`); };
  const square = value => { if (!value || !Number.isInteger(value.row) || !Number.isInteger(value.col) || value.row < 0 || value.row > 7 || value.col < 0 || value.col > 7) throw new TypeError("Invalid 8x8 coordinate."); };
  if (["move", "promotion", "shotgunReload", "wizardSpell", "fileSurgeSkip"].includes(payload.type)) square(payload.from);
  if (payload.type === "move") square(payload.move);
  if (payload.type === "card") { string(payload.cardId, "cardId"); string(payload.cardInstanceId, "cardInstanceId"); if (payload.target !== undefined && payload.target !== null && (typeof payload.target !== "object" || Array.isArray(payload.target))) throw new TypeError("Invalid card target."); }
  if (payload.type === "wizardSpell") { if (!["meteor", "lightning", "shield", "timeStop"].includes(payload.spellId)) throw new TypeError("Invalid wizard spell."); square(payload.target); }
  if (payload.type === "promotionChoice") string(payload.promotionType, "promotionType");
  if (payload.type === "draftPick") string(payload.cardInstanceId, "cardInstanceId");
  if (payload.type === "draftBundlePick" && (!Number.isInteger(payload.bundleIndex) || payload.bundleIndex < 0 || payload.bundleIndex > 2 || !Array.isArray(payload.cardInstanceIds) || payload.cardInstanceIds.length !== 2 || payload.cardInstanceIds.some(value => typeof value !== "string" || !value))) throw new TypeError("Invalid chaos draft bundle.");
  if (payload.type === "trolleyChoice") { string(payload.windowId, "windowId"); if (![0, 1].includes(payload.doomedIndex)) throw new TypeError("Invalid trolley choice."); }
}
function validateAction(value, candidate) {
  validatePosition(value);
  if (!candidate || candidate.protocolVersion !== VERSIONS.action || candidate.positionId !== value.positionId) throw new TypeError("Stale or incompatible action.");
  exactKeys(candidate, ["protocolVersion", "positionId", "actionId", "payload"], "action");
  const expected = action(value, candidate.payload);
  if (expected.actionId !== candidate.actionId) throw new TypeError("Action identity mismatch.");
  return candidate;
}
function validateObservation(value) {
  if (!value || value.protocolVersion !== VERSIONS.observation) throw new TypeError("Observation version mismatch.");
  color(value.viewer); color(value.turn);
  const keys = ["protocolVersion", "viewer", "board", "turn", "ownCards", "opponentHandCount", "publicState", "history", "informationStateKey"];
  if (Object.keys(value).some(key => !keys.includes(key)) || keys.some(key => !Object.hasOwn(value, key))) throw new TypeError("Unexpected observation fields.");
  if (!Array.isArray(value.board) || value.board.length !== 8 || value.board.some(row => !Array.isArray(row) || row.length !== 8)) throw new TypeError("Observation board must be 8x8.");
  value.board.flat().forEach(validatePublicPiece);
  if (!Array.isArray(value.ownCards) || !Array.isArray(value.history) || !Number.isSafeInteger(value.opponentHandCount) || value.opponentHandCount < 0) throw new TypeError("Invalid observation card/history payload.");
  if (!value.publicState || typeof value.publicState !== "object" || Array.isArray(value.publicState)) throw new TypeError("Invalid public state.");
  const publicKeys = [...observationPolicy.statePublicFields, ...observationPolicy.derivedPublicFields];
  if (Object.keys(value.publicState).some(key => !publicKeys.includes(key))) throw new TypeError("Unknown public field requires an observation contract version update.");
  if (value.publicState.projectionVersion !== observationPolicy.projectionVersion || value.publicState.observationPolicyHash !== digest(observationPolicy)) throw new TypeError("Observation projection policy mismatch.");
  if (!Object.hasOwn(value.publicState, "deathmatchStatus")) throw new TypeError("Missing public deathmatch status.");
  const deathmatchErrors = validate(observationPolicy.deathmatchSchema, value.publicState.deathmatchStatus, "publicState.deathmatchStatus", RUNTIME_SCHEMA_NAME);
  if (deathmatchErrors.length) throw new TypeError(`Invalid public deathmatch status: ${deathmatchErrors.join("; ")}`);
  for(const key of observationPolicy.statePublicFields){
    if(!Object.hasOwn(value.publicState,key))continue;
    const shape=observationPolicy.stateValueSchemas?.[key];
    if(!shape)throw new TypeError(`Missing source public value schema ${key}.`);
    const errors=validate(shape,value.publicState[key],`publicState.${key}`,RUNTIME_SCHEMA_NAME);
    if(errors.length)throw new TypeError(`Invalid source public value: ${errors.join("; ")}`);
  }
  if(Object.hasOwn(value.publicState,"selectionPhase")){
    const errors=validate(observationPolicy.selectionSchema,value.publicState.selectionPhase,"selectionPhase",RUNTIME_SCHEMA_NAME);
    if(errors.length)throw new TypeError(`Invalid public selection surface: ${errors.join("; ")}`);
  }
  for (const name of ["boardMarks", "relationships", "overlays"]) {
    if (!Object.hasOwn(value.publicState, name)) throw new TypeError(`Missing public surface ${name}.`);
    surface(name, value.publicState[name]);
  }
  value.ownCards.forEach(validatePublicCard); value.history.forEach(event=>validatePublicTransition(event));
  if (value.publicState.revealedOpponentCards) value.publicState.revealedOpponentCards.forEach(validatePublicCard);
  const { informationStateKey, ...content } = value;
  if (informationStateKey !== digest(content)) throw new TypeError("Information state identity mismatch.");
  return value;
}
return Object.freeze({ baseline, catalog, observationPolicy, executionProfile, executionProfileSha256, ORACLE_PROFILE_VERSION, RUNTIME_SCHEMA_NAME, VERSIONS, canonical, summarizeStateFields, digest, jsonCopy, deepFreeze, rng, nextRandom, validateRng, position, validatePosition, action, validateAction, validatePayload, validateObservation, validateGameEvent, validateResult });
}
module.exports = Object.freeze({ ...createRuntimeContract(), createRuntimeContract });
