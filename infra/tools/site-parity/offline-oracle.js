#!/usr/bin/env node
"use strict";
const { loadMain, cacheRoot, sha256 } = require("./frozen-site");
const contract = require("../../../bridge/tools/runtime-contract");
const readline = require("node:readline");
const { installEnumerationSource } = require("./client-enumeration");

// DOM absence is an explicit execution profile. Some source render hooks perform
// rule cleanup, and queued replay settlement consumes the same source RNG stream.
// This profile cannot establish future RNG equality with a populated browser DOM.
const HEADLESS_PROFILE = Object.freeze({
  version: contract.ORACLE_PROFILE_VERSION,
  rendererContext: "cold-activePieceAnimationUntil-at-admission",
  retained: Object.freeze(["source-transitions", "draft-availability-predicates", "potion-cleanup", "terminal-rule-ticket-cleanup", "queued-replay-settlement", "history-notation-rng"]),
  excluded: Object.freeze(["dom-card-animation-rng", "dom-update-log-rng", "dom-render-probes-and-ui-state", "network-persistence", "editor-ui", "timers"]),
  queryRng: "restored-position-probe-only",
  decisionMode: "local-explicit-decisions",
  passiveSimulationDepth: 0,
  browserFutureRngEquality: false,
});
const PRESENTATION_HOOKS = ["setStatus", "toast", "playSound", "renderJokerChoicePanel", "renderHistoryControls"];
const BOOTSTRAP = `
aiSimulationDepth = 0;
${PRESENTATION_HOOKS.map(name => `${name}=()=>{};`).join("\n")}
const __sourceRenderAll=renderAll, __sourceRuleTicketPanel=renderRuleTicketPanel;
renderRuleTicketPanel=()=>{if(state?.mode==="gameover")__sourceRuleTicketPanel();};
renderAll=()=>{
  if(state?.simpleBoardEditor?.enabled)throw new Error("Headless editor UI is unsupported.");
  const previousDepth=aiSimulationDepth;
  try { aiSimulationDepth=Math.max(1,previousDepth); __sourceRenderAll(); }
  finally { aiSimulationDepth=previousDepth; }
  if(previousDepth===0)renderRuleTicketPanel();
};
// Launch capture is absent: the source animation returns null before its DOM
// branch's random draw. Grand insertion still runs its source null-ghost reveal.
animateCard=()=>null;
captureDraftChoiceTransition=()=>({}); playDraftChoiceTransition=()=>[];
captureCardLaunch=()=>null; cardLaunchForDeckCard=()=>null;
chooseAiPromotion=()=>{};
// Explicit promotionChoice is a separate decision; the local oracle does not
// run the site's AI strategy or its usefulness pruning of legal candidates.
isAiUsefulSpecialMove=()=>true;
const __microtasks=[];
queueMicrotask=callback=>{
  if(typeof callback!=="function")throw new TypeError("Invalid microtask callback.");
  if(__microtasks.length>=__maxMicrotasks)throw new Error("Headless microtask budget exceeded.");
  __microtasks.push(callback);
};
function __settleMicrotasks(){
  let executed=0;
  while(__microtasks.length){
    if(++executed>__maxMicrotasks)throw new Error("Headless microtask budget exceeded.");
    __microtasks.shift()();
  }
  return executed;
}
function __encode(value) { return JSON.stringify(value,(_key,current)=>current instanceof Set?{__simType:"Set",values:[...current]}:current instanceof Map?{__simType:"Map",entries:[...current]}:current); }
function __decode(value) { return JSON.parse(JSON.stringify(value),(_key,current)=>current?.__simType==="Set"?new Set(current.values):current?.__simType==="Map"?new Map(current.entries):current); }
function __relinkSnapshotBoards(value) {
  // The client's relinkBoardPieceReferences62055 creates one ID map per
  // board. History/replay frames are independent snapshots (61255, 88741),
  // so a piece with the same ID in two frames must not share live attributes.
  relinkBoardPieceReferences(value.board);
  for (const frame of value.boardHistory || []) if (frame?.board) relinkBoardPieceReferences(frame.board);
  for (const frame of [value.replayBaseFrame, value.replayTailFrame]) if (frame?.board) relinkBoardPieceReferences(frame.board);
}
function __playerTricksterMovement(item, viewer) {
  // The online renderer uses playerColor rather than physical board turn.
  // Observe(viewer) has that same per-player ownership boundary. This read
  // probe restores the UI context before any rule function can run.
  if (state.mode === "gameover") return tricksterMovementBadgeType(item, false);
  const saved = [online.enabled, online.role, online.playerColor];
  try { online.enabled=true;online.role="player";online.playerColor=viewer;return tricksterMovementBadgeType(item,false); }
  finally { [online.enabled,online.role,online.playerColor]=saved; }
}
function __publicPieceStatus(item, row, col, viewer) {
  const status = {}, flag=(name, active)=>{if(active)status[name]=true;}, count=(name,value)=>{if(value!==""&&value!==null&&value!==undefined)status[name]=Number(value);};
  // All these names occur in createPieceElement75203..75767 or its two
  // status appenders20951/4178. Nested rule objects never leave this view.
  for(const name of ["chameleon","chimera","staked","explosive","brutalKnight","bribed","witchTrial","disarmed","severed","inertia","frenzy","crownBearer","iceSheet","lastResistance","sacrificeProtection","necromancy","regencyHeir","bloodCurse","poisonedPawn","evasion","ghost","metalized","loyalist","parry","emptyLunchbox","nullification","recurrence","wanted","grapplerBound","callingCard","basicTraining"])flag(name,item[name]);
  flag("protected",pieceProtectionActive(item));flag("holdout",item.type==="pawn"&&item.holdoutPromotion);
  flag("cooling",shouldShowMannerCaptureLockVisual(item));flag("exhaustionLocked",isExhaustionMoveBlocked(state.exhaustion,item));
  flag("diceLocked",isDiceLocked(item,state));flag("quantum",item.quantum);flag("quantumShadow",item.isQuantumShadow);
  flag("poisonStunned",isPoisonStunned(item));flag("twinsLinked",item.twinBondId);
  flag("regencyRoyal",item.regencyHeir&&isRegencyRoyalHeir(item,state));
  flag("stealthed",pieceHiddenFromAt(item,row,col,state.board)&&(isFullRecordReveal()||item.color===viewer));
  flag("magicGirlAwakened",item.type==="magicGirl"&&(typeof item.simpleEditorMagicGirlAwakened==="boolean"?item.simpleEditorMagicGirlAwakened:state.magicGirlSurge?.[item.color]));
  const saturationActive=Boolean(state.saturationRule||item.potionSaturation), saturation=saturationActive?saturationCaptureCount(item,SATURATION_CAPTURE_LIMIT):0;
  flag("saturationLocked",saturation>=SATURATION_CAPTURE_LIMIT);if(saturation>0)count("saturationCaptures",saturation);
  const restriction=item.captureRestriction||(item.type==="trickster"||shouldShowHallucinatedQueen(item))&&(isGuardLikePiece(item,state)?"immune":pieceHasAbility(item,"jester")?"royal-only":null);
  if(restriction)status.captureRestriction=restriction;
  flag("sirenWarning",Number.isInteger(row)&&Number.isInteger(col)&&sirenConversionWarningActive(state.sirenExposure,item.id));
  const promotion=fieldPromotionCaptureBadgeValue(item);if(promotion){const ready=Math.max(0,Number(item.totalCaptures)||0)>=2;flag("fieldPromotionReady",ready);count("fieldPromotionCapturesRemaining",ready?0:1);}
  if(item.type==="bishop"&&typeof state.bishopInfiltration?.[item.color]==="number"&&state.bishopInfiltration[item.color]>0)count("ghillieRemaining",state.bishopInfiltration[item.color]);
  if(pieceHasAbility(item,"reaper"))count("reaperCaptures",Math.max(0,Math.min(REAPER_CAPTURE_TARGET,Math.floor(Number(item.reaperCaptures)||0))));
  const resurrection=undeadResurrectionBadgeValueFor(item,{row,col,board:state.board});if(resurrection)count("undeadResurrectionRemaining",resurrection);
  if(item.type==="pawn"&&item.vipInvitation)count("vipRemaining",vipInvitationRemaining(item));
  if(item.type==="babyBear"&&(item.babyBearGrowAtTurn||item.babyBearGrowAtMove))count("babyBearGrowthRemaining",babyBearGrowthRemaining(item));
  else if(septemberCounterLimit(item.type)>0&&(Number(item.bearRetaliationsRemaining)||0)>0)count("retaliationsRemaining",Math.max(1,Math.min(septemberCounterLimit(item.type),Number(item.bearRetaliationsRemaining)||0)));
  if(item.crownBearer){const crown=normalizeCrownRule(state.crownRule,state.board),ids=Array.isArray(item.crownTokenIds)?item.crownTokenIds:[];count("crownHeldMoves",Math.max(0,...crownRuleEntries(crown).filter(entry=>entry.holderId&&entry.holderId===item.id||ids.includes(entry.id)).map(entry=>Number(entry.heldMoves?.[item.color])||0)));}
  if(item.metalized&&(Number(item.metalCooldown)||0)>0)count("metalCooldown",Math.max(0,Number(item.metalCooldown)||0));
  if(item.emptyLunchbox)count("emptyLunchboxRemaining",Math.max(0,Math.min(9,(Number(item.emptyLunchbox.deadlineTurn)||0)-(Number(state.turnsTaken?.[item.color])||0))));
  if(isPoisonStunned(item))count("poisonStunRemaining",Math.max(1,Math.min(9,Number(item.poisonStunTurns)||1)));
  const frozen=Math.max(0,Math.min(9,Number(item.frozenByCard?.remaining)||0));if(frozen>0)count("frozenRemaining",frozen);
  // Holdout/stake text is capped, but their accessible labels expose the
  // complete remaining count; keep that public information (75543/75550).
  for(const [name,value,max]of [["holdoutRemaining",holdoutRemaining(item),Number.MAX_SAFE_INTEGER],["stakedRemaining",stakedRemaining(item),Number.MAX_SAFE_INTEGER],["severanceRemaining",severanceRemaining(item),9],["iceSheetKingRemaining",iceSheetKingBadgeValue(item),9],["lastResistanceRemaining",lastResistanceRemaining(item),9]])if(value)count(name,Math.min(max,Math.max(1,value)));
  if(item.witchTrial)count("witchTrialRemaining",Math.max(1,Math.min(9,Number(item.witchTrial.remaining)||1)));
  const protection=Math.max(0,Number(item.sacrificeProtection?.remaining)||0);if(protection)count("sacrificeProtectionRemaining",Math.max(1,Math.min(9,protection)));
  for(const [name,value]of [["pawnReverseRemaining",pawnReverseBadgeValueFor(item)],["royalCommandRemaining",royalCommandBadgeValueFor(item)],["ultimatumRemaining",ultimatumBadgeValueFor(item)],["prophecyRemaining",prophecyBadgeValueFor(item)],["armisticeRemaining",armisticeBadgeValueFor(item)],["galeRemaining",galeBadgeValueFor(item)]])if(value)count(name,value);
  const guardian=feudalGuardianType(item);if(guardian)status.feudalGuardianType=guardian;
  const trickster=__playerTricksterMovement(item,viewer);if(trickster)status.tricksterMovement=trickster;
  const horse=horseRidingBadgeType(item);if(horse)status.horseRiding=horse;
  const imperial=imperialStudyMoveTypes(item);if(imperial.length)status.imperialStudyTypes=imperial;
  if(item.type==="merchant")count("gold",item.gold??0);if(item.bribedRemaining)count("bribedRemaining",item.bribedRemaining);
  const necromancy=Math.max(0,Math.min(9,Number(item.necromancyRemaining||item.necromancy?.remaining)||0));if(necromancy)count("necromancyRemaining",necromancy);
  if((item.type!=="trickster"||trickster)&&(pieceHasAbility(item,"medium")||pieceHasAbility(item,"parrot"))){const medium=pieceHasAbility(item,"medium"),memory=medium?state.mediumMovement:state.parrotMovement?.[item.color],type=memory?.type?.replace(/-([a-z])/g,(_,c)=>c.toUpperCase())||"";if(type)status[medium?"mediumMovement":"parrotMovement"]=type==="windmill"?memory.windmillMode==="rook"?"windmillRook":"windmillBishop":type;}
  flag("locustReady",state.locustSwarm?.[item.color]&&locustReady(item,row,col,state));
  if(isFullRecordReveal()){flag("spy",COLORS.includes(item.spyOwner));flag("trojanHorse",item.trojanHorse);}
  // The optional info preference does not change available knowledge: the
  // viewer can enable it. Hallucination really suppresses that help panel.
  const potions=!isPieceInfoSuppressedByHallucination()?activePotionEffects(item):[];if(potions.length)status.potionEffects=potions;
  return status;
}
function __publicPieceView(item, row, col, viewer) {
  if(!item||!pieceVisibleToColorAt(item,row,col,viewer,state.board))return null;
  return __publicPieceContent(item,row,col,viewer);
}
function __publicPieceContent(item,row,col,viewer) {
  item={...item,type:visualPieceType(item.type)};
  const type=visiblePieceType(item),own=item.color===viewer,view={type,color:item.color,status:__publicPieceStatus(item,row,col,viewer)};
  if(own||isFullRecordReveal())view.moved=Boolean(item.moved);
  for(const name of ["shielded","frozen","submerged"])if(Object.hasOwn(item,name))view[name]=Boolean(item[name]);
  if(isHpPiece(item)){const max=Math.max(1,Number(item.maxHp)||Number(item.hp)||1);view.maxHp=max;view.hp=Math.max(0,Math.min(max,Number.isFinite(Number(item.hp))?Number(item.hp):max));}
  if(pieceHasAbility(item,"wizard")&&Number.isFinite(Number(item.mana))&&(item.type!=="trickster"||view.status.tricksterMovement)){view.mana=Number(item.mana);if(own)view.maxMana=Number(item.maxMana??5);}
  if(item.type==="shotgunKing"){view.facing=item.facing||defaultFacing(item.color);if(own){view.ammo=Number(item.ammo??0);view.maxAmmo=Number(item.maxAmmo??3);}}
  if(isLargePiece(item))for(const name of ["anchorRow","anchorCol"])if(Number.isInteger(item[name]))view[name]=item[name];
  if(item.type==="log"&&item.logDir)view.logDir={dr:item.logDir.dr,dc:item.logDir.dc};
  if(item.type==="windmill")view.windmillMode=item.windmillMode==="rook"?"rook":"bishop";
  return view;
}
function __publicBoardSurface(viewer) {
  // Mirror renderBoard72823 and renderChainBondOverlay73220. A terrain or
  // forecast marker may be visible on a fogged/occupied hidden square even
  // when its occupant is not; preserve the renderer's individual gates.
  const board=state.board,marks=[],relationships=[],overlays=[],fog=madAiFogVisibleSquares(board,false),key=squareKey;
  const add=(kind,row,col,extra={})=>marks.push({kind,square:{row,col},...extra});
  const black=new Set(activeBlackHoleCells(false).map(key)),bombs=new Set(activeRuleBombs().map(key)),portals=new Set((activePortalRule(board)?.cells||[]).map(key)),platforms=new Set(activePlatformCells(state.platformRule).map(key));
  const scarecrows=new Map((state.pendingScarecrows||[]).flatMap(entry=>{if(!entry.pieceId)return [[key(entry),entry]];const found=findPieceOnBoardById(board,entry.pieceId);return found?[[key(found),{...entry,row:found.row,col:found.col}]]:[];}));
  const lobsters=new Map((state.pendingLobsters||[]).map(entry=>[key(entry),entry])),pendingPortals=new Set((state.pendingPortals||[]).flatMap(entry=>(entry.cells||[]).map(key)));
  const otherworld=new Map((state.pendingOtherworld||[]).filter(entry=>Number.isInteger(Number(entry?.row))&&Number.isInteger(Number(entry?.col))).map(entry=>[key({row:Number(entry.row),col:Number(entry.col)}),entry]));
  const victory=new Map((state.gomokuVictoryCells||[]).map((cell,index)=>[key(cell),index])),trail=new Set(state.accelerationTrail?.hiddenFrom!==viewer?(state.accelerationTrail?.cells||[]).map(key):[]);
  for(let row=0;row<8;row++)for(let col=0;col<8;col++){
    const square={row,col},id=key(square),occupant=board[row][col],fogged=Boolean(fog&&!fog.has(id)),hidden=Boolean(occupant&&(fogged||isHiddenFromCurrentTurn(occupant,row,col,board)));
    for(const [kind,set]of [["blackHole",black],["ruleBomb",bombs],["portal",portals],["platform",platforms],["accelerationTrail",trail]])if(set.has(id))add(kind,row,col);
    if(isPalaceSquare(row,col))add("palace",row,col);if(isActiveCrownGround(row,col,board))add("crownGround",row,col);
    const hazard=hazardClass(row,col);if(hazard)add(hazard.split(" ").at(-1)==="lightning"?"lightning":"meteor",row,col);
    if(state.conveyorRule){const direction=conveyorDirectionAt(row,col,board);if(direction)add("conveyor",row,col,{direction});}
    if(fogged)add("fogHidden",row,col);if(occupant&&isEncouraged(occupant,row,col))add("encouraged",row,col);
    if(occupant?.type==="pawn"&&state.resolveReady?.[occupant.color])add("resolveReady",row,col);
    if(occupant&&!hidden&&state.winterKingdom?.previewIds?.includes(occupant.id))add("winterForecast",row,col);
    if(state.platformRule?.previewCell?.row===row&&state.platformRule.previewCell.col===col)add("platformForecast",row,col);
    if(victory.has(id))add("gomokuVictory",row,col,{index:victory.get(id)});
    for(const owner of COLORS){const flag=state.captureTheFlag?.flags?.[owner];if(flag?.row===row&&flag.col===col)add("captureFlag",row,col,{owner});}
    const scarecrow=scarecrows.get(id),lobster=lobsters.get(id),returning=otherworld.get(id);
    if(scarecrow&&(!scarecrow.pieceId||occupant?.scarecrowReserved))add("scarecrowReserved",row,col);
    if(scarecrow&&!fogged&&(scarecrow.pieceId?occupant&&!hidden:!occupant||hidden))add("scarecrowPreview",row,col,{owner:scarecrow.color==="black"?"black":"white",remaining:pendingScarecrowTurns(scarecrow)});
    if(lobster){add("lobsterReserved",row,col);if(!occupant)add("lobsterPreview",row,col,{owner:lobster.color==="black"?"black":"white",remaining:pendingLobsterTurns(lobster,state.moveCount)});}
    if(pendingPortals.has(id)){add("portalReserved",row,col);if(!occupant)add("portalPreview",row,col);}
    if(returning&&(!fogged||returning.color===viewer))add("otherworldOrigin",row,col,{remaining:Math.max(0,Math.ceil(undeadResurrectionRemainingHalfMoves(returning,state.moveCount)/2))});
    if(!fogged){for(const owner of COLORS){const d=expansionNamedSquare("d",owner,8,8),e=expansionNamedSquare("e",owner,8,8);if(state.d4?.[owner]&&d?.row===row&&d.col===col)add("d4Forbidden",row,col,{owner});if(state.e4?.[owner]&&e?.row===row&&e.col===col)add("e4Destination",row,col,{owner});}if((state.tabooPending||[]).some(entry=>entry.square?.row===row&&entry.square.col===col))add("taboo",row,col);for(const rotation of revolvingDoorMarkersAt(board,row,col,false))add("revolvingDoor",row,col,{rotation});}
  }
  const quantumSeen=new Set();
  forEachBoardSquare(board,(item,row,col)=>{
    if(!item)return;
    const visible=!isHiddenFromCurrentTurn(item,row,col,board)&&(!fog||pieceVisibleToColorAt(item,row,col,viewer,board,fog));
    for(const ability of ["knightmaster","clockwork","paladin","idol","siren","reaper"]){
      if(!pieceHasAbility(item,ability)||ability!=="siren"&&!visible||ability==="paladin"&&!usesSeptember18Balance(state)&&!lightSquare(row,col))continue;
      for(const cell of knightmasterAuraCells(row,col))add(ability+"Aura",cell.row,cell.col);
    }
    if(item.type==="darkWizard"&&item.darkMagicCircle&&visible)for(const cell of darkMagicCircleCells(item,row,col,board))add("darkMagicDomain",cell.row,cell.col);
    if(!item.id||!item.quantum||quantumSeen.has(item.id)||isHiddenFromCurrentTurn(item))return;
    quantumSeen.add(item.id);const cells=quantumCellsForItemAt(item,item.quantum.row,item.quantum.col);
    if(!cells.length||fog&&!cells.some(cell=>fog.has(key(cell))))return;
    overlays.push({kind:"quantum",cells:cells.map(cell=>({row:cell.row,col:cell.col})),piece:__publicPieceContent(item,undefined,undefined,viewer)});
  });
  for(const bond of normalizeChainBonds(state.chainBonds||[])){
    const first=findPieceOnBoardById(board,bond.aId),second=findPieceOnBoardById(board,bond.bId);
    if(!first||!second||fog&&(!fog.has(key(first))||!fog.has(key(second)))||isHiddenFromCurrentTurn(first.item)||isHiddenFromCurrentTurn(second.item))continue;
    if(first.row===second.row&&first.col===second.col)continue;
    relationships.push({kind:"chain",owner:bond.by==="black"?"black":"white",from:{row:first.row,col:first.col},to:{row:second.row,col:second.col},length:chainBondLengthProfile(first,second).kind});
  }
  return {boardMarks:marks,relationships,overlays};
}
function __publicCardRevelation(card) {
  const revealed={};
  if(randomRouletteResultHelpItem(card))revealed.rouletteType=card.randomRouletteResultType;
  if(suspiciousPotionResultHelpItem(card))revealed.potionEffect=card.suspiciousPotionResultId;
  const box=revealedBoxCard(card);if(box)revealed.boxCardId=box.id;
  return revealed;
}
`;
const decisionActor = state => state.mode === "draft" && state.draft?.color || state.pendingPromotion?.color || state.activeTrolley?.color || state.turn;
function validateBoardAliases(state) {
  const boards = [["state.board", state.board]];
  if (Array.isArray(state.boardHistory)) state.boardHistory.forEach((frame, index) => {
    if (frame && Object.hasOwn(frame, "board")) boards.push([`state.boardHistory[${index}].board`, frame.board]);
  });
  for (const name of ["replayBaseFrame", "replayTailFrame"]) {
    if (state[name] && Object.hasOwn(state[name], "board")) boards.push([`state.${name}.board`, state[name].board]);
  }
  for (const [label, board] of boards) {
    if (!Array.isArray(board) || board.length !== 8 || board.some(row => !Array.isArray(row) || row.length !== 8)) throw new TypeError(`Invalid snapshot board shape at ${label}.`);
    const groups = new Map();
    for (let row = 0; row < 8; row++) for (let col = 0; col < 8; col++) {
      const piece = board[row][col];
      if (piece === null) continue;
      if (!piece || typeof piece !== "object" || Array.isArray(piece)) throw new TypeError(`Invalid snapshot piece at ${label}[${row}][${col}].`);
      const large = ["colossus", "bigRook", "bigBishop"].includes(piece.type);
      if (large && (typeof piece.id !== "string" || !piece.id)) throw new TypeError(`Large snapshot piece needs a stable ID at ${label}.`);
      if (Object.hasOwn(piece, "id") && typeof piece.id !== "string") throw new TypeError(`Invalid snapshot piece ID at ${label}.`);
      if (!piece.id) continue;
      const group = groups.get(piece.id) || { piece, cells: [], signature: contract.canonical(piece), large };
      if (group.signature !== contract.canonical(piece)) throw new TypeError(`Conflicting snapshot piece ID ${piece.id} at ${label}.`);
      group.cells.push({ row, col }); groups.set(piece.id, group);
    }
    for (const [id, { cells, large }] of groups) {
      if (!large) {
        if (cells.length !== 1) throw new TypeError(`Duplicate non-large snapshot piece ID ${id} at ${label}.`);
        continue;
      }
      // Source relinkBoardPieceReferences62055 only groups by ID. Exile
      // 103528 moves one bigRook/bigBishop cell to origin while the three
      // remaining cells retain that object and its old anchor. Both actual
      // source relink and normalizeDeserializedState preserve this reachable
      // disconnected shape. Geometry belongs to creation/action legality;
      // snapshot restoration must neither reject nor silently repair it.
    }
  }
}
function publicStateValue(value, field) {
  function inspect(current) {
    if (!current || typeof current !== "object") return;
    for (const [key, child] of Object.entries(current)) {
      if (/^(?:id|instanceId|hiddenFrom|seed|rng|rngState|positionId|positionKey|pawnId|kingId)$/i.test(key) || /(?:piece|pawn|king|preview|frozen)Ids?$/i.test(key)) throw new Error(`Nested visibility review required for ${field}.${key}.`);
      inspect(child);
    }
  }
  inspect(value); return contract.jsonCopy(value);
}

