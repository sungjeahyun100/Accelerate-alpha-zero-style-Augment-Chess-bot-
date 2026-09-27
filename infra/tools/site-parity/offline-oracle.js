#!/usr/bin/env node
"use strict";
const { loadMain, cacheRoot, sha256 } = require("./frozen-site");
const contract = require("../../../bridge/tools/runtime-contract");
const readline = require("node:readline");
const { installEnumerationSource } = require("./client-enumeration");

// These hooks are presentation only. Game/draft transitions remain the site's code.
const PRESENTATION_HOOKS = ["renderAll", "setStatus", "toast", "playSound", "animateCard", "animateGrandDraftDeckInsertion", "scheduleGameOverReplayRecord", "renderRuleTicketPanel", "renderJokerChoicePanel"];
const BOOTSTRAP = `
aiSimulationDepth = 0;
${PRESENTATION_HOOKS.map(name => `${name}=()=>{};`).join("\n")}
captureDraftChoiceTransition=()=>({}); playDraftChoiceTransition=()=>[];
captureCardLaunch=()=>null; cardLaunchForDeckCard=()=>null;
chooseAiPromotion=()=>{};
isAiUsefulSpecialMove=()=>true;
function __encode(value) { return JSON.stringify(value,(_key,current)=>current instanceof Set?{__simType:"Set",values:[...current]}:current instanceof Map?{__simType:"Map",entries:[...current]}:current); }
function __decode(value) { return JSON.parse(JSON.stringify(value),(_key,current)=>current?.__simType==="Set"?new Set(current.values):current?.__simType==="Map"?new Map(current.entries):current); }
`;
const decisionActor = state => state.mode === "draft" && state.draft?.color || state.pendingPromotion?.color || state.activeTrolley?.color || state.turn;
function publicStateValue(value, field) {
  function inspect(current) {
    if (!current || typeof current !== "object") return;
    for (const [key, child] of Object.entries(current)) {
      if (/^(?:id|instanceId|hiddenFrom|seed|rng|rngState|positionId|positionKey|pawnId|kingId)$/.test(key) || /pieceId$/i.test(key)) throw new Error(`Nested visibility review required for ${field}.${key}.`);
      inspect(child);
    }
  }
  inspect(value); return contract.jsonCopy(value);
}