class OfflineOracle {
  constructor(root = cacheRoot(), { maxCandidates = 100000, maxMicrotasks = 256 } = {}) {
    if (!Number.isSafeInteger(maxCandidates) || maxCandidates < 1) throw new TypeError("maxCandidates must be positive.");
    if (!Number.isSafeInteger(maxMicrotasks) || maxMicrotasks < 1 || maxMicrotasks > 4096) throw new TypeError("maxMicrotasks must be between 1 and 4096.");
    this.main = loadMain(root);
    this.maxCandidates = maxCandidates;
    const file = this.main.manifest.files.find(file => /^main-/.test(file.name));
    const frozen = contract.catalog.source.files.find(source => source.name === file.name);
    if (!frozen || file.sha256 !== frozen.sha256) throw new Error("Oracle source is not the adopted frozen rules version.");
    this.main.context.__maxMicrotasks = maxMicrotasks;
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
    // Validate before changing this VM or its RNG; the source ID-only helper
    // would otherwise silently merge malformed conflicting snapshot objects.
    validateBoardAliases(position.state);
    const random = contract.jsonCopy(position.rng);
    this.main.context.__position = position.state;
    try {
      // Reviving collections and reconnecting board references can fail. Keep
      // the prepared state separate until both operations succeed, including
      // the existing RNG and callbacks owned by the currently live position.
      this.main.context.__restoredState = this.evaluate("(()=>{const candidate=__decode(__position);__relinkSnapshotBoards(candidate);return candidate;})()");
      // Source75801 keeps this animation deadline Map outside its snapshots.
      // Profile v2 admits each immutable position with a cold render cache;
      // source Set fields and cache activity within the action stay intact.
      this.evaluate("state=__restoredState;activePieceAnimationUntil.clear();__microtasks.length=0;scheduledGameOverReplayState=null;selectedGameStyle=state.gameStyle||'normal';localPlayMode='local';playMode='local';");
      this.random = random;
    } finally {
      delete this.main.context.__position;
      delete this.main.context.__restoredState;
    }
  }
  snapshot(history = []) {
    this.evaluate("__settleMicrotasks()");
    return contract.position(this.state(), this.random, history);
  }
  newGame(config = {}, seed = 0, tape = []) {
    const allowed = ["gameStyle", "draftDelete", "ruleCardIds", "starWinLimit", "deathmatchEnabled", "deathmatchLimitTurns"];
    if (Object.keys(config).some(key => !allowed.includes(key))) throw new TypeError("Unknown game configuration field.");
    const style = config.gameStyle || "normal";
    if (!["normal", "chaos", "grand"].includes(style)) throw new TypeError("Only normal, chaos and grand 8x8 are supported.");
    if (config.ruleCardIds && (!Array.isArray(config.ruleCardIds) || config.ruleCardIds.some(id => !contract.catalog.cards.some(card => card.id === id && card.draftCategory === "RULE")))) throw new TypeError("Unknown RULE card.");
    this.random = contract.rng(seed, tape);
    this.evaluate("__microtasks.length=0;scheduledGameOverReplayState=null;");
    this.main.context.__config = { ...config, gameStyle: style };
    for (const key of ["draftDelete", "deathmatchEnabled"]) if (config[key] !== undefined && typeof config[key] !== "boolean") throw new TypeError(`Invalid ${key}.`);
    for (const key of ["starWinLimit", "deathmatchLimitTurns"]) if (config[key] !== undefined && (!Number.isSafeInteger(config[key]) || config[key] < 1)) throw new TypeError(`Invalid ${key}.`);
    this.evaluate("activePieceAnimationUntil.clear();selectedGameStyle=__config.gameStyle; localPlayMode='local'; playMode='local'; resetGame(false,[]); state.draftDelete=__config.draftDelete===true; if(__config.deathmatchEnabled!==undefined)state.deathmatchEnabled=__config.deathmatchEnabled; if(__config.deathmatchLimitTurns!==undefined)state.deathmatchLimitTurns=__config.deathmatchLimitTurns; if(__config.starWinLimit!==undefined)state.starWinLimit=__config.starWinLimit; if(__config.ruleCardIds?.length){state.ruleOpeningEnabled=true;state.ruleSelectionEnabled=true;state.selectedRuleCardIds=[...__config.ruleCardIds];maybeApplyOpeningRuleEvent();finishRuleOpeningEvent();}beginInitialGameFlow();");
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
    const board = JSON.parse(this.evaluate("JSON.stringify(state.board.map((row,r)=>row.map((cell,c)=>__publicPieceView(cell,r,c,__viewer))))"));
    const cardView = (card, slot) => {
      this.main.context.__card = card;
      const revealed = JSON.parse(this.evaluate("JSON.stringify(__publicCardRevelation(__card))"));
      return { ...Object.fromEntries(Object.entries(card).filter(([key]) => policy.cardPublicFields.includes(key) && key !== "revealed")), ...(slot === undefined ? {} : { slot }), ...(Object.keys(revealed).length ? { revealed } : {}) };
    };
    const opponent = viewer === "white" ? "black" : "white";
    const ownCards = (state.deckSlots?.[viewer] || []).map((card, slot) => card && cardView(card, slot)).filter(Boolean);
    const other = (state.deckSlots?.[opponent] || []).filter(Boolean);
    const publicState = Object.fromEntries(policy.statePublicFields.filter(key => Object.hasOwn(state, key)).map(key => [key, publicStateValue(["winterKingdom","captureTheFlag"].includes(key)?{enabled:key==="winterKingdom"?Boolean(state[key]?.enabled):Boolean(state[key])}:state[key], key)]));
    publicState.phase = this.evaluate("getPhase()");
    publicState.revealedOpponentCards = (state.deckSlots?.[opponent] || []).map((card, slot) => card && cardView(card, slot)).filter(Boolean);
    publicState.ownStarTotal = this.evaluate(`deckStarTotal(${JSON.stringify(viewer)})`);
    publicState.opponentStarTotal = this.evaluate(`deckStarTotal(${JSON.stringify(opponent)})`);
    publicState.ruleCardIds = [state.appliedRuleCard?.id, ...(state.additionalRuleCards || []).map(card => card.id)].filter(Boolean);
    publicState.pendingRuleCardIds = JSON.parse(this.evaluate("JSON.stringify((state.pendingRuleTickets||[]).map(entry=>entry?.ruleId).filter(id=>CARD_BY_ID.has(id)))"));
    publicState.rulesVersion = position.rulesVersion;
    publicState.catalogVersion = position.catalogVersion;
    publicState.projectionVersion = policy.projectionVersion;
    publicState.observationPolicyHash = contract.digest(policy);
    Object.assign(publicState, JSON.parse(this.evaluate("JSON.stringify(__publicBoardSurface(__viewer))")));
    publicState.captures = Object.fromEntries(Object.entries(state.captures || {}).map(([color, cells]) => [color, cells.slice(-12).map(cell => typeof cell === "string" ? { type: cell } : Object.fromEntries(Object.entries(cell).filter(([key]) => ["type", "color", "logDir", "windmillMode"].includes(key))))]));
    publicState.clock = state.clock && Object.fromEntries(Object.entries(state.clock).filter(([key]) => ["enabled", "initialMs", "incrementMs", "whiteMs", "blackMs", "runningColor", "timeoutWinner", "timeoutLoser"].includes(key)));
    publicState.lastMove = state.lastMove && state.lastMove.hiddenFrom !== viewer ? Object.fromEntries(Object.entries(state.lastMove).filter(([key]) => ["from", "to", "color", "kind"].includes(key)).map(([key,value])=>[key,["from","to"].includes(key)?{row:value.row,col:value.col}:value])) : null;
    publicState.selectionPhase = state.pendingPromotion ? { kind: "promotion", color: state.pendingPromotion.color, row: state.pendingPromotion.row ?? null, col: state.pendingPromotion.col ?? null, choices: viewer === state.pendingPromotion.color ? state.pendingPromotion.choices || [] : [] } : state.activeTrolley ? { kind: "trolley", color: state.activeTrolley.color, choices: JSON.parse(this.evaluate("JSON.stringify(state.activeTrolley.choices.map(choice=>(choice.pieces||[]).map(cell=>({type:cell.type||cell.piece?.type||null,color:cell.color||cell.piece?.color||null}))))")) } : null;
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
  actionStream(position, { cardId = null, legal = true } = {}) {
    contract.validatePosition(position);
    if (cardId !== null && !contract.catalog.cards.some(card => card.id === cardId)) throw new TypeError("Unknown stream card filter.");
    if (typeof legal !== "boolean") throw new TypeError("Stream legal filter must be boolean.");
    const owned = contract.deepFreeze(contract.jsonCopy(position));
    this.restore(owned);
    const state = owned.state;
    const special = state.mode !== "play" || state.pendingPromotion || state.activeTrolley;
    let iterator;
    if (special) iterator = this.candidates(owned).filter(payload => cardId === null || payload.cardId === cardId)[Symbol.iterator]();
    else {
      if (state.ruleTicketChoice || state.jokerChoice || state.barricadeDirectionChoice || state.targeting) throw new Error("UI presentation choices must be normalized to the atomic card target before snapshot import.");
      this.main.context.__streamCardId = cardId;
      iterator = this.evaluate("iterateCandidatePayloads(state.turn,__streamCardId)");
    }
    let exhausted = false;
    return {
      nextPage: (limit = 256, { maxExamined = 4096 } = {}) => {
        if (!Number.isSafeInteger(limit) || limit < 1 || limit > 4096) throw new TypeError("Page size must be 1..4096.");
        if (!Number.isSafeInteger(maxExamined) || maxExamined < 1 || maxExamined > 65536) throw new TypeError("Examined-action budget must be 1..65536.");
        const actions = [];
        let examined = 0;
        while (!exhausted && actions.length < limit && examined < maxExamined) {
          // Other streams, source validation and observations can use this VM
          // between pages. Resume each owned iterator against its own position.
          this.restore(owned);
          let next;
          if (special) next = iterator.next();
          else {
            this.main.context.__candidateIterator = iterator;
            next = JSON.parse(this.evaluate("JSON.stringify(__candidateIterator.next())"));
          }
          if (next.done) { exhausted = true; break; }
          examined++;
          const candidate = contract.action(owned, next.value);
          if (!legal || special || this.apply(owned, candidate, { recordHistory: false }).ok) actions.push(candidate);
        }
        return contract.deepFreeze({ actions, exhausted, examined, stopReason: exhausted ? "exhausted" : actions.length === limit ? "page-limit" : "examined-budget" });
      },
    };
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
module.exports = { OfflineOracle, PRESENTATION_HOOKS, HEADLESS_PROFILE, decisionActor };
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