class OfflineOracle {
  constructor(root = cacheRoot(), { maxCandidates = 100000 } = {}) {
    if (!Number.isSafeInteger(maxCandidates) || maxCandidates < 1) throw new TypeError("maxCandidates must be positive.");
    this.main = loadMain(root);
    this.maxCandidates = maxCandidates;
    const file = this.main.manifest.files.find(file => /^main-/.test(file.name));
    const frozen = contract.catalog.source.files.find(source => source.name === file.name);
    if (!frozen || file.sha256 !== frozen.sha256) throw new Error("Oracle source is not the adopted frozen rules version.");
    this.main.evaluate(BOOTSTRAP);
    this.main.context.__maxCandidates = maxCandidates;
    this.main.evaluate(installEnumerationSource());
    const fixedTime = Date.parse(this.main.manifest.frozenAt);
    this.main.context.__fixedTime = fixedTime;
    this.main.evaluate("const __NativeDate = Date; Date = class extends __NativeDate { constructor(...args){super(...(args.length?args:[__fixedTime]));} static now(){return __fixedTime;} };");
    this.random = contract.rng(0);
    this.main.context.__random = () => { const next = contract.nextRandom(this.random); this.random = next.rng; return next.value; };
    this.main.evaluate("Math.random=__random;");
  }
  evaluate(source) { return this.main.evaluate(source, 15000); }
  state() { return JSON.parse(this.evaluate("__encode(state)")); }
  restore(position) {
    contract.validatePosition(position);
    this.random = contract.jsonCopy(position.rng);
    this.main.context.__position = position.state;
    this.evaluate("state=__decode(__position);selectedGameStyle=state.gameStyle||'normal';localPlayMode='local';playMode='local';");
  }
  snapshot(history = []) { return contract.position(this.state(), this.random, history); }
  newGame(config = {}, seed = 0, tape = []) {
    const allowed = ["gameStyle", "draftDelete", "ruleCardIds", "starWinLimit", "deathmatchEnabled", "deathmatchLimitTurns"];
    if (Object.keys(config).some(key => !allowed.includes(key))) throw new TypeError("Unknown game configuration field.");
    const style = config.gameStyle || "normal";
    if (!["normal", "chaos", "grand"].includes(style)) throw new TypeError("Only normal, chaos and grand 8x8 are supported.");
    if (config.ruleCardIds && (!Array.isArray(config.ruleCardIds) || config.ruleCardIds.some(id => !contract.catalog.cards.some(card => card.id === id && card.draftCategory === "RULE")))) throw new TypeError("Unknown RULE card.");
    this.random = contract.rng(seed, tape);
    this.main.context.__config = { ...config, gameStyle: style };
    for (const key of ["draftDelete", "deathmatchEnabled"]) if (config[key] !== undefined && typeof config[key] !== "boolean") throw new TypeError(`Invalid ${key}.`);
    for (const key of ["starWinLimit", "deathmatchLimitTurns"]) if (config[key] !== undefined && (!Number.isSafeInteger(config[key]) || config[key] < 1)) throw new TypeError(`Invalid ${key}.`);
    this.evaluate("selectedGameStyle=__config.gameStyle; localPlayMode='local'; playMode='local'; resetGame(false,[]); state.draftDelete=__config.draftDelete===true; if(__config.deathmatchEnabled!==undefined)state.deathmatchEnabled=__config.deathmatchEnabled; if(__config.deathmatchLimitTurns!==undefined)state.deathmatchLimitTurns=__config.deathmatchLimitTurns; if(__config.starWinLimit!==undefined)state.starWinLimit=__config.starWinLimit; if(__config.ruleCardIds?.length){state.ruleOpeningEnabled=true;state.ruleSelectionEnabled=true;state.selectedRuleCardIds=[...__config.ruleCardIds];maybeApplyOpeningRuleEvent();finishRuleOpeningEvent();}beginInitialGameFlow();");
    return this.snapshot();
  }
  result(position) {
    contract.validatePosition(position);
    const state = position.state;
    const terminal = state.mode === "gameover";
    const value = { protocolVersion: contract.VERSIONS.result, status: terminal ? "terminal" : "ongoing", winner: terminal && ["white", "black"].includes(state.winner) ? state.winner : null, outcome: terminal ? state.winner || "draw" : null, reason: terminal ? state.replayEndReason || "" : "" };
    contract.validateResult(value); return value;
  }
  observe(position, viewer) {
    if (!["white", "black"].includes(viewer)) throw new TypeError("Invalid observation viewer.");
    this.restore(position);
    const state = position.state;
    const policy = contract.observationPolicy;
    const classified = new Set([...policy.statePublicFields, ...Object.values(policy.stateFieldClassification).flat()]);
    const unknown = Object.keys(state).filter(key => !classified.has(key));
    if (unknown.length) throw new Error(`Unclassified site state fields require a visibility review: ${unknown.join(", ")}`);
    this.main.context.__viewer = viewer;
    this.evaluate("localViewColor=()=>__viewer; boardViewColor=()=>__viewer;");
    const projected = JSON.parse(this.evaluate("JSON.stringify(state.board.map((row,r)=>row.map((cell,c)=>cell&&pieceVisibleToColorAt(cell,r,c,__viewer,state.board)?{...cell,type:visiblePieceType(cell)}:null)))"));
    const board = projected.map(row => row.map(cell => cell && Object.fromEntries(Object.entries(cell).filter(([key]) => policy.piecePublicFields.includes(key)))));
    const cardView = (card, slot) => ({ ...Object.fromEntries(Object.entries(card).filter(([key]) => policy.cardPublicFields.includes(key))), ...(slot === undefined ? {} : { slot }) });
    const opponent = viewer === "white" ? "black" : "white";
    const ownCards = (state.deckSlots?.[viewer] || []).map((card, slot) => card && cardView(card, slot)).filter(Boolean);
    const other = (state.deckSlots?.[opponent] || []).filter(Boolean);
    const publicState = Object.fromEntries(policy.statePublicFields.filter(key => Object.hasOwn(state, key)).map(key => [key, publicStateValue(state[key], key)]));
    publicState.phase = this.evaluate("getPhase()");
    publicState.revealedOpponentCards = (state.deckSlots?.[opponent] || []).map((card, slot) => card && cardView(card, slot)).filter(Boolean);
    publicState.ownStarTotal = this.evaluate(`deckStarTotal(${JSON.stringify(viewer)})`);
    publicState.opponentStarTotal = this.evaluate(`deckStarTotal(${JSON.stringify(opponent)})`);
    publicState.ruleCardIds = [state.appliedRuleCard?.id, ...(state.additionalRuleCards || []).map(card => card.id)].filter(Boolean);
    publicState.rulesVersion = position.rulesVersion;
    publicState.catalogVersion = position.catalogVersion;
    publicState.captures = Object.fromEntries(Object.entries(state.captures || {}).map(([color, cells]) => [color, cells.slice(-12).map(cell => typeof cell === "string" ? { type: cell } : Object.fromEntries(Object.entries(cell).filter(([key]) => ["type", "color", "logDir", "windmillMode"].includes(key))))]));
    publicState.clock = state.clock && Object.fromEntries(Object.entries(state.clock).filter(([key]) => ["enabled", "initialMs", "incrementMs", "whiteMs", "blackMs", "runningColor", "timeoutWinner", "timeoutLoser"].includes(key)));
    publicState.lastMove = state.lastMove && state.lastMove.hiddenFrom !== viewer ? Object.fromEntries(Object.entries(state.lastMove).filter(([key]) => ["from", "to", "color", "kind"].includes(key)).map(([key,value])=>[key,["from","to"].includes(key)?{row:value.row,col:value.col}:value])) : null;
    publicState.selectionPhase = state.pendingPromotion ? { kind: "promotion", color: state.pendingPromotion.color, row: state.pendingPromotion.row ?? null, col: state.pendingPromotion.col ?? null, choices: viewer === state.pendingPromotion.color ? state.pendingPromotion.choices || [] : [] } : state.activeTrolley ? { kind: "trolley", color: state.activeTrolley.color, windowId: state.activeTrolley.id, choices: JSON.parse(this.evaluate("JSON.stringify(state.activeTrolley.choices.map(choice=>(choice.pieces||[]).map(cell=>({type:cell.type||cell.piece?.type||null,color:cell.color||cell.piece?.color||null}))))")) } : null;
    publicState.legalHints = this.publicHints(position, viewer);
    publicState.tabooPending = (state.tabooPending || []).map(entry => ({ color: entry.color, square: contract.jsonCopy(entry.square) }));
    publicState.ownPlans = (state.pendingFreeMoves || []).filter(entry => entry.color === viewer).map(entry => ({ kind: "premove", triggerColor: entry.triggerColor, triggerTurn: entry.triggerTurn, moves: entry.moves.map(move => ({ from: contract.jsonCopy(move.from), to: contract.jsonCopy(move.to) })) }));
    if (state.mode === "draft" && (state.draft?.color === viewer || state.draft?.kind === "grand")) publicState.draft = { kind: state.draft.kind || state.gameStyle, phase: state.draft.phase, color: state.draft.color, choices: (state.draft.choices || []).map(card=>cardView(card)) };
    const history = position.history.map(entry => entry.public[viewer]);
    const observation = { protocolVersion: contract.VERSIONS.observation, viewer, board, turn: state.turn, ownCards, opponentHandCount: other.length, publicState, history };
    observation.informationStateKey = contract.digest(observation);
    contract.validateObservation(observation);
    return contract.deepFreeze(observation);
  }
  publicHints(position, viewer) {
    this.restore(position);
    this.main.context.__viewer = contract.jsonCopy(viewer);
    this.evaluate("localViewColor=()=>__viewer;boardViewColor=()=>__viewer;");
    if (position.state.mode !== "play" || position.state.turn !== viewer || position.state.pendingPromotion || position.state.activeTrolley) return { moves: [], cardTargets: [] };
    // These are the cells the client highlights after selecting a visible own
    // piece/card, not worker actions. Capture flags, IDs and private plans never leave.
    return JSON.parse(this.evaluate(`JSON.stringify((()=>{const moves=[],cardTargets=[];for(let row=0;row<8;row++)for(let col=0;col<8;col++){const piece=state.board[row]?.[col];if(!piece||piece.color!==__viewer||!pieceVisibleToColorAt(piece,row,col,__viewer,state.board))continue;const keys=[...new Set(getLegalMoves(row,col).flatMap(moveHighlightKeys))];if(keys.length)moves.push({from:{row,col},destinations:keys.map(key=>{const [row,col]=key.split('-').map(Number);return {row,col};})});}for(const card of getVisibleCards(__viewer)){if(card.used||card.recovering||!card.target)continue;const targets=getVisibleCardTargetSquares(card);cardTargets.push({cardInstanceId:card.instanceId,targets});}return {moves,cardTargets};})())`));
  }
  candidates(position) {
    this.restore(position);
    const state = position.state;
    if (state.mode === "gameover") return [];
    if (state.mode === "draft") {
      const raw = this.evaluate("isGrandDraftState()?grandAvailableCardsForColor(state.draft.color).map(c=>({type:'draftPick',color:state.draft.color,cardInstanceId:c.instanceId})):isChaosDraftState()?chaosDraftBundles().map(b=>({type:'draftBundlePick',color:state.draft.color,bundleIndex:b.index,cardInstanceIds:b.cards.map(c=>c.instanceId)})):(state.draft.choices||[]).map(c=>({type:'draftPick',color:state.draft.color,cardInstanceId:c.instanceId}))");
      return JSON.parse(JSON.stringify(raw));
    }
    if (state.pendingPromotion) {
      return JSON.parse(this.evaluate("JSON.stringify((state.pendingPromotion.choices||promotionChoicesFor(state.board[state.pendingPromotion.row]?.[state.pendingPromotion.col],state.pendingPromotion.row)).map(choice=>({type:'promotionChoice',color:state.pendingPromotion.color,promotionType:typeof choice==='string'?choice:choice.type})))"));
    }
    if (state.activeTrolley) return (state.activeTrolley.choices || []).map((_choice, doomedIndex) => ({ type: "trolleyChoice", color: state.activeTrolley.color, windowId: state.activeTrolley.id, doomedIndex }));
    if (state.ruleTicketChoice || state.jokerChoice || state.barricadeDirectionChoice || state.targeting) throw new Error("UI presentation choices must be normalized to the atomic card target before snapshot import.");
    const raw = JSON.parse(this.evaluate("JSON.stringify(collectValidAiActions(state.turn,{includeCards:true,exhaustiveCards:true}))"));
    if (raw.length > this.maxCandidates) throw new Error("Legal action enumeration exceeded its explicit candidate budget.");
    return raw;
  }
  actions(position) {
    const raw = this.candidates(position).map(payload => contract.action(position, payload));
    if (position.state.mode === "draft" || position.state.pendingPromotion || position.state.activeTrolley) return raw;
    // Targets and spell surfaces are complete candidates; the client's actual
    // transition code removes rejected selections without mutating the original.
    return raw.filter(candidate => this.apply(position, candidate, { recordHistory: false }).ok);
  }
  apply(position, candidate, { recordHistory = true } = {}) {
    contract.validateAction(position, candidate);
    this.restore(position);
    const payload = candidate.payload;
    const actor = decisionActor(position.state);
    if (payload.color !== actor) return this.rejected(position, "Wrong acting player.");
    this.main.context.__action = payload;
    let result;
    if (["draftPick", "draftBundlePick"].includes(payload.type)) {
      const allowed = this.candidates(position).some(action => contract.canonical(action) === contract.canonical(payload));
      if (!allowed) return this.rejected(position, "Draft choice is not in the current offer.");
      this.main.context.__action = payload;
      result = this.evaluate("(()=>{const color=state.draft.color,phase=state.draft.phase;const grand=isGrandDraftState();let ok;if(__action.type==='draftBundlePick')ok=finishChaosDraftBundle({index:__action.bundleIndex},{auto:true});else ok=finishDraft(state.draft.choices.find(c=>c.instanceId===__action.cardInstanceId),null,{auto:true});if(ok){if(grand)completeGrandDraftStep(color,state.draft.pickIndex);else completeDraftStep(color,phase);}return {ok};})()");
    } else if (payload.type === "promotionChoice") {
      const allowed = this.candidates(position).some(action => contract.canonical(action) === contract.canonical(payload));
      if (!allowed) return this.rejected(position, "Invalid promotion choice.");
      result = this.evaluate("choosePromotion(__action.promotionType);({ok:!state.pendingPromotion})");
    } else if (payload.type === "trolleyChoice") {
      if (!position.state.activeTrolley || position.state.activeTrolley.id !== payload.windowId || ![0, 1].includes(payload.doomedIndex)) return this.rejected(position, "Invalid trolley window response.");
      result = this.evaluate("resolveTrolleyBundle(__action.doomedIndex);({ok:!state.activeTrolley})");
    } else {
      if (position.state.mode !== "play") return this.rejected(position, "Actions are only accepted during play.");
      if (payload.type === "card") {
        const card = (position.state.deckSlots?.[payload.color] || []).find(card => card?.instanceId === payload.cardInstanceId);
        if (!card || card.id !== payload.cardId || card.used || card.recovering) return this.rejected(position, "The exact card instance is unavailable.");
      }
      result = this.evaluate("applyAiAction(__action)");
    }
    if (!result?.ok) return this.rejected(position, result?.message || "Site rejected action.");
    const current = this.snapshot();
    if (!recordHistory) return { protocolVersion: contract.VERSIONS.step, ok: true, position: current, result: this.result(current) };
    const events = ["white", "black"].map(viewer => {
      const before = this.observe(position, viewer), after = this.observe(current, viewer);
      const boardChanges = [];
      for (let row = 0; row < 8; row++) for (let col = 0; col < 8; col++) if (contract.canonical(before.board[row][col]) !== contract.canonical(after.board[row][col])) boardChanges.push({ square: { row, col }, before: before.board[row][col], after: after.board[row][col] });
      return { viewer, event: { kind: "transition", actor: payload.color, nextActor: decisionActor(current.state), phase: current.state.mode, boardChanges, ownCards: after.ownCards, revealedOpponentCards: after.publicState.revealedOpponentCards, captures: after.publicState.captures, result: this.result(current) } };
    });
    const event = { protocolVersion: "accelerate-game-event-v1", actor: payload.color, action: contract.jsonCopy(payload), turnChanged: position.state.turn !== current.state.turn, public: Object.fromEntries(events.map(entry => [entry.viewer, entry.event])) };
    const next = contract.position(current.state, current.rng, [...position.history, event]);
    return { protocolVersion: contract.VERSIONS.step, ok: true, position: next, result: this.result(next), event: { actionId: candidate.actionId, actor: payload.color, nextActor: decisionActor(next.state) } };
  }
  rejected(position, message) { return { protocolVersion: contract.VERSIONS.step, ok: false, position, result: this.result(position), error: { code: "ACTION_REJECTED", message } }; }
}
module.exports = { OfflineOracle, PRESENTATION_HOOKS, decisionActor };
if (require.main === module) {
  const root = process.argv[2] || cacheRoot();
  let oracle;
  try { oracle = new OfflineOracle(root); } catch (error) { console.error(error.stack); process.exit(2); }
  const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  input.on("line", line => {
    try {
      const request = JSON.parse(line);
      const handlers = { new_game: () => oracle.newGame(request.config, request.seed, request.tape), get_legal_actions: () => oracle.actions(request.position), get_public_hints: () => oracle.publicHints(request.position, request.viewer), apply_action: () => oracle.apply(request.position, request.action), get_result: () => oracle.result(request.position), observe: () => oracle.observe(request.position, request.viewer) };
      if (!handlers[request.command]) throw new TypeError("Unknown oracle command.");
      process.stdout.write(JSON.stringify({ id: request.id, value: handlers[request.command]() }) + "\n");
    } catch (error) { process.stdout.write(JSON.stringify({ error: { code: "ORACLE_ERROR", message: error.message } }) + "\n"); }
  });
}
