(function() {
  "use strict";
  let ATTACK_MEMO = null;
  function cloneGameData(value, options) {
    const fail = () => {
      throw new DOMException("Value cannot be cloned", "DataCloneError");
    };
    if (options?.transfer?.length) fail();
    const seen = /* @__PURE__ */ new Map();
    function copy(input) {
      if (typeof input === "function" || typeof input === "symbol") return fail();
      if (input === null || typeof input !== "object") return input;
      if (seen.has(input)) return seen.get(input);
      let output;
      if (Array.isArray(input)) output = new Array(input.length);
      else if (input instanceof Date) output = new Date(input.getTime());
      else if (input instanceof RegExp) output = new RegExp(input.source, input.flags);
      else if (input instanceof Map) output = /* @__PURE__ */ new Map();
      else if (input instanceof Set) output = /* @__PURE__ */ new Set();
      else if (input instanceof ArrayBuffer) output = input.slice(0);
      else if (ArrayBuffer.isView(input)) {
        const buffer = copy(input.buffer);
        output = input instanceof DataView ? new DataView(buffer, input.byteOffset, input.byteLength) : new input.constructor(buffer, input.byteOffset, input.length);
      } else if (input instanceof Error) {
        output = new Error(input.message);
        output.name = input.name;
        output.stack = input.stack;
      } else if (Object.getPrototypeOf(input) === Object.prototype || Object.getPrototypeOf(input) === null) {
        output = {};
      } else return fail();
      seen.set(input, output);
      if (input instanceof Map) for (const [key, item] of input) output.set(copy(key), copy(item));
      else if (input instanceof Set) for (const item of input) output.add(copy(item));
      else if (input instanceof Error) {
        if (Object.prototype.hasOwnProperty.call(input, "cause")) output.cause = copy(input.cause);
      } else if (Array.isArray(input) || Object.getPrototypeOf(input) === Object.prototype || Object.getPrototypeOf(input) === null) {
        for (const key of Object.keys(input)) {
          Object.defineProperty(output, key, {
            value: copy(input[key]),
            writable: true,
            enumerable: true,
            configurable: true
          });
        }
      }
      return output;
    }
    return copy(value);
  }
  if (typeof globalThis.structuredClone !== "function") {
    globalThis.structuredClone = cloneGameData;
  }
  function septemberBigBishopOpening(color, minorType = "knight") {
    return color === "white" ? { anchor: "f2", footprint: ["f2", "g2", "f1", "g1"], placements: [["f3", "pawn", true], ["g3", "pawn", true], ["h3", minorType, true]] } : { anchor: "f8", footprint: ["f8", "g8", "f7", "g7"], placements: [["f6", "pawn", true], ["g6", "pawn", true], ["h6", minorType, true]] };
  }
  function septemberLargeCaptureLimit(type) {
    return ["bigBishop", "big-bishop"].includes(type) ? 3 : ["bigRook", "big-rook"].includes(type) ? 2 : Infinity;
  }
  function septemberOvertakeCanCross(enabled, attacker, target) {
    return Boolean(enabled && attacker?.type === "rook" && target?.color === attacker.color && !["wall", "football", "monster", "blackHole", "black-hole", "coffin"].includes(target.type));
  }
  // ---- 2026-09-15 site patch: 16 new cards, grafted from the fresh
  // aiWorker.js (site-oracle/aiWorker-fresh-20260915.js). Real file gates
  // several of these behind a `catalogHash` A/B flag that our engine never
  // sets, so the ground-truth default (hash absent -> `!== false`, i.e.
  // enabled) is what's ported here: thiefRemake, minorExtinction,
  // rookParrotTarget, thiefRequiredJump all default ON.
  // 2026-09-19 site patch (balance/remakes): these gates default to the
  // latest rules when no catalogHash/flag is set, same convention as above.
  // thiefRequiredJump flipped to opt-in (site: `=== true` when hash absent);
  // thiefQuietJump replaced it as the default thief rule.
  function usesSeptember18Balance(state) {
    return state?.september18Balance !== false;
  }
  function usesParrotBasicMovement(state) {
    return state?.parrotBasicMovement !== false;
  }
  function usesUnifiedJumpObstacles(state) {
    return state?.unifiedJumpObstacles !== false;
  }
  function usesThiefQuietJump(state) {
    return state?.thiefQuietJump !== false;
  }
  // 2026-09-19 site default: the "internal six fixes" gate is ON unless state.internalSixFixes === false.
  function usesInternalSixFixes(state) {
    return state?.internalSixFixes !== false;
  }
  function usesThiefRequiredJump(state) {
    return state?.thiefRequiredJump === true;
  }
  function crossesReservedScarecrow(from, move, entries = []) {
    if (!from || !move || move.colossusBody || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.setLogDirection || move.merchantBuy) return false;
    const reserved = entries.filter((e) => (e?.reserved || e?.solid) && !e.pieceId);
    const segment = (a, b) => {
      if (!a || !b) return false;
      const dr = b.row - a.row, dc = b.col - a.col, n = Math.max(Math.abs(dr), Math.abs(dc));
      if (!n || dr && dc && Math.abs(dr) !== Math.abs(dc)) return false;
      return reserved.some((e) => {
        for (let i = 1; i <= (e.solid ? n - 1 : n); i++) if (e.row === a.row + Math.sign(dr) * i && e.col === a.col + Math.sign(dc) * i) return true;
        return false;
      });
    };
    if (move.dragonSwap || move.substitutionSwap || move.relaySwap || move.switcherooMove || move.symmetryMove) return reserved.some((e) => e.reserved && e.row === move.row && e.col === move.col);
    if (move.bent) return false;
    if (move.portalEntry && move.portalExit) return segment(from, move.portalEntry) || segment(move.portalExit, move) || reserved.some((e) => e.row === move.portalExit.row && e.col === move.portalExit.col);
    return segment(from, move);
  }
  function usesMinorExtinction(state) {
    return state?.extinctionMinorTargets !== false;
  }
  function extinctionTargetTypeAllowed(type, state) {
    return !usesMinorExtinction(state) || ["knight", "bishop", "camel"].includes(type);
  }
  const usesEnemyOnlyRadiance = usesMinorExtinction;
  function usesRookParrotTarget(state) {
    return state?.parrotRookTarget !== false;
  }
  function usesThiefRemake(state) {
    return state?.thiefRemake !== false;
  }
  function uniqueBoardPieces(board) {
    const seen = /* @__PURE__ */ new Set(), result = [];
    board.forEach((line, row) => line.forEach((item, col) => {
      if (!item || seen.has(item.id || item)) return;
      seen.add(item.id || item);
      result.push({ item, row, col });
    }));
    return result;
  }
  function highlanderEligible(pieces, color) {
    const own = pieces.filter((item) => item.color === color), types = own.map((item) => item.type);
    return own.length > 0 && new Set(types).size === own.length;
  }
  function thiefOffsets() {
    return [-3, -2, -1, 1, 2, 3].flatMap((n) => [[n, 0], [0, n]]);
  }
  function thiefBaseMoveAllowed(from, to, at, state) {
    const dr = to.row - from.row, dc = to.col - from.col, distance = Math.max(Math.abs(dr), Math.abs(dc));
    if (dr === 0 === (dc === 0) || distance > 3) return false;
    if (usesThiefQuietJump(state)) return !at(to.row, to.col) || !thiefJumpedPiece(from, to, at);
    if (!usesThiefRequiredJump(state)) return true;
    const screens = /* @__PURE__ */ new Set();
    for (let step = 1; step < distance; step++) {
      const piece = at(from.row + Math.sign(dr) * step, from.col + Math.sign(dc) * step);
      if (piece) screens.add(piece.instanceId || piece.id || piece);
    }
    return screens.size === 1;
  }
  function thiefJumpedPiece(from, to, at) {
    const dr = to.row - from.row, dc = to.col - from.col;
    if (dr === 0 === (dc === 0) || Math.max(Math.abs(dr), Math.abs(dc)) > 3) return false;
    for (let step = 1; step < Math.max(Math.abs(dr), Math.abs(dc)); step++) {
      if (at(from.row + Math.sign(dr) * step, from.col + Math.sign(dc) * step)) return true;
    }
    return false;
  }
  function thiefMoveFlags(from, to, at, state) {
    return usesThiefQuietJump(state) && thiefJumpedPiece(from, to, at) ? { thiefQuietJump: true } : {};
  }
  function longEnPassantCandidates({ from, color, direction, rights, at, canCapture }) {
    const found = /* @__PURE__ */ new Set(), moves = [];
    for (const right of rights || []) {
      if (!right || right.color === color || right.row !== from.row + direction || right.col === from.col || ![right.row, right.col, right.capturedRow, right.capturedCol].every(Number.isInteger) || right.col !== right.capturedCol || at(right.row, right.col)) continue;
      const victim = at(right.capturedRow, right.capturedCol);
      if (!victim || victim.color !== right.color || victim.type !== "pawn" || !canCapture(victim)) continue;
      const key = `${right.row},${right.col},${right.capturedRow},${right.capturedCol}`;
      if (found.has(key)) continue;
      found.add(key);
      moves.push({
        row: right.row,
        col: right.col,
        enPassant: true,
        capture: true,
        capturedRow: right.capturedRow,
        capturedCol: right.capturedCol,
        longEnPassant: true
      });
    }
    return moves;
  }
  function falseStartMoves(entries, color, rows, cols, isBlocked) {
    const direction = color === "white" ? -1 : 1, own = entries.filter((entry) => entry.item.color === color);
    const parent = Array.from({ length: cols }, (_, i) => i), find = (c) => parent[c] === c ? c : parent[c] = find(parent[c]);
    for (const entry of own) for (const cell of entry.cells || [entry]) parent[find(cell.col)] = find(entry.col);
    const shifts = /* @__PURE__ */ new Map(), blocked = /* @__PURE__ */ new Set();
    const enemy = new Set(entries.filter((entry) => entry.item.color !== color).flatMap((entry) => (entry.cells || [entry]).map((cell) => `${cell.row},${cell.col}`)));
    for (let step = 1; step <= 2; step++) {
      for (const entry of own) {
        const group = find(entry.col);
        if (blocked.has(group)) continue;
        if ((entry.cells || [entry]).some((cell) => {
          const row = cell.row + direction * step;
          return row < 0 || row >= rows || enemy.has(`${row},${cell.col}`) || isBlocked(row, cell.col);
        })) blocked.add(group);
      }
      for (const entry of own) {
        const group = find(entry.col);
        if (!blocked.has(group)) shifts.set(group, step);
      }
    }
    return own.filter((entry) => shifts.has(find(entry.col))).map((entry) => ({ ...entry, to: { row: entry.row + direction * shifts.get(find(entry.col)), col: entry.col } }));
  }
  function locustReady(item, row, col, state) {
    if (usesSeptember18Balance(state) && !["knight", "bishop", "camel", "rook"].includes(item?.type)) return false;
    return item?.type !== "pawn" && item?.moved !== true && !item?.locustUsed && item?.locustOrigin?.row === row && item?.locustOrigin?.col === col;
  }
  function markLocustOrigins(board, color) {
    for (const { item, row, col } of uniqueBoardPieces(board)) if (item.color === color && item.type !== "pawn" && !item.moved && !item.locustUsed)
      item.locustOrigin = { row, col };
  }
  function thiefQueenCandidates(board, color, isRoyal = () => false) {
    return uniqueBoardPieces(board).filter(({ item }) => item.color === color && item.type === "queen" && !isRoyal(item));
  }
  function thiefMoveDirection(from, to) {
    if (!from || !to) return null;
    const dr = Math.sign(to.row - from.row), dc = Math.sign(to.col - from.col);
    return dr || dc ? `${dr},${dc}` : null;
  }
  function thiefMoveAllowed(item, to, state, from) {
    const ability = item?.type === "trickster" ? item.tricksterMoveType : item?.type;
    if (!usesThiefRemake(state) || ability !== "thief") return true;
    const direction = thiefMoveDirection(to.portalEntry || to, from);
    return !(direction && direction === item.thiefLastDirection) && ![to, to.portalEntry, to.portalExit].filter(Boolean).some((cell) => (item.thiefVisited || []).includes(`${cell.row},${cell.col}`));
  }
  function noteThiefMove(item, from, to, state, extraCells = []) {
    if (!item || !from || !to) return;
    if (!usesThiefRemake(state)) {
      if ((item.type === "thief" || item.tricksterMoveType === "thief") && !Number.isInteger(item.thiefTurnsLeft)) item.thiefTurnsLeft = 2;
      return;
    }
    item.thiefVisited = [.../* @__PURE__ */ new Set([...item.thiefVisited || [], ...[from, to, ...extraCells].filter(Boolean).map((cell) => `${cell.row},${cell.col}`)])];
    const direction = thiefMoveDirection(from, to.portalEntry || extraCells[0] || to);
    if (direction) item.thiefLastDirection = direction;
  }
  function tickThiefArrests(board, color, state) {
    const removed = [];
    for (const entry of uniqueBoardPieces(board)) {
      if (entry.item.color !== color) continue;
      if (usesThiefRemake(state)) {
        delete entry.item.thiefVisited;
        delete entry.item.thiefLastDirection;
        const ability = entry.item.type === "trickster" ? entry.item.tricksterMoveType : entry.item.type;
        if (ability !== "thief" || entry.item.submerged) continue;
        removed.push(entry);
        for (const line of board) for (let c = 0; c < line.length; c++) if (entry.item.id && line[c]?.id === entry.item.id || line[c] === entry.item) line[c] = null;
        continue;
      }
      if (!Number.isInteger(entry.item.thiefTurnsLeft)) continue;
      entry.item.thiefTurnsLeft -= 1;
      if (entry.item.thiefTurnsLeft <= 0) {
        removed.push(entry);
        for (const line of board) for (let c = 0; c < line.length; c++) if (entry.item.id && line[c]?.id === entry.item.id || line[c] === entry.item) line[c] = null;
      }
    }
    return removed;
  }
  // NOTE: real's own catalog ids are kebab-case for 3 of these
  // (false-start/locust-swarm/long-en-passant) while their `effect` field
  // is camelCase (falseStart/locustSwarm/longEnPassant); this project's own
  // selfplay-worker-merged.js constructs card objects with `id === effect`
  // (always camelCase), so every dispatch gate below keys off `.effect`
  // (camelCase, matches both real cards and selfplay-drawn cards) rather
  // than `.id`.
  const INTERNAL_EIGHT_IDS = Object.freeze(["highlander", "thief", "disassembly", "false-start", "proficiency", "locust-swarm", "long-en-passant", "extinction"]);
  const INTERNAL_EIGHT_EFFECTS = Object.freeze(["highlander", "thief", "disassembly", "falseStart", "proficiency", "locustSwarm", "longEnPassant", "extinction"]);
  const INTERNAL_EIGHT_PASSIVE_EFFECTS = Object.freeze(["highlander", "falseStart", "proficiency", "locustSwarm", "longEnPassant"]);
  function internalWorkerCallbacks(boardState) {
    return {
      isRoyal: (item) => isWorkerKingRole(boardState, item),
      blocked: (row, col) => isWorkerCollapsedSquare(boardState, row, col) || isWorkerCrownGroundSquare(boardState, row, col) || isBlackHoleCell(boardState, row, col) || isWorkerPendingSpawnReservedSquare(boardState, row, col) || isWorkerPendingPortalReservedSquare(boardState, row, col) || Boolean(workerFindQuantumAt(boardState, row, col)),
      remove: (item) => clearPieceCells(boardState, item),
      create: (color, type, row, col) => workerCreatePiece(color, type, boardState, row, col),
      endGame: (winner) => {
        boardState.mode = "gameover";
        boardState.winner = winner;
      }
    };
  }
  function applyEightLocalCard(state, card2, target, color, { isRoyal, blocked, remove, create, endGame }) {
    const entries = uniqueBoardPieces(state.board), selected = state.board[target?.row]?.[target?.col];
    if (card2.effect === "thief") return null;
    if (card2.effect === "extinction") {
      if (!selected || selected.color !== color || isRoyal(selected) || !extinctionTargetTypeAllowed(selected.type, state)) return { ok: false, message: usesMinorExtinction(state) ? "아군 마이너 피스를 선택하세요." : "킹이 아닌 아군 기물을 선택하세요." };
      const victims = entries.filter((entry) => entry.item.type === selected.type), lost = /* @__PURE__ */ new Set();
      for (const entry of victims) {
        if (isRoyal(entry.item)) lost.add(entry.item.color);
        remove(entry.item, entry.row, entry.col);
      }
      if (lost.size) endGame(lost.size === 2 ? "draw" : lost.has("white") ? "black" : "white", "멸종으로 왕족이 제거되었습니다.");
      return { ok: true, message: `${victims.length}개의 기물을 삭제했습니다.` };
    }
    if (state[card2.effect]?.[color]) return { ok: false, message: "이미 적용되어 있습니다." };
    state[card2.effect] = { ...state[card2.effect] || {}, [color]: true };
    if (card2.effect === "falseStart") {
      for (const entry of entries) entry.cells = state.board.flatMap((line, row) => line.flatMap((item, col) => item && (item === entry.item || item.id && item.id === entry.item.id) ? [{ row, col }] : []));
      const plan = falseStartMoves(entries, color, state.board.length, state.board[0].length, blocked);
      for (const entry of plan) for (const cell of entry.cells) state.board[cell.row][cell.col] = null;
      for (const entry of plan) {
        const delta = entry.to.row - entry.row;
        for (const cell of entry.cells) state.board[cell.row + delta][cell.col] = entry.item;
        if (Number.isInteger(entry.item.anchorRow)) entry.item.anchorRow += delta;
        if (entry.item.locustOrigin) entry.item.locustOrigin = { ...entry.to };
      }
      state.enPassant = null;
    }
    if (card2.effect === "locustSwarm") markLocustOrigins(state.board, color);
    return { ok: true, message: `${card2.name || card2.id} 효과가 적용되었습니다.` };
  }
  function applyWorkerInternalEightCard(boardState, card2, target, color) {
    if (card2.effect === "thief") {
      if (usesThiefRemake(boardState)) {
        const candidates = thiefQueenCandidates(boardState.board, color, (item3) => isWorkerKingRole(boardState, item3));
        if (!candidates.length) return { ok: false };
        const { item: item2 } = candidates[workerStableIndex(`${card2.instanceId || card2.id}:${boardState.moveCount || 0}`, candidates.length)];
        item2.type = "thief";
        item2.moved = true;
        item2.submerged = true;
        return { ok: true };
      }
      const item = get(boardState, target?.row, target?.col);
      if (!item || item.color !== color || item.type !== "rook" || isWorkerKingRole(boardState, item)) return { ok: false };
      item.type = "thief";
      return { ok: true };
    }
    const callbacks = internalWorkerCallbacks(boardState);
    if (card2.effect === "falseStart") callbacks.blocked = (row, col) => isWorkerCollapsedSquare(boardState, row, col) || isWorkerCrownGroundSquare(boardState, row, col) || isBlackHoleCell(boardState, row, col) || isWorkerPendingSpawnReservedSquare(boardState, row, col) || Boolean(workerFindQuantumAt(boardState, row, col));
    return applyEightLocalCard(boardState, card2, target, color, callbacks);
  }
  function resolveInternalHighlander(boardState) {
    if (boardState.mode === "gameover") return false;
    const pieces = uniqueBoardPieces(boardState.board).map(({ item }) => ({ color: item.color, type: isWorkerKingRole(boardState, item) ? "king" : item.type }));
    for (const color of ["white", "black"]) if (boardState.highlander?.[color] && highlanderEligible(pieces, color)) {
      boardState.mode = "gameover";
      boardState.winner = color;
      return true;
    }
    return false;
  }
  function disassembleMovedQueen(state, item, from, to, originalType, { isRoyal, blocked, create }) {
    if (originalType !== "queen" || item.type !== "queen" || !state.disassembly?.[item.color] || isRoyal(item) || state.board[to.row]?.[to.col] !== item || from.row === to.row && from.col === to.col) return false;
    item.type = "bishop";
    item.moved = true;
    if (!state.board[from.row]?.[from.col] && !blocked(from.row, from.col)) {
      const rook = create(item.color, "rook", from.row, from.col);
      rook.moved = true;
      state.board[from.row][from.col] = rook;
    }
    return true;
  }
  // ---- 5-card batch (symmetry / brutus / clockwork / mutation / parrot)
  const INTERNAL_FIVE_IDS = Object.freeze(["symmetry", "brutus", "clockwork", "mutation", "parrot"]);
  function clockworkHasNeighbor(item, row, col, at) {
    for (let dr = -1; dr <= 1; dr++) for (let dc = -1; dc <= 1; dc++) {
      if (!dr && !dc) continue;
      const neighbor = at(row + dr, col + dc);
      if (neighbor && neighbor !== item && (!item.id || neighbor.id !== item.id) && neighbor.color === item.color) return true;
    }
    return false;
  }
  function symmetryDestination(row, col, columns = 8) {
    return { row, col: columns - 1 - col };
  }
  function canMutationPromote(state, item, row, rows = 8) {
    return item?.type === "pawn" && state.mutation?.[item.color] === true && row === (item.color === "white" ? 0 : rows - 1);
  }
  function movementTypeName(type) {
    return String(type || "").replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  }
  function parrotTransformTypes(state) {
    return usesRookParrotTarget(state) ? ["rook"] : ["bishop", "knight", "camel"];
  }
  function rememberedBaseMovement(item, previous = null) {
    if (!item) return null;
    const type = movementTypeName(pieceAbilityType(item));
    if (type === "parrot") return previous ? structuredClone(previous) : null;
    return {
      type,
      ...item.logDirection ? { logDirection: structuredClone(item.logDirection) } : {},
      ...item.windmillMode ? { windmillMode: item.windmillMode } : {},
      ...type === "trickster" && item.tricksterMoveType ? { type: movementTypeName(item.tricksterMoveType) } : {}
    };
  }
  function rememberLocalMovement(state, item) {
    if (!item || !["white", "black"].includes(item.color)) return;
    state.parrotMovement ||= { white: null, black: null };
    state.parrotMovement[item.color] = rememberedBaseMovement(item, state.parrotMovement[item.color]);
  }
  function applyFiveLocalCard(state, card2, target, color, { isRoyal, random = Math.random } = {}) {
    if (!INTERNAL_FIVE_IDS.includes(card2.id)) return null;
    if (["symmetry", "mutation"].includes(card2.id)) {
      if (state[card2.id]?.[color]) return { ok: false, message: "이미 적용되어 있습니다." };
      state[card2.id] = { ...state[card2.id] || {}, [color]: true };
      return { ok: true, message: `${card2.name || card2.id} 효과가 적용되었습니다.` };
    }
    let item = state.board[target?.row]?.[target?.col];
    if (card2.id === "brutus") {
      const rooks = state.board.flat().filter((piece) => piece?.color === color && piece.type === "rook" && !isRoyal(piece));
      item = rooks.length ? rooks[Math.min(rooks.length - 1, Math.floor(random() * rooks.length))] : null;
    }
    const rookTarget = card2.id === "brutus" || card2.id === "parrot" && usesRookParrotTarget(state);
    if (!item || item.color !== color || isRoyal(item) || !(rookTarget ? item.type === "rook" : ["bishop", "knight", "camel"].includes(item.type))) return { ok: false, message: card2.id === "brutus" ? "변경할 아군 룩이 없습니다." : rookTarget ? "변경할 아군 룩을 선택하세요." : "아군 마이너 피스를 선택하세요." };
    item.type = card2.id;
    item.moved = true;
    item.freshNoCaptureUntil = (Number(state.turnsTaken?.[item.color]) || 0) + 1;
    return { ok: true, message: `${card2.name || card2.id}(으)로 변경했습니다.` };
  }
  const INTERNAL_ORTH = [[-1, 0], [1, 0], [0, -1], [0, 1]], INTERNAL_DIAG = [[-1, -1], [-1, 1], [1, -1], [1, 1]], INTERNAL_KING_DELTAS = [...INTERNAL_ORTH, ...INTERNAL_DIAG];
  const INTERNAL_KNIGHT_DELTAS = [[-2, -1], [-2, 1], [-1, -2], [-1, 2], [1, -2], [1, 2], [2, -1], [2, 1]];
  function parrotBaseMoves({ memory, row, col, color, moved = true, rows = 8, cols = 8, at, canCapture, state, isTransparent = () => false }) {
    if (!memory?.type) return [];
    const type = movementTypeName(memory.type), moves = [], dir = color === "white" ? -1 : 1;
    const inside = (r, c) => r >= 0 && r < rows && c >= 0 && c < cols;
    const add = (r, c, { quiet = false, captureOnly = false, ...extra } = {}) => {
      if (!inside(r, c)) return;
      const target = at(r, c);
      if (target ? quiet || !canCapture(target, r, c) : captureOnly) return;
      moves.push({ row: r, col: c, ...target ? { capture: true } : {}, ...extra });
    };
    const leap = (offsets, opts = {}) => offsets.forEach(([dr, dc]) => add(row + dr, col + dc, opts));
    const ray = (directions2, limit = 32, opts = {}) => directions2.forEach(([dr, dc]) => {
      for (let n = 1; n <= limit; n++) {
        const r = row + dr * n, c = col + dc * n;
        if (!inside(r, c)) break;
        add(r, c, opts);
        if (at(r, c)) break;
      }
    });
    if (["hook", "brutus"].includes(type)) return hookPathMoves({ row, col, rowCount: rows, colCount: cols, getOccupant: at, isTransparent, canCapture: (target, cell) => canCapture(target, cell.row, cell.col) });
    if (["pawn", "squire", "standard-bearer"].includes(type)) {
      if (!at(row + dir, col)) {
        add(row + dir, col);
        if (!moved && row === (color === "white" ? rows - 2 : 1) && !at(row + 2 * dir, col)) add(row + 2 * dir, col);
      }
      leap([[dir, -1], [dir, 1]], { captureOnly: true });
    } else if (["queen", "bear", "clockwork"].includes(type)) ray(INTERNAL_KING_DELTAS);
    else if (["rook", "big-rook"].includes(type)) ray(INTERNAL_ORTH);
    else if (["bishop", "big-bishop"].includes(type)) ray(INTERNAL_DIAG);
    else if (["knight", "unicorn", "dragon", "assassin"].includes(type)) leap(INTERNAL_KNIGHT_DELTAS);
    else if (type === "paladin") leap(INTERNAL_KNIGHT_DELTAS, { quiet: true });
    else if (type === "royal-knight") {
      leap(INTERNAL_KNIGHT_DELTAS);
      if (!usesParrotBasicMovement(state)) leap(INTERNAL_KING_DELTAS);
    } else if (["king", "octopus", "man", "guard", "reaper", "recruiter", "vip", "crown", "dark-wizard", "magic-girl", "undead", "hedgehog", "vampire-lord", "siren", "berserker", "log", "monster"].includes(type)) leap(INTERNAL_KING_DELTAS);
    else if (["ferz", "knightmaster", "princess"].includes(type)) leap(INTERNAL_DIAG);
    else if (type === "amazon") {
      ray(INTERNAL_KING_DELTAS);
      leap(INTERNAL_KNIGHT_DELTAS);
    } else if (type === "cardinal") for (const [startDr, startDc] of INTERNAL_DIAG) {
      let r = row, c = col, dr = startDr, dc = startDc;
      const seen = /* @__PURE__ */ new Set();
      for (let n = 0; n < rows * cols * 4; n++) {
        if (!inside(r + dr, c)) dr *= -1;
        if (!inside(r, c + dc)) dc *= -1;
        r += dr;
        c += dc;
        const key = `${r},${c},${dr},${dc}`;
        if (!inside(r, c) || seen.has(key) || r === row && c === col) break;
        seen.add(key);
        add(r, c);
        if (at(r, c)) break;
      }
    }
    else if (type === "camel") leap(INTERNAL_KNIGHT_DELTAS.map(([r, c]) => [r === 2 ? 3 : r === -2 ? -3 : r, c === 2 ? 3 : c === -2 ? -3 : c]));
    else if (type === "alfil") leap(INTERNAL_DIAG.map(([r, c]) => [r * 2, c * 2]));
    else if (["eagle", "alibaba"].includes(type)) {
      leap(INTERNAL_DIAG.map(([r, c]) => [r * 2, c * 2]));
      leap(INTERNAL_ORTH.map(([r, c]) => [r * 2, c * 2]));
    } else if (type === "thief") for (let n = 1; n <= 3; n++) leap(INTERNAL_ORTH.map(([r, c]) => [r * n, c * n]).filter(([dr, dc]) => thiefBaseMoveAllowed({ row, col }, { row: row + dr, col: col + dc }, at, state)));
    else if (type === "grasshopper") for (const [dr, dc] of INTERNAL_KING_DELTAS) {
      let r = row + dr, c = col + dc;
      while (inside(r, c) && !at(r, c)) {
        r += dr;
        c += dc;
      }
      if (inside(r, c)) add(r + dr, c + dc);
    }
    else if (["jester", "idol"].includes(type)) ray(INTERNAL_KING_DELTAS, 32, { quiet: true });
    else if (type === "lobster") leap([[dir, -1], [dir, 0], [dir, 1]]);
    else if (type === "fanatic") {
      leap([[dir, -1], [dir, 1]], { quiet: true });
      leap([[dir, 0]], { captureOnly: true });
    } else if (type === "bat") ray(INTERNAL_ORTH, 2);
    else if (type === "slime") leap(INTERNAL_ORTH.map(([r, c]) => [r * 3, c * 3]));
    else if (type === "colossus") leap(INTERNAL_ORTH);
    else if (type === "siege-ram") for (let n = 1; n <= 2; n++) leap(INTERNAL_ORTH.map(([r, c]) => [r * n, c * n]));
    else if (["wizard", "shotgun-king", "time-traveler"].includes(type)) leap(INTERNAL_KING_DELTAS, { quiet: true });
    else if (type === "campfire") leap(INTERNAL_ORTH, { quiet: true });
    else if (type === "missionary") leap(INTERNAL_DIAG, { quiet: true });
    else if (type === "herald") for (let n = 1; n <= 3; n++) leap(INTERNAL_ORTH.map(([r, c]) => [r * n, c * n]), { quiet: true });
    else if (type === "protestant") return protestantPathMoves({ row, col, rowCount: rows, colCount: cols, getOccupant: at, canCapture });
    else if (type === "pegasus") {
      for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) if (r !== row || c !== col) add(r, c, { quiet: true });
      leap(INTERNAL_KNIGHT_DELTAS);
    } else if (type === "prime-minister") for (const [dr, dc] of INTERNAL_KING_DELTAS) {
      const r = row + dr, c = col + dc;
      if (!inside(r, c)) continue;
      add(r, c);
      if (!at(r, c)) {
        for (const [r2, c2] of INTERNAL_KING_DELTAS) if (r + r2 !== row || c + c2 !== col) add(r + r2, c + c2);
      }
    }
    else if (["checker", "checker-king"].includes(type)) {
      const directions2 = type === "checker" ? [[dir, -1], [dir, 1]] : INTERNAL_DIAG;
      leap(directions2, { quiet: true });
      for (const [dr, dc] of directions2) {
        const target = at(row + dr, col + dc);
        if (target && canCapture(target, row + dr, col + dc)) add(row + 2 * dr, col + 2 * dc, { quiet: true, jumpCapture: { row: row + dr, col: col + dc } });
      }
    } else if (type === "windmill") ray(memory.windmillMode === "rook" ? INTERNAL_ORTH : INTERNAL_DIAG);
    else if (type === "cannon") for (const [dr, dc] of INTERNAL_ORTH) {
      let screen = false;
      for (let n = 1; n < 32; n++) {
        const r = row + dr * n, c = col + dc * n;
        if (!inside(r, c)) break;
        const target = at(r, c);
        if (!screen) {
          if (!target) add(r, c, { quiet: true });
          else screen = true;
        } else if (target) {
          add(r, c, { captureOnly: true });
          break;
        }
      }
    }
    if (type === "thief") moves.forEach((move) => Object.assign(move, thiefMoveFlags({ row, col }, move, at, state)));
    return [...new Map(moves.map((move) => [`${move.row},${move.col}`, move])).values()];
  }
  function workerPositionsOf(boardState, predicate) {
    const result = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (predicate(piece)) result.push({ row, col });
    });
    return result;
  }
  function resolveWorkerBrutus(boardState, color) {
    for (const { item, row, col } of uniqueBoardPieces(boardState.board)) {
      if (boardState.mode === "gameover") return;
      if (item.color !== color || !pieceHasAbility(item, "brutus") || isFrozenPiece(item) || Number(item.poisonStunTurns) > 0 || Number(item.stakedTurns) > 0) continue;
      if (isPoisonStunned(item) || isExhaustionMoveBlocked(boardState.exhaustion, item) || workerIdolEncoreRepeatBlocked(boardState, item) || isWorkerUndergroundBunkerKing(item) || boardState.zugzwang?.[color] && !isWorkerRoyalIdentityPiece(boardState, item) || isWorkerCollapsedSquare(boardState, row, col)) continue;
      const kings = uniqueBoardPieces(boardState.board).filter((e) => e.item !== item && e.item.color === color && isWorkerRoyalIdentityPiece(boardState, e.item));
      const candidates = parrotBaseMoves({ state: boardState, memory: { type: "brutus" }, row, col, color, rows: boardRowCount(boardState), cols: boardColCount(boardState), at: (r, c) => get(boardState, r, c), isTransparent: (target) => isGhostTransparentFor(item, target, "hook", boardState), canCapture: (target) => kings.some((e) => e.item === target) && canWorkerCaptureTarget(color, target, item, boardState, { allowFriendly: true }) }).map((move) => ({ ...move, brutusBetrayal: true }));
      const moves = candidates.filter((move) => isWorkerBaseMoveAllowed(boardState, item, row, col, move) && !(item.disarmed?.remaining > 0));
      for (const target of kings) {
        if (!moves.some((m) => m.row === target.row && m.col === target.col) || target.item.shielded) continue;
        set(boardState, row, col, null);
        set(boardState, target.row, target.col, item);
        item.moved = true;
        recordWorkerCapturedPieces(boardState, color, [target.item], item);
        recordWorkerDirectCaptures(boardState, item, [target.item]);
        rememberLocalMovement(boardState, item);
        if (!isDemocracyProtectedRoyal(boardState.democracy, target.item)) {
          boardState.mode = "gameover";
          boardState.winner = opponent(color);
        } else {
          boardState.kingDead ||= { white: false, black: false };
          boardState.kingDead[color] = true;
        }
        return;
      }
    }
  }
  function workerSymmetryMoves(boardState, item, row, col) {
    if (!boardState.symmetry?.[item.color] || ["wall", "football", "blackHole", "coffin"].includes(item.type)) return [];
    const large = workerIsLargePiece(item), to = symmetryDestination(row, col, boardColCount(boardState) - (large ? 1 : 0));
    if (to.col === col) return [];
    if (large) {
      const cells = colossusCells(to.row, to.col);
      if (cells.some(({ row: r, col: c }) => get(boardState, r, c)?.color === item.color && get(boardState, r, c) !== item)) return [];
      const captures = workerColossusLandingCaptures(boardState, cells, item, item.color, septemberLargeCaptureLimit(item.type));
      if (!captures) return [];
      return [{ ...to, anchorRow: to.row, anchorCol: to.col, highlightCells: cells, symmetryMove: true, ...item.type === "colossus" ? { colossusMove: true, colossusLandingCaptures: captures } : { bigRookMove: true, bigRookLandingCaptures: captures } }];
    }
    const target = get(boardState, to.row, to.col);
    if (target && (target.color === item.color || !canWorkerCaptureTarget(item.color, target, item, boardState))) return [];
    return [{ ...to, symmetryMove: true, ...target ? { capture: true } : {} }];
  }
  // ---- 3-card batch (paladin / octopus / metal)
  const INTERNAL_THREE_IDS = Object.freeze(["paladin", "octopus", "metal"]);
  const threeType = (type) => String(type || "").replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  const lightSquare = (row, col) => (row + col) % 2 === 0;
  const INTERNAL_THREE_DIRECTIONS = [[-1, 0], [1, 0], [0, -1], [0, 1], [-1, -1], [-1, 1], [1, -1], [1, 1]];
  // `board` arrays are cloned fresh per search node (see cloneState) and never
  // mutated in place afterward, so caching by array identity is safe: a given
  // board reference always represents exactly one immutable position.
  const THREE_CACHE = /* @__PURE__ */ new WeakMap();
  function threeCacheFor(board) {
    let cache = THREE_CACHE.get(board);
    if (!cache) {
      cache = { hasPaladin: void 0, radiance: /* @__PURE__ */ new Map() };
      THREE_CACHE.set(board, cache);
    }
    return cache;
  }
  function boardHasPaladin(board) {
    const cache = threeCacheFor(board);
    if (cache.hasPaladin === void 0) cache.hasPaladin = board.some((line) => line.some((p) => threeType(pieceAbilityType(p)) === "paladin"));
    return cache.hasPaladin;
  }
  function radianceCells(board, movingColor = null, { allSquares = true, includeCenter = allSquares } = {}) {
    const cache = threeCacheFor(board);
    const key = (movingColor || "") + "|" + (allSquares ? 1 : 0) + (includeCenter ? 1 : 0);
    const cached = cache.radiance.get(key);
    if (cached) return cached;
    const result = /* @__PURE__ */ new Set();
    for (const { item, row, col } of uniqueBoardPieces(board)) if (threeType(pieceAbilityType(item)) === "paladin" && (allSquares || lightSquare(row, col)) && (!movingColor || item.color !== movingColor)) {
      if (includeCenter) result.add(`${row},${col}`);
      for (const [dr, dc] of INTERNAL_THREE_DIRECTIONS) if (board[row + dr]?.[col + dc] !== void 0) result.add(`${row + dr},${col + dc}`);
    }
    cache.radiance.set(key, result);
    return result;
  }
  function octopusHasEnemy(board, row, col, color) {
    return INTERNAL_THREE_DIRECTIONS.some(([dr, dc]) => {
      const target = board[row + dr]?.[col + dc];
      return target && ["white", "black"].includes(target.color) && target.color !== color;
    });
  }
  function completeThreeTurn(board, color) {
    for (const { item, row, col } of uniqueBoardPieces(board)) if (item.color === color) {
      if (item.metalized && Number(item.metalCooldown) > 0) item.metalCooldown--;
      if (threeType(pieceAbilityType(item)) === "octopus" && !octopusHasEnemy(board, row, col, color)) item.submerged = true;
    }
  }
  function metalPositions(board) {
    return new Map(uniqueBoardPieces(board).filter(({ item }) => item.metalized).map(({ item, row, col }) => [item.id || item, { row, col }]));
  }
  function noteMetalRelocations(board, before, completedColor = null) {
    if (!before?.size) return;
    for (const { item, row, col } of uniqueBoardPieces(board)) {
      const old = before.get(item.id || item);
      if (item.metalized && old && (old.row !== row || old.col !== col)) item.metalCooldown = item.color === completedColor ? 3 : 4;
    }
  }
  function applyThreeLocalCard(state, card2, target, color, { isRoyal, isRanged }) {
    const item = state.board[target?.row]?.[target?.col];
    if (!item || item.color !== color || isRoyal(item)) return { ok: false, message: "왕을 제외한 아군 대상 기물을 선택하세요." };
    if (card2.id === "metal") {
      if (item.metalized || !isRanged(item)) return { ok: false, message: "금속화되지 않은 아군 원거리 기물을 선택하세요." };
      item.metalized = true;
      item.metalCooldown = 0;
    } else {
      if (item.type !== (card2.id === "paladin" ? "knight" : "rook")) return { ok: false, message: "변경할 아군 기물을 선택하세요." };
      item.type = card2.id;
      item.moved = true;
      item.freshNoCaptureUntil = (Number(state.turnsTaken?.[item.color]) || 0) + 1;
    }
    return { ok: true, message: `${card2.name || card2.id} 효과가 적용되었습니다.` };
  }
  function threeMoveAllowed(board, item, row, col, move, { movementType = pieceAbilityType(item), teleport = false, secondary = false, enemyOnlyRadiance = true, hookPathRange = true, allSquaresRadiance = true } = {}) {
    if (!item || !move) return false;
    const to = move.portalLanding && move.portalExit ? move.portalExit : move;
    if (!Number.isInteger(to.row) || !Number.isInteger(to.col)) return true;
    const distance = Math.max(Math.abs(to.row - row), Math.abs(to.col - col));
    if (item.metalized && (Number(item.metalCooldown) > 0 || (hookPathRange ? oneStepRestrictedMoveDistance(movementType, row, col, to) : distance) > 1)) return false;
    const target = board[to.row]?.[to.col];
    if (!secondary && move.castle && move.rookFrom && move.rookTo) {
      const rook = board[move.rookFrom.row]?.[move.rookFrom.col];
      if (rook && !threeMoveAllowed(board, rook, move.rookFrom.row, move.rookFrom.col, move.rookTo, { secondary: true, enemyOnlyRadiance, hookPathRange, allSquaresRadiance })) return false;
    }
    if (!secondary && target && (move.relaySwap || move.substitutionSwap || move.dragonSwap || move.switcherooMove)) {
      if (!threeMoveAllowed(board, target, to.row, to.col, { row, col }, { teleport: true, secondary: true, enemyOnlyRadiance, hookPathRange, allSquaresRadiance })) return false;
    }
    if (threeType(pieceAbilityType(item)) === "paladin" && !move.crownGroundCapture && (move.capture || move.jumpCapture || target && target.color !== item.color && !["crown", "wall"].includes(threeType(target.type)))) return false;
    if (lightSquare(row, col) || !boardHasPaladin(board)) return true;
    const lit = radianceCells(board, enemyOnlyRadiance ? item.color : null, { allSquares: allSquaresRadiance }), blocked = (r, c) => lit.has(`${r},${c}`);
    const landing = move.highlightCells || [to];
    if (landing.some((cell) => blocked(cell.row, cell.col)) || [move.portalEntry, move.portalExit].filter(Boolean).some((cell) => blocked(cell.row, cell.col))) return false;
    if (distance === 0 || teleport || move.symmetryMove || move.substitutionSwap || move.relaySwap) return true;
    const type = threeType(movementType);
    const blocker = { type: "wall", color: null };
    const at = (r, c) => blocked(r, c) ? blocker : board[r]?.[c];
    if (["hook", "brutus", "protestant"].includes(type)) {
      const generate = type === "protestant" ? protestantPathMoves : hookPathMoves;
      return generate({ row, col, rowCount: board.length, colCount: board[0].length, getOccupant: at, canCapture: (p) => p !== blocker }).some((m) => m.row === to.row && m.col === to.col);
    }
    if (type === "prime-minister" && distance === 2) return INTERNAL_THREE_DIRECTIONS.some(([dr2, dc2]) => !blocked(row + dr2, col + dc2) && !board[row + dr2]?.[col + dc2] && Math.max(Math.abs(to.row - row - dr2), Math.abs(to.col - col - dc2)) === 1);
    const leap = ["knight", "paladin", "royal-knight", "camel", "alfil", "eagle", "alibaba", "dragon", "assassin", "thief", "herald", "grasshopper", "slime", "pegasus", "checker", "checker-king"];
    if (leap.includes(type) || move.jumpCapture || move.longEnPassant) return true;
    if (type === "cardinal") {
      for (const [dr0, dc0] of INTERNAL_THREE_DIRECTIONS.filter(([r, c]) => r && c)) {
        let r = row, c = col, dr2 = dr0, dc2 = dc0;
        const seen = /* @__PURE__ */ new Set();
        for (let n = 0; n < board.length * board[0].length * 4; n++) {
          if (r + dr2 < 0 || r + dr2 >= board.length) dr2 = -dr2;
          if (c + dc2 < 0 || c + dc2 >= board[0].length) dc2 = -dc2;
          r += dr2;
          c += dc2;
          const key = `${r},${c},${dr2},${dc2}`;
          if (seen.has(key) || blocked(r, c)) break;
          seen.add(key);
          if (r === to.row && c === to.col) return true;
          if (board[r]?.[c]) break;
        }
      }
      return false;
    }
    const dr = to.row - row, dc = to.col - col;
    if (!dr || !dc || Math.abs(dr) === Math.abs(dc)) {
      for (let n = 1; n < distance; n++) if (blocked(row + Math.sign(dr) * n, col + Math.sign(dc) * n)) return false;
    }
    return true;
  }
  const make = (id, name, phase, stars, text, effect, target = null, passive = false) => Object.freeze({
    id,
    name,
    phase,
    stars,
    text,
    effect,
    art: id,
    target,
    passive
  });
  const SEPTEMBER_CARD_DEFINITIONS = Object.freeze([
    make(
      "majesty",
      "위엄",
      "MIDDLE",
      3,
      "적군 메이저 피스가 아군 킹 주변 8칸으로 올 수 없습니다.",
      "majesty",
      null,
      true
    ),
    make(
      "infiltration",
      "잠입",
      "MIDDLE",
      3.5,
      "아군 기물이 상대 홈 랭크까지 침투할 경우, 주변에 기물이 없다면 잠복을 얻습니다.",
      "infiltration",
      null,
      true
    ),
    make(
      "killer-king",
      "킬러 킹",
      "MIDDLE",
      4,
      "아군 킹이 상대방 킹을 잡을 땐 룩처럼도 움직일 수 있습니다.",
      "killerKing",
      null,
      true
    ),
    make(
      "campfire",
      "캠프파이어",
      "PIECE",
      4,
      "아군 룩 하나를 지정해 캠프파이어로 변화시킵니다.",
      "campfire",
      "own-rook"
    ),
    make(
      "outpost",
      "전초기지",
      "MIDDLE",
      3.5,
      "상대 진영에 더 가까운 비킹 기물 하나를 선택해 보호를 부여합니다. 보호는 다음 움직임 전까지 유지됩니다.",
      "outpost",
      "own-outpost-piece"
    ),
    make(
      "hedgehog",
      "고슴도치",
      "PIECE",
      3.5,
      "퀸을 고슴도치로 변경합니다.",
      "hedgehog",
      "own-queen"
    ),
    make(
      "nullification",
      "상쇄",
      "MIDDLE",
      3.5,
      "원하는 아군 기물 하나에 상쇄를 부여합니다.",
      "nullification",
      "own-piece"
    ),
    make(
      "recurrence",
      "회귀",
      "END",
      4,
      "원하는 아군 기물 하나에 회귀를 부여합니다.",
      "recurrence",
      "own-piece"
    ),
    make(
      "princess",
      "프린세스",
      "PIECE",
      3.5,
      "룩 하나를 선택해 프린세스로 변경합니다.",
      "princess",
      "own-rook"
    ),
    make(
      "resolve",
      "결의",
      "MIDDLE",
      3.5,
      "아군 폰이 잡힌 턴에 가장 먼저 움직인 폰은 첫번째 이동에 한해 턴을 소모하지 않습니다.",
      "resolve",
      null,
      true
    ),
    make(
      "vanguard",
      "선봉",
      "MIDDLE",
      4,
      "제일 선두에 있는 아군 폰들이 랍스터처럼도 움직일 수 있습니다.",
      "vanguard",
      null,
      true
    ),
    make(
      "reversal",
      "반전",
      "END",
      4,
      "사용한 턴에 아군 룩과 비숍의 행마가 뒤바뀝니다.",
      "reversal"
    ),
    make(
      "miracle",
      "기적",
      "MIDDLE",
      4.5,
      "현재 아군 비숍이 바로 잡을 수 있는 기물들을 모두 전향시킵니다.",
      "miracle"
    ),
    make(
      "big-bishop",
      "BISHOP",
      "OPENING",
      4,
      "조금 큰 비숍을 가지고 게임을 시작합니다.",
      "bigBishop",
      null,
      true
    ),
    make(
      "overtake",
      "추월",
      "MIDDLE",
      4.5,
      "아군 룩이 아군 기물을 뛰어넘으며 이동할 수 있습니다.",
      "overtake",
      null,
      true
    )
  ]);
  const SEPTEMBER_RULE_DEFINITION = make(
    "capture-the-flag",
    "깃발 뽑기",
    "RULE",
    0,
    "양측 아군 홈 랭크에 랜덤으로 깃발 칸이 생깁니다. 적 기물이 깃발 칸에 들어온 뒤 1턴이 지나면 패배합니다.",
    "captureTheFlag"
  );
  const SEPTEMBER_ALL_DEFINITIONS = Object.freeze([...SEPTEMBER_CARD_DEFINITIONS, SEPTEMBER_RULE_DEFINITION]);
  Object.freeze(SEPTEMBER_ALL_DEFINITIONS.map((card) => card.id));
  const SEPTEMBER_PASSIVE_EFFECTS = Object.freeze(SEPTEMBER_CARD_DEFINITIONS.filter((card) => card.passive).map((card) => card.effect));
  function septemberPassiveState(source) {
    return Object.fromEntries(SEPTEMBER_PASSIVE_EFFECTS.filter((key) => key !== "bigBishop" && Object.hasOwn(source || {}, key)).map((key) => [key, { white: source[key]?.white === true, black: source[key]?.black === true }]));
  }
  function septemberCounterLimit(type) {
    return type === "hedgehog" ? 3 : type === "bear" ? 2 : 0;
  }
  // pieceHasCounterAbility (our_only_functions.txt) NOT grafted: checked
  // every one of its 5 call sites in engine.optimized.js (generateMovesForPiece,
  // resolveWorkerBearRetaliation, applyMoveAction's retaliationSurvivorIds,
  // recordWorkerNewCardCaptureReactions's deferBearReaction,
  // recordWorkerDirectCaptures) against the real file -- every single one
  // already has the exact equivalent inline (`Boolean(septemberCounterLimit(pieceAbilityType(item)))`,
  // behaviorally identical to `septemberCounterLimit(pieceAbilityType(item)) > 0`
  // since the limit is never negative), just not factored into a named
  // helper. Grafting the helper with no reachable call site would be dead
  // code, so this is NOT-NEEDED, not grafted.
  function septemberHomeRow(color, rowCount = 8) {
    return color === "white" ? rowCount - 1 : color === "black" ? 0 : null;
  }
  function septemberCellsAdjacent(a, b, orthogonal = false) {
    const dr = Math.abs(a.row - b.row), dc = Math.abs(a.col - b.col);
    return orthogonal ? dr + dc === 1 : Math.max(dr, dc) === 1;
  }
  function septemberMajestyBlocks({ enabled, major, destinations, royalCells }) {
    return Boolean(destinations.some((to) => royalCells.some((king) => septemberCellsAdjacent(to, king))));
  }
  function septemberKillerKingTargets({ from, color, at, isRoyal, rowCount = 8, colCount = 8 }) {
    const targets = [];
    for (const [dr, dc] of [[-1, 0], [1, 0], [0, -1], [0, 1]]) {
      for (let row = from.row + dr, col = from.col + dc; row >= 0 && row < rowCount && col >= 0 && col < colCount; row += dr, col += dc) {
        const target = at(row, col);
        if (!target) continue;
        if (target.color !== color && isRoyal(target, row, col)) targets.push({ row, col });
        break;
      }
    }
    return targets;
  }
  function septemberOutpostEligible(color, cells, rowCount = 8) {
    if (!["white", "black"].includes(color)) return false;
    return cells.some(({ row }) => color === "white" ? row < rowCount / 2 : row >= rowCount / 2);
  }
  function septemberInfiltrationReady({ enabled, color, cells, occupiedCells, rowCount = 8 }) {
    if (!enabled || !["white", "black"].includes(color)) return false;
    const home = septemberHomeRow(color === "white" ? "black" : "white", rowCount);
    if (!cells.some((cell) => cell.row === home)) return false;
    const body = new Set(cells.map(({ row, col }) => `${row},${col}`));
    return !occupiedCells.some((other) => !body.has(`${other.row},${other.col}`) && cells.some((cell) => septemberCellsAdjacent(cell, other)));
  }
  const septemberBoardActionOrigins = /* @__PURE__ */ new WeakMap();
  function septemberBoardEntries(board) {
    const entries = /* @__PURE__ */ new Map();
    board.forEach((line, row) => line.forEach((item, col) => {
      if (!item) return;
      const id = item.id || item;
      if (!entries.has(id)) entries.set(id, { id, item, color: item.color, cells: [] });
      entries.get(id).cells.push({ row, col });
    }));
    return [...entries.values()];
  }
  function septemberNewInfiltratorIds(before, after, enabled, rowCount = 8) {
    const origins = new Map(before.map((entry) => [entry.id, entry.cells]));
    const occupiedCells = after.flatMap((entry) => entry.cells);
    return after.filter((entry) => {
      const origin = origins.get(entry.id);
      if (!origin || origin.length === entry.cells.length && origin.every((cell, i) => cell.row === entry.cells[i].row && cell.col === entry.cells[i].col)) return false;
      return septemberInfiltrationReady({
        enabled: enabled?.[entry.color],
        color: entry.color,
        cells: entry.cells,
        occupiedCells,
        rowCount
      });
    }).map((entry) => entry.id);
  }
  function septemberBeginBoardAction(boardState) {
    if (boardState.infiltration?.white || boardState.infiltration?.black || boardState.board.some((line) => line.some((item) => item?.outpostProtected))) {
      septemberBoardActionOrigins.set(boardState, septemberBoardEntries(boardState.board));
    } else septemberBoardActionOrigins.delete(boardState);
  }
  function septemberResolveBoardInfiltration(boardState) {
    const before = septemberBoardActionOrigins.get(boardState);
    if (!before) return [];
    const after = septemberBoardEntries(boardState.board);
    const origins = new Map(before.map((entry) => [entry.id, entry.cells]));
    for (const entry of after) {
      const origin = origins.get(entry.id);
      if (entry.item.outpostProtected && origin && (origin.length !== entry.cells.length || origin.some((cell, i) => cell.row !== entry.cells[i].row || cell.col !== entry.cells[i].col))) delete entry.item.outpostProtected;
    }
    septemberBoardActionOrigins.set(boardState, after);
    const ids = new Set(septemberNewInfiltratorIds(before, after, boardState.infiltration, boardState.board.length));
    const granted = [];
    for (const entry of after) if (ids.has(entry.id) && !entry.item.submerged) {
      entry.item.submerged = true;
      granted.push(entry);
    }
    return granted;
  }
  function septemberCampfireProtects({ color, cells, campfireCells }) {
    return Boolean(["white", "black"].includes(color) && campfireCells.some((fire) => fire.color === color && cells.some((cell) => septemberCellsAdjacent(cell, fire, true))));
  }
  function septemberBoardCampfireProtects(board, target) {
    if (!target?.color || target.type === "scarecrow") return false;
    const entries = septemberBoardEntries(board);
    const body = entries.find((entry) => entry.item === target || target.id && entry.id === target.id);
    if (!body) return false;
    const campfireCells = entries.filter((entry) => entry.item !== target && (entry.item.type === "campfire" || entry.item.type === "trickster" && entry.item.tricksterMoveType === "campfire")).flatMap((entry) => entry.cells.map((cell) => ({ ...cell, color: entry.color })));
    return septemberCampfireProtects({ color: target.color, cells: body.cells, campfireCells });
  }
  function septemberNullificationBlocks(target, attacker) {
    return Boolean(target?.type !== "scarecrow" && target?.nullification && attacker && target.color !== attacker.color && target.type === attacker.type);
  }
  function septemberVanguardPawn(piece, row, pawns) {
    if (piece?.type !== "pawn" || !["white", "black"].includes(piece.color)) return false;
    return !pawns.some((other) => other.color === piece.color && (piece.color === "white" ? other.row < row : other.row > row));
  }
  function septemberPrincessHasQueenMovement(color, pieces) {
    return !pieces.some((piece) => piece.color === color && piece.type === "queen" && piece.regencyHeir !== true);
  }
  // Perf (2026-09-16, same pattern as boardHasPaladin/radianceCells above):
  // every call site was doing `board.flat().filter(Boolean)` -- a fresh
  // O(64) scan + array allocation -- then handing it to the plain function
  // above, PER CANDIDATE MOVE, for any princess on the board. Cache per
  // board object (safe for the same reason as the paladin cache: cloneState
  // makes a new board array per search node and this engine never mutates
  // a board array in place after that). Board-shaped-null-safe since one
  // call site (workerRangedPieceOptions) passes `boardState?.board` which
  // can be undefined.
  const PRINCESS_QUEEN_MOVEMENT_CACHE = /* @__PURE__ */ new WeakMap();
  function septemberPrincessHasQueenMovementCached(board, color) {
    if (!board) return true; // matches the old `[]` fallback: .some() on [] is false, !false is true
    let cache = PRINCESS_QUEEN_MOVEMENT_CACHE.get(board);
    if (!cache) {
      cache = {};
      PRINCESS_QUEEN_MOVEMENT_CACHE.set(board, cache);
    }
    if (cache[color] === void 0) {
      cache[color] = !board.some((line) => line.some((piece) => piece && piece.color === color && piece.type === "queen" && piece.regencyHeir !== true));
    }
    return cache[color];
  }
  function septemberRecurrenceCandidates({ color, size = 1, rowCount = 8, colCount = 8, isOpen }) {
    if (!["white", "black"].includes(color)) return [];
    const rows = Array.from({ length: Math.max(0, rowCount - size + 1) }, (_, row) => row);
    if (color === "white") rows.reverse();
    for (const row of rows) {
      const candidates = [];
      for (let col = 0; col <= colCount - size; col++) {
        const cells = Array.from({ length: size * size }, (_, i) => ({ row: row + Math.floor(i / size), col: col + i % size }));
        if (cells.every((cell) => isOpen(cell.row, cell.col))) candidates.push({ row, col, cells });
      }
      if (candidates.length) return candidates;
    }
    return [];
  }
  function septemberQueueRecurrence(boardState, piece, capturedBy) {
    if (!piece?.recurrence || !["white", "black"].includes(piece.color)) return false;
    if (!Array.isArray(boardState.pendingRecurrences)) boardState.pendingRecurrences = [];
    if (!boardState.pendingRecurrences.some((entry) => entry.piece.id === piece.id)) {
      const saved = structuredClone(piece);
      delete saved.recurrence;
      delete saved.outpostProtected;
      saved.moved = true;
      if (Number(saved.maxHp) > 0) saved.hp = saved.maxHp;
      boardState.pendingRecurrences.push({ piece: saved, capturedBy });
    }
    if (Array.isArray(boardState.undeadResurrections)) boardState.undeadResurrections = boardState.undeadResurrections.filter((entry) => entry.piece?.id !== piece.id);
    return true;
  }
  function septemberResolveBoardRecurrences(boardState, { isOpen, choose, onRevive = () => {
  } }) {
    if (!boardState.pendingRecurrences?.length) return [];
    const remaining = [], revived = [];
    for (const entry of boardState.pendingRecurrences) {
      const piece = entry.piece;
      if (septemberBoardEntries(boardState.board).some((existing) => existing.id === piece.id)) continue;
      const size = ["bigRook", "colossus", "bigBishop"].includes(piece.type) ? 2 : 1;
      const pool = septemberRecurrenceCandidates({
        color: piece.color,
        size,
        rowCount: boardState.board.length,
        colCount: boardState.board[0].length,
        isOpen: (row, col) => !boardState.board[row][col] && isOpen(row, col)
      });
      if (!pool.length) {
        remaining.push(entry);
        continue;
      }
      const destination = choose(pool, entry);
      for (const cell of destination.cells) boardState.board[cell.row][cell.col] = piece;
      if (size > 1) {
        piece.anchorRow = destination.row;
        piece.anchorCol = destination.col;
      }
      if (Array.isArray(boardState.captures?.[entry.capturedBy])) boardState.captures[entry.capturedBy] = boardState.captures[entry.capturedBy].filter((item) => item?.id !== piece.id);
      revived.push({ piece, ...destination });
      onRevive(piece, destination);
    }
    boardState.pendingRecurrences = remaining;
    return revived;
  }
  function septemberNotePawnCapture(state, piece) {
    const color = piece?.color;
    if (piece?.type !== "pawn" || !state.resolve?.[color]) return;
    if (state.resolveSpentTurn?.[color] === (Number(state.turnsTaken?.[color]) || 0)) return;
    if (!state.resolveReady) state.resolveReady = { white: false, black: false };
    state.resolveReady[color] = true;
  }
  function septemberConsumeResolveMove(state, color, movedType) {
    if (movedType !== "pawn" || !state.resolve?.[color] || !state.resolveReady?.[color]) return false;
    state.resolveReady[color] = false;
    if (!state.resolveSpentTurn) state.resolveSpentTurn = {};
    state.resolveSpentTurn[color] = Number(state.turnsTaken?.[color]) || 0;
    if (!state.resolveMoveCredit) state.resolveMoveCredit = { white: false, black: false };
    state.resolveMoveCredit[color] = true;
    return true;
  }
  function septemberUseResolveCredit(state, color) {
    if (!state.resolveMoveCredit?.[color]) return false;
    state.resolveMoveCredit[color] = false;
    state.enPassant = null;
    return true;
  }
  function septemberFinishResolveTurn(state, color) {
    if (state.resolveReady) state.resolveReady[color] = false;
    if (state.resolveMoveCredit) state.resolveMoveCredit[color] = false;
  }
  function septemberAdvanceFlags(rule, { at, actor, beforeTurns, afterTurns }) {
    if (!rule?.flags) return { rule, defeated: [] };
    const next = structuredClone(rule), defeated = [];
    if (!next.occupations) next.occupations = { white: null, black: null };
    for (const owner of ["white", "black"]) {
      const cell = next.flags[owner], occupant = cell ? at(cell.row, cell.col) : null;
      if (!occupant?.id || !["white", "black"].includes(occupant.color) || occupant.color === owner) {
        next.occupations[owner] = null;
        continue;
      }
      const previous = rule.occupations?.[owner];
      if (previous?.pieceId === occupant.id) {
        if ((Number(afterTurns[owner]) || 0) > previous.afterOwnerTurn) defeated.push(owner);
      } else {
        next.occupations[owner] = {
          pieceId: occupant.id,
          afterOwnerTurn: (Number(beforeTurns[owner]) || 0) + (actor === owner ? 1 : 0)
        };
      }
    }
    return { rule: next, defeated };
  }
  const MOVING_STREAK_TARGET = 5;
  function highGroundBlocksCaptureCell(from, landing, target, largeLanding, isHighGround) {
    if (!isHighGround(target.row, target.col)) return false;
    const source = largeLanding ? { row: from.row + target.row - landing.row, col: from.col + target.col - landing.col } : from;
    return !isHighGround(source.row, source.col);
  }
  function resolveWitchTrialCapture(piece, captured) {
    if (!captured || !piece?.witchTrial) return false;
    delete piece.witchTrial;
    piece.shielded = true;
    return true;
  }
  const CHAIN_MAX_DISTANCE = 2;
  const REAPER_CAPTURE_TARGET = 4;
  const RULE_BOMB_COUNT = 3;
  const SATURATION_CAPTURE_LIMIT = 3;
  const PERIODIC_COLLAPSE_INTERVAL = 20;
  const MISTAKE_CHANCE = 0.2;
  const MISTAKE_CARD_CHANCE = 0.5;
  const ROYAL_COMMAND_DURATION_TURNS = 2;
  const LAST_WARMTH_PIECE_LIMIT = 4;
  const EXHAUSTION_CONSECUTIVE_MOVE_LIMIT = 3;
  const PORTAL_SQUARE_NAMES = Object.freeze(["c3", "f6"]);
  const PARRY_CHANCE = 0.4;
  const PLATFORM_INTERVAL_TURNS = 5;
  function platformInterval(rule) {
    return rule?.countUnit === "ply" && rule?.cadence === "full-turn" ? PLATFORM_INTERVAL_TURNS * 2 : PLATFORM_INTERVAL_TURNS;
  }
  const SIREN_CONVERSION_TURNS = 2;
  const SIREN_CONVERSION_HALF_MOVES = SIREN_CONVERSION_TURNS;
  const UNDEAD_RESURRECTION_TURNS = 3;
  const UNDEAD_RESURRECTION_HALF_MOVES = UNDEAD_RESURRECTION_TURNS * 2;
  const OTHERWORLD_RETURN_HALF_TURNS = 14 * 2;
  const LOBSTER_SUMMON_HALF_TURNS = 2;
  const ULTIMATUM_DURATION_HALF_TURNS = 4 * 2;
  const QUEENS_GAMBIT_FIXED_FILE_COL = 3;
  const SPECIAL_PROMOTION_TYPES = Object.freeze(["queen", "rook", "bishop", "knight"]);
  const TURN_EXCLUSIVE_CARD_KEYS = Object.freeze(["trolley", "premove", "freeMove"]);
  function advanceCrownHoldEntry(entry, holderColor, currentSharedTurn, limit = 10) {
    const next = {
      ...entry || {},
      heldMoves: {
        white: Math.max(0, Number(entry?.heldMoves?.white) || 0),
        black: Math.max(0, Number(entry?.heldMoves?.black) || 0)
      }
    };
    if (!["white", "black"].includes(holderColor)) return next;
    const turn = Math.max(0, Math.floor(Number(currentSharedTurn) || 0));
    if (next.holdingColor !== holderColor) {
      next.holdingColor = holderColor;
      next.heldMoves.white = 0;
      next.heldMoves.black = 0;
      next.lastCountedMove = turn;
    }
    const elapsedMoves = Math.max(0, turn - Math.max(0, Number(next.lastCountedMove) || 0));
    next.lastCountedMove = turn;
    if (!elapsedMoves) return next;
    const other = holderColor === "white" ? "black" : "white";
    next.heldMoves[other] = 0;
    next.heldMoves[holderColor] = Math.min(
      Math.max(1, Math.floor(Number(limit) || 10)),
      next.heldMoves[holderColor] + elapsedMoves
    );
    return next;
  }
  function crownHoldWinningColor(entries, limit = 10) {
    const target = Math.max(1, Math.floor(Number(limit) || 10));
    return ["white", "black"].find((color) => (Array.isArray(entries) ? entries : []).some((entry) => entry?.holdingColor === color && Number(entry?.heldMoves?.[color]) >= target)) || "";
  }
  function collectColossusSummonVictims(board, cells, sacrifices = []) {
    if (!Array.isArray(board) || !Array.isArray(cells) || !Array.isArray(sacrifices)) return [];
    const victims = [];
    const byId = /* @__PURE__ */ new Map();
    const byObject = /* @__PURE__ */ new Map();
    const add = (item, row, col, sacrificed) => {
      if (!item || !Number.isInteger(row) || !Number.isInteger(col)) return;
      const existing = item.id ? byId.get(String(item.id)) : byObject.get(item);
      if (existing) {
        if (sacrificed) existing.sacrificed = true;
        return;
      }
      const entry = { item, row, col, sacrificed: Boolean(sacrificed) };
      victims.push(entry);
      if (item.id) byId.set(String(item.id), entry);
      else if (typeof item === "object") byObject.set(item, entry);
    };
    sacrifices.forEach(({ item, row, col }) => add(item, row, col, true));
    cells.forEach(({ row, col }) => add(board[row]?.[col] ?? null, row, col, false));
    return victims;
  }
  function isFieldPromotionRecyclingReady(piece, fieldPromotion, recycling = false) {
    return Boolean(
      recycling && piece?.type === "pawn" && (piece.color === "white" || piece.color === "black") && fieldPromotion?.[piece.color] && !piece.specialPromotionUsed && Math.max(0, Number(piece.totalCaptures) || 0) >= 2
    );
  }
  function radicalChargeHpHitCount(board, target, row, col, dr, dc) {
    if (!Array.isArray(board) || !target) return 0;
    const verticalLongLeg = Math.abs(dr) > Math.abs(dc);
    const crossedCells = verticalLongLeg ? [
      { row: row + Math.sign(dr), col },
      { row: row + Math.sign(dr), col: col + dc }
    ] : [
      { row, col: col + Math.sign(dc) },
      { row: row + dr, col: col + Math.sign(dc) }
    ];
    return crossedCells.filter((cell) => board[cell.row]?.[cell.col] === target).length;
  }
  const ENCYCLOPEDIA_PIECE_VALUES = Object.freeze({
    "queen": 9,
    "rook": 5,
    "bishop": 3,
    "missionary": 3,
    "knight": 3,
    "pawn": 1,
    "protestant": 3,
    "herald": 5,
    "cannon": 4,
    "fanatic": 1,
    "primeMinister": 9,
    "eagle": 2,
    "amazon": 13,
    "cardinal": 7,
    "pegasus": 5,
    "jester": 9,
    "camel": 2,
    "log": 2,
    "hook": 15,
    "grasshopper": 4,
    "dragon": 5,
    "man": 4,
    "assassin": 4,
    "reaper": 9,
    "knightmaster": 3,
    "standardBearer": 2,
    "guard": 2,
    "recruiter": 9,
    "squire": 1,
    "checker": 1,
    "checkerKing": 3,
    "wizard": 9,
    "alfil": 1,
    "windmill": 4,
    "idol": 9,
    "lobster": 2,
    "babyBear": 4,
    "bear": 17,
    "siegeRam": 5,
    "magicGirl": 6,
    "berserker": 7,
    "slime": 5,
    "siren": 9,
    "trickster": 5,
    "undead": 4,
    "campfire": 4,
    "hedgehog": 9,
    "princess": 6,
    "colossus": 12,
    "bigRook": 8,
    "bigBishop": 8
  });
  const SACRIFICE_PIECE_VALUES = ENCYCLOPEDIA_PIECE_VALUES;
  function pieceCombatValue(pieceOrType, state) {
    const raw = typeof pieceOrType === "string" ? pieceOrType : pieceOrType?.type;
    const type = canonicalChessNPow30PieceType(String(raw ?? "").replace(/-([a-z])/g, (_, letter) => letter.toUpperCase()));
    if (!usesSeptember18Balance(state) && ["grasshopper", "checker", "checkerKing", "campfire"].includes(type)) return type === "checker" ? 2 : type === "checkerKing" ? 4 : 5;
    return Object.hasOwn(ENCYCLOPEDIA_PIECE_VALUES, type) ? ENCYCLOPEDIA_PIECE_VALUES[type] : null;
  }
  const LEGACY_CHESS_N_POW_30_PIECE_TYPES = Object.freeze([
    "queen",
    "rook",
    "bishop",
    "missionary",
    "knight",
    "pawn",
    "protestant",
    "herald",
    "cannon",
    "fanatic",
    "primeMinister",
    "eagle",
    "amazon",
    "cardinal",
    "pegasus",
    "jester",
    "camel",
    "log",
    "hook",
    "grasshopper",
    "dragon",
    "man",
    "assassin",
    "reaper",
    "knightmaster",
    "standardBearer",
    "guard",
    "recruiter",
    "squire",
    "checker",
    "checkerKing",
    "wizard",
    "alfil",
    "windmill",
    "idol",
    "lobster",
    "babyBear",
    "bear",
    "siegeRam",
    "magicGirl",
    "berserker",
    "slime",
    "siren",
    "trickster",
    "undead"
  ]);
  const PRE_LATEST_CHESS_N_POW_30_PIECE_TYPES = Object.freeze([
    ...LEGACY_CHESS_N_POW_30_PIECE_TYPES,
    "campfire",
    "hedgehog",
    "princess"
  ]);
  const CHESS_N_POW_30_PIECE_TYPES = Object.freeze([
    ...PRE_LATEST_CHESS_N_POW_30_PIECE_TYPES,
    "thief",
    "paladin",
    "octopus",
    "parrot",
    "clockwork",
    "brutus"
  ]);
  const CHESS_N_POW_30_PIECE_VALUES = Object.freeze(Object.fromEntries(
    CHESS_N_POW_30_PIECE_TYPES.map((type) => [type, SACRIFICE_PIECE_VALUES[type]])
  ));
  const CHESS_N_POW_30_TYPE_ALIASES = Object.freeze({
    alibaba: "eagle",
    unicorn: "pegasus",
    logRolling: "log",
    windmillBishop: "windmill",
    windmillRook: "windmill"
  });
  function canonicalChessNPow30PieceType(pieceOrType) {
    const type = typeof pieceOrType === "string" ? pieceOrType : pieceOrType?.type;
    return type ? CHESS_N_POW_30_TYPE_ALIASES[type] || type : "";
  }
  const CHESS_N_POW_30_PIECES_PER_SIDE = 15;
  const CHESS_N_POW_30_MIN_TOTAL_VALUE = 39;
  const CHESS_N_POW_30_MAX_TOTAL_VALUE = 70;
  function chessNPow30LineupWays(pieceValues) {
    const ways = Array.from(
      { length: CHESS_N_POW_30_PIECES_PER_SIDE + 1 },
      () => Array(CHESS_N_POW_30_MAX_TOTAL_VALUE + 1).fill(0)
    );
    ways[0][0] = 1;
    for (let slots = 1; slots <= CHESS_N_POW_30_PIECES_PER_SIDE; slots += 1) {
      for (let total = 0; total <= CHESS_N_POW_30_MAX_TOTAL_VALUE; total += 1) {
        ways[slots][total] = Object.keys(pieceValues).reduce((sum, type) => {
          const value = pieceValues[type];
          return value <= total ? sum + ways[slots - 1][total - value] : sum;
        }, 0);
      }
    }
    return ways;
  }
  const CHESS_N_POW_30_LINEUP_WAYS = chessNPow30LineupWays(CHESS_N_POW_30_PIECE_VALUES);
  const PRE_BALANCE_CHAOS_VALUES = Object.freeze({ ...CHESS_N_POW_30_PIECE_VALUES, grasshopper: 5, checker: 2, checkerKing: 4, campfire: 5 });
  const PRE_EXPANSION_CHAOS_VALUES = Object.freeze(Object.fromEntries(LEGACY_CHESS_N_POW_30_PIECE_TYPES.map((type) => [type, PRE_BALANCE_CHAOS_VALUES[type]])));
  Object.freeze({ ...PRE_EXPANSION_CHAOS_VALUES, slime: 2 });
  Object.freeze(
    Array.from(
      { length: CHESS_N_POW_30_MAX_TOTAL_VALUE - CHESS_N_POW_30_MIN_TOTAL_VALUE + 1 },
      (_, index) => CHESS_N_POW_30_MIN_TOTAL_VALUE + index
    ).filter((total) => CHESS_N_POW_30_LINEUP_WAYS[CHESS_N_POW_30_PIECES_PER_SIDE][total] > 0)
  );
  const CHESS_N_POW_30_RANGED_TYPES = Object.freeze([
    "rook",
    "bishop",
    "queen",
    "bear",
    "amazon",
    "cardinal",
    "cannon",
    "herald",
    "hook",
    "protestant",
    "windmill",
    "jester",
    "idol"
  ]);
  new Set(CHESS_N_POW_30_RANGED_TYPES);
  const TRICKSTER_MOVEMENT_TYPES = Object.freeze(
    CHESS_N_POW_30_PIECE_TYPES.filter((type) => !["trickster", "log", "babyBear"].includes(type))
  );
  Object.freeze(TRICKSTER_MOVEMENT_TYPES.filter((type) => !["campfire", "hedgehog", "princess"].includes(type)));
  function tricksterAbilityType$1(piece) {
    if (piece?.type !== "trickster") return "";
    return TRICKSTER_MOVEMENT_TYPES.includes(piece.tricksterMoveType) ? piece.tricksterMoveType : "";
  }
  function pieceAbilityType(piece) {
    return tricksterAbilityType$1(piece) || String(piece?.type || "");
  }
  function pieceHasAbility(piece, type) {
    return Boolean(type) && pieceAbilityType(piece) === type;
  }
  function isSlimeSpecialMovementLocked(piece) {
    return pieceHasAbility(piece, "slime");
  }
  const RANGED_PIECE_TYPES = Object.freeze([
    "brutus",
    "bigBishop",
    "rook",
    "bishop",
    "queen",
    "bear",
    "amazon",
    "cardinal",
    "cannon",
    "herald",
    "hook",
    "protestant",
    "windmill",
    "windmillBishop",
    "windmillRook",
    "bigRook",
    "jester",
    "idol"
  ]);
  const RANGED_PIECE_TYPE_SET = new Set(RANGED_PIECE_TYPES);
  function isRangedPiece(piece, options = {}) {
    const type = String(options.type || piece?.type || "");
    if (RANGED_PIECE_TYPE_SET.has(type)) return true;
    if (type === "princess") return Boolean(options.princessQueenMovement);
    if (type === "magicGirl") return Boolean(options.magicGirlAwakened);
    if (type === "berserker") {
      return ["rook-king-step", "amazon"].includes(options.berserkerTier);
    }
    if (type !== "trickster") return false;
    const movementType = TRICKSTER_MOVEMENT_TYPES.includes(options.tricksterMoveType) ? options.tricksterMoveType : TRICKSTER_MOVEMENT_TYPES.includes(piece?.tricksterMoveType) ? piece.tricksterMoveType : "";
    if (!movementType || movementType === "trickster") return false;
    return isRangedPiece(piece, { ...options, type: movementType, tricksterMoveType: "" });
  }
  function isIceSheetEffectTargetPiece(piece, options = {}) {
    if (!piece) return false;
    if (["magicGirl", "berserker", "trickster"].includes(piece.type)) return true;
    return isRangedPiece(piece, options);
  }
  function queensGambitRandomFileCandidates(columnCount, fixedCol = QUEENS_GAMBIT_FIXED_FILE_COL, pawnFiles = []) {
    const count = Math.max(0, Math.floor(Number(columnCount) || 0));
    if (!count) return [];
    const normalizedFixedCol = Math.max(0, Math.min(count - 1, Math.floor(Number(fixedCol) || 0)));
    const allFiles = Array.from({ length: count }, (_, col) => col);
    const separatedFiles = allFiles.filter((col) => Math.abs(col - normalizedFixedCol) > 1);
    const differentFiles = allFiles.filter((col) => col !== normalizedFixedCol);
    const candidates = separatedFiles.length ? separatedFiles : differentFiles.length ? differentFiles : allFiles;
    const occupied = candidates.filter((col) => pawnFiles.includes(col));
    return occupied.length ? occupied : candidates;
  }
  function clearQueensGambitProtectionAfterTransformation(piece) {
    if (!piece?.queensGambitProtection) return false;
    const previousProtected = piece.queensGambitPreviousProtected === true;
    for (const timedProtection of [
      piece.lastResistance,
      piece.sacrificeProtection,
      piece.coronationProtection
    ]) {
      if (timedProtection && !previousProtected) timedProtection.previousProtected = false;
    }
    delete piece.queensGambitProtection;
    delete piece.queensGambitPreviousProtected;
    if (!previousProtected && !piece.lastResistance && !piece.sacrificeProtection && !piece.coronationProtection) {
      delete piece.protected;
    }
    return true;
  }
  function reconcileQueensGambitProtectionAfterTypeChange(piece, previousType, { promotion = false } = {}) {
    if (!piece?.queensGambitProtection || piece.type === previousType || promotion) return false;
    return clearQueensGambitProtectionAfterTransformation(piece);
  }
  const INSIGHT_PROTECTION_KEYS = Object.freeze([
    "shielded",
    "protected",
    "lastResistance",
    "sacrificeProtection",
    "coronationProtection",
    "queensGambitProtection"
  ]);
  const INSIGHT_ENEMY_POSITIVE_KEYS = Object.freeze([
    "parry",
    "loyalist",
    "submerged",
    "ghost",
    "poisonedPawn",
    "explosive",
    "chimera",
    "chameleon",
    "basicTraining",
    "trojanHorse",
    "holdoutPromotion",
    "royalCommand",
    "outpostProtected",
    "nullification",
    "recurrence"
  ]);
  const INSIGHT_ALLY_NEGATIVE_KEYS = Object.freeze([
    "disarmed",
    "severed",
    "inertia",
    "potionManner",
    "potionSaturation",
    "iceSheet",
    "witchTrial",
    "frozen",
    "frozenByCard",
    "bloodCurse"
  ]);
  function deleteTruthyPieceKeys(piece, keys) {
    let found = false;
    keys.forEach((key) => {
      if (!piece?.[key]) return;
      delete piece[key];
      found = true;
    });
    return found;
  }
  function clearInsightPieceEffects(piece, actingColor, options = {}) {
    const result = { removed: 0, evasionRemoved: false };
    if (!piece || !["white", "black"].includes(actingColor) || !["white", "black"].includes(piece.color)) return result;
    const enemyColor = actingColor === "white" ? "black" : "white";
    if (piece.color === enemyColor) {
      const hadPotionBasicTraining = Boolean(piece.potionBasicTraining);
      if (deleteTruthyPieceKeys(piece, INSIGHT_PROTECTION_KEYS)) result.removed += 1;
      if (piece.hiddenFrom === actingColor) {
        delete piece.hiddenFrom;
        result.removed += 1;
      }
      if (piece.evasion) {
        delete piece.evasion;
        result.evasionRemoved = true;
        result.removed += 1;
      }
      INSIGHT_ENEMY_POSITIVE_KEYS.forEach((key) => {
        if (!piece[key]) return;
        delete piece[key];
        result.removed += 1;
      });
      if (hadPotionBasicTraining && !piece.basicTraining) delete piece.potionBasicTraining;
      if (piece.twinBondId || piece.twinPartnerId || piece.twinSwapPending) {
        delete piece.twinBondId;
        delete piece.twinPartnerId;
        delete piece.twinSwapPending;
        result.removed += 1;
      }
      if (piece.feudalContractId) {
        delete piece.feudalContractId;
        result.removed += 1;
      }
      if (piece.quantum || piece.quantumNoCaptureUntil || piece.quantumFirstObservationFails) {
        delete piece.quantum;
        delete piece.quantumNoCaptureUntil;
        delete piece.quantumFirstObservationFails;
        result.removed += 1;
      }
      if (Array.isArray(piece.imperialMoves) && piece.imperialMoves.length > 0) {
        delete piece.imperialMoves;
        result.removed += 1;
      }
      if (piece.staked && (!piece.staked.by || piece.staked.by === enemyColor)) {
        delete piece.staked;
        result.removed += 1;
      }
      if (piece.vipInvitation && piece.vipInvitation.by === enemyColor) {
        delete piece.vipInvitation;
        result.removed += 1;
      }
      return result;
    }
    if (piece.color !== actingColor) return result;
    const hadPotionManner = Boolean(piece.potionManner);
    const hadPotionSaturation = Boolean(piece.potionSaturation);
    INSIGHT_ALLY_NEGATIVE_KEYS.forEach((key) => {
      if (!piece[key]) return;
      delete piece[key];
      result.removed += 1;
    });
    if (hadPotionManner && !piece.potionManner) piece.coolGuyCapturedLast = false;
    if (hadPotionSaturation && !piece.potionSaturation) piece.capturesMade = 0;
    if (piece.callingCard && (!piece.callingCard.by || piece.callingCard.by === enemyColor)) {
      delete piece.callingCard;
      result.removed += 1;
    }
    if (piece.emptyLunchbox && (!piece.emptyLunchbox.by || piece.emptyLunchbox.by === enemyColor)) {
      delete piece.emptyLunchbox;
      result.removed += 1;
    }
    if (piece.vipInvitation && piece.vipInvitation.by === enemyColor) {
      delete piece.vipInvitation;
      result.removed += 1;
    }
    if (piece.spyOwner && piece.spyOwner === enemyColor) {
      delete piece.spyOwner;
      result.removed += 1;
    }
    if ((Number(piece.poisonStunTurns) || 0) > 0) {
      delete piece.poisonStunTurns;
      delete piece.poisonStunColor;
      result.removed += 1;
    }
    if (options.coolGuy && piece.coolGuyCapturedLast) {
      piece.coolGuyCapturedLast = false;
      result.removed += 1;
    }
    if (options.saturationRule && (Number(piece.capturesMade) || 0) > 0) {
      piece.capturesMade = 0;
      result.removed += 1;
    }
    return result;
  }
  function cancelAllQuantumPhenomena(boardState) {
    if (!boardState || typeof boardState !== "object") return 0;
    const pending = boardState.quantumPending && typeof boardState.quantumPending === "object" ? boardState.quantumPending : {};
    boardState.quantumPending = { ...pending, white: false, black: false };
    const board = Array.isArray(boardState.board) ? boardState.board : [];
    const seen = /* @__PURE__ */ new Set();
    let cleared = 0;
    for (const row of board) {
      if (!Array.isArray(row)) continue;
      for (const piece of row) {
        if (!piece || typeof piece !== "object" || seen.has(piece)) continue;
        seen.add(piece);
        const hadQuantum = piece.quantum !== void 0 || piece.quantumNoCaptureUntil !== void 0 || piece.quantumFirstObservationFails !== void 0;
        delete piece.quantum;
        delete piece.quantumNoCaptureUntil;
        delete piece.quantumFirstObservationFails;
        if (hadQuantum) cleared += 1;
      }
    }
    return cleared;
  }
  function normalizeCardsUsedThisTurn(value) {
    return {
      white: Math.max(0, Math.floor(Number(value?.white) || 0)),
      black: Math.max(0, Math.floor(Number(value?.black) || 0))
    };
  }
  function isTurnExclusiveCardUseBlocked(cardKey, usedCount = 0) {
    return TURN_EXCLUSIVE_CARD_KEYS.includes(String(cardKey || "")) && Math.max(0, Math.floor(Number(usedCount) || 0)) > 0;
  }
  function isCenterTwoByTwoCell(row, col, rowCount = 8, colCount = 8) {
    if (![row, col, rowCount, colCount].every(Number.isInteger) || rowCount < 2 || colCount < 2) return false;
    const centerRows = /* @__PURE__ */ new Set([Math.floor((rowCount - 1) / 2), Math.ceil((rowCount - 1) / 2)]);
    const centerCols = /* @__PURE__ */ new Set([Math.floor((colCount - 1) / 2), Math.ceil((colCount - 1) / 2)]);
    return centerRows.has(row) && centerCols.has(col);
  }
  function nextPlatformTurn(currentTurn, interval = PLATFORM_INTERVAL_TURNS) {
    const current = Math.max(0, Math.floor(Number(currentTurn) || 0));
    const step = Math.max(1, Math.floor(Number(interval) || PLATFORM_INTERVAL_TURNS));
    return (Math.floor(current / step) + 1) * step;
  }
  function normalizePlatformRule(value, currentTurn = 0, rowCount = 8, colCount = 8) {
    if (!value || typeof value !== "object" || value.enabled === false) return null;
    const validCell = (cell2) => Boolean(
      cell2 && Number.isInteger(cell2.row) && Number.isInteger(cell2.col) && cell2.row >= 0 && cell2.row < rowCount && cell2.col >= 0 && cell2.col < colCount
    );
    const cells = [];
    const seen = /* @__PURE__ */ new Set();
    [value.cell, ...Array.isArray(value.cells) ? value.cells : []].forEach((entry) => {
      if (!validCell(entry)) return;
      const key = `${entry.row}:${entry.col}`;
      if (seen.has(key)) return;
      seen.add(key);
      cells.push({ row: entry.row, col: entry.col });
    });
    const cell = cells[0] || null;
    return {
      enabled: true,
      ...value.countUnit === "ply" ? { countUnit: "ply" } : {},
      ...value.cadence === "full-turn" ? { cadence: "full-turn" } : {},
      nextAt: Math.max(1, Math.floor(Number(value.nextAt) || nextPlatformTurn(currentTurn))),
      cell,
      cells,
      fixed: Boolean(value.fixed),
      spawnedAt: Math.max(0, Math.floor(Number(value.spawnedAt) || 0)),
      nonce: Math.max(0, Math.floor(Number(value.nonce) || 0)),
      triggeredIds: Array.isArray(value.triggeredIds) ? [...new Set(value.triggeredIds.filter((id) => typeof id === "string").slice(-128))] : []
    };
  }
  function platformTurnCount(turnsTaken, platform) {
    const white = Math.max(0, Number(turnsTaken?.white) || 0);
    const black = Math.max(0, Number(turnsTaken?.black) || 0);
    return platform?.countUnit === "ply" ? white + black : Math.min(white, black);
  }
  function promotionRankAdvance(earlyPromotion = false, fastGrowth = false) {
    return (earlyPromotion ? 2 : 0) + (fastGrowth ? 3 : 0);
  }
  function advancedPromotionRow(color, rowCount = 8, earlyPromotion = false, fastGrowth = false) {
    const lastRow = Math.max(0, Math.floor(Number(rowCount) || 0) - 1);
    const advance = Math.min(lastRow, promotionRankAdvance(earlyPromotion, fastGrowth));
    return color === "white" ? advance : lastRow - advance;
  }
  const LATEST_MERCHANT_GUILD_EXCLUSIVE_CARD_IDS = Object.freeze([
    "king-of-the-hill",
    "iron-monarch",
    "horse-riding",
    "racing-king",
    "switcheroo",
    "imperial-studies",
    "underground-bunker",
    "last-resistance",
    "black-magic",
    "knightmate",
    "qxe1",
    "castling",
    "encouragement"
  ]);
  Object.freeze([
    Object.freeze(["big-rook", "london-system", "horde"]),
    Object.freeze(["pawn-conversion", "en-passant-bang"]),
    Object.freeze(["democracy", "mongolian-gambit"]),
    Object.freeze(["democracy", "black-magic"]),
    Object.freeze(["democracy", "last-stand"]),
    ...LATEST_MERCHANT_GUILD_EXCLUSIVE_CARD_IDS.map((cardId) => Object.freeze(["merchant-guild", cardId]))
  ]);
  function isLastWarmthActive(pieceCount) {
    const count = Number(pieceCount);
    return Number.isFinite(count) && count <= LAST_WARMTH_PIECE_LIMIT;
  }
  function shouldRetainBackwardKnightTurn(backwardKnight, color, movedAsType, fromRow, toRow, capturedSomething, freeMoveActive = false) {
    if (freeMoveActive || !capturedSomething || movedAsType !== "knight" || !backwardKnight?.[color]) return false;
    const delta = Number(toRow) - Number(fromRow);
    if (!Number.isFinite(delta) || delta === 0) return false;
    if (color === "white") return delta > 0;
    if (color === "black") return delta < 0;
    return false;
  }
  function canTrojanHorseRecaptureAttacker(attacker) {
    return Boolean(attacker && !["colossus", "shotgunKing"].includes(attacker.type));
  }
  function isIndirectAttackImmunePiece(piece) {
    return Boolean(piece && ["guard", "jester"].includes(pieceAbilityType(piece)));
  }
  function isBombImmuneNeutralPiece(piece) {
    return Boolean(piece && ["football", "monster"].includes(piece.type));
  }
  function isGuardConvertiblePawnType(type) {
    return type === "pawn" || type === "fanatic";
  }
  function findGuardPawnByOrigin(candidates, kingOrigin, pawnDirection) {
    if (!Array.isArray(candidates) || !Number.isInteger(kingOrigin?.row) || !Number.isInteger(kingOrigin?.col)) return null;
    const direction = Math.sign(Number(pawnDirection) || 0);
    if (!direction) return null;
    const targetRow2 = kingOrigin.row + direction;
    return candidates.find((candidate) => candidate?.origin?.row === targetRow2 && candidate?.origin?.col === kingOrigin.col) || null;
  }
  const DIAGONAL_CHESS_CARD_SQUARES = Object.freeze({
    checker: Object.freeze({
      white: Object.freeze(["a5", "e1"]),
      black: Object.freeze(["h4", "d8"])
    }),
    elephantEscape: Object.freeze({
      white: Object.freeze(["a6", "f1"]),
      black: Object.freeze(["h3", "c8"])
    }),
    apprenticeKnights: Object.freeze({
      white: Object.freeze(["b4", "d2"]),
      black: Object.freeze(["g5", "e7"])
    })
  });
  function colossusOpeningAnchor(rowCount, colCount, color, diagonalChessActive = false) {
    const rows = Math.max(0, Number(rowCount) || 0);
    const cols = Math.max(0, Number(colCount) || 0);
    const col = Math.max(0, Math.floor(cols / 2) - 1);
    if (diagonalChessActive && rows === 8 && cols === 8) {
      return color === "white" ? { row: rows - 4, col } : { row: 2, col };
    }
    return color === "white" ? { row: Math.max(0, rows - 3), col } : { row: 1, col };
  }
  function colossusHordeSacrificeCells(rowCount, colCount, color, diagonalChessActive = false) {
    const rows = Math.max(0, Number(rowCount) || 0);
    const cols = Math.max(0, Number(colCount) || 0);
    const anchor = colossusOpeningAnchor(rows, cols, color, diagonalChessActive);
    const homeBodyRow = color === "white" ? anchor.row + 1 : anchor.row;
    return [
      { row: homeBodyRow, col: anchor.col - 1 },
      { row: anchor.row, col: anchor.col },
      { row: anchor.row + 1, col: anchor.col },
      { row: anchor.row, col: anchor.col + 1 },
      { row: anchor.row + 1, col: anchor.col + 1 },
      { row: homeBodyRow, col: anchor.col + 2 }
    ].filter(({ row, col }) => row >= 0 && row < rows && col >= 0 && col < cols);
  }
  function isColossusPawnPreserved({
    row,
    col,
    rookSquares = [],
    pawnDirection,
    colCount,
    diagonalChessActive = false
  } = {}) {
    const rooks = Array.isArray(rookSquares) ? rookSquares : [];
    if (diagonalChessActive && rooks.length) {
      return rooks.some((rook) => Number(rook?.row) + Number(pawnDirection) === Number(row) && Number(rook?.col) === Number(col));
    }
    const preservedCols = rooks.length ? new Set(rooks.map((rook) => Number(rook?.col))) : /* @__PURE__ */ new Set([0, Math.max(0, Number(colCount) - 1)]);
    return preservedCols.has(Number(col));
  }
  function selectColossusSacrificePawns(pawns, isPreserved, requiredCount = 6) {
    const entries = Array.isArray(pawns) ? pawns.filter(Boolean) : [];
    const required = Math.max(0, Math.floor(Number(requiredCount) || 0));
    if (required === 0) return [];
    const preserved = typeof isPreserved === "function" ? isPreserved : () => false;
    const centralPawns = [];
    const rookPawns = [];
    entries.forEach((entry) => {
      (preserved(entry) ? rookPawns : centralPawns).push(entry);
    });
    return [...centralPawns, ...rookPawns].slice(0, required);
  }
  const MISTAKE_IMMOBILE_TYPES = /* @__PURE__ */ new Set(["colossus", "bigRook", "bigBishop", "wall", "football", "monster", "blackHole", "coffin"]);
  const TRANSCENDENCE_UPGRADES = Object.freeze({
    pawn: Object.freeze(["knight", "bishop"]),
    knight: Object.freeze(["rook"]),
    bishop: Object.freeze(["rook"]),
    rook: Object.freeze(["queen"])
  });
  const DUTCH_TRANSFORM_TYPES = Object.freeze(["rook", "bishop", "knight"]);
  const DUTCH_TRANSFORM_TYPE_SET = new Set(DUTCH_TRANSFORM_TYPES);
  function mistakeRollTriggers(roll, chance = MISTAKE_CHANCE) {
    const value = Number(roll);
    const probability = Math.max(0, Math.min(1, Number(chance) || 0));
    return Number.isFinite(value) && value >= 0 && value < probability;
  }
  function isMistakeReversalTarget(piece) {
    return Boolean(
      piece && ["white", "black"].includes(piece.color) && !MISTAKE_IMMOBILE_TYPES.has(piece.type)
    );
  }
  function isDutchTransformPiece(piece, color = piece?.color) {
    return Boolean(piece && piece.color === color && DUTCH_TRANSFORM_TYPE_SET.has(piece.type));
  }
  function isCamouflageMatchingSquare(color, row, col) {
    if (!["white", "black"].includes(color) || !Number.isInteger(row) || !Number.isInteger(col)) return false;
    const lightSquare = (row + col) % 2 === 0;
    return color === "white" ? lightSquare : !lightSquare;
  }
  function hasDoubleCheckAttackers(attackers) {
    const entries = attackers instanceof Set ? [...attackers] : Array.isArray(attackers) ? attackers : [];
    return new Set(entries.filter((entry) => entry != null && entry !== "")).size >= 2;
  }
  function createRoyalCommandWindow(turnsTaken, duration = ROYAL_COMMAND_DURATION_TURNS) {
    const currentTurn = Math.max(0, Math.floor(Number(turnsTaken) || 0));
    const normalizedDuration = Math.max(1, Math.floor(Number(duration) || ROYAL_COMMAND_DURATION_TURNS));
    const activeTurn = currentTurn + 1;
    return { activeTurn, expiresTurn: activeTurn + normalizedDuration };
  }
  function normalizeRoyalCommandWindow(value) {
    const activeTurn = Number(value?.activeTurn);
    if (!Number.isInteger(activeTurn) || activeTurn < 0) return null;
    const storedExpiresTurn = Number(value?.expiresTurn);
    const expiresTurn = Number.isInteger(storedExpiresTurn) && storedExpiresTurn > activeTurn ? storedExpiresTurn : activeTurn + 1;
    return { activeTurn, expiresTurn };
  }
  function isRoyalCommandWindowActive(value, turnsTaken) {
    const window = normalizeRoyalCommandWindow(value);
    const currentTurn = Math.max(0, Math.floor(Number(turnsTaken) || 0));
    return Boolean(window && window.activeTurn <= currentTurn && currentTurn < window.expiresTurn);
  }
  function shouldExpireRoyalCommandWindowAfterTurn(value, turnsTaken) {
    const window = normalizeRoyalCommandWindow(value);
    const currentTurn = Math.max(0, Math.floor(Number(turnsTaken) || 0));
    return Boolean(window && currentTurn + 1 >= window.expiresTurn);
  }
  function moveThreatensSquare(move, targetRow2, targetCol2) {
    if (!move || !Number.isInteger(targetRow2) || !Number.isInteger(targetCol2)) return false;
    const matches = (cell) => cell?.row === targetRow2 && cell?.col === targetCol2;
    const directDestination = !move.shotgunBlast && !move.setLogDirection && !move.substitutionSwap && !move.dragonSwap && !move.switcherooMove && !move.merchantBuy ? move : null;
    const cells = [
      Number.isInteger(directDestination?.row) && Number.isInteger(directDestination?.col) ? directDestination : null,
      move.portalEntry,
      move.portalExit,
      Number.isInteger(move.capturedRow) && Number.isInteger(move.capturedCol) ? { row: move.capturedRow, col: move.capturedCol } : null,
      move.jumpCapture
    ];
    if (Array.isArray(move.sectorCells)) cells.push(...move.sectorCells);
    if (Array.isArray(move.highlightCells) && (move.shotgunBlast || move.colossusAttack)) {
      cells.push(...move.highlightCells);
    }
    if (Array.isArray(move.colossusLandingCaptures)) cells.push(...move.colossusLandingCaptures);
    if (Array.isArray(move.bigRookLandingCaptures)) cells.push(...move.bigRookLandingCaptures);
    return cells.some(matches);
  }
  function transcendenceUpgradeOptions(type) {
    return [...TRANSCENDENCE_UPGRADES[String(type || "")] || []];
  }
  function transcendenceUpgradeType(type, roll = 0) {
    const options = transcendenceUpgradeOptions(type);
    if (!options.length) return "";
    const value = Number(roll);
    const normalized = Number.isFinite(value) ? Math.max(0, Math.min(0.999999, value)) : 0;
    return options[Math.floor(normalized * options.length)] || options[0];
  }
  function monochromePieceType(type, enabled = false) {
    const normalized = String(type || "");
    return enabled && normalized === "knight" ? "camel" : normalized;
  }
  function normalizedBoardSize(value) {
    return Math.max(0, Math.floor(Number(value) || 0));
  }
  function normalizeArmistice(value) {
    const remaining = Math.max(0, Math.floor(Number(value?.remaining ?? value) || 0));
    if (!remaining) return null;
    const actedColors = [...new Set(
      (Array.isArray(value?.actedColors) ? value.actedColors : []).filter((color) => color === "white" || color === "black")
    )];
    return {
      remaining,
      by: value?.by === "white" || value?.by === "black" ? value.by : null,
      actedColors
    };
  }
  function isArmisticeCaptureBlocked(value, attacker, target) {
    return Boolean(
      normalizeArmistice(value) && attacker && target && ["white", "black"].includes(attacker.color) && ["white", "black"].includes(target.color) && attacker.color !== target.color
    );
  }
  function poisonStunTurns(piece) {
    return Math.max(0, Math.floor(Number(piece?.poisonStunTurns) || 0));
  }
  function isPoisonStunned(piece) {
    return poisonStunTurns(piece) > 0;
  }
  function shouldTickPoisonStun(piece, color) {
    if (!["white", "black"].includes(color) || !isPoisonStunned(piece)) return false;
    if (["white", "black"].includes(piece?.poisonStunColor)) {
      return piece.poisonStunColor === color;
    }
    return piece?.color === color || piece?.color === "neutral";
  }
  function normalizeExhaustionEntry(value) {
    return {
      enabled: Boolean(value?.enabled),
      pieceId: typeof value?.pieceId === "string" ? value.pieceId : "",
      count: Math.max(0, Math.floor(Number(value?.count) || 0))
    };
  }
  function normalizeExhaustionState(value) {
    return {
      white: normalizeExhaustionEntry(value?.white),
      black: normalizeExhaustionEntry(value?.black)
    };
  }
  function advanceExhaustionStreak(value, color, pieceId) {
    const state = normalizeExhaustionState(value);
    if (!["white", "black"].includes(color) || !state[color].enabled) return state;
    const id = String(pieceId || "");
    if (!id) return state;
    state[color] = state[color].pieceId === id ? { ...state[color], count: state[color].count + 1 } : { ...state[color], pieceId: id, count: 1 };
    return state;
  }
  function isExhaustionMoveBlocked(value, piece) {
    if (!piece || !["white", "black"].includes(piece.color)) return false;
    if (piece.regencyHeir || piece.crownRoyal || ["king", "royalKnight", "shotgunKing", "darkWizard"].includes(piece.type)) return false;
    const entry = normalizeExhaustionState(value)[piece.color];
    return Boolean(entry.enabled && entry.pieceId && entry.pieceId === piece.id && entry.count >= EXHAUSTION_CONSECUTIVE_MOVE_LIMIT);
  }
  function normalizeDemocracyState(value) {
    return {
      white: Boolean(value?.white),
      black: Boolean(value?.black)
    };
  }
  function isDemocracyProtectedMerchant(democracy, piece) {
    return false;
  }
  function isDemocracyProtectedRoyal(democracy, piece) {
    return Boolean(
      ["white", "black"].includes(piece?.color) && democracy?.[piece.color] && (piece.regencyHeir || piece.crownRoyal || ["king", "royalKnight", "shotgunKing"].includes(piece.type))
    );
  }
  function normalizePendingPortals(value, rowCount, colCount) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    return (Array.isArray(value) ? value : []).flatMap((entry, index) => {
      const color = entry?.color;
      const cells = Array.isArray(entry?.cells) ? entry.cells.map((cell) => ({ row: Number(cell?.row), col: Number(cell?.col) })).filter((cell) => Number.isInteger(cell.row) && Number.isInteger(cell.col) && cell.row >= 0 && cell.row < rows && cell.col >= 0 && cell.col < cols) : [];
      const unique = [...new Map(cells.map((cell) => [`${cell.row},${cell.col}`, cell])).values()];
      if (!["white", "black"].includes(color) || unique.length !== 2) return [];
      return [{
        id: String(entry?.id || `pending-portal-${index}`),
        ...entry?.blocksMovement === true ? { blocksMovement: true } : {},
        color,
        cells: unique,
        triggerTurn: Math.max(0, Math.floor(Number(entry?.triggerTurn) || 0))
      }];
    });
  }
  function portalSquareFromName(name, rowCount, colCount) {
    const match = String(name || "").match(/^([a-z])(\d+)$/i);
    if (!match) return null;
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    const col = match[1].toLowerCase().charCodeAt(0) - 97;
    const row = rows - Number(match[2]);
    if (row < 0 || row >= rows || col < 0 || col >= cols) return null;
    return { row, col };
  }
  function portalRuleCells(rowCount, colCount) {
    return PORTAL_SQUARE_NAMES.map((name) => portalSquareFromName(name, rowCount, colCount)).filter(Boolean);
  }
  function normalizePortalRule(value, rowCount, colCount) {
    if (!(value === true || value?.enabled)) return null;
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    const customCells = Array.isArray(value?.cells) ? value.cells.map((cell) => ({ row: Number(cell?.row), col: Number(cell?.col) })).filter((cell) => Number.isInteger(cell.row) && Number.isInteger(cell.col) && cell.row >= 0 && cell.row < rows && cell.col >= 0 && cell.col < cols) : [];
    const cells = customCells.length === 2 && (customCells[0].row !== customCells[1].row || customCells[0].col !== customCells[1].col) ? customCells : portalRuleCells(rowCount, colCount);
    return cells.length === PORTAL_SQUARE_NAMES.length ? { enabled: true, cells } : null;
  }
  function portalCounterpartCell(value, row, col, rowCount, colCount) {
    const rule = normalizePortalRule(value, rowCount, colCount);
    if (!rule || !Number.isInteger(row) || !Number.isInteger(col)) return null;
    const index = rule.cells.findIndex((cell) => cell.row === row && cell.col === col);
    if (index < 0) return null;
    return { ...rule.cells[(index + 1) % rule.cells.length] };
  }
  function isPortalCastlingRequestBlocked(from, to, portalCells) {
    if (!Number.isInteger(from?.row) || !Number.isInteger(from?.col) || !Number.isInteger(to?.row) || !Number.isInteger(to?.col) || from.row !== to.row || Math.abs(from.col - to.col) !== 2 || !Array.isArray(portalCells)) return false;
    return portalCells.some((cell) => cell?.row === to.row && cell?.col === to.col);
  }
  function portalRayContinuation(value, row, col, dr, dc, rowCount, colCount) {
    if (!Number.isInteger(dr) || !Number.isInteger(dc) || dr === 0 && dc === 0) return null;
    const exit = portalCounterpartCell(value, row, col, rowCount, colCount);
    if (!exit) return null;
    return {
      entry: { row, col },
      exit,
      next: { row: exit.row + dr, col: exit.col + dc }
    };
  }
  function canCannonPieceServeAsScreen(target, state = null) {
    if (usesUnifiedJumpObstacles(state)) return Boolean(target && target.type !== "cannon");
    return Boolean(
      target && (target.type === "wall" || !target.installationId) && !["football", "scarecrow"].includes(target.type) && (target.type !== "monster" || target.blackMagicMonster || target.blackMagicOwner)
    );
  }
  function cannonPathMoves({
    row,
    col,
    rowCount = 8,
    colCount = 8,
    portalRule = null,
    directions = [[-1, 0], [1, 0], [0, -1], [0, 1]],
    getOccupant = () => null,
    canCapture = () => false,
    isCannon = (target) => target?.type === "cannon",
    canServeAsScreen = (target) => !isCannon(target),
    isTransparent = () => false,
    isPortalBlocked = () => false
  } = {}) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    if (!Number.isInteger(row) || !Number.isInteger(col) || row < 0 || row >= rows || col < 0 || col >= cols) return [];
    const orthogonals2 = (directions || []).filter(([dr, dc]) => Number.isInteger(dr) && Number.isInteger(dc) && Math.abs(dr) + Math.abs(dc) === 1);
    const rule = normalizePortalRule(portalRule, rows, cols);
    const inBounds2 = (targetRow2, targetCol2) => targetRow2 >= 0 && targetRow2 < rows && targetCol2 >= 0 && targetCol2 < cols;
    const moves = [];
    for (const [dr, dc] of orthogonals2) {
      let nextRow = row + dr;
      let nextCol = col + dc;
      let jumped = false;
      let transit = null;
      while (inBounds2(nextRow, nextCol)) {
        const target = getOccupant(nextRow, nextCol);
        const transparent = Boolean(target && isTransparent(target, nextRow, nextCol));
        const portalExit = rule ? portalCounterpartCell(rule, nextRow, nextCol, rows, cols) : null;
        if (portalExit && transit) break;
        if (target && !transparent) {
          if (!jumped) {
            if (isCannon(target, nextRow, nextCol) || !canServeAsScreen(target, nextRow, nextCol)) break;
            jumped = true;
            nextRow += dr;
            nextCol += dc;
            continue;
          }
          if (!isCannon(target, nextRow, nextCol) && canCapture(target, nextRow, nextCol)) {
            moves.push({
              row: nextRow,
              col: nextCol,
              ...transit ? {
                portalThrough: true,
                portalEntry: { ...transit.entry },
                portalExit: { ...transit.exit }
              } : {}
            });
          }
          break;
        }
        if (jumped && !target) {
          moves.push({
            row: nextRow,
            col: nextCol,
            ...transit ? {
              portalThrough: true,
              portalEntry: { ...transit.entry },
              portalExit: { ...transit.exit }
            } : {}
          });
        }
        if (jumped && target && transparent && portalExit) {
          moves.push({ row: nextRow, col: nextCol, portalTransparentEntry: true });
        }
        if (portalExit) {
          if (isPortalBlocked(nextRow, nextCol) || isPortalBlocked(portalExit.row, portalExit.col)) break;
          const exitTarget = getOccupant(portalExit.row, portalExit.col);
          if (exitTarget && !isTransparent(exitTarget, portalExit.row, portalExit.col)) break;
          transit = {
            entry: { row: nextRow, col: nextCol },
            exit: { ...portalExit }
          };
          nextRow = portalExit.row + dr;
          nextCol = portalExit.col + dc;
          continue;
        }
        nextRow += dr;
        nextCol += dc;
      }
    }
    return moves;
  }
  function protestantPathMoves({
    row,
    col,
    rowCount = 8,
    colCount = 8,
    portalRule = null,
    directions = [[-1, -1], [-1, 1], [1, -1], [1, 1]],
    getOccupant = () => null,
    canCapture = () => false,
    isPortalBlocked = () => false
  }) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    if (!Number.isInteger(row) || !Number.isInteger(col) || row < 0 || row >= rows || col < 0 || col >= cols) return [];
    const rule = normalizePortalRule(portalRule, rows, cols);
    const moves = [];
    for (const [dr, dc] of directions) {
      if (!Number.isInteger(dr) || !Number.isInteger(dc) || Math.abs(dr) !== 1 || Math.abs(dc) !== 1) continue;
      let cursor = { row, col };
      let transit = null;
      for (let step = 1; step <= 3; step += 1) {
        cursor = { row: cursor.row + dr, col: cursor.col + dc };
        if (cursor.row < 0 || cursor.row >= rows || cursor.col < 0 || cursor.col >= cols) break;
        const target = getOccupant(cursor.row, cursor.col);
        if (!target || canCapture(target, cursor.row, cursor.col)) {
          moves.push({
            row: cursor.row,
            col: cursor.col,
            ...transit ? {
              portalThrough: true,
              portalEntry: { ...transit.entry },
              portalExit: { ...transit.exit }
            } : {}
          });
        }
        if (transit || !rule) continue;
        const exit = portalCounterpartCell(rule, cursor.row, cursor.col, rows, cols);
        if (!exit) continue;
        if (target || getOccupant(exit.row, exit.col) || isPortalBlocked(cursor.row, cursor.col) || isPortalBlocked(exit.row, exit.col)) break;
        transit = { entry: { ...cursor }, exit: { ...exit } };
        cursor = { ...exit };
      }
    }
    return moves;
  }
  const HOOK_ORTHOGONAL_DIRECTIONS = [[-1, 0], [1, 0], [0, -1], [0, 1]];
  const hookIcePathsByMove = /* @__PURE__ */ new WeakMap();
  function filterHookIceSheetTerminalMoves(moves, isEligible = () => true) {
    const list = Array.isArray(moves) ? moves : [];
    const maxStepByPath = /* @__PURE__ */ new Map();
    list.forEach((move) => {
      if (!isEligible(move)) return;
      (hookIcePathsByMove.get(move) || []).forEach((path) => {
        if (!path?.key || !Number.isInteger(path.step) || path.step < 1) return;
        maxStepByPath.set(path.key, Math.max(maxStepByPath.get(path.key) || 0, path.step));
      });
    });
    if (!maxStepByPath.size) return list;
    return list.filter((move) => {
      if (!isEligible(move)) return true;
      const paths = hookIcePathsByMove.get(move) || [];
      if (!paths.length) return true;
      return paths.some((path) => maxStepByPath.get(path.key) === path.step);
    });
  }
  function isInertiaOneStepMove(pieceType, row, col, move) {
    if (!Number.isInteger(row) || !Number.isInteger(col) || !move) return false;
    if (["hook", "brutus"].includes(pieceType) && (move.bent === true || move.portalThrough === true)) {
      return false;
    }
    const targetRow2 = Number.isInteger(move.anchorRow) ? move.anchorRow : move.row;
    const targetCol2 = Number.isInteger(move.anchorCol) ? move.anchorCol : move.col;
    return Number.isInteger(targetRow2) && Number.isInteger(targetCol2) && Math.max(Math.abs(targetRow2 - row), Math.abs(targetCol2 - col)) === 1;
  }
  function oneStepRestrictedMoveDistance(pieceType, row, col, destination) {
    const dr = Math.abs(destination.row - row);
    const dc = Math.abs(destination.col - col);
    return ["hook", "brutus"].includes(pieceType) ? dr + dc : Math.max(dr, dc);
  }
  function conveyorCellDestination(row, col, rowCount = 8, colCount = 8) {
    const rows = Math.max(0, Math.trunc(Number(rowCount) || 0));
    const cols = Math.max(0, Math.trunc(Number(colCount) || 0));
    const currentRow = Number(row);
    const currentCol = Number(col);
    if (rows < 2 || cols < 2 || !Number.isInteger(currentRow) || !Number.isInteger(currentCol)) return null;
    const lastRow = rows - 1;
    const lastCol = cols - 1;
    if (currentRow === 0 && currentCol < lastCol) return { row: currentRow, col: currentCol + 1 };
    if (currentCol === lastCol && currentRow < lastRow) return { row: currentRow + 1, col: currentCol };
    if (currentRow === lastRow && currentCol > 0) return { row: currentRow, col: currentCol - 1 };
    if (currentCol === 0 && currentRow > 0) return { row: currentRow - 1, col: currentCol };
    return null;
  }
  function advanceConveyorReservations(entries, rowCount = 8, colCount = 8) {
    if (!Array.isArray(entries)) return [];
    return entries.map((entry) => {
      if (!entry) return entry;
      const destination = conveyorCellDestination(entry.row, entry.col, rowCount, colCount);
      return destination ? { ...entry, ...destination } : entry;
    });
  }
  function conveyorEnPassantAfterMove(enPassant, pawn, square, captureDirection, rowCount = 8, colCount = 8) {
    const rows = Math.max(0, Math.trunc(Number(rowCount) || 0));
    const cols = Math.max(0, Math.trunc(Number(colCount) || 0));
    const capturedRow = Number(square?.row);
    const capturedCol = Number(square?.col);
    const direction = Number(captureDirection);
    if (!enPassant || !pawn || pawn.type !== "pawn" || pawn.color !== enPassant.color || !Number.isInteger(capturedRow) || !Number.isInteger(capturedCol) || !Number.isInteger(direction)) return null;
    const row = capturedRow + direction;
    if (row < 0 || row >= rows || capturedCol < 0 || capturedCol >= cols) return null;
    const hadAdditionalRight = Array.isArray(enPassant.additional) && enPassant.additional.length > 0;
    const additionalRow = capturedRow + direction * 2;
    return {
      ...enPassant,
      row,
      col: capturedCol,
      capturedRow,
      capturedCol,
      color: pawn.color,
      ...hadAdditionalRight && additionalRow >= 0 && additionalRow < rows ? {
        additional: [{
          row: additionalRow,
          col: capturedCol,
          capturedRow,
          capturedCol,
          color: pawn.color
        }]
      } : {}
    };
  }
  function retainSameTurnEnPassant(enPassant, movingColor) {
    if (!enPassant || enPassant.color !== movingColor) return null;
    return {
      ...enPassant,
      ...Array.isArray(enPassant.additional) ? {
        additional: enPassant.additional.map((entry) => ({ ...entry }))
      } : {}
    };
  }
  function enPassantRightEntries(enPassant) {
    if (!enPassant || typeof enPassant !== "object") return [];
    const entries = [enPassant];
    if (Array.isArray(enPassant.additional)) entries.push(...enPassant.additional);
    const seen = /* @__PURE__ */ new Set();
    return entries.filter((entry) => {
      if (!entry || typeof entry !== "object") return false;
      const values = [entry.row, entry.col, entry.capturedRow, entry.capturedCol];
      if (values.some((value) => !Number.isInteger(value)) || !["white", "black"].includes(entry.color)) return false;
      const key = `${entry.row},${entry.col},${entry.capturedRow},${entry.capturedCol},${entry.color}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function pawnEnPassantRightAfterAdvance(piece, from, to, move = {}, movedAsType = piece?.type) {
    const fromRow = Number(from?.row);
    const fromCol = Number(from?.col);
    const toRow = Number(to?.row);
    const toCol = Number(to?.col);
    if (!piece || piece.type !== "pawn" || movedAsType !== "pawn" || !Number.isInteger(fromRow) || !Number.isInteger(fromCol) || !Number.isInteger(toRow) || !Number.isInteger(toCol) || fromCol !== toCol) return null;
    const distance = Math.abs(toRow - fromRow);
    const eligible = distance === 2 && move.standardPawnDoubleStep === true || distance === 3 && move.pawnSprintTripleStep === true;
    if (!eligible) return null;
    const direction = Math.sign(toRow - fromRow);
    if (!direction) return null;
    return {
      row: toRow - direction,
      col: toCol,
      capturedRow: toRow,
      capturedCol: toCol,
      color: piece.color,
      ...distance === 3 ? {
        // Sprint crosses two squares. Both crossed squares may be used as the
        // en-passant landing square during the opponent's immediately following
        // turn, while either capture removes the pawn from its actual landing.
        additional: [{
          row: toRow - direction * 2,
          col: toCol,
          capturedRow: toRow,
          capturedCol: toCol,
          color: piece.color
        }]
      } : {}
    };
  }
  function hookPathMoves({
    row,
    col,
    rowCount,
    colCount,
    portalRule = null,
    directions = HOOK_ORTHOGONAL_DIRECTIONS,
    getOccupant = () => null,
    canCapture = () => false,
    isTransparent = () => false,
    isPortalBlocked = () => false
  } = {}) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    if (!Number.isInteger(row) || !Number.isInteger(col) || row < 0 || row >= rows || col < 0 || col >= cols) return [];
    const orthogonals2 = (directions || []).filter(([dr, dc]) => Number.isInteger(dr) && Number.isInteger(dc) && Math.abs(dr) + Math.abs(dc) === 1);
    const moves = [];
    const inBounds2 = (targetRow2, targetCol2) => targetRow2 >= 0 && targetRow2 < rows && targetCol2 >= 0 && targetCol2 < cols;
    const addMove = (targetRow2, targetCol2, bent, transit, pathKey, pathStep, extra = {}) => {
      const move = {
        row: targetRow2,
        col: targetCol2,
        ...bent ? { bent: true } : {},
        ...transit ? {
          portalThrough: true,
          portalEntry: { ...transit.entry },
          portalExit: { ...transit.exit }
        } : {},
        ...extra
      };
      if (pathKey) hookIcePathsByMove.set(move, [{ key: pathKey, step: pathStep }]);
      moves.push(move);
    };
    const walk = (startRow, startCol, dr, dc, bent = false, transit = null, pathKey = "", pathStep = 1) => {
      let nextRow = startRow;
      let nextCol = startCol;
      let portalTransit = transit;
      let nextPathStep = pathStep;
      while (inBounds2(nextRow, nextCol)) {
        const context = { row: nextRow, col: nextCol, bent, portalTransit };
        const target = getOccupant(nextRow, nextCol);
        const transparent = Boolean(target && isTransparent(target, context));
        const portalExit = portalCounterpartCell(portalRule, nextRow, nextCol, rows, cols);
        if (portalExit && portalTransit) break;
        if (target && !transparent) {
          if (canCapture(target, context)) addMove(nextRow, nextCol, bent, portalTransit, pathKey, nextPathStep);
          break;
        }
        if (target && transparent && portalExit) addMove(nextRow, nextCol, bent, portalTransit, pathKey, nextPathStep, { portalTransparentEntry: true });
        if (!target) addMove(nextRow, nextCol, bent, portalTransit, pathKey, nextPathStep);
        if (portalExit) {
          if (isPortalBlocked(nextRow, nextCol) || isPortalBlocked(portalExit.row, portalExit.col)) break;
          const exitContext = { row: portalExit.row, col: portalExit.col, bent, portalTransit };
          const exitTarget = getOccupant(portalExit.row, portalExit.col);
          const exitTransparent = Boolean(exitTarget && isTransparent(exitTarget, exitContext));
          if (exitTarget && !exitTransparent) break;
          portalTransit = {
            entry: { row: nextRow, col: nextCol },
            exit: { ...portalExit }
          };
          if (!bent && !exitTarget) {
            orthogonals2.forEach(([turnR, turnC]) => {
              if (turnR * dr + turnC * dc !== 0) return;
              const bentPathKey = `${pathKey}|bend:${portalExit.row},${portalExit.col}:${turnR},${turnC}`;
              walk(portalExit.row + turnR, portalExit.col + turnC, turnR, turnC, true, portalTransit, bentPathKey, 1);
            });
          }
          nextRow = portalExit.row + dr;
          nextCol = portalExit.col + dc;
          nextPathStep += 1;
          continue;
        }
        if (!bent && !target) {
          orthogonals2.forEach(([turnR, turnC]) => {
            if (turnR * dr + turnC * dc !== 0) return;
            const bentPathKey = `${pathKey}|bend:${nextRow},${nextCol}:${turnR},${turnC}`;
            walk(nextRow + turnR, nextCol + turnC, turnR, turnC, true, portalTransit, bentPathKey, 1);
          });
        }
        nextRow += dr;
        nextCol += dc;
        nextPathStep += 1;
      }
    };
    orthogonals2.forEach(([dr, dc]) => walk(row + dr, col + dc, dr, dc, false, null, `straight:${dr},${dc}`, 1));
    const uniqueMoves2 = [];
    const moveByDestination = /* @__PURE__ */ new Map();
    moves.forEach((move) => {
      const destinationKey = `${move.row}:${move.col}`;
      const existing = moveByDestination.get(destinationKey);
      if (!existing) {
        moveByDestination.set(destinationKey, move);
        uniqueMoves2.push(move);
        return;
      }
      const mergedPaths = new Map(
        (hookIcePathsByMove.get(existing) || []).map((path) => [path.key, path])
      );
      (hookIcePathsByMove.get(move) || []).forEach((path) => {
        const current = mergedPaths.get(path.key);
        if (!current || path.step > current.step) mergedPaths.set(path.key, path);
      });
      hookIcePathsByMove.set(existing, [...mergedPaths.values()]);
    });
    return uniqueMoves2;
  }
  function ruleBombCandidateCells(rowCount, colCount) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    const rankRows = [.../* @__PURE__ */ new Set([rows - 4, rows - 5])].filter((row) => row >= 0 && row < rows);
    const cells = [];
    rankRows.forEach((row) => {
      for (let col = 0; col < cols; col += 1) {
        if (col === 4) continue;
        cells.push({ row, col });
      }
    });
    return cells;
  }
  function normalizeRuleBombs(value, rowCount, colCount, limit = RULE_BOMB_COUNT) {
    if (!Array.isArray(value)) return [];
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    const maxBombs = Math.max(0, Math.min(rows * cols, Math.floor(Number(limit) || 0)));
    const seen = /* @__PURE__ */ new Set();
    const bombs = [];
    value.forEach((entry, index) => {
      if (bombs.length >= maxBombs) return;
      const row = Number(entry?.row);
      const col = Number(entry?.col);
      const key = `${row}:${col}`;
      if (!Number.isInteger(row) || !Number.isInteger(col) || row < 0 || row >= rows || col < 0 || col >= cols || seen.has(key)) return;
      seen.add(key);
      const bomb = {
        id: typeof entry?.id === "string" && entry.id ? entry.id.slice(0, 160) : `rule-bomb-${index}-${row}-${col}`,
        row,
        col
      };
      if (typeof entry?.ignorePieceId === "string" && entry.ignorePieceId) {
        bomb.ignorePieceId = entry.ignorePieceId.slice(0, 160);
      }
      bombs.push(bomb);
    });
    return bombs;
  }
  function maxCollapseDepth(rowCount, colCount) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    return Math.ceil(Math.min(rows, cols) / 2);
  }
  function normalizeCollapseDepth(value, rowCount, colCount, legacyCollapsed = false) {
    const fallback = legacyCollapsed ? 1 : 0;
    const depth = Math.max(0, Math.floor(Number(value) || fallback));
    return Math.min(maxCollapseDepth(rowCount, colCount), depth);
  }
  function isCollapsedAtDepth(row, col, rowCount, colCount, depth) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    const normalizedDepth = normalizeCollapseDepth(depth, rows, cols);
    if (!Number.isInteger(row) || !Number.isInteger(col) || row < 0 || row >= rows || col < 0 || col >= cols) return false;
    return normalizedDepth > 0 && (row < normalizedDepth || row >= rows - normalizedDepth || col < normalizedDepth || col >= cols - normalizedDepth);
  }
  function collapseRingCells(rowCount, colCount, depth = 0) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    const ring = Math.max(0, Math.floor(Number(depth) || 0));
    if (ring >= maxCollapseDepth(rows, cols)) return [];
    const minRow = ring;
    const maxRow = rows - 1 - ring;
    const minCol = ring;
    const maxCol = cols - 1 - ring;
    if (minRow > maxRow || minCol > maxCol) return [];
    const cells = [];
    for (let row = minRow; row <= maxRow; row += 1) {
      for (let col = minCol; col <= maxCol; col += 1) {
        if (row === minRow || row === maxRow || col === minCol || col === maxCol) cells.push({ row, col });
      }
    }
    return cells;
  }
  function nextPeriodicCollapseTurn(currentTurn, interval = PERIODIC_COLLAPSE_INTERVAL) {
    const current = Math.max(0, Math.floor(Number(currentTurn) || 0));
    const span = Math.max(1, Math.floor(Number(interval) || PERIODIC_COLLAPSE_INTERVAL));
    return (Math.floor(current / span) + 1) * span;
  }
  const RANDOM_ROULETTE_MAJOR_TYPES = Object.freeze([
    "hedgehog",
    "princess",
    "bigBishop",
    "queen",
    "rook",
    "amazon",
    "man",
    "colossus",
    "bigRook",
    "herald",
    "jester",
    "recruiter",
    "hook",
    "primeMinister",
    "assassin",
    "wizard",
    "windmill",
    "crown",
    "bear",
    "magicGirl",
    "berserker",
    "siren",
    "trickster",
    "reaper",
    "undead"
  ]);
  const RANDOM_ROULETTE_PIECE_TYPES = Object.freeze([
    "pawn",
    "knight",
    "bishop",
    "rook",
    "queen",
    "colossus",
    "bigRook",
    "protestant",
    "herald",
    "cannon",
    "fanatic",
    "primeMinister",
    "eagle",
    "amazon",
    "cardinal",
    "pegasus",
    "jester",
    "camel",
    "log",
    "hook",
    "grasshopper",
    "dragon",
    "man",
    "assassin",
    "knightmaster",
    "standardBearer",
    "recruiter",
    "squire",
    "checker",
    "checkerKing",
    "wizard",
    "alfil",
    "bat",
    "guard",
    "reaper",
    "idol",
    "babyBear",
    "lobster",
    "missionary",
    "bear",
    "windmill",
    "siegeRam",
    "magicGirl",
    "berserker",
    "slime",
    "siren",
    "trickster",
    "undead",
    "campfire",
    "hedgehog",
    "princess"
  ]);
  const RANDOM_ROULETTE_MAJOR_TYPE_SET = new Set(RANDOM_ROULETTE_MAJOR_TYPES);
  const RANDOM_ROULETTE_LARGE_TYPE_SET = /* @__PURE__ */ new Set(["colossus", "bigRook", "bigBishop"]);
  const IRON_MONARCH_KING_TYPES = /* @__PURE__ */ new Set(["king", "royalKnight", "shotgunKing", "darkWizard"]);
  const IRON_MONARCH_CAPTURE_TYPES = /* @__PURE__ */ new Set(["pawn", "fanatic"]);
  const DESPERADO_ROYAL_TYPES = /* @__PURE__ */ new Set(["king", "royalKnight", "shotgunKing", "darkWizard"]);
  function isDesperadoRoyalCaptureBlocked(attacker, target) {
    return Boolean(
      attacker?.desperado && target && (target.regencyHeir || target.crownRoyal || DESPERADO_ROYAL_TYPES.has(target.type))
    );
  }
  function isIronMonarchExtraMoveCapture(enabledByColor, piece, directCapturedPiece) {
    return Boolean(
      piece?.color && enabledByColor?.[piece.color] && (piece.regencyHeir || IRON_MONARCH_KING_TYPES.has(piece.type)) && directCapturedPiece && directCapturedPiece.color !== piece.color && IRON_MONARCH_CAPTURE_TYPES.has(directCapturedPiece.type)
    );
  }
  function normalizeCollapsedCells(value, rowCount, colCount) {
    const rows = normalizedBoardSize(rowCount);
    const cols = normalizedBoardSize(colCount);
    if (!Array.isArray(value)) return [];
    const seen = /* @__PURE__ */ new Set();
    return value.reduce((cells, entry) => {
      const row = Number(entry?.row);
      const col = Number(entry?.col);
      const key = `${row}:${col}`;
      if (!Number.isInteger(row) || !Number.isInteger(col) || row < 0 || row >= rows || col < 0 || col >= cols || seen.has(key)) return cells;
      seen.add(key);
      cells.push({ row, col });
      return cells;
    }, []);
  }
  function isCollapsedBoardCell(row, col, rowCount, colCount, depth, collapsedCells = []) {
    if (isCollapsedAtDepth(row, col, rowCount, colCount, depth)) return true;
    return normalizeCollapsedCells(collapsedCells, rowCount, colCount).some((cell) => cell.row === row && cell.col === col);
  }
  function isRandomRouletteTargetPiece(piece) {
    return Boolean(
      piece && ["white", "black"].includes(piece.color) && !piece.regencyHeir && !piece.crownRoyal && RANDOM_ROULETTE_MAJOR_TYPE_SET.has(piece.type)
    );
  }
  function randomRouletteOutcomeTypes(currentType = "", allowLarge = true) {
    return RANDOM_ROULETTE_PIECE_TYPES.filter((type) => type !== currentType && (allowLarge || !RANDOM_ROULETTE_LARGE_TYPE_SET.has(type)));
  }
  function randomRouletteInitialPieceState(type, currentSharedTurn = 0) {
    const sharedTurn = Math.max(0, Math.floor(Number(currentSharedTurn) || 0));
    if (type === "colossus") return { hp: 3, maxHp: 3 };
    if (["bigRook", "bigBishop"].includes(type)) return { hp: 2, maxHp: 2 };
    if (type === "wizard") return { mana: 0, maxMana: 5 };
    if (type === "windmill") return { windmillMode: "bishop" };
    if (type === "log") return { logDir: null };
    if (type === "babyBear") return { babyBearGrowAtTurn: sharedTurn + 7 };
    if (type === "bear") return { bearRetaliationsRemaining: 2 };
    if (type === "hedgehog") return { bearRetaliationsRemaining: 3 };
    return {};
  }
  const OVERWHELM_KING_TYPES = /* @__PURE__ */ new Set([
    "king",
    "royalKnight",
    "shotgunKing",
    "merchant",
    "timeTraveler",
    "vampireLord"
  ]);
  const HIGHWAY_DIRECT_CAPTURE_FORBIDDEN_TYPES = /* @__PURE__ */ new Set([
    "guard",
    "recruiter",
    "wizard",
    "herald",
    "merchant",
    "coffin",
    "scarecrow",
    "blackHole",
    "timeAfterimage"
  ]);
  const JESTER_HIGHWAY_CAPTURE_TARGET_TYPES = /* @__PURE__ */ new Set([
    "king",
    "royalKnight",
    "shotgunKing",
    "merchant"
  ]);
  function isHighwayCaptureAllowed(attacker, target) {
    const attackerType = attacker?.type;
    if (!attackerType || HIGHWAY_DIRECT_CAPTURE_FORBIDDEN_TYPES.has(attackerType)) return false;
    if (attackerType !== "jester") return true;
    return Boolean(target && JESTER_HIGHWAY_CAPTURE_TARGET_TYPES.has(target.type));
  }
  function isOverwhelmKingPiece(piece) {
    return Boolean(piece && (piece.regencyHeir || OVERWHELM_KING_TYPES.has(piece.type)));
  }
  function isOverwhelmRoyalTarget(piece) {
    return Boolean(piece && (piece.type === "queen" && piece.regencyHeir !== true || isOverwhelmKingPiece(piece)));
  }
  function isOverwhelmCaptureBlocked(overwhelm, attacker, target) {
    return Boolean(
      attacker?.color && target?.color && attacker.color !== target.color && overwhelm?.[target.color] && isOverwhelmKingPiece(attacker) && isOverwhelmRoyalTarget(target)
    );
  }
  function isQueensGambitProtectedPawn(piece, col, color = piece?.color, queenCol = 3, randomCol = 2) {
    return Boolean(
      piece && piece.color === color && piece.type === "pawn" && (col === randomCol || col === queenCol)
    );
  }
  function isFreeMovePlanDue(entry, movingColor, turnsTaken = {}) {
    if (!entry || !["white", "black"].includes(movingColor)) return false;
    const triggerTurn = Math.max(1, Math.floor(Number(entry.triggerTurn) || 1));
    return entry.triggerColor === movingColor && (Number(turnsTaken?.[movingColor]) || 0) >= triggerTurn;
  }
  function normalizePendingIcbm(value) {
    if (!Array.isArray(value)) return [];
    return value.slice(0, 32).flatMap((entry, index) => {
      const color = entry?.color === "black" ? "black" : entry?.color === "white" ? "white" : "";
      const sourceQueenId = typeof entry?.sourceQueenId === "string" ? entry.sourceQueenId.slice(0, 160) : "";
      const targetQueenId = typeof entry?.targetQueenId === "string" ? entry.targetQueenId.slice(0, 160) : "";
      if (!color || !sourceQueenId || !targetQueenId) return [];
      return [{
        id: typeof entry?.id === "string" && entry.id ? entry.id.slice(0, 160) : `icbm-${index}-${sourceQueenId}-${targetQueenId}`.slice(0, 160),
        color,
        triggerTurn: Math.max(1, Math.floor(Number(entry?.triggerTurn) || 1)),
        sourceQueenId,
        targetQueenId
      }];
    });
  }
  function isPendingIcbmDue(entry, color, turnsTaken = {}) {
    if (!entry || entry.color !== color || !["white", "black"].includes(color)) return false;
    return (Number(turnsTaken?.[color]) || 0) >= Math.max(1, Math.floor(Number(entry.triggerTurn) || 1));
  }
  function advanceReaperCaptureCount(value, target = REAPER_CAPTURE_TARGET) {
    const next = Math.max(0, Math.floor(Number(value) || 0)) + 1;
    const limit = Math.max(1, Math.floor(Number(target) || REAPER_CAPTURE_TARGET));
    return { count: Math.min(next, limit), triggered: next >= limit };
  }
  function isFreeMoveCaptureBlocked(value, attacker, target) {
    const color = attacker?.color;
    const opposingColor = color === "white" ? "black" : color === "black" ? "white" : null;
    return Boolean(opposingColor && value?.[color] === true && target?.color === opposingColor);
  }
  function chaosChessHalfTurnCount(turnsTaken = {}) {
    return ["white", "black"].reduce(
      (total, color) => total + Math.max(0, Math.floor(Number(turnsTaken?.[color]) || 0)),
      0
    );
  }
  function isChaosChessCaptureBlocked(untilHalfTurn, turnsTaken = {}) {
    const deadline = Number(untilHalfTurn);
    return Number.isFinite(deadline) && deadline > chaosChessHalfTurnCount(turnsTaken);
  }
  function reaperSoulCountsCapture(reaperColor) {
    return ["white", "black"].includes(reaperColor);
  }
  function saturationCaptureCount(pieceOrValue, limit = SATURATION_CAPTURE_LIMIT) {
    const value = typeof pieceOrValue === "object" && pieceOrValue ? pieceOrValue.capturesMade : pieceOrValue;
    const maximum = Math.max(1, Math.floor(Number(limit) || SATURATION_CAPTURE_LIMIT));
    return Math.min(maximum, Math.max(0, Math.floor(Number(value) || 0)));
  }
  function advanceSaturationCaptureCount(value, amount = 1, limit = SATURATION_CAPTURE_LIMIT) {
    const maximum = Math.max(1, Math.floor(Number(limit) || SATURATION_CAPTURE_LIMIT));
    const added = Math.max(0, Math.floor(Number(amount) || 0));
    const count = Math.max(0, Math.floor(Number(value) || 0)) + added;
    return { count, saturated: count >= maximum };
  }
  function isSaturationCaptureLocked(enabled, piece, limit = SATURATION_CAPTURE_LIMIT) {
    return Boolean(enabled && piece && saturationCaptureCount(piece, limit) >= Math.max(1, Math.floor(Number(limit) || SATURATION_CAPTURE_LIMIT)));
  }
  function isFreeMoveSourceIntact(plan, located, color) {
    return Boolean(
      plan?.pieceId && located?.item?.id === plan.pieceId && located.item.color === color && located.row === plan.from?.row && located.col === plan.from?.col
    );
  }
  function findFreeMoveLegalCandidate(plan, legalMoves = []) {
    if (!plan?.to || !Array.isArray(legalMoves)) return null;
    return legalMoves.find((move) => Number.isInteger(move?.row) && Number.isInteger(move?.col) && move.row === plan.to.row && move.col === plan.to.col) || null;
  }
  const NON_MOVING_BOARD_TYPES = /* @__PURE__ */ new Set(["wall", "football", "monster", "blackHole"]);
  const LARGE_BOARD_TYPES = /* @__PURE__ */ new Set(["colossus", "bigRook", "big-rook", "bigBishop", "big-bishop"]);
  function adjacentEscapeSquares(row, col, rowCount, colCount) {
    const squares = [];
    for (let dr = -1; dr <= 1; dr += 1) {
      for (let dc = -1; dc <= 1; dc += 1) {
        if (dr === 0 && dc === 0) continue;
        const nextRow = row + dr;
        const nextCol = col + dc;
        if (nextRow < 0 || nextRow >= rowCount || nextCol < 0 || nextCol >= colCount) continue;
        squares.push({ row: nextRow, col: nextCol });
      }
    }
    return squares;
  }
  function monsterStepCandidates(row, col, rowCount, colCount, isOpen = () => true) {
    return adjacentEscapeSquares(row, col, rowCount, colCount).filter((square) => isOpen(square.row, square.col));
  }
  function normalizeMovingEntry(value) {
    return {
      enabled: Boolean(value?.enabled),
      pieceId: typeof value?.pieceId === "string" ? value.pieceId : "",
      count: Math.max(0, Math.min(MOVING_STREAK_TARGET - 1, Math.floor(Number(value?.count) || 0)))
    };
  }
  function normalizeMovingState(value) {
    return {
      white: normalizeMovingEntry(value?.white),
      black: normalizeMovingEntry(value?.black)
    };
  }
  function advanceMovingStreak(value, pieceId, target = MOVING_STREAK_TARGET) {
    const entry = normalizeMovingEntry(value);
    const normalizedTarget = Math.max(1, Math.floor(Number(target) || MOVING_STREAK_TARGET));
    const normalizedPieceId = typeof pieceId === "string" ? pieceId : "";
    if (!entry.enabled || !normalizedPieceId) return { entry, triggered: false };
    const count = entry.pieceId === normalizedPieceId ? entry.count + 1 : 1;
    if (count >= normalizedTarget) {
      return {
        entry: { enabled: true, pieceId: "", count: 0 },
        triggered: true
      };
    }
    return {
      entry: { enabled: true, pieceId: normalizedPieceId, count },
      triggered: false
    };
  }
  function resetMovingStreakForPiece(value, pieceId) {
    const entry = normalizeMovingEntry(value);
    const normalizedPieceId = typeof pieceId === "string" ? pieceId : "";
    if (!entry.enabled || !normalizedPieceId || entry.pieceId !== normalizedPieceId) return entry;
    return { enabled: true, pieceId: "", count: 0 };
  }
  function isPromotionRushEligiblePiece(piece, color = piece?.color) {
    return Boolean(
      piece && piece.color === color && piece.type !== "pawn" && !LARGE_BOARD_TYPES.has(piece.type) && !NON_MOVING_BOARD_TYPES.has(piece.type)
    );
  }
  function isPromotionRushActive(piece, turnsTaken = 0) {
    if (!piece?.promotionRushUntil) return false;
    return Math.max(0, Number(turnsTaken) || 0) < Number(piece.promotionRushUntil);
  }
  function canSubstitutePieces(moving, target, enabled = true) {
    return Boolean(
      enabled && moving && target && moving !== target && moving.color && target.color && moving.color !== "neutral" && target.color !== "neutral" && moving.color !== target.color && moving.type === target.type && !isSlimeSpecialMovementLocked(moving) && !isSlimeSpecialMovementLocked(target) && !NON_MOVING_BOARD_TYPES.has(moving.type)
    );
  }
  function chebyshevDistance(first, second) {
    if (!first || !second) return Infinity;
    const rowDistance = Math.abs(Number(first.row) - Number(second.row));
    const colDistance = Math.abs(Number(first.col) - Number(second.col));
    if (!Number.isFinite(rowDistance) || !Number.isFinite(colDistance)) return Infinity;
    return Math.max(rowDistance, colDistance);
  }
  function nearestAlibabaPlacementCandidates(origin, rows, cols, isOpen, isSafe = () => true) {
    const originRow = Number(origin?.row);
    const originCol = Number(origin?.col);
    const rowCount = Math.max(0, Math.floor(Number(rows) || 0));
    const colCount = Math.max(0, Math.floor(Number(cols) || 0));
    if (!Number.isInteger(originRow) || !Number.isInteger(originCol) || !rowCount || !colCount || typeof isOpen !== "function") {
      return [];
    }
    const maxRadius = Math.max(rowCount, colCount);
    for (let radius = 1; radius <= maxRadius; radius += 1) {
      const candidates = [];
      const minRow = Math.max(0, originRow - radius);
      const maxRow = Math.min(rowCount - 1, originRow + radius);
      const minCol = Math.max(0, originCol - radius);
      const maxCol = Math.min(colCount - 1, originCol + radius);
      for (let row = minRow; row <= maxRow; row += 1) {
        for (let col = minCol; col <= maxCol; col += 1) {
          if (Math.max(Math.abs(row - originRow), Math.abs(col - originCol)) !== radius) continue;
          const candidate = { row, col };
          if (isOpen(candidate)) candidates.push(candidate);
        }
      }
      if (!candidates.length) continue;
      const safeCandidates = typeof isSafe === "function" ? candidates.filter((candidate) => isSafe(candidate)) : candidates;
      return safeCandidates.length ? safeCandidates : candidates;
    }
    return [];
  }
  function isWithinChainRange(first, second, maxDistance = CHAIN_MAX_DISTANCE) {
    const limit = Math.max(0, Math.floor(Number(maxDistance) || 0));
    return chebyshevDistance(first, second) <= limit;
  }
  function partitionChainBondsByRange(bonds, findPiece) {
    const active = [];
    const broken = [];
    normalizeChainBonds(bonds).forEach((bond) => {
      const first = typeof findPiece === "function" ? findPiece(bond.aId) : null;
      const second = typeof findPiece === "function" ? findPiece(bond.bId) : null;
      if (!first || !second || !isWithinChainRange(first, second)) broken.push(bond);
      else active.push(bond);
    });
    return { active, broken };
  }
  function isChainBondHostileTo(bond, color) {
    if (!bond || !["white", "black"].includes(color)) return false;
    return bond.by === (color === "white" ? "black" : "white");
  }
  function isBoardCorner(row, col, rowCount, colCount) {
    const rows = Math.max(1, Math.floor(Number(rowCount) || 0));
    const cols = Math.max(1, Math.floor(Number(colCount) || 0));
    if (!Number.isInteger(row) || !Number.isInteger(col)) return false;
    const rowEdge = row < Math.min(2, rows) || row >= Math.max(0, rows - 2);
    const colEdge = col < Math.min(2, cols) || col >= Math.max(0, cols - 2);
    return rowEdge && colEdge;
  }
  function isAdjacentCell(first, second) {
    if (!first || !second) return false;
    const dr = Math.abs(Number(first.row) - Number(second.row));
    const dc = Math.abs(Number(first.col) - Number(second.col));
    return Number.isFinite(dr) && Number.isFinite(dc) && Math.max(dr, dc) === 1;
  }
  function normalizeChainBonds(value) {
    if (!Array.isArray(value)) return [];
    const seen = /* @__PURE__ */ new Set();
    const bonds = [];
    value.slice(0, 64).forEach((entry, index) => {
      const aId = typeof entry?.aId === "string" ? entry.aId.slice(0, 160) : "";
      const bId = typeof entry?.bId === "string" ? entry.bId.slice(0, 160) : "";
      if (!aId || !bId || aId === bId) return;
      const pairKey = [aId, bId].sort().join("\0");
      if (seen.has(pairKey)) return;
      seen.add(pairKey);
      bonds.push({
        id: typeof entry?.id === "string" && entry.id ? entry.id.slice(0, 160) : `chain-${index}-${aId}-${bId}`.slice(0, 160),
        aId,
        bId,
        by: entry?.by === "black" ? "black" : "white"
      });
    });
    return bonds;
  }
  function isGhostTransparentFor$1(attacker, target, attackerType = attacker?.type, options = {}) {
    return Boolean(
      attacker && target?.ghost && attacker.color && attacker.color === target.color && isRangedPiece(attacker, { ...options, type: attackerType })
    );
  }
  function isStealthTransparentFor$1(attacker, target, options = {}) {
    if (!attacker?.color || !target?.color || attacker.color === target.color) return false;
    if (target.hiddenFrom === attacker.color) return true;
    if (!options.camouflageRule || options.royal || !Array.isArray(options.board)) return false;
    if (Number.isInteger(target.anchorRow) && Number.isInteger(target.anchorCol)) {
      return isCamouflageMatchingSquare(target.color, target.anchorRow, target.anchorCol);
    }
    for (let row = 0; row < options.board.length; row += 1) {
      const col = options.board[row]?.findIndex((piece) => piece === target || target.id && piece?.id === target.id);
      if (col >= 0) return isCamouflageMatchingSquare(target.color, row, col);
    }
    return false;
  }
  function frontlineResponseBlocksCapture(enabled, target, attackerFrom, targetSquare, attackerType = "") {
    if (target?.type !== "rook") return false;
    if (attackerType === "hook") return true;
    const fromRow = Number(attackerFrom?.row);
    const fromCol = Number(attackerFrom?.col);
    const toRow = Number(targetSquare?.row);
    const toCol = Number(targetSquare?.col);
    if (![fromRow, fromCol, toRow, toCol].every(Number.isInteger)) return false;
    return fromRow === toRow || fromCol === toCol;
  }
  function primeMinisterHasDiagonalCapturePath(from, to, isOpen) {
    if (!from || !to || typeof isOpen !== "function") return false;
    const dr = Math.abs(to.row - from.row), dc = Math.abs(to.col - from.col);
    if (dr === 1 && dc === 1) return true;
    if (Math.max(dr, dc) < 1 || Math.max(dr, dc) > 2) return false;
    for (let r = -1; r <= 1; r += 1) for (let c = -1; c <= 1; c += 1) {
      if (r === 0 && c === 0) continue;
      const row = from.row + r, col = from.col + c;
      if (Math.abs(to.row - row) === 1 && Math.abs(to.col - col) === 1 && isOpen(row, col)) return true;
    }
    return false;
  }
  function berserkerMovementTier(alliedPieceCount) {
    const count = Math.max(0, Math.floor(Number(alliedPieceCount) || 0));
    if (count <= 5) return "amazon";
    if (count <= 9) return "rook-king-step";
    return "king-step";
  }
  function verticalFiveColumns(board, color, targetLength = 5) {
    if (!Array.isArray(board) || !board.length || !["white", "black"].includes(color)) return [];
    const rows = board.length;
    const cols = Math.max(0, ...board.map((row) => Array.isArray(row) ? row.length : 0));
    const needed = Math.max(2, Math.floor(Number(targetLength) || 5));
    const wins = [];
    for (let col = 0; col < cols; col += 1) {
      let run = [];
      for (let row = 0; row < rows; row += 1) {
        const piece = board[row]?.[col] || null;
        const key = piece?.color === color ? boardCellKey(piece) || `${row}:${col}` : "";
        if (!key || run.some((entry) => entry.pieceKey === key)) {
          run = [];
        } else {
          run.push({ row, col, pieceKey: key });
          if (run.length >= needed) {
            wins.push(run.slice(-needed).map(({ row: winRow, col: winCol }) => ({ row: winRow, col: winCol })));
            break;
          }
        }
      }
    }
    return wins;
  }
  function nextSirenExposure(previous, nearbyEnemyIds, threshold = SIREN_CONVERSION_HALF_MOVES) {
    const prior = previous && typeof previous === "object" && !Array.isArray(previous) ? previous : {};
    const uniqueIds = [...new Set((Array.isArray(nearbyEnemyIds) ? nearbyEnemyIds : []).filter(Boolean).map(String))];
    const counts = {};
    const converted = [];
    const limit = Math.max(1, Math.floor(Number(threshold) || SIREN_CONVERSION_HALF_MOVES));
    uniqueIds.forEach((id) => {
      counts[id] = Math.max(0, Math.floor(Number(prior[id]) || 0)) + 1;
      if (counts[id] >= limit) converted.push(id);
    });
    return { counts, converted };
  }
  const TRICKSTER_UNDEAD_STATUS_FIELDS = Object.freeze([
    "poisonedPawn",
    "poisonStunTurns",
    "poisonStunColor",
    "evasion",
    "frozen",
    "frozenByCard",
    "iceSheet",
    "crownBearer",
    "callingCard",
    "loyalist",
    "parry",
    "emptyLunchbox",
    "shielded",
    "protected",
    "queensGambitProtection",
    "queensGambitPreviousProtected",
    "basicTraining",
    "potionBasicTraining",
    "coronationProtection",
    "ghost",
    "chameleon",
    "undergroundBunker",
    "witchTrial",
    "disarmed",
    "lastResistance",
    "staked",
    "severed",
    "inertia",
    "promotionRushUntil",
    "sacrificeProtection",
    "frenzy",
    "cardNoCaptureUntil",
    "spyOwner",
    "hiddenFrom",
    "submerged",
    "trojanHorse",
    "quantum",
    "quantumFirstObservationFails"
  ]);
  function clearTricksterUndeadResurrectionStatuses(piece) {
    if (piece?.type !== "trickster") return;
    for (const key of TRICKSTER_UNDEAD_STATUS_FIELDS) delete piece[key];
  }
  function undeadResurrectionDueMoveCount(moveCount, halfMoves = UNDEAD_RESURRECTION_HALF_MOVES) {
    const current = Math.max(0, Math.floor(Number(moveCount) || 0));
    const delay = Math.max(1, Math.floor(Number(halfMoves) || UNDEAD_RESURRECTION_HALF_MOVES));
    return current + delay + 1;
  }
  function undeadResurrectionRemainingHalfMoves(entry, moveCount) {
    const scheduled = Number(entry?.remainingHalfTurns);
    if (Number.isSafeInteger(scheduled) && scheduled >= 0) return scheduled;
    const current = Math.max(0, Math.floor(Number(moveCount) || 0));
    const due = Math.max(0, Math.floor(Number(entry?.dueMoveCount) || 0));
    return due ? Math.max(0, due - current) : 0;
  }
  function advanceScheduledHalfTurn(entry) {
    const remaining = Number(entry?.remainingHalfTurns);
    if (!Number.isSafeInteger(remaining) || remaining < 0) return entry;
    return { ...entry, remainingHalfTurns: Math.max(0, remaining - 1) };
  }
  const SUSPICIOUS_POTION_EFFECTS = Object.freeze([
    { id: "sacrificeProtection", label: "보호(희생)", hoverLabel: "보호(희생)", description: "3턴 동안 공격당하지 않습니다.", scope: "nonKing" },
    { id: "lastResistance", label: "보호(마지막 저항)", hoverLabel: "보호(마지막 저항)", description: "3턴 동안 공격당하지 않습니다.", scope: "king" },
    { id: "coronationProtection", label: "보호(대관식)", hoverLabel: "보호(대관식)", description: "다음 자기 턴이 돌아올 때까지 공격당하지 않습니다." },
    { id: "shield", label: "가호", description: "다음 공격을 한 번 막습니다." },
    { id: "evasion", label: "회피", description: "잡힐 위기에 처하면 인접한 빈칸으로 한 번 피합니다. 피할 공간이 없으면 잡히며, 발동하면 효과가 사라집니다." },
    { id: "parry", label: "패링", description: "다음 공격을 받을 때 40% 확률로 공격자를 반격합니다." },
    { id: "basicTraining", label: "기초 교습", description: "폰의 행마법을 추가로 얻습니다.", exclude: ["king", "pawn"] },
    { id: "stealth", label: "은신", description: "상대방에게 인식되지 않습니다." },
    { id: "loyalist", label: "충신", description: "현재 위치와 관계없이 턴을 소모해 아군 킹 주변 빈칸으로 이동할 수 있습니다.", exclude: ["king"] },
    { id: "ghost", label: "고스트", description: "아군 원거리 기물이 이 기물을 뛰어넘을 수 있습니다." },
    { id: "chameleon", label: "카멜레온", description: "기물을 잡으면 잡은 기물로 변합니다.", exclude: ["king"] },
    { id: "submerge", label: "잠복", description: "공격 대상으로 선정되지 않습니다. 주변 8칸에 상대 기물이 있으면 잠복을 잃습니다." },
    { id: "freeze", label: "빙결", description: "3턴 동안 움직이거나 공격받지 않습니다." },
    { id: "chimera", label: "키메라", description: "움직일 때마다 폰·나이트·비숍·룩 중 하나로 변하며, 낮은 확률로 퀸이 됩니다.", exclude: ["king"] },
    { id: "witchTrial", label: "마녀재판", description: "3턴 안에 기물을 잡지 못하면 제거됩니다. 기물을 잡으면 표식이 사라지고 가호를 얻습니다.", exclude: ["king"] },
    { id: "stake", label: "말뚝", description: "4수 동안 움직일 수 없으며, 시간이 끝나면 가호를 얻습니다." },
    { id: "callingCard", label: "예고장", description: "이 기물이 잡히면 다른 아군 기물 하나가 추가로 제거됩니다.", exclude: ["king"] },
    { id: "emptyLunchbox", label: "빈 찬합", description: "3수 안에 자기 킹과 인접하지 못하면 자결합니다.", exclude: ["king"] },
    { id: "poisonStun", label: "독", description: "2수 동안 움직일 수 없습니다." },
    { id: "disarm", label: "무장해제", description: "다음 자기 턴이 돌아올때까지 기물을 잡을 수 없습니다." },
    { id: "mannerNoCapture", label: "포획 금지(매너)", description: "평범한 이동을 한 번 하기 전까지 기물을 잡을 수 없습니다." },
    { id: "saturationNoCapture", label: "포획 금지(포화)", description: "더 이상 기물을 잡을 수 없습니다." },
    { id: "explosive", label: "폭발(자폭병)", description: "잡히면 주위 3×3 범위를 폭발시키며, 폭발은 아군도 제거합니다.", scope: "pawn" },
    { id: "poisonedPawn", label: "독이 듬(독이 든 폰)", hoverLabel: "독이 든 폰", description: "이 폰을 잡은 기물은 2수 동안 움직일 수 없습니다.", scope: "pawn" },
    { id: "queensGambitProtection", label: "보호(퀸즈 갬빗)", hoverLabel: "보호(퀸즈 갬빗)", description: "공격당하지 않습니다. 프로모션 시에도 유지되지만, 다른 기물로 변경될 경우 보호를 잃습니다.", scope: "pawn" },
    { id: "trojanHorse", label: "트로이 목마", description: "직접 잡히면 공격한 기물을 즉시 되잡고 그 자리에 아군 폰을 생성합니다.", scope: "knight" },
    { id: "severance", label: "절단", description: "해당 기물은 다음 2수 동안 한 번에 한 칸씩만 이동할 수 있습니다.", scope: "ranged" },
    { id: "inertia", label: "관성", description: "해당 기물은 이제 한 칸씩 이동할 수 없습니다.", scope: "ranged" },
    { id: "outpostProtection", label: "보호(전초기지)", description: "공격당하지 않습니다. 움직이면 해제됩니다." },
    { id: "nullification", label: "상쇄", description: "같은 종류의 상대 기물에게 공격당하지 않습니다." },
    { id: "recurrence", label: "회귀", description: "잡히면 무작위 홈 랭크에 다시 소환되며 회귀를 잃습니다.", exclude: ["king"] }
  ]);
  function suspiciousPotionEffectsForPiece(piece, options = {}) {
    const isKing = Boolean(options.isKing);
    const isRanged = Boolean(options.isRanged);
    return SUSPICIOUS_POTION_EFFECTS.filter((effect) => {
      if (options.legacyRandomPools && ["outpostProtection", "nullification", "recurrence"].includes(effect.id)) return false;
      if (isKing && effect.exclude?.includes("king")) return false;
      if (piece?.type === "pawn" && effect.exclude?.includes("pawn")) return false;
      if (!effect.scope) return true;
      if (effect.scope === "king") return isKing;
      if (effect.scope === "nonKing") return !isKing;
      if (effect.scope === "pawn") return piece?.type === "pawn";
      if (effect.scope === "knight") return piece?.type === "knight";
      if (effect.scope === "ranged") return isRanged;
      return false;
    });
  }
  function boardCellKey(value) {
    if (!value) return "";
    return String(value.id || `${value.color || ""}:${value.type || ""}`);
  }
  function canResolveSiegeRamMove(piece, move) {
    if (!move?.siegeRamMove) return false;
    if (piece?.type === "siegeRam") return true;
    if (move.imperialStudy === "siegeRam") {
      return Array.isArray(piece?.imperialMoves) && piece.imperialMoves.includes("siegeRam");
    }
    return piece?.type === "trickster" && piece.tricksterMoveType === "siegeRam" && move.tricksterMove === true;
  }
  function expireZugzwangAfterCompletedTurn(value, color) {
    const next = {
      white: Boolean(value?.white),
      black: Boolean(value?.black)
    };
    if (color === "white" || color === "black") next[color] = false;
    return next;
  }
  function replayDeltaMatchesBoard(board, delta) {
    if (!Array.isArray(board) || !Array.isArray(delta) || !delta.length) return false;
    return delta.every((entry) => {
      const current = board?.[entry.row]?.[entry.col] || null;
      return boardCellKey(current) === boardCellKey(entry.after) && JSON.stringify(current) === JSON.stringify(entry.after);
    });
  }
  function openingPassiveSkipsFreshCapture(cardIdOrEffect) {
    return ["dutch", "horde", "london-system", "londonSystem", "big-bishop", "bigBishop", "big-rook", "bigRook"].includes(cardIdOrEffect);
  }
  function fianchettoRestrictedPieceType(type) {
    return type === "pawn";
  }
  function resetConvertedLargePieceHealth(piece) {
    const hp = { colossus: 3, bigRook: 2, bigBishop: 2, "big-rook": 2, "big-bishop": 2 }[piece?.type];
    if (hp) {
      piece.hp = hp;
      piece.maxHp = hp;
    }
    return piece;
  }
  function merchantPurchaseIsRoyal(piece) {
    const raw = typeof piece === "string" ? piece : piece?.type ?? piece?.typeOverride ?? piece?.baseType;
    const type = String(raw ?? "").replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    return ["king", "royalKnight", "darkWizard", "shotgunKing", "vip", "merchant", "timeTraveler", "vampireLord"].includes(type) || [piece, piece?.attributes].some((value) => value?.crownRoyal === true || value?.editorRoyal === true || value?.regencyHeir === true);
  }
  function merchantPurchasePrice(piece) {
    if (!piece) return null;
    if (merchantPurchaseIsRoyal(piece)) return 20;
    const type = typeof piece === "string" ? piece : piece.type ?? piece.typeOverride ?? piece.baseType;
    if (type === "coffin") return 1;
    const value = pieceCombatValue(type);
    return Number.isFinite(value) ? Math.max(2, value) : null;
  }
  function isRationalCapture({ movingValue, capturedValue, canBeRecaptured }) {
    const attackerValue = Number(movingValue);
    const targetValue = Number(capturedValue);
    if (!(attackerValue > 0) || !(targetValue > 0)) return false;
    return attackerValue < targetValue || canBeRecaptured === false;
  }
  function scoreExchangePreference({
    movingValue = 0,
    capturedValues = [],
    canBeRecaptured = true,
    unitValue = 1
  } = {}) {
    const attackerValue = Math.max(0, Number(movingValue) || 0);
    const values = Array.isArray(capturedValues) ? capturedValues.map((value) => Math.max(0, Number(value) || 0)).filter((value) => value > 0) : [];
    if (!(attackerValue > 0) || !values.length) return 0;
    const capturedValue = values.reduce((sum, value) => sum + value, 0);
    const unit = Math.max(Number.EPSILON, Math.abs(Number(unitValue) || 1));
    const netGain = capturedValue - (canBeRecaptured ? attackerValue : 0);
    const similarMargin = Math.max(unit * 0.6, Math.min(attackerValue, capturedValue) * 0.16);
    const exchangeBonus = Math.abs(netGain) <= similarMargin ? Math.min(0.9, values.length * 0.3) : Math.min(0.15, values.length * 0.03);
    return netGain / unit + exchangeBonus;
  }
  function scoreExchangeSimplification({
    materialLead = 0,
    remainingPieces = 0,
    unitValue = 1
  } = {}) {
    const unit = Math.max(Number.EPSILON, Math.abs(Number(unitValue) || 1));
    const lead = (Number(materialLead) || 0) / unit;
    const count = Math.max(0, Math.floor(Number(remainingPieces) || 0));
    if (lead >= 1) return -count * 0.08;
    if (Math.abs(lead) <= 0.65) return -count * 0.035;
    return 0;
  }
  function scoreSuicideBomberTarget({
    valid = true,
    targetValue = 0,
    blastBalance = 0,
    capturerValue = 0,
    capturable = false,
    ownRoyalAtRisk = false,
    advance = 0
  } = {}) {
    if (!valid || ownRoyalAtRisk) return -5e4;
    const safeTargetValue = Math.max(0, Number(targetValue) || 0);
    const safeBlastBalance = Number(blastBalance) || 0;
    const safeCapturerValue = Math.max(0, Number(capturerValue) || 0);
    const safeAdvance = Math.max(0, Math.min(6, Number(advance) || 0));
    let score = 90 + safeAdvance * 20 - safeTargetValue * 0.12;
    if (capturable) {
      score += Math.min(700, safeCapturerValue * 0.58);
      score += safeBlastBalance * (safeBlastBalance < 0 ? 1.62 : 0.72);
    } else {
      score -= 80;
      score += safeBlastBalance * (safeBlastBalance < 0 ? 0.8 : 0.32);
    }
    return Math.max(-5e4, Math.min(2600, score));
  }
  function scoreTerminalOutcome({
    mode = "",
    winner = "",
    perspectiveColor = "",
    winScore = 1e5
  } = {}) {
    if (mode !== "gameover") return null;
    const magnitude = Math.max(1, Math.abs(Number(winScore) || 1e5));
    if (winner === perspectiveColor) return magnitude;
    if (!winner || winner === "draw") return 0;
    return -magnitude;
  }
  function scoreCrownObjective({
    holderColor = "",
    perspectiveColor = "",
    heldMoves = 0,
    holderThreatened = false,
    groundDistance = Infinity
  } = {}) {
    if (holderColor) {
      if (holderColor !== perspectiveColor) return 0;
      const progress = Math.max(0, Math.min(10, Math.floor(Number(heldMoves) || 0)));
      let score = 650 + progress * 160 + progress * progress * 75;
      if (progress >= 8) score += (progress - 7) * 4e3;
      if (holderThreatened) score *= progress >= 8 ? 0.42 : 0.58;
      return Math.round(score);
    }
    const distance = Number(groundDistance);
    if (!Number.isFinite(distance)) return 0;
    return Math.max(0, Math.round(360 - Math.max(0, distance) * 55));
  }
  function scoreProphecyObjective({ remainingHalfTurns = 0 } = {}) {
    const remaining = Math.max(0, Math.min(6, Math.floor(Number(remainingHalfTurns) || 0)));
    if (!remaining) return 0;
    const elapsed = 6 - remaining;
    let score = 600 + elapsed * 420 + elapsed * elapsed * 150;
    if (remaining <= 2) score += (3 - remaining) * 3200;
    return Math.round(score);
  }
  function scoreWitchTrialTarget({
    valid = true,
    targetValue = 0,
    immediateCaptureCount = 0,
    maxCaptureValue = 0,
    totalCaptureValue = 0,
    decisiveCapture = false,
    disabled = false,
    shielded = false,
    currentlyThreatened = false
  } = {}) {
    if (!valid || decisiveCapture) return -5e4;
    const safeTargetValue = Math.max(0, Number(targetValue) || 0);
    const safeCaptureCount = Math.max(0, Number(immediateCaptureCount) || 0);
    const safeMaxCaptureValue = Math.max(0, Number(maxCaptureValue) || 0);
    const safeTotalCaptureValue = Math.max(0, Number(totalCaptureValue) || 0);
    let score = safeTargetValue * 0.55;
    if (safeCaptureCount > 0) {
      score -= 420 + safeCaptureCount * 65 + safeMaxCaptureValue * 0.85 + safeTotalCaptureValue * 0.15;
    } else {
      score += disabled ? 210 : 160;
    }
    if (shielded) score -= 180;
    if (currentlyThreatened) score -= 180 + safeTargetValue * 0.25;
    return Math.max(-5e4, Math.min(2600, score));
  }
  const AI_DEFAULT_SEARCH_TIME_MS = 3500;
  function nonNegativeMs(value, fallback = 0) {
    const number = Number(value);
    return Number.isFinite(number) ? Math.max(0, number) : Math.max(0, Number(fallback) || 0);
  }
  function normalizeAiWorkerTimeLimit(value, fallbackMs = AI_DEFAULT_SEARCH_TIME_MS) {
    const fallback = nonNegativeMs(fallbackMs, AI_DEFAULT_SEARCH_TIME_MS);
    if (value === null || value === void 0 || !Number.isFinite(Number(value))) return fallback;
    return Math.min(fallback, Math.max(0, Number(value)));
  }
  const INF = 1e9;
  const COLORS = ["white", "black"];
  // Real site value is 4 (its own live-play search ceiling). Raised to 12
  // (matching engine.optimized.js's own deliberate choice) since this cap
  // is part of OUR search-quality layer, not site "rules" -- confirmed
  // 2026-09-14 that this is a hard Math.min(MAX_DEPTH, depth) cap, so
  // callers requesting a deeper search (e.g. self-play depth=6 experiments)
  // were being silently capped at 4 before this fix.
  const MAX_DEPTH = 12;
  const DEFAULT_DEPTH = MAX_DEPTH;
  const TIME_LIMIT_MS = AI_DEFAULT_SEARCH_TIME_MS;
  const HARD_TIME_LIMIT_MS = 16750;
  const ROOT_SAFETY_GRACE_MS = 180;
  const MONSTER_NEXT_DEPTH_GRACE_MS = 120;
  const MONSTER_NEXT_DEPTH_GROWTH = 2;
  const EARLY_KING_HOME_TURNS = 15;
  const CARD_USE_MIN_GAIN = 1;
  const ROOT_HANGING_MAJOR_VALUE = 300;
  const SAME_TURN_CARD_INTENT_TOLERANCE = 60;
  const BLOOD_MOON_DAY_TURNS = 4;
  const BLOOD_MOON_NIGHT_TURNS = 3;
  const BLOOD_EFFECT_IDS = ["summon", "veil", "sunlight", "curse", "coffin"];
  const CHIMERA_BASE_TYPES = ["pawn", "knight", "bishop", "rook"];
  const CHIMERA_TYPES = [...CHIMERA_BASE_TYPES, "queen"];
  const TROLLEY_LOW_SCORE_OPTIONS = [2, 3, 4, 5, 6, 7, 8];
  const TROLLEY_MAX_HIGH_SCORE = 10;
  const TROLLEY_MAX_SCORE_GAP = 2;
  const TROLLEY_MAX_BUNDLE_PIECES = 4;
  const TROLLEY_MAX_BUNDLES_PER_SCORE = 240;
  const TROLLEY_MAX_ELIGIBLE_PIECES = 32;
  const AI_OPENING_FIRST_MOVE_CANDIDATES = {
    white: [
      { from: "e2", to: "e4" },
      { from: "d2", to: "d4" },
      { from: "g1", to: "f3" }
    ],
    black: [
      { from: "e7", to: "e5" },
      { from: "d7", to: "d5" },
      { from: "g8", to: "f6" }
    ]
  };
  const RECYCLING_PROMOTION_EXCLUDED_TYPES = /* @__PURE__ */ new Set([
    "king",
    "royalKnight",
    "shotgunKing",
    "darkWizard",
    "merchant",
    "timeTraveler",
    "vampireLord",
    "colossus",
    "bigRook",
    "wall",
    "football",
    "monster",
    "blackHole",
    "coffin",
    "crown"
  ]);
  const BASIC_TRAINING_FORBIDDEN_TYPES = /* @__PURE__ */ new Set(["pawn", "king", "queen", "primeMinister", "jester", "guard", "amazon", "man", "idol", "babyBear", "bear"]);
  function workerSharedTurnCount(boardState) {
    return Math.min(Number(boardState?.turnsTaken?.white) || 0, Number(boardState?.turnsTaken?.black) || 0);
  }
  function applyWorkerTricksterAbilityDefaults(boardState, piece) {
    const abilityType = piece?.type === "trickster" ? piece.tricksterMoveType : "";
    if (!abilityType) return "";
    const defaults = randomRouletteInitialPieceState(abilityType, workerSharedTurnCount(boardState));
    Object.entries(defaults).forEach(([key, value]) => {
      if (piece[key] === void 0 || piece[key] === null) {
        piece[key] = campaignAuthorityV1States.has(boardState) ? clonePiece(value) : clonePlain(value);
      }
    });
    if (abilityType === "reaper" && !Number.isFinite(Number(piece.reaperCaptures))) piece.reaperCaptures = 0;
    if (legacyWindmillMovementStates.has(boardState) && abilityType === "windmill") piece.windmillMode = "bishop";
    return abilityType;
  }
  function setWorkerTricksterAbilityType(boardState, piece, type) {
    if (!piece || piece.type !== "trickster") return "";
    piece.tricksterMoveType = TRICKSTER_MOVEMENT_TYPES.includes(type) ? type : TRICKSTER_MOVEMENT_TYPES[0] || "queen";
    return applyWorkerTricksterAbilityDefaults(boardState, piece);
  }
  function rerollWorkerTricksterAbility(boardState, piece, seed = "") {
    if (!piece || piece.type !== "trickster") return tricksterAbilityType(piece);
    const index = workerStableIndex(seed, TRICKSTER_MOVEMENT_TYPES.length);
    return setWorkerTricksterAbilityType(boardState, piece, TRICKSTER_MOVEMENT_TYPES[index] || "queen");
  }
  function workerPieceHasActiveOrPreviousTricksterAbility(piece, type) {
    return pieceHasAbility(piece, type) || Boolean(
      piece?.type === "trickster" && piece.tricksterPreviousAbilityForTurn === type
    );
  }
  function clearWorkerPreviousTricksterAbilities(boardState) {
    forEachPiece(boardState, (piece) => {
      if (piece?.type === "trickster") delete piece.tricksterPreviousAbilityForTurn;
    });
  }
  const SAME_TURN_MOVE_EFFECT_CARD_EFFECTS = /* @__PURE__ */ new Set([
    "bishopSnipe",
    "breakthroughOrder",
    "charge",
    "enPassantBang",
    "promotionRush",
    "substitution",
    "switcheroo"
  ]);
  const MAJOR_PIECE_TYPES = new Set(RANDOM_ROULETTE_MAJOR_TYPES);
  const SHOTGUN_BLAST_AMMO_COST = 2;
  const SHOTGUN_SNIPE_AMMO_COST = 3;
  let workerBoardRows = 8;
  let workerBoardCols = 8;
  const PIECE_VALUES = {
    pawn: 100,
    squire: 120,
    standardBearer: 220,
    guard: 150,
    checker: 240,
    checkerKing: 300,
    recruiter: 410,
    fanatic: 120,
    wall: 0,
    football: 0,
    monster: 0,
    blackHole: 0,
    man: 400,
    royalKnight: 1e4,
    darkWizard: 1e4,
    log: 150,
    alfil: 210,
    ferz: 210,
    camel: 260,
    knight: 310,
    eagle: 310,
    alibaba: 310,
    bishop: 320,
    missionary: 200,
    protestant: 330,
    cardinal: 700,
    cannon: 410,
    grasshopper: 400,
    knightmaster: 450,
    windmill: 470,
    rook: 520,
    herald: 520,
    assassin: 560,
    reaper: 620,
    dragon: 650,
    pegasus: 650,
    unicorn: 650,
    primeMinister: 760,
    amazon: 1400,
    queen: 940,
    wizard: 980,
    hook: 1040,
    colossus: 1200,
    bigRook: 800,
    shotgunKing: 1250,
    timeTraveler: 1e4,
    vampireLord: 1e4,
    bat: 260,
    coffin: 120,
    idol: 200,
    lobster: 200,
    babyBear: 120,
    bear: 1100,
    crown: 940,
    siegeRam: 720,
    magicGirl: 760,
    berserker: 700,
    slime: 680,
    siren: 820,
    trickster: 900,
    undead: 720,
    campfire: 400,
    paladin: 450,
    octopus: 500,
    brutus: 1e3,
    clockwork: 500,
    parrot: 500,
    thief: 900,
    hedgehog: 900,
    princess: 600,
    bigBishop: 800,
    vip: 1e4,
    merchant: 1e4,
    king: 1e4
  };
  const CARD_EFFECT_VALUES = {
    summonColossus: 1500,
    bigRook: 900,
    shotgunKing: 1300,
    wizard: 980,
    merchantGuild: 820,
    chess960: 130,
    chess344200: 130,
    monochromeChess: 160,
    revelation: 140,
    football: 180,
    monsterRule: 210,
    blackHole: 220,
    ruleBombs: 260,
    saturation: 300,
    periodicCollapse: 300,
    portal: 260,
    crownRule: 360,
    conveyorRule: 300,
    mistake: 240,
    transcendence: 420,
    camouflageRule: 240,
    winterKingdom: 240,
    machoChess: 180,
    recycling: 220,
    highGround: 220,
    highway: 220,
    coolGuy: 190,
    mongolianGambit: 220,
    reformation: 230,
    londonSystem: 260,
    lastStand: 260,
    earlyPromotion: 250,
    holdout: 320,
    finalWeapon: 420,
    fastGrowth: 270,
    constitutionalMonarchy: 300,
    retreat: 260,
    horde: 420,
    jester: 600,
    initiative: 300,
    enPassantBang: 280,
    evasion: 320,
    randomRoulette: 520,
    freeMove: 460,
    submerge: 260,
    gale: 420,
    idol: 300,
    homecoming: 240,
    otherworld: 520,
    twins: 320,
    judgment: 400,
    lobster: 220,
    babyBear: 760,
    callingCard: 420,
    frenzy: 480,
    zugzwang: 560,
    vip: 520,
    icbm: 780,
    guard: 260,
    reaper: 580,
    trojanHorse: 420,
    madHorse: 360,
    freeze: 520,
    sacrifice: 460,
    clonePassive: 760,
    joker: 620,
    vanish: 720,
    moving: 420,
    promotionRush: 480,
    substitution: 390,
    chain: 420,
    ghost: 360,
    cornerKick: 360,
    conversion: 340,
    taunt: 300,
    basicTraining: 280,
    hallucination: 120,
    blueJeans: 620,
    reversePawns: 220,
    spy: 320,
    conscription: 420,
    barricade: 320,
    queenCavalry: 240,
    chameleonMutation: 230,
    chimera: 360,
    collapse: 620,
    ultimatum: 620,
    socialism: 580,
    vortex: 520,
    palace: 500,
    dice: 430,
    disarm: 420,
    witchTrial: 420,
    snipe: 400,
    royalShield: 380,
    undergroundBunker: 360,
    encouragement: 360,
    queensGambit: 520,
    martyrdom: 320,
    lastResistance: 360,
    coronation: 360,
    easternPolicy: 260,
    elephantEscape: 240,
    suicideBomber: 300,
    bribe: 330,
    horseRiding: 360,
    eagle: 320,
    log: 220,
    amazon: 760,
    ordination: 300,
    pegasus: 560,
    racingKing: 460,
    radicalCharge: 340,
    hook: 560,
    switcheroo: 260,
    herald: 500,
    stealth: 260,
    grasshopper: 340,
    dragon: 520,
    knightmate: 420,
    binaMate: 760,
    overwhelm: 720,
    dutch: 680,
    assassin: 520,
    knightmaster: 360,
    standardBearer: 260,
    apprenticeKnights: 180,
    checker: 260,
    acceleration: 350,
    quantumMechanics: 320,
    alekhineMachineGun: 300,
    severance: 300,
    inertia: 360,
    iceSheet: 320,
    fianchetto: 320,
    pawnConversion: 320,
    ruleTicket: 320,
    insight: 360,
    panic: 260,
    trolley: 520,
    prophecy: 640,
    charge: 420,
    breakthroughOrder: 260,
    royalCommand: 220,
    feudalContract: 360,
    ironMonarch: 380,
    exile: 300,
    freeCastling: 320,
    canceling: 300,
    localConscription: 320,
    bishopSnipe: 360,
    windmill: 420,
    backwardKnight: 280,
    fileSurge: 340,
    rookLift: 360,
    underpromotion: 300,
    stake: 260,
    fanaticalRitual: 380,
    reposition: 320,
    emergencyEvacuation: 330,
    traitor: 520,
    timePhaseShift: 240,
    timeClumsyAttack: 260,
    timeIsMine: 520,
    timeSaveLoad: 360,
    knightJourneyHint: 80,
    armistice: 520,
    frontlineResponse: 420,
    hypocrisy: 520,
    relay: 360,
    fieldPromotion: 520,
    cleanupPieces: 300,
    poisonedPawn: 360,
    portalGun: 420,
    exhaustion: 500,
    missionary: 440,
    mistakeCard: 520,
    democracy: 620,
    kingOfTheHill: 430,
    loyalist: 420,
    blackMagic: 720,
    parry: 520,
    fleetingDream: 620,
    emptyLunchbox: 520,
    genevaConvention: 480,
    platformRule: 340,
    bloodCard: 420
  };
  const WORKER_RULE_TICKET_CANDIDATES = [
    { id: "monochrome-chess", effect: "monochromeChess", stars: 2 },
    { id: "revelation", effect: "revelation", stars: 3 },
    { id: "football", effect: "football", stars: 2.5 },
    { id: "monster", effect: "monsterRule", stars: 0 },
    { id: "black-hole", effect: "blackHole", stars: 3 },
    { id: "rule-bombs", effect: "ruleBombs", stars: 3 },
    { id: "saturation", effect: "saturation", stars: 3 },
    { id: "periodic-collapse", effect: "periodicCollapse", stars: 3 },
    { id: "portal", effect: "portal", stars: 3 },
    { id: "crown", effect: "crownRule", stars: 3 },
    { id: "conveyor", effect: "conveyorRule", stars: 3 },
    { id: "mistake", effect: "mistake", stars: 3 },
    { id: "transcendence", effect: "transcendence", stars: 3 },
    { id: "camouflage-color", effect: "camouflageRule", stars: 2 },
    { id: "winter-kingdom", effect: "winterKingdom", stars: 3.5 },
    { id: "macho-chess", effect: "machoChess", stars: 2.5 },
    { id: "high-ground", effect: "highGround", stars: 3 },
    { id: "highway", effect: "highway", stars: 0 },
    { id: "recycling", effect: "recycling", stars: 3 },
    { id: "platform", effect: "platformRule", stars: 0 }
  ];
  const WORKER_RULE_TICKET_EXCLUDED_RULE_IDS = /* @__PURE__ */ new Set(["chess-960", "chess-344200", "diagonal-chess"]);
  if (typeof self !== "undefined" && typeof self.addEventListener === "function") self.addEventListener("message", (event) => {
    const data = event.data || {};
    if (data.type === "beta-parity") {
      try {
        self.postMessage({
          type: "beta-parity-result",
          requestId: data.requestId,
          result: applyAugmentBaseClientMoveForParity(data.before, data.actor, data.payload)
        });
      } catch (error) {
        self.postMessage({
          type: "beta-parity-result",
          requestId: data.requestId,
          result: { ok: false, code: "CLIENT_EVALUATION_ERROR" },
          error: error?.message || String(error)
        });
      }
      return;
    }
    if (data.type !== "search") return;
    try {
      const state = normalizeState(data.state);
      const result = searchBestAction(
        state,
        data.rootActions || [],
        data.color || "black",
        data.depth || DEFAULT_DEPTH,
        data.timeLimitMs
      );
      self.postMessage({
        type: "result",
        id: data.id,
        action: result.action,
        score: result.score,
        nodes: result.nodes,
        cutoffs: result.cutoffs
      });
    } catch (error) {
      self.postMessage({
        type: "result",
        id: data.id,
        action: null,
        error: error?.message || String(error)
      });
    }
  });
  const FLEXIBLE_BUDGET_MULTIPLIER = 8;
  // Grafted from engine.optimized.js (our-only enhancement, replacing the
  // real file's searchBestAction wholesale): adds an options bag
  // (flexibleBudget / evalFn / skipOpeningBook), a killers/tt-carrying
  // search context (consumed by minimax/quiescence, grafted earlier), and
  // accepts a partial (timed-out but partially-ranked) depth result when
  // flexibleBudget is set instead of always discarding it. Every real-file
  // line here (forced-royal-capture/spell shortcuts, opening-book probe,
  // pickRootHardSafetyFallback, the depth-iteration loop shape) is kept
  // identical to what aiWorker-raw.js already had -- confirmed by diffing
  // the two versions line-by-line before this replacement, nothing
  // real-only is lost.
  // Per-search memo of hangingMaterialRisk for states that are only READ during the
  // search (every root action's tacticalSafetyAdjustment re-asks for the same
  // "before" state). Entries die with the search and whenever applyAction touches
  // the state (invalidateBoardCaches), so results are identical to recomputing.
  let HANG_MEMO = null;
  function searchBestAction(boardState, rootActions, aiColor, depth, timeLimitMs = TIME_LIMIT_MS, options = {}) {
    const outer = HANG_MEMO;
    HANG_MEMO = /* @__PURE__ */ new WeakMap();
    // options.params (experiments): {nullMoveMinDepth, nullMoveReduction, lmrMinDepth, lmrMoveThreshold, quiescenceMaxPlies}
    const saved = [NULL_MOVE_MIN_DEPTH, NULL_MOVE_REDUCTION, LMR_MIN_DEPTH, LMR_MOVE_THRESHOLD, QUIESCENCE_MAX_PLIES];
    const p = options && options.params;
    if (p) {
      const num = (v, d) => (Number.isFinite(Number(v)) && v !== null && v !== "" ? Number(v) : d);
      NULL_MOVE_MIN_DEPTH = num(p.nullMoveMinDepth, NULL_MOVE_MIN_DEPTH);
      NULL_MOVE_REDUCTION = num(p.nullMoveReduction, NULL_MOVE_REDUCTION);
      LMR_MIN_DEPTH = num(p.lmrMinDepth, LMR_MIN_DEPTH);
      LMR_MOVE_THRESHOLD = num(p.lmrMoveThreshold, LMR_MOVE_THRESHOLD);
      QUIESCENCE_MAX_PLIES = num(p.quiescenceMaxPlies, QUIESCENCE_MAX_PLIES);
    }
    try {
      return searchBestActionCore(boardState, rootActions, aiColor, depth, timeLimitMs, options);
    } finally {
      HANG_MEMO = outer;
      [NULL_MOVE_MIN_DEPTH, NULL_MOVE_REDUCTION, LMR_MIN_DEPTH, LMR_MOVE_THRESHOLD, QUIESCENCE_MAX_PLIES] = saved;
    }
  }
  function memoHangingMaterialRisk(boardState, color) {
    if (!HANG_MEMO) return hangingMaterialRisk(boardState, color);
    let entry = HANG_MEMO.get(boardState);
    if (!entry) { entry = {}; HANG_MEMO.set(boardState, entry); }
    if (entry[color] === undefined) entry[color] = hangingMaterialRisk(boardState, color);
    return entry[color];
  }
  function searchBestActionCore(boardState, rootActions, aiColor, depth, timeLimitMs = TIME_LIMIT_MS, options = {}) {
    setWorkerBoardDimensions(boardState);
    // options.limits (2026-09-19) -- Stockfish-style "go" parameters. When absent
    // everything below behaves exactly as before.
    //   depth              max depth (overrides the depth argument)
    //   movetimeMs         soft time budget per move (overrides timeLimitMs)
    //   infinite           no time limit (stops on depth / nodes only)
    //   nodes              stop after about this many nodes
    //   minDepth           always finish at least this depth (may run to hardTimeMs)
    //   extend             (default true) when the soft time is up but the current
    //                      depth is mostly done (extendMinProgress of the root moves,
    //                      default 0.6) or the best move is still changing, keep
    //                      going until hardTimeMs instead of throwing the work away
    //   extendFactor       hardTimeMs default = movetimeMs * extendFactor (default 2)
    //   hardTimeMs         absolute cap incl. extension
    //   predictiveStop     (default true) do not start a depth that, judging by the
    //                      previous depth's cost, cannot finish in the time left
    const limits = options.limits && typeof options.limits === "object" ? options.limits : null;
    const maxDepth = Math.max(1, Math.min(MAX_DEPTH, Number(limits?.depth ?? depth) || DEFAULT_DEPTH));
    const startedAt = performance.now();
    const requestedTimeMs = limits ? (limits.infinite ? Infinity : limits.movetimeMs) : timeLimitMs;
    const boundedTimeLimitMs = limits
      ? (requestedTimeMs === Infinity || Number.isFinite(Number(requestedTimeMs)) ? Math.max(0, Number(requestedTimeMs)) : nonNegativeMs(timeLimitMs, TIME_LIMIT_MS))
      : normalizeAiWorkerTimeLimit(timeLimitMs, timeLimitMs ?? TIME_LIMIT_MS);
    const flexibleBudget = Boolean(options.flexibleBudget);
    let tm = null;
    if (limits) {
      const extendOn = limits.extend !== false;
      const minDepth = Math.max(0, Math.min(maxDepth, Math.floor(Number(limits.minDepth) || 0)));
      const factor = Math.max(1, Number(limits.extendFactor) || 2);
      const hardMs = Number.isFinite(Number(limits.hardTimeMs))
        ? Math.max(boundedTimeLimitMs, Number(limits.hardTimeMs))
        : (extendOn || minDepth > 0 ? boundedTimeLimitMs * factor : boundedTimeLimitMs);
      tm = {
        softMs: boundedTimeLimitMs,
        hardMs,
        extendOn,
        minDepth,
        extendMinProgress: Math.min(1, Math.max(0.05, Number(limits.extendMinProgress) || 0.6)),
        predictiveStop: limits.predictiveStop !== false,
        maxNodes: Math.max(0, Number(limits.nodes) || 0),
        extended: false
      };
    }
    const effectiveTimeLimitMs = limits
      ? boundedTimeLimitMs
      : flexibleBudget
        ? Math.min(HARD_TIME_LIMIT_MS, boundedTimeLimitMs * FLEXIBLE_BUDGET_MULTIPLIER)
        : boundedTimeLimitMs;
    const evalFn = typeof options.evalFn === "function" ? options.evalFn : null;
    const context = {
      aiColor,
      evalFn,
      nodes: 0,
      cutoffs: 0,
      startedAt,
      deadline: startedAt + effectiveTimeLimitMs,
      hardDeadline: startedAt + (tm ? tm.hardMs : Math.min(HARD_TIME_LIMIT_MS, effectiveTimeLimitMs)),
      tm,
      rootDone: 0,
      rootTotal: 0,
      currentDepth: 0,
      bestChanged: false,
      timedOut: false,
      // Root applications are deterministic. Reuse their full end-turn result,
      // including monster movement, across safety checks and iterative depths.
      rootProbeCache: /* @__PURE__ */ new WeakMap(),
      killers: {},
      tt: /* @__PURE__ */ new Map()
    };
    let orderedRoot = orderActions(rootActions.map(cloneAction), boardState, aiColor);
    const forcedRoyalCapture = findForcedRoyalCaptureSequence(boardState, orderedRoot, aiColor, context);
    if (forcedRoyalCapture) {
      return { action: forcedRoyalCapture, score: INF / 2, nodes: 0, cutoffs: 0, completedDepth: 0, forced: true };
    }
    const forcedRoyalSpell = orderedRoot.find((action) => action?.forcedRoyalSpell) || null;
    if (forcedRoyalSpell) {
      return { action: forcedRoyalSpell, score: INF / 3, nodes: 0, cutoffs: 0, completedDepth: 0, forced: true };
    }
    const openingAction = options.skipOpeningBook ? null : pickOpeningFirstMoveAction(boardState, aiColor, orderedRoot);
    if (openingAction) {
      const openingIssue = rootActionHardSafetyIssue(boardState, openingAction, aiColor, context);
      if (!openingIssue) {
        return { action: openingAction, score: INF / 4, nodes: 0, cutoffs: 0, completedDepth: 0, opening: true };
      }
    }
    let bestAction = pickRootHardSafetyFallback(boardState, orderedRoot, aiColor, context) || orderedRoot.find((action) => action?.type !== "card") || null;
    let bestScore = -INF;
    let completedDepth = 0;
    let lastCandidates = [];
    let previousCompletedDepthMs = 0;
    const hasRuleMonster = workerHasRuleMonster(boardState);
    // Perf-visibility for the extension UI (ported from the old extension
    // engine.js): per-depth timing/node breakdown plus a live "searching
    // depth N" marker. Purely additive bookkeeping, never affects a decision.
    // `self` may not exist (plain Node) -- everything is best-effort.
    const depthProfile = [];
    const iterHistory = [];
    for (let currentDepth = 1; currentDepth <= maxDepth; currentDepth += 1) {
      if (tm && currentDepth > 1) {
        const elapsedMs = performance.now() - startedAt;
        if (currentDepth > tm.minDepth) {
          const remainingMs = tm.softMs - elapsedMs;
          if (remainingMs <= 0) break;
          if (tm.predictiveStop && iterHistory.length) {
            const last = iterHistory[iterHistory.length - 1];
            const prev = iterHistory.length > 1 ? iterHistory[iterHistory.length - 2] : 0;
            const ratio = prev > 0 ? Math.min(8, Math.max(1.5, last / prev)) : 3;
            if (remainingMs < last * ratio * 0.5) break;
          }
        } else if (elapsedMs >= tm.hardMs) {
          break;
        }
      }
      if (hasRuleMonster && completedDepth >= 2 && !monsterSearchHasTimeForNextDepth(context.deadline - performance.now(), previousCompletedDepthMs)) {
        break;
      }
      // Set BEFORE this depth's (possibly multi-second) work so the UI can show
      // it while it happens; cleared after the loop (a stale indicator is worse than none).
      try { self.__augSearchLive = { aiColor, depth: currentDepth, maxDepth, startedAt: Date.now() }; } catch (e) {}
      const nodesBeforeDepth = context.nodes;
      const depthStartedAt = performance.now();
      context.currentDepth = currentDepth;
      const depthResult = searchAtDepth(boardState, orderedRoot, aiColor, currentDepth, context);
      depthProfile.push({
        depth: currentDepth,
        ms: performance.now() - depthStartedAt,
        nodes: context.nodes - nodesBeforeDepth,
        completed: Boolean(depthResult.completed),
        score: typeof depthResult.score === "number" ? depthResult.score : null
      });
      if (depthResult.action && (depthResult.completed || flexibleBudget)) {
        bestAction = depthResult.action;
        bestScore = depthResult.score;
        completedDepth = currentDepth;
        lastCandidates = depthResult.candidates || lastCandidates;
        previousCompletedDepthMs = performance.now() - depthStartedAt;
        iterHistory.push(previousCompletedDepthMs);
        orderedRoot = [bestAction, ...orderedRoot.filter((action) => !sameAction(action, bestAction))];
      }
      if (context.timedOut) break;
    }
    try { self.__augSearchLive = null; } catch (e) {}
    try {
      self.__augLastSearchProfile = {
        timestamp: Date.now(),
        aiColor,
        totalMs: performance.now() - startedAt,
        totalNodes: context.nodes,
        cutoffs: context.cutoffs,
        completedDepth,
        maxDepth,
        depthProfile
      };
    } catch (e) {
      // profiling is best-effort, never worth failing the actual search over
    }
    return { action: bestAction, score: bestScore, nodes: context.nodes, cutoffs: context.cutoffs, completedDepth, candidates: lastCandidates, timeMs: performance.now() - startedAt, extended: Boolean(tm?.extended) };
  }
  function pickRootHardSafetyFallback(boardState, orderedRoot, aiColor, context = null) {
    let firstLegal = null;
    let firstNonDecisiveIssue = null;
    for (const action of orderedRoot || []) {
      if (!action) continue;
      if (firstLegal && rootSafetyDeadlineTight(context)) break;
      const probe = rootActionProbe(boardState, action, aiColor, context);
      if (!probe) continue;
      firstLegal ||= action;
      const issue = rootCandidateHardSafetyIssue(boardState, probe, aiColor);
      if (!issue) return action;
      if (issue !== "decisive-reply" && !firstNonDecisiveIssue) firstNonDecisiveIssue = action;
    }
    return firstNonDecisiveIssue || firstLegal;
  }
  function rootActionHardSafetyIssue(boardState, action, aiColor, context = null) {
    const probe = rootActionProbe(boardState, action, aiColor, context);
    if (!probe) return "illegal";
    return rootCandidateHardSafetyIssue(boardState, probe, aiColor);
  }
  function rootActionProbe(boardState, action, aiColor, context = null) {
    if (!boardState || !action) return null;
    const cache = context?.rootProbeCache;
    if (cache?.has(action)) return cache.get(action);
    const next = cloneState(boardState);
    const applied = applyAction(next, cloneAction(action), aiColor);
    if (!applied.ok) {
      cache?.set(action, null);
      return null;
    }
    const probe = {
      action,
      score: applied.score + actionOrderingScore(action, boardState, aiColor),
      appliedScore: applied.score,
      afterState: next,
      captureSwing: workerActionCaptureSwing(boardState, action, aiColor)
    };
    cache?.set(action, probe);
    return probe;
  }
  function searchAtDepth(boardState, orderedRoot, aiColor, depth, context) {
    let bestAction = null;
    let bestScore = -INF;
    let bestNonCardScore = -Infinity;
    let alpha = -INF;
    const beta = INF;
    context.timedOut = false;
    context.rootTotal = orderedRoot.length;
    context.rootDone = 0;
    context.bestChanged = false;
    const rootPrevBest = orderedRoot[0];
    const nonCardActions = orderedRoot.filter((action) => action?.type !== "card");
    const cardActions = orderedRoot.filter((action) => action?.type === "card");
    const scoredCandidates = [];
    const rejectedSoftCandidates = [];
    const rootThreatenedPieces = workerThreatenedPieces(boardState, aiColor);
    const evaluateRootAction = (action) => {
      if (isTimedOut(context)) return false;
      const candidateProbe = rootActionProbe(boardState, action, aiColor, context);
      if (!candidateProbe) return true;
      const next = candidateProbe.afterState;
      const beforeTurn = boardState.turn;
      const beforeActionsRemaining = boardState.actionsRemaining;
      const appliedScore = candidateProbe.appliedScore;
      const nextDepth = childDepthAfterAction(depth, action, beforeTurn, beforeActionsRemaining, next);
      const tacticalAdjustment = tacticalSafetyAdjustment(boardState, next, action, aiColor);
      candidateProbe._rootRationalHangingCapture = rootCandidateMakesRationalCapture(
        boardState,
        candidateProbe,
        aiColor,
        rootThreatenedPieces
      );
      const capturePreference = workerWorthwhileCapturePreference(boardState, action, aiColor) + (candidateProbe._rootRationalHangingCapture ? 160 : 0);
      candidateProbe.score = appliedScore + tacticalAdjustment + capturePreference + actionOrderingScore(action, boardState, aiColor);
      if (!rootCandidateDecisivelyWins(boardState, candidateProbe, aiColor)) {
        if (rootCandidateHardSafetyIssue(boardState, candidateProbe, aiColor)) return true;
        if (rootCandidateSoftSafetyIssue(boardState, candidateProbe, aiColor, context)) {
          rejectedSoftCandidates.push(candidateProbe);
          return true;
        }
      }
      if (isTimedOut(context)) return false;
      const score = appliedScore + tacticalAdjustment + capturePreference + minimax(next, nextDepth, alpha, beta, next.turn === aiColor, context);
      if (context.timedOut) return false;
      if (action?.type !== "card") {
        bestNonCardScore = Math.max(bestNonCardScore, score);
      } else if (!isRootCardUseBeneficial(score, bestNonCardScore, boardState, aiColor, findCard(boardState, action), next, action)) {
        return true;
      }
      scoredCandidates.push({
        ...candidateProbe,
        score
      });
      if (score > bestScore) {
        bestScore = score;
        bestAction = action;
        if (action !== rootPrevBest) context.bestChanged = true;
      }
      alpha = Math.max(alpha, bestScore);
      return true;
    };
    for (const action of nonCardActions) {
      if (!evaluateRootAction(action)) break;
      context.rootDone += 1;
    }
    if (!context.timedOut) {
      for (const action of cardActions) {
        if (!evaluateRootAction(action)) break;
        context.rootDone += 1;
      }
    }
    // Grafted from engine.optimized.js (our-only addition): degenerate
    // fallback for very sparse positions (few pieces left, e.g. late
    // endgames) where the hard/soft root-safety heuristics above can end up
    // flagging every candidate (they were tuned assuming a normal-density
    // position) -- without this, real search would never run at all and
    // the move would silently fall back to a shallow heuristic pick. Never
    // fires in normal positions (where scoredCandidates is already
    // non-empty), so it doesn't cost anything there.
    if (!scoredCandidates.length && !context.timedOut) {
      for (const action of nonCardActions) {
        if (isTimedOut(context)) break;
        const candidateProbe = rootActionProbe(boardState, action, aiColor, context);
        if (!candidateProbe) continue;
        const next = candidateProbe.afterState;
        const nextDepth = childDepthAfterAction(depth, action, boardState.turn, boardState.actionsRemaining, next);
        const tacticalAdjustment = tacticalSafetyAdjustment(boardState, next, action, aiColor);
        const capturePreference = workerWorthwhileCapturePreference(boardState, action, aiColor);
        const appliedScore = candidateProbe.appliedScore;
        const score = appliedScore + tacticalAdjustment + capturePreference + minimax(next, nextDepth, alpha, beta, next.turn === aiColor, context);
        if (context.timedOut) break;
        scoredCandidates.push({ ...candidateProbe, score });
        if (score > bestScore) {
          bestScore = score;
          bestAction = action;
        }
        alpha = Math.max(alpha, bestScore);
      }
    }
    const selected = selectRootCandidateAfterSafety(boardState, scoredCandidates, aiColor, context, rejectedSoftCandidates);
    return {
      action: selected?.action || bestAction,
      score: Number.isFinite(selected?.score) ? selected.score : bestScore,
      completed: !context.timedOut,
      candidates: scoredCandidates
    };
  }
  function selectRootCandidateAfterSafety(boardState, candidates, aiColor, context, rejectedSoftCandidates = []) {
    const sorted = (candidates || []).filter((candidate) => candidate?.action).sort((a, b) => b.score - a.score);
    if (!sorted.length) {
      return selectRootCaptureCompromise(boardState, rejectedSoftCandidates, aiColor, context) || rejectedSoftCandidates[0] || null;
    }
    const decisive = sorted.find((candidate) => rootCandidateDecisivelyWins(boardState, candidate, aiColor));
    if (decisive) return decisive;
    const hardSafe = [];
    for (const candidate of sorted) {
      if (!rootCandidateHardSafetyIssue(boardState, candidate, aiColor)) {
        hardSafe.push(candidate);
        if (rootSafetyDeadlineTight(context)) break;
      }
    }
    if (!hardSafe.length) {
      return sorted.find((candidate) => rootCandidateHardSafetyIssue(boardState, candidate, aiColor) !== "decisive-reply") || sorted[0] || null;
    }
    for (const candidate of hardSafe) {
      if (!rootCandidateSoftSafetyIssue(boardState, candidate, aiColor, context)) return candidate;
      if (rootSafetyDeadlineTight(context)) break;
    }
    return selectRootCaptureCompromise(boardState, [...hardSafe, ...rejectedSoftCandidates], aiColor, context) || hardSafe[0] || sorted[0] || null;
  }
  function rootCandidateHardSafetyIssue(boardState, candidate, aiColor) {
    if (candidate?._rootHardSafetyIssue !== void 0) return candidate._rootHardSafetyIssue;
    let issue = "";
    if (rootCandidateDecisivelyWins(boardState, candidate, aiColor)) issue = "";
    else if (!rootCandidateHasSameTurnMoveEffectIntent(boardState, candidate, aiColor)) issue = "unused-card-effect";
    else if (rootCandidateAllowsImmediateDecisiveReply(candidate.afterState, aiColor)) issue = "decisive-reply";
    else if (rootCandidateHangsFreeMaterial(candidate, aiColor)) issue = "hangs-material";
    else if (rootCandidateViolatesEarlyKingHome(boardState, candidate, aiColor)) issue = "early-king-advance";
    candidate._rootHardSafetyIssue = issue;
    return issue;
  }
  // Grafted from engine.optimized.js (our-only addition, not in the real
  // site engine): cheap "does this move just give away material for
  // nothing" hard filter, applied to every root candidate regardless of
  // whether the deeper search has time to confirm it.
  function rootCandidateHangsFreeMaterial(candidate, aiColor) {
    const action = candidate?.action;
    const afterState = candidate?.afterState;
    if (!action || action.type !== "move" || !afterState) return false;
    const dest = action.move || {};
    const movedPiece = get(afterState, dest.row, dest.col);
    if (!movedPiece || movedPiece.color !== aiColor || isWorkerDecisiveCaptureTarget(afterState, movedPiece)) return false;
    const threat = workerBestCaptureThreat(afterState, movedPiece, dest.row, dest.col);
    if (!threat) return false;
    const movedValue = workerStrategicPieceValue(afterState, movedPiece);
    const attackerValue = pieceValue(threat.piece);
    const gained = candidate.captureSwing?.value || 0;
    if (candidate.captureSwing?.decisive) return false;
    return attackerValue <= movedValue - 40 && movedValue - gained > 220;
  }
  function rootCandidateHasSameTurnMoveEffectIntent(boardState, candidate, color) {
    const effect = sameTurnMoveEffectCardEffect(boardState, candidate?.action);
    if (!effect) return true;
    const afterState = candidate?.afterState;
    if (!afterState || afterState.mode === "gameover" || afterState.turn !== color) return false;
    const followups = generateActions(afterState, color).filter((action) => action?.type === "move");
    if (!followups.length) return false;
    const scored = followups.map((action, index) => ({
      action,
      index,
      score: sameTurnEffectFollowupPlanScore(afterState, action, color)
    })).filter((entry) => Number.isFinite(entry.score)).sort((a, b) => b.score - a.score || a.index - b.index);
    if (!scored.length) return false;
    const effectBest = scored.find((entry) => moveUsesSameTurnCardEffect(boardState, afterState, entry.action, candidate.action, effect, color));
    return Boolean(effectBest && effectBest.score >= scored[0].score - SAME_TURN_CARD_INTENT_TOLERANCE);
  }
  function sameTurnMoveEffectCardEffect(boardState, action) {
    if (action?.type !== "card") return "";
    const card = findCard(boardState, action);
    const effect = card?.effect || "";
    return SAME_TURN_MOVE_EFFECT_CARD_EFFECTS.has(effect) ? effect : "";
  }
  function sameTurnEffectFollowupPlanScore(boardState, action, color) {
    if (!boardState || action?.type !== "move") return -INF;
    const beforeValue = evaluateState(boardState, color);
    const next = cloneState(boardState);
    const applied = applyAction(next, cloneAction(action), color);
    if (!applied.ok) return -INF;
    const positionDelta = evaluateState(next, color) - beforeValue;
    return applied.score + tacticalSafetyAdjustment(boardState, next, action, color) + workerWorthwhileCapturePreference(boardState, action, color) + actionOrderingScore(action, boardState, color) + positionDelta * 0.22;
  }
  function moveUsesSameTurnCardEffect(beforeState, afterCardState, action, cardAction, effect, color) {
    if (action?.type !== "move") return false;
    const moving = get(afterCardState, action.from?.row, action.from?.col);
    if (!moving || moving.color !== color) return false;
    if (effect === "bishopSnipe") return Boolean(action.move?.bishopSnipe);
    if (effect === "charge") return sameSquare(action.from, cardAction?.target) && Boolean(action.move?.chargeRush);
    if (effect === "enPassantBang") return Boolean(action.move?.enPassantFrenzy);
    if (effect === "freeCastling") return Boolean(action.move?.freeCastling);
    if (effect === "promotionRush") return sameSquare(action.from, cardAction?.target) && Boolean(action.move?.promotionRushMove);
    if (effect === "substitution") return Boolean(action.move?.substitutionSwap);
    if (effect === "switcheroo") return Boolean(action.move?.switcherooMove);
    if (effect === "breakthroughOrder") return workerMoveUsesBreakthroughCapture(beforeState, action, color);
    return false;
  }
  function workerMoveUsesBreakthroughCapture(boardState, action, color) {
    const moving = get(boardState, action.from?.row, action.from?.col);
    if (!moving || moving.color !== color || !["pawn", "squire", "standardBearer", "fanatic"].includes(renderType(moving))) return false;
    if (action.from?.col !== action.move?.col) return false;
    const target = get(boardState, action.move?.row, action.move?.col);
    return canWorkerCaptureTarget(color, target, moving, boardState);
  }
  // Grafted from engine.optimized.js (our-only addition): mirrors
  // rootCandidateIgnoresMajorHangingPiece but for the enemy's hand -- a
  // candidate that lets the opponent's best usable card jump sharply in
  // threat value gets flagged even when that reply isn't outright decisive.
  const ENEMY_CARD_THREAT_INCREASE_THRESHOLD = 220;
  const ENEMY_CARD_THREAT_MIN_ABSOLUTE = 240;
  function workerBestEnemyCardThreat(boardState, color) {
    let best = 0;
    deck(boardState, color).forEach((card) => {
      const value = singleCardThreatValue(boardState, card, color);
      if (value > best) best = value;
    });
    return best;
  }
  function rootCandidateIgnoresEnemyCardThreat(boardState, candidate, aiColor) {
    if (!boardState || !candidate?.afterState || !COLORS.includes(aiColor)) return false;
    if (candidate.captureSwing?.decisive) return false;
    const enemy = opponent(aiColor);
    if (candidate.afterState.turn !== enemy) return false;
    const beforeThreat = workerBestEnemyCardThreat(boardState, enemy);
    const afterThreat = workerBestEnemyCardThreat(candidate.afterState, enemy);
    const increase = afterThreat - beforeThreat;
    if (increase < ENEMY_CARD_THREAT_INCREASE_THRESHOLD || afterThreat < ENEMY_CARD_THREAT_MIN_ABSOLUTE) return false;
    const gained = Math.max(candidate.captureSwing?.value || 0, rootCandidateImmediateCaptureValue(boardState, candidate, aiColor));
    if (gained >= increase) return false;
    return true;
  }
  function rootCandidateSoftSafetyIssue(boardState, candidate, aiColor, context) {
    if (candidate?._rootSoftSafetyIssue !== void 0) return candidate._rootSoftSafetyIssue;
    let issue = "";
    if (rootCandidateMovesPieceIntoBadCapture(boardState, candidate, aiColor)) issue = "bad-capture-square";
    else if (rootCandidateAdvancesIntoEnemyProtectedSquare(boardState, candidate, aiColor)) issue = "enemy-protected-advance";
    else if (rootCandidateIgnoresMajorHangingPiece(boardState, candidate, aiColor)) issue = "ignored-hanging-piece";
    else if (rootCandidateIgnoresEnemyCardThreat(boardState, candidate, aiColor)) issue = "ignored-card-threat";
    else if (!rootSafetyDeadlineTight(context) && rootCandidateCreatesFleeTrap(boardState, candidate, aiColor, context)) issue = "flee-trap";
    candidate._rootSoftSafetyIssue = issue;
    return issue;
  }
  function selectRootCaptureCompromise(boardState, candidates, aiColor, context) {
    const captureOptions = [];
    for (const candidate of candidates || []) {
      if (rootCandidateHardSafetyIssue(boardState, candidate, aiColor)) continue;
      const movingKey = rootMovedPieceKey(boardState, candidate, aiColor);
      if (!movingKey) continue;
      if (rootPieceHasSoftSafeCandidate(boardState, candidates, aiColor, movingKey, context)) continue;
      const captureValue = rootCandidateHighestCaptureValue(boardState, candidate, aiColor);
      if (captureValue <= 0) continue;
      captureOptions.push({ candidate, captureValue });
    }
    return captureOptions.sort((a, b) => b.captureValue - a.captureValue || b.candidate.score - a.candidate.score)[0]?.candidate || null;
  }
  function rootPieceHasSoftSafeCandidate(boardState, candidates, aiColor, movingKey, context) {
    return (candidates || []).some((candidate) => rootMovedPieceKey(boardState, candidate, aiColor) === movingKey && !rootCandidateHardSafetyIssue(boardState, candidate, aiColor) && !(rootSafetyDeadlineTight(context) && candidate._rootSoftSafetyIssue === void 0) && !rootCandidateSoftSafetyIssue(boardState, candidate, aiColor, context));
  }
  function rootCandidateHighestCaptureValue(boardState, candidate, aiColor) {
    if (!candidate?.action || candidate.action.type !== "move") return 0;
    return workerActionCaptureEntries(boardState, candidate.action, aiColor).reduce((best, entry) => Math.max(best, workerStrategicPieceValue(boardState, entry.piece)), 0);
  }
  function rootMovedPieceKey(boardState, candidate, aiColor) {
    const moving = rootMovedPieceBefore(boardState, candidate?.action, aiColor);
    if (!moving) return "";
    return moving.piece?.id || `${moving.piece?.type || "piece"}:${moving.row}:${moving.col}`;
  }
  function rootCandidateDecisivelyWins(boardState, candidate, aiColor) {
    if (candidate?.afterState?.mode === "gameover" && candidate.afterState.winner === aiColor) return true;
    return actionDecisivelyWins(boardState, candidate?.action, aiColor, candidate?.afterState);
  }
  function rootCandidateAllowsImmediateDecisiveReply(afterState, aiColor) {
    if (!afterState) return false;
    if (afterState.mode === "gameover") return afterState.winner && afterState.winner !== aiColor && afterState.winner !== "draw";
    const enemy = opponent(aiColor);
    if (afterState.turn !== enemy) return false;
    const replies = generateActions(afterState, enemy);
    if (replies.some((reply) => actionDecisivelyWins(afterState, reply, enemy))) return true;
    return rootCandidateAllowsImmediateDecisiveSetupReply(afterState, replies, enemy);
  }
  function rootCandidateAllowsImmediateDecisiveSetupReply(afterState, replies, enemy) {
    for (const setup of replies) {
      if (!rootActionCanSetUpSameTurnDecisiveReply(afterState, setup)) continue;
      const next = cloneState(afterState);
      const applied = applyAction(next, cloneAction(setup), enemy);
      if (!applied.ok || next.mode === "gameover" || next.turn !== enemy) continue;
      const followups = generateActions(next, enemy);
      if (followups.some((followup) => actionDecisivelyWins(next, followup, enemy))) return true;
    }
    return false;
  }
  function rootActionCanSetUpSameTurnDecisiveReply(boardState, action) {
    if (action?.type !== "card") return false;
    const card = findCard(boardState, action);
    if (!card || card.used || card.recovering || isWorkerCardPendingNextTurn(card)) return false;
    return card.effect === "bishopSnipe";
  }
  // Cheap exact-in-practice prefilter (tools/perf/prefilter-check.js: 0 violations
  // over 12k replies): a plain move by a standard piece (no extra move fields) can
  // only end the game / remove a royal by landing on it, so skip clone+apply unless
  // the destination holds a piece of the victim that might be royal.
  const PLAIN_MOVER_TYPES = /* @__PURE__ */ new Set(["pawn", "knight", "bishop", "rook", "queen", "king"]);
  const PLAIN_TARGET_TYPES = /* @__PURE__ */ new Set(["pawn", "knight", "bishop", "rook", "queen"]);
  function actionCannotRemoveRoyal(boardState, action, color) {
    if (action.type !== "move" || !action.from || !action.move) return false;
    const mover = boardState.board?.[action.from.row]?.[action.from.col];
    if (!mover || !PLAIN_MOVER_TYPES.has(mover.type)) return false;
    for (const key in action.move) if (key !== "row" && key !== "col") return false;
    const dest = boardState.board?.[action.move.row]?.[action.move.col];
    if (dest && dest.color !== color && !PLAIN_TARGET_TYPES.has(dest.type)) return false;
    return true;
  }
  function actionDecisivelyWins(boardState, action, color, afterState = null) {
    if (!boardState || !action) return false;
    if (!afterState && actionCannotRemoveRoyal(boardState, action, color)) return false;
    const beforeRoyal = royalPieceIdentityKeys(boardState, opponent(color));
    const next = afterState || cloneState(boardState);
    if (!afterState) {
      const applied = applyAction(next, cloneAction(action), color);
      if (!applied.ok) return false;
    }
    if (next.mode === "gameover" && next.winner === color) return true;
    if (!beforeRoyal.size) return false;
    const afterRoyal = royalPieceIdentityKeys(next, opponent(color));
    if (afterRoyal.size < beforeRoyal.size) return true;
    for (const key of beforeRoyal) {
      if (!afterRoyal.has(key)) return true;
    }
    return false;
  }
  function royalPieceIdentityKeys(boardState, color) {
    const keys = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color === color && isWorkerDecisiveCaptureTarget(boardState, piece)) {
        keys.add(`${piece.id || piece.type}:${row}:${col}`);
      }
    });
    return keys;
  }
  function rootCandidateViolatesEarlyKingHome(boardState, candidate, aiColor) {
    const action = candidate?.action;
    if (!rootActionMovesPieceOnBoard(boardState, action)) return false;
    const moving = rootMovedPieceBefore(boardState, action, aiColor);
    if (!moving || !isWorkerRuleRoyalKing(boardState, moving.piece)) return false;
    if ((Number(boardState?.turnsTaken?.[aiColor]) || 0) >= EARLY_KING_HOME_TURNS) return false;
    const move = action.move || {};
    if (!rootRoyalMoveIncreasesHomeDistance(boardState, moving, move, aiColor)) return false;
    if (!rootMoveHasProfitableCapture(candidate.captureSwing, 0)) return true;
    return rootCandidateAllowsImmediateDecisiveReply(candidate.afterState, aiColor);
  }
  function rootRoyalMoveIncreasesHomeDistance(boardState, moving, move, color) {
    if (!Number.isInteger(move?.row)) return false;
    const homeRow = color === "white" ? boardRowCount(boardState) - 1 : 0;
    const beforeDistance = Math.abs(moving.row - homeRow);
    const afterDistance = Math.abs(move.row - homeRow);
    return afterDistance > beforeDistance;
  }
  function rootCandidateMovesPieceIntoBadCapture(boardState, candidate, aiColor) {
    const action = candidate?.action;
    if (!rootActionMovesPieceOnBoard(boardState, action)) return false;
    const moving = rootMovedPieceBefore(boardState, action, aiColor);
    if (!moving || isWorkerDecisiveCaptureTarget(boardState, moving.piece)) return false;
    const afterPiece = rootMovedPieceAfter(candidate.afterState, moving, action);
    if (!afterPiece) return false;
    const threat = workerBestCaptureThreat(candidate.afterState, afterPiece.piece, afterPiece.row, afterPiece.col);
    if (!threat) return false;
    const movingValue = workerStrategicPieceValue(boardState, moving.piece);
    if (rootMoveHasProfitableCapture(candidate.captureSwing, movingValue)) return false;
    if (isWorkerPieceDefendedByFriend(candidate.afterState, afterPiece.piece, afterPiece.row, afterPiece.col) && pieceValue(threat.piece) > movingValue) {
      return false;
    }
    return true;
  }
  function rootCandidateAdvancesIntoEnemyProtectedSquare(boardState, candidate, aiColor) {
    const action = candidate?.action;
    if (!rootActionMovesPieceOnBoard(boardState, action)) return false;
    const moving = rootMovedPieceBefore(boardState, action, aiColor);
    if (!moving || isWorkerDecisiveCaptureTarget(boardState, moving.piece)) return false;
    const move = action.move || {};
    if ((move.row - moving.row) * pawnDir(boardState, aiColor) <= 0) return false;
    const afterPiece = rootMovedPieceAfter(candidate.afterState, moving, action);
    if (!afterPiece) return false;
    const ownControls = workerSquareControlCount(candidate.afterState, afterPiece.piece, afterPiece.row, afterPiece.col, aiColor);
    const enemyControls = workerSquareControlCount(candidate.afterState, afterPiece.piece, afterPiece.row, afterPiece.col, opponent(aiColor));
    return enemyControls > ownControls;
  }
  function rootCandidateIgnoresMajorHangingPiece(boardState, candidate, aiColor) {
    if (!boardState || !candidate?.afterState || !COLORS.includes(aiColor)) return false;
    if (candidate.captureSwing?.decisive) return false;
    const hanging = workerMajorHangingPieces(boardState, aiColor);
    if (!hanging.length) return false;
    const rationalCapture = candidate._rootRationalHangingCapture ?? rootCandidateMakesRationalCapture(boardState, candidate, aiColor, hanging);
    if (rationalCapture) return false;
    const maxHangingValue = Math.max(...hanging.map((entry) => entry.value));
    if (rootCandidateImmediateCaptureValue(boardState, candidate, aiColor) >= maxHangingValue) return false;
    const beforeRisk = workerTrackedHangingRisk(boardState, hanging);
    const afterRisk = workerTrackedHangingRisk(candidate.afterState, hanging);
    if (afterRisk <= beforeRisk * 0.45) return false;
    return true;
  }
  function rootCandidateMakesRationalCapture(boardState, candidate, aiColor, hanging = workerMajorHangingPieces(boardState, aiColor)) {
    const action = candidate?.action;
    const moving = rootMovedPieceBefore(boardState, action, aiColor);
    if (!moving || !hanging.length) return false;
    const capturedAttackers = workerActionCaptureEntries(boardState, action, aiColor).filter((captured) => hanging.some((entry) => {
      const threatened = findWorkerPieceByRef(boardState, entry);
      if (!threatened) return false;
      return workerThreatTargetCells(boardState, threatened.piece, threatened.row, threatened.col).some((cell) => attacksSquare(boardState, captured.piece, captured.row, captured.col, cell.row, cell.col));
    }));
    const capturedValue = capturedAttackers.reduce((sum, entry) => sum + workerStrategicPieceValue(boardState, entry.piece), 0);
    if (capturedValue <= 0) return false;
    const movingValue = workerStrategicPieceValue(boardState, moving.piece);
    if (isRationalCapture({ movingValue, capturedValue, canBeRecaptured: true })) return true;
    const afterPiece = rootMovedPieceAfter(candidate.afterState, moving, action);
    if (!afterPiece) return false;
    const threat = workerBestCaptureThreat(candidate.afterState, afterPiece.piece, afterPiece.row, afterPiece.col);
    return isRationalCapture({ movingValue, capturedValue, canBeRecaptured: Boolean(threat) });
  }
  function rootCandidateImmediateCaptureValue(boardState, candidate, aiColor) {
    const action = candidate?.action;
    let value = Number(candidate?.captureSwing?.value) || 0;
    if (action?.target && ["card", "wizardSpell"].includes(action.type)) {
      const target = get(boardState, action.target.row, action.target.col);
      if (target?.color === opponent(aiColor) && !isFrozenPiece(target)) {
        value = Math.max(value, workerStrategicPieceValue(boardState, target));
      }
    }
    return value;
  }
  function workerThreatenedPieces(boardState, color) {
    const pieces = [];
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || ["wall", "football", "blackHole"].includes(piece.type) || isFrozenPiece(piece)) return;
      const key = piece.id || `${piece.type}:${row}:${col}`;
      if (seen.has(key)) return;
      const threat = workerBestCaptureThreat(boardState, piece, row, col);
      if (!threat) return;
      seen.add(key);
      pieces.push({ id: piece.id || "", type: piece.type || "", color, row, col });
    });
    return pieces;
  }
  function workerMajorHangingPieces(boardState, color) {
    const pieces = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || isWorkerDecisiveCaptureTarget(boardState, piece)) return;
      const value = workerStrategicPieceValue(boardState, piece);
      if (value < ROOT_HANGING_MAJOR_VALUE) return;
      const threat = workerBestCaptureThreat(boardState, piece, row, col);
      if (!threat) return;
      const threatValue = pieceValue(threat.piece);
      if (isWorkerPieceDefendedByFriend(boardState, piece, row, col) && threatValue > value) return;
      const penalty = workerHangingPenalty(boardState, piece, row, col);
      if (penalty <= 0) return;
      pieces.push({
        id: piece.id || "",
        type: piece.type || "",
        color,
        row,
        col,
        value,
        penalty
      });
    });
    return pieces;
  }
  function workerTrackedHangingRisk(boardState, entries = []) {
    return entries.reduce((sum, entry) => {
      const found = findWorkerPieceByRef(boardState, entry);
      if (!found || found.piece.color !== entry.color) return sum;
      return sum + workerHangingPenalty(boardState, found.piece, found.row, found.col);
    }, 0);
  }
  function rootCandidateCreatesFleeTrap(boardState, candidate, aiColor, context) {
    const action = candidate?.action;
    if (!rootActionMovesPieceOnBoard(boardState, action)) return false;
    const moving = rootMovedPieceBefore(boardState, action, aiColor);
    if (!moving || isWorkerDecisiveCaptureTarget(boardState, moving.piece)) return false;
    if (candidate?.afterState?.turn !== opponent(aiColor)) return false;
    const movedRef = rootMovedPieceRef(moving, action);
    const enemy = opponent(aiColor);
    const replies = generateActions(candidate.afterState, enemy);
    for (const reply of replies) {
      if (rootSafetyDeadlineTight(context)) return false;
      if (rootReplyCreatesBadFleeThreat(candidate.afterState, reply, aiColor, movedRef, moving, candidate.captureSwing)) {
        return true;
      }
    }
    return false;
  }
  function rootReplyCreatesBadFleeThreat(afterState, reply, aiColor, movedRef, moving, captureSwing) {
    if (reply?.type !== "move") return false;
    if (!rootActionMovesPieceOnBoard(afterState, reply)) return false;
    const enemy = opponent(aiColor);
    const enemyBefore = rootMovedPieceBefore(afterState, reply, enemy);
    if (!enemyBefore || isWorkerDecisiveCaptureTarget(afterState, enemyBefore.piece)) return false;
    const replyState = cloneState(afterState);
    const applied = applyAction(replyState, cloneAction(reply), aiColor);
    if (!applied.ok) return false;
    const ownAfter = findWorkerPieceByRef(replyState, movedRef);
    const movingValue = workerStrategicPieceValue(afterState, moving?.piece);
    if (!ownAfter) return !rootMoveHasProfitableCapture(captureSwing, movingValue);
    const enemyAfter = rootMovedPieceAfter(replyState, enemyBefore, reply);
    if (!enemyAfter) return false;
    if (!attacksSquare(replyState, enemyAfter.piece, enemyAfter.row, enemyAfter.col, ownAfter.row, ownAfter.col)) return false;
    const enemyValue = pieceValue(enemyAfter.piece);
    if (workerPieceCanCaptureThreat(replyState, ownAfter, enemyAfter) && enemyValue >= movingValue) return false;
    return true;
  }
  function rootActionMovesPieceOnBoard(boardState, action) {
    if (action?.type !== "move") return false;
    const move = action.move || {};
    if (!Number.isInteger(move.row) || !Number.isInteger(move.col)) return false;
    if (move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.setLogDirection) return false;
    if (workerPortalCaptureCells(move).some(({ row, col }) => get(boardState, row, col)?.shielded) && !move.colossusAttack && !move.shotgunBlast && !move.shotgunSnipe) return false;
    return true;
  }
  function rootMovedPieceBefore(boardState, action, color) {
    if (action?.type !== "move") return null;
    const from = action.from || {};
    const piece = get(boardState, from.row, from.col);
    if (!piece || piece.color !== color) return null;
    return { piece, row: from.row, col: from.col };
  }
  function rootMovedPieceAfter(afterState, moving, action) {
    if (!afterState || !moving || action?.type !== "move") return null;
    const move = workerActionDestination(action) || action.move || {};
    return findWorkerPieceByRef(afterState, {
      id: moving.piece?.id,
      row: move.row,
      col: move.col
    });
  }
  function rootMovedPieceRef(moving, action) {
    const move = action?.move || {};
    return {
      id: moving?.piece?.id,
      row: Number.isInteger(move.row) ? move.row : moving?.row,
      col: Number.isInteger(move.col) ? move.col : moving?.col
    };
  }
  function rootMoveHasProfitableCapture(captureSwing, movingValue) {
    const swing = captureSwing || { value: 0, decisive: false };
    const threshold = Math.max(0, Number(movingValue) || 0);
    return Boolean(swing.decisive || swing.value > 0 && swing.value >= threshold);
  }
  function workerPieceCanCaptureThreat(boardState, ownEntry, threatEntry) {
    return Boolean(
      ownEntry?.piece && threatEntry?.piece && attacksSquare(boardState, ownEntry.piece, ownEntry.row, ownEntry.col, threatEntry.row, threatEntry.col)
    );
  }
  function isWorkerPieceDefendedByFriend(boardState, targetPiece, targetRow2, targetCol2) {
    if (!boardState || !targetPiece?.color) return false;
    let defended = false;
    forEachPiece(boardState, (piece, row, col) => {
      if (defended || !piece || piece.color !== targetPiece.color || piece.type === "wall") return;
      if (piece === targetPiece || piece.id && targetPiece.id && piece.id === targetPiece.id) return;
      if (isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return;
      if (workerFriendlyControlsSquare(boardState, piece, row, col, targetPiece, targetRow2, targetCol2)) defended = true;
    });
    return defended;
  }
  function workerSquareControlCount(boardState, targetPiece, targetRow2, targetCol2, color) {
    if (!boardState || !targetPiece || !color) return 0;
    let count = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece || piece.color !== color || piece.type === "wall") return;
      if (piece === targetPiece || piece.id && targetPiece.id && piece.id === targetPiece.id) return;
      if (isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return;
      if (workerControlsSquare(boardState, piece, row, col, targetPiece, targetRow2, targetCol2)) count += 1;
    });
    return count;
  }
  function workerControlsSquare(boardState, piece, row, col, targetPiece, targetRow2, targetCol2) {
    const original = get(boardState, targetRow2, targetCol2);
    if (original !== targetPiece) return false;
    const needsFriendlyProxy = piece.color === targetPiece.color;
    const proxy = needsFriendlyProxy ? {
      ...targetPiece,
      color: opponent(piece.color),
      protected: false,
      shielded: false
    } : original;
    if (needsFriendlyProxy) set(boardState, targetRow2, targetCol2, proxy);
    try {
      return attacksSquare(boardState, piece, row, col, targetRow2, targetCol2);
    } finally {
      if (needsFriendlyProxy) set(boardState, targetRow2, targetCol2, original);
    }
  }
  function workerFriendlyControlsSquare(boardState, piece, row, col, targetPiece, targetRow2, targetCol2) {
    const original = get(boardState, targetRow2, targetCol2);
    if (original !== targetPiece) return false;
    const proxy = {
      ...targetPiece,
      color: opponent(piece.color),
      protected: false,
      shielded: false
    };
    set(boardState, targetRow2, targetCol2, proxy);
    try {
      return attacksSquare(boardState, piece, row, col, targetRow2, targetCol2);
    } finally {
      set(boardState, targetRow2, targetCol2, original);
    }
  }
  function rootSafetyDeadlineTight(context) {
    const softDeadline = Number.isFinite(Number(context?.deadline)) ? Number(context.deadline) : Infinity;
    const hardDeadline = Number.isFinite(Number(context?.hardDeadline)) ? Number(context.hardDeadline) : Infinity;
    const deadline = Math.min(softDeadline, hardDeadline);
    return Number.isFinite(deadline) && performance.now() >= deadline - ROOT_SAFETY_GRACE_MS;
  }
  function workerHasRuleMonster(boardState) {
    let found = false;
    forEachPiece(boardState, (piece) => {
      if (piece?.type === "monster") found = true;
    });
    return found;
  }
  function monsterSearchHasTimeForNextDepth(remainingMs, previousDepthMs) {
    const remaining = Math.max(0, Number(remainingMs) || 0);
    const previous = Math.max(0, Number(previousDepthMs) || 0);
    return previous <= 0 || remaining >= previous * MONSTER_NEXT_DEPTH_GROWTH + MONSTER_NEXT_DEPTH_GRACE_MS;
  }
  // Grafted from engine.optimized.js (our-only addition): rootCardComboFollowupBonus
  // (best next-turn card threat available right after this card's use, since
  // cards never end the turn -- see childDepthAfterAction) plus a small
  // library of specific named combo detectors (CARD_COMBO_DETECTORS /
  // namedCardComboBonus), feed a bounded contextual discount into
  // isRootCardUseBeneficial below instead of the real file's bare
  // score-vs-baseline gate.
  const CARD_COMBO_FOLLOWUP_WEIGHT = 0.35;
  function rootCardComboFollowupBonus(afterState, aiColor) {
    if (!afterState || afterState.mode === "gameover" || afterState.turn !== aiColor) return 0;
    let best = 0;
    deck(afterState, aiColor).forEach((card) => {
      const value = singleCardThreatValue(afterState, card, aiColor);
      if (value > best) best = value;
    });
    return best;
  }
  const CARD_COMBO_DETECTORS = [
    {
      name: "portalGun-king-escape-block",
      effects: /* @__PURE__ */ new Set(["portalGun"]),
      weight: 0.5,
      detect(boardState, action, afterState, color) {
        const enemy = opponent(color);
        const king = findWorkerKingRole(boardState, enemy);
        if (!king || !afterState || afterState.turn !== color) return 0;
        const escapeSquares = generateActions(boardState, enemy).filter(
          (a) => a.type === "move" && a.from?.row === king.row && a.from?.col === king.col
        );
        if (escapeSquares.length !== 2) return 0;
        const candidates = generateActions(afterState, color).filter((a) => a.type === "move").slice(0, 12);
        for (const candidate of candidates) {
          const next = cloneState(afterState);
          const applied = applyAction(next, cloneAction(candidate), color);
          if (applied.ok && isSquareAttacked(next, king.row, king.col, color)) return 480;
        }
        return 0;
      }
    },
    {
      name: "vip-witchTrial-lockout",
      effects: /* @__PURE__ */ new Set(["vip", "witchTrial"]),
      weight: 1,
      detect(boardState, action, afterState, color) {
        const square = action?.target;
        if (!square || !Number.isInteger(square.row) || !Number.isInteger(square.col) || !afterState) return 0;
        const piece = get(afterState, square.row, square.col);
        if (!piece || piece.color !== opponent(color)) return 0;
        if (!piece.witchTrial || !isWorkerDecisiveCaptureTarget(afterState, piece)) return 0;
        const canEverCapture = generateActions(afterState, piece.color).some(
          (a) => a.type === "move" && a.from?.row === square.row && a.from?.col === square.col && get(afterState, a.move?.row, a.move?.col)
        );
        return canEverCapture ? 0 : 900;
      }
    }
  ];
  function namedCardComboBonus(boardState, action, afterState, color) {
    const effect = findCard(boardState, action)?.effect || "";
    let total = 0;
    for (const detector of CARD_COMBO_DETECTORS) {
      if (!detector.effects.has(effect)) continue;
      total += (detector.detect(boardState, action, afterState, color) || 0) * (detector.weight ?? 1);
    }
    return total;
  }
  const CARD_CONTEXT_MAX_DISCOUNT = 150;
  function isRootCardUseBeneficial(score, bestNonCardScore, boardState, aiColor, card, afterState, action) {
    const baseline = Number.isFinite(bestNonCardScore) ? bestNonCardScore : evaluateState(boardState, aiColor);
    const contextual = card ? singleCardThreatValue(boardState, card, aiColor) : 0;
    const comboBonus = rootCardComboFollowupBonus(afterState, aiColor) * CARD_COMBO_FOLLOWUP_WEIGHT;
    const namedBonus = action ? namedCardComboBonus(boardState, action, afterState, aiColor) : 0;
    const discount = Math.max(-CARD_CONTEXT_MAX_DISCOUNT, Math.min(CARD_CONTEXT_MAX_DISCOUNT, (contextual + comboBonus + namedBonus) * 0.3));
    return score > baseline + CARD_USE_MIN_GAIN - discount;
  }
  function pickOpeningFirstMoveAction(boardState, color, actions) {
    if (!isOpeningFirstMoveTurn(boardState, color)) return null;
    const choices = (AI_OPENING_FIRST_MOVE_CANDIDATES[color] || []).map((candidate) => ({
      candidate,
      action: actions.find((action) => actionMatchesOpeningCandidate(boardState, action, candidate))
    })).filter(({ action }) => Boolean(action));
    if (!choices.length) return null;
    const selected = choices[Math.floor(Math.random() * choices.length)];
    return workerKnightSafeOpeningAction(boardState, color, selected, choices);
  }
  function isOpeningFirstMoveTurn(boardState, color) {
    return Boolean(
      boardState?.mode === "play" && boardState.turn === color && boardRowCount(boardState) === 8 && boardColCount(boardState) === 8 && (Number(boardState.turnsTaken?.[color]) || 0) === 0 && !boardState.activeTrolley && !activeWorkerForcedExtraMove(boardState, color)
    );
  }
  function actionMatchesOpeningCandidate(boardState, action, candidate) {
    if (!action || action.type !== "move" || !candidate) return false;
    const from = parseWorkerSquare(boardState, candidate.from);
    const to = parseWorkerSquare(boardState, candidate.to);
    return action.from?.row === from.row && action.from?.col === from.col && action.move?.row === to.row && action.move?.col === to.col;
  }
  function workerKnightSafeOpeningAction(boardState, color, selected, choices) {
    if (!selected?.action) return null;
    const file = workerCentralPawnOpeningFile(selected.candidate);
    if (!file || !workerOpeningPawnTargetAttackedByEnemyKnight(boardState, color, selected.candidate)) return selected.action;
    const alternateFile = file === "e" ? "d" : "e";
    const alternate = choices.find((entry) => workerCentralPawnOpeningFile(entry.candidate) === alternateFile);
    if (alternate?.action && !workerOpeningPawnTargetAttackedByEnemyKnight(boardState, color, alternate.candidate)) return alternate.action;
    return choices.find((entry) => !workerCentralPawnOpeningFile(entry.candidate))?.action || alternate?.action || selected.action;
  }
  function workerCentralPawnOpeningFile(candidate) {
    const file = String(candidate?.from || "").slice(0, 1).toLowerCase();
    return file === "d" || file === "e" ? file : "";
  }
  function workerOpeningPawnTargetAttackedByEnemyKnight(boardState, color, candidate) {
    const target = parseWorkerSquare(boardState, candidate?.to);
    if (!inBounds(target.row, target.col, boardState)) return false;
    const enemy = opponent(color);
    let attacked = false;
    forEachPiece(boardState, (piece, row, col) => {
      if (attacked || piece?.color !== enemy || piece.type !== "knight") return;
      attacked = workerKnightDeltasForMove(boardState, row, col, enemy).some(([dr, dc]) => row + dr === target.row && col + dc === target.col);
    });
    return attacked;
  }
  // Grafted from engine.optimized.js (our-only additions): search-layer
  // helpers not yet wired into minimax/searchBestAction below -- kept as
  // standalone (currently-unreachable) functions here, then connected when
  // minimax and searchBestAction are replaced wholesale with our enhanced
  // versions (see the graft-progress log for why that step comes last: it
  // needs every one of these already in place first).
  // let (not const): options.params can override them for one search call (experiments);
  // searchBestAction restores the defaults afterwards, so the default behaviour is unchanged.
  let NULL_MOVE_MIN_DEPTH = 3;
  let NULL_MOVE_REDUCTION = 2;
  let LMR_MIN_DEPTH = 3;
  let LMR_MOVE_THRESHOLD = 4;
  function nullMoveOk(boardState, color) {
    const king = criticalPieces(boardState, color)[0];
    if (king && isSquareAttacked(boardState, king.row, king.col, opponent(color))) return false;
    let nonPawnCount = 0;
    forEachPiece(boardState, (piece) => {
      if (piece.color === color && piece.type !== "pawn" && piece.type !== "wall") nonPawnCount += 1;
    });
    return nonPawnCount >= 3;
  }
  function isCaptureAction(boardState, action) {
    if (!action || action.type !== "move") return false;
    const dest = action.move || {};
    return Boolean(get(boardState, dest.row, dest.col));
  }
  const CARD_TACTICAL_RELEVANCE_THRESHOLD = 200;
  function isTacticallyRelevantCardAction(boardState, action, color) {
    if (!action || action.type !== "card") return false;
    return singleCardThreatValue(boardState, findCard(boardState, action), color) >= CARD_TACTICAL_RELEVANCE_THRESHOLD;
  }
  let QUIESCENCE_MAX_PLIES = 6;
  function quiescence(boardState, alpha, beta, isMaximizingPlayer, context, qDepth) {
    context.nodes += 1;
    if ((context.nodes & 255) === 0 && isTimedOut(context)) return (context.evalFn || evaluateState)(boardState, context.aiColor);
    const standPat = (context.evalFn || evaluateState)(boardState, context.aiColor);
    if (boardState.mode === "gameover" || qDepth <= 0) return standPat;
    const color = boardState.turn || (isMaximizingPlayer ? context.aiColor : opponent(context.aiColor));
    if (isMaximizingPlayer) {
      if (standPat >= beta) return standPat;
      alpha = Math.max(alpha, standPat);
    } else {
      if (standPat <= alpha) return standPat;
      beta = Math.min(beta, standPat);
    }
    // Filter first, order after: the order key (score desc, index asc) keeps the relative order of a subsequence, so this equals order-then-filter but scores far fewer actions.
    const captureActions = orderActions(generateActions(boardState, color).filter((action) => isCaptureAction(boardState, action) || (qDepth === QUIESCENCE_MAX_PLIES && isTacticallyRelevantCardAction(boardState, action, color))), boardState, color);
    if (!captureActions.length) return standPat;
    if (isMaximizingPlayer) {
      let value2 = standPat;
      for (const action of captureActions) {
        if (isTimedOut(context)) break;
        const next = cloneState(boardState);
        const applied = applyAction(next, action, context.aiColor);
        if (!applied.ok) continue;
        const score = applied.score + quiescence(next, alpha, beta, next.turn === context.aiColor, context, qDepth - 1);
        value2 = Math.max(value2, score);
        alpha = Math.max(alpha, value2);
        if (beta <= alpha) {
          context.cutoffs += 1;
          break;
        }
      }
      return value2;
    }
    let value = standPat;
    for (const action of captureActions) {
      if (isTimedOut(context)) break;
      const next = cloneState(boardState);
      const applied = applyAction(next, action, context.aiColor);
      if (!applied.ok) continue;
      const score = applied.score + quiescence(next, alpha, beta, next.turn === context.aiColor, context, qDepth - 1);
      value = Math.min(value, score);
      beta = Math.min(beta, value);
      if (beta <= alpha) {
        context.cutoffs += 1;
        break;
      }
    }
    return value;
  }
  function recordKillerMove(context, depth, action) {
    if (!context.killers) context.killers = {};
    const list = context.killers[depth] || (context.killers[depth] = []);
    if (list.some((k) => sameAction(k, action))) return;
    list.unshift(action);
    if (list.length > 2) list.length = 2;
  }
  function reorderWithKillers(boardState, actions, killers) {
    if (!killers || !killers.length) return actions;
    const captures = [];
    const killerMoves = [];
    const rest = [];
    const usedKillerIdx = new Set();
    actions.forEach((action) => {
      if (isCaptureAction(boardState, action)) {
        captures.push(action);
        return;
      }
      const ki = killers.findIndex((k, idx) => !usedKillerIdx.has(idx) && sameAction(k, action));
      if (ki !== -1) {
        usedKillerIdx.add(ki);
        killerMoves.push(action);
        return;
      }
      rest.push(action);
    });
    return [...captures, ...killerMoves, ...rest];
  }
  function positionKey(boardState) {
    let key = boardState.turn === "white" ? "w" : "b";
    const rows = boardRowCount(boardState);
    const cols = boardColCount(boardState);
    for (let r = 0; r < rows; r++) {
      const row = boardState.board[r] || [];
      for (let c = 0; c < cols; c++) {
        const p = row[c];
        key += p ? (p.color === "white" ? "W" : p.color === "black" ? "B" : "N") + p.type : ".";
      }
    }
    return key;
  }
  // Grafted from engine.optimized.js (our-only enhancement, replacing the
  // real file's minimax wholesale): adds a transposition table (positionKey
  // + context.tt, grafted earlier as a standalone helper), null-move
  // pruning (nullMoveOk), late-move-reduction gated on
  // isTacticallyRelevantCardAction, the killer-move heuristic
  // (recordKillerMove/reorderWithKillers), path-based repetition detection,
  // and dropping into quiescence search at depth<=0 instead of a flat
  // evaluateState call -- all grafted earlier as standalone (until-now
  // unreachable) functions specifically for this wiring. Every real-file
  // line here (the timeout check, the gameover/depth<=0 base case shape,
  // action generation/ordering, the max/min alpha-beta loops themselves) is
  // preserved, just extended -- confirmed by diffing the two versions
  // line-by-line before this replacement.
  function minimax(boardState, depth, alpha, beta, isMaximizingPlayer, context) {
    context.nodes += 1;
    if ((context.nodes & 255) === 0 && isTimedOut(context)) return (context.evalFn || evaluateState)(boardState, context.aiColor);
    if (boardState.mode === "gameover") return (context.evalFn || evaluateState)(boardState, context.aiColor);
    const posKey = positionKey(boardState);
    if (!context.pathCounts) context.pathCounts = new Map();
    if (context.pathCounts.get(posKey) >= 1) return 0;
    if (depth <= 0) return quiescence(boardState, alpha, beta, isMaximizingPlayer, context, QUIESCENCE_MAX_PLIES);
    const color = boardState.turn || (isMaximizingPlayer ? context.aiColor : opponent(context.aiColor));
    const alphaOrig = alpha;
    const betaOrig = beta;
    const ttKey = depth + "|" + posKey;
    const ttEntry = context.tt.get(ttKey);
    if (ttEntry) {
      if (ttEntry.flag === "exact") return ttEntry.score;
      if (ttEntry.flag === "lower") alpha = Math.max(alpha, ttEntry.score);
      else if (ttEntry.flag === "upper") beta = Math.min(beta, ttEntry.score);
      if (alpha >= beta) return ttEntry.score;
    }
    context.pathCounts.set(posKey, (context.pathCounts.get(posKey) || 0) + 1);
    try {
      if (depth >= NULL_MOVE_MIN_DEPTH && nullMoveOk(boardState, color)) {
        const nullState = cloneState(boardState);
        nullState.turn = opponent(color);
        const nullMaximizing = nullState.turn === context.aiColor;
        const nullValue = minimax(nullState, Math.max(0, depth - 1 - NULL_MOVE_REDUCTION), alpha, beta, nullMaximizing, context);
        if (isMaximizingPlayer && nullValue >= beta) {
          context.cutoffs += 1;
          return beta;
        }
        if (!isMaximizingPlayer && nullValue <= alpha) {
          context.cutoffs += 1;
          return alpha;
        }
      }
      const actions = reorderWithKillers(boardState, orderActions(generateActions(boardState, color), boardState, color), context.killers?.[depth]);
      if (!actions.length) return (context.evalFn || evaluateState)(boardState, context.aiColor) + (color === context.aiColor ? -2500 : 2500);
      if (isMaximizingPlayer) {
        let value2 = -INF;
        let moveIndex = 0;
        for (const action of actions) {
          if (isTimedOut(context)) break;
          const next = cloneState(boardState);
          const beforeTurn = next.turn;
          const beforeActionsRemaining = next.actionsRemaining;
          const capture = isCaptureAction(boardState, action);
          const applied = applyAction(next, action, context.aiColor);
          if (!applied.ok) continue;
          const nextDepth = childDepthAfterAction(depth, action, beforeTurn, beforeActionsRemaining, next);
          const nextMaximizing = next.turn === context.aiColor;
          let score;
          if (nextDepth >= LMR_MIN_DEPTH && moveIndex >= LMR_MOVE_THRESHOLD && !capture && !isTacticallyRelevantCardAction(boardState, action, color)) {
            score = applied.score + minimax(next, nextDepth - 1, alpha, beta, nextMaximizing, context);
            if (score > alpha) score = applied.score + minimax(next, nextDepth, alpha, beta, nextMaximizing, context);
          } else {
            score = applied.score + minimax(next, nextDepth, alpha, beta, nextMaximizing, context);
          }
          value2 = Math.max(value2, score);
          alpha = Math.max(alpha, value2);
          moveIndex += 1;
          if (beta <= alpha) {
            context.cutoffs += 1;
            if (!capture) recordKillerMove(context, depth, action);
            break;
          }
        }
        if (!context.timedOut) {
          const flag = value2 <= alphaOrig ? "upper" : value2 >= betaOrig ? "lower" : "exact";
          context.tt.set(ttKey, { score: value2, flag });
        }
        return value2;
      }
      let value = INF;
      let moveIndex = 0;
      for (const action of actions) {
        if (isTimedOut(context)) break;
        const next = cloneState(boardState);
        const beforeTurn = next.turn;
        const beforeActionsRemaining = next.actionsRemaining;
        const capture = isCaptureAction(boardState, action);
        const applied = applyAction(next, action, context.aiColor);
        if (!applied.ok) continue;
        const nextDepth = childDepthAfterAction(depth, action, beforeTurn, beforeActionsRemaining, next);
        const nextMaximizing = next.turn === context.aiColor;
        let score;
        if (nextDepth >= LMR_MIN_DEPTH && moveIndex >= LMR_MOVE_THRESHOLD && !capture && !isTacticallyRelevantCardAction(boardState, action, color)) {
          score = applied.score + minimax(next, nextDepth - 1, alpha, beta, nextMaximizing, context);
          if (score < beta) score = applied.score + minimax(next, nextDepth, alpha, beta, nextMaximizing, context);
        } else {
          score = applied.score + minimax(next, nextDepth, alpha, beta, nextMaximizing, context);
        }
        value = Math.min(value, score);
        beta = Math.min(beta, value);
        moveIndex += 1;
        if (beta <= alpha) {
          context.cutoffs += 1;
          if (!capture) recordKillerMove(context, depth, action);
          break;
        }
      }
      if (!context.timedOut) {
        const flag = value <= alphaOrig ? "upper" : value >= betaOrig ? "lower" : "exact";
        context.tt.set(ttKey, { score: value, flag });
      }
      return value;
    } finally {
      const remaining = context.pathCounts.get(posKey) - 1;
      if (remaining <= 0) context.pathCounts.delete(posKey);
      else context.pathCounts.set(posKey, remaining);
    }
  }
  function childDepthAfterAction(depth, action, beforeTurn, beforeActionsRemaining, afterState) {
    if (depth <= 0) return 0;
    const consumesAction = action && ["move", "shotgunReload", "wizardSpell"].includes(action.type);
    const beforeRemaining = Math.max(1, Number(beforeActionsRemaining) || 1);
    const afterRemaining = Math.max(1, Number(afterState.actionsRemaining) || 1);
    if (consumesAction && afterState.turn === beforeTurn && afterRemaining < beforeRemaining) return depth;
    return depth - 1;
  }
  function findForcedRoyalCaptureSequence(boardState, orderedRoot, aiColor, context = null) {
    const direct = orderedRoot.find((action) => {
      const probe = rootActionProbe(boardState, action, aiColor, context);
      return probe && actionCapturesRoyal(boardState, action, aiColor, probe.afterState);
    });
    if (direct) return direct;
    for (const action of orderedRoot) {
      if (!action || !["move", "shotgunReload", "wizardSpell"].includes(action.type)) continue;
      const next = cloneState(boardState);
      const applied = applyAction(next, cloneAction(action), aiColor);
      if (!applied.ok || next.mode === "gameover" || next.turn !== aiColor) continue;
      const followups = generateActions(next, aiColor);
      if (followups.some((followup) => actionCapturesRoyal(next, followup, aiColor))) return action;
    }
    return null;
  }
  function actionCapturesRoyal(boardState, action, color, afterState = null) {
    const before = royalPieceKeys(boardState, opponent(color));
    if (!before.size) return false;
    const next = afterState || cloneState(boardState);
    if (!afterState) {
      const applied = applyAction(next, cloneAction(action), color);
      if (!applied.ok) return false;
    }
    if (next.mode === "gameover" && next.winner === color) return true;
    const after = royalPieceKeys(next, opponent(color));
    if (after.size < before.size) return true;
    for (const key of before) {
      if (!after.has(key)) return true;
    }
    return false;
  }
  function royalPieceKeys(boardState, color) {
    const keys = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color === color && isWorkerDecisiveCaptureTarget(boardState, piece)) {
        keys.add(`${piece.id || piece.type}:${row}:${col}:${piece.hp ?? ""}`);
      }
    });
    return keys;
  }
  // Decides, once, whether to keep searching past the soft deadline: only when
  // the depth in progress is mostly done or its best move is still changing
  // (throwing that work away is the expensive mistake), or when minDepth has not
  // been reached yet. Never extends past tm.hardMs.
  function canExtendSearch(context) {
    const tm = context.tm;
    if (!tm || tm.extended || !(tm.hardMs > tm.softMs)) return false;
    if (context.currentDepth < tm.minDepth) return true;
    if (!tm.extendOn) return false;
    const progress = context.rootTotal > 0 ? context.rootDone / context.rootTotal : 0;
    if (progress >= tm.extendMinProgress) return true;
    return context.bestChanged && progress >= 0.3;
  }
  function isTimedOut(context) {
    const tm = context.tm;
    if (tm && tm.maxNodes && context.nodes >= tm.maxNodes) {
      context.timedOut = true;
      return true;
    }
    const now = performance.now();
    if (now <= context.deadline) return false;
    if (tm && canExtendSearch(context)) {
      tm.extended = true;
      context.deadline = context.startedAt + tm.hardMs;
      if (now <= context.deadline) return false;
    }
    context.timedOut = true;
    return true;
  }
  function orderActions(actions, boardState, perspectiveColor) {
    return actions.map((action, index) => ({ action, index, score: actionOrderingScore(action, boardState, perspectiveColor) })).sort((a, b) => b.score - a.score || a.index - b.index).map((entry) => entry.action);
  }
  function actionOrderingScore(action, boardState, perspectiveColor) {
    const color = action.color || boardState.turn || perspectiveColor;
    if (action.type === "fileSurgeSkip") return 75;
    if (action.type === "shotgunReload") return 520;
    if (action.type === "wizardSpell") {
      return wizardActionScore(boardState, action, color);
    }
    if (action.type === "card") {
      const card = findCard(boardState, action);
      const target2 = action.target ? get(boardState, action.target.row, action.target.col) : null;
      let score2 = 80 + cardValue(card) * 0.22;
      if (card?.effect === "bloodCard") {
        return score2 + 160 + bloodEffectScore(boardState, action.target?.bloodEffectId || card.bloodEffectId || "summon", color);
      }
      if (card?.effect === "witchTrial") {
        return score2 + workerWitchTrialTargetScore(boardState, target2, action.target?.row, action.target?.col, color);
      }
      if (card?.effect === "suicideBomber") {
        return score2 + workerSuicideBomberTargetScore(boardState, target2, action.target?.row, action.target?.col, color);
      }
      if (!card?.target) score2 += 120;
      if (target2?.color === opponent(color)) score2 += 250 + pieceValue(target2) * 0.9;
      if (target2?.color === color) score2 += 60 + pieceValue(target2) * 0.12;
      return score2;
    }
    const moving = get(boardState, action.from?.row, action.from?.col);
    const destination = workerActionDestination(action) || action.move || {};
    const target = get(boardState, destination.row, destination.col);
    let score = moving ? pieceValue(moving) * 0.05 : 0;
    if (action.move?.madHorseCapture && target?.color === color) {
      score -= 700 + pieceValue(target) * 2;
    }
    score += workerUltimatumMoveReliefScore(boardState, moving) * 1.35;
    const hangingPenalty = workerHangingPenalty(boardState, moving, action.from?.row, action.from?.col);
    if (hangingPenalty > 0) score += hangingPenalty * 0.55;
    if (!action.move?.substitutionSwap && !action.move?.relaySwap && !moving?.repositionSecondMove && canWorkerCaptureTarget(color, target, moving, boardState, {
      allowBasicTrainingCapture: Boolean(action.move?.basicTrainingCapture)
    })) {
      score += 500 + workerStrategicPieceValue(boardState, target) * 1.8 - workerStrategicPieceValue(boardState, moving) * 0.25;
      if (isWorkerDecisiveCaptureTarget(boardState, target)) score += 5e4;
    } else if (moving && !target && isSquareAttacked(boardState, destination.row, destination.col, opponent(color))) {
      score -= Math.min(620, pieceValue(moving) * 0.42);
    }
    score -= workerLowValueCaptureOrderingPenalty(boardState, action, color);
    score += workerCrownMoveOrderingScore(boardState, action);
    score += workerExplosiveCaptureOrderingScore(boardState, action, color, perspectiveColor);
    if (action.move?.capturedRow !== void 0) score += 320;
    if (moving && isWorkerDecisiveCaptureTarget(boardState, moving)) {
      if (isSquareAttacked(boardState, action.from.row, action.from.col, opponent(color))) score += 900;
      if (isSquareAttacked(boardState, destination.row, destination.col, opponent(color))) score -= 1200;
    }
    score -= oscillationPenalty(boardState, action, color) * 1.3;
    if (["pawn", "squire", "standardBearer"].includes(moving?.type) && isPromotionRow(moving, destination.row, boardState)) score += 700;
    return score;
  }
  function generateActions(boardState, color) {
    if (boardState.mode === "gameover") return [];
    const actions = [];
    const forcedExtraMove = activeWorkerForcedExtraMove(boardState, color);
    const forcedEnPassant = workerHasForcedEnPassant(boardState, color);
    if (!(forcedExtraMove?.piece?.thiefSecondMove && usesThiefRemake(boardState)) && (forcedExtraMove?.piece?.thiefSecondMove || forcedExtraMove?.piece?.fileSurgeSecondMove || forcedExtraMove?.piece?.rookLiftSecondMove || forcedExtraMove?.piece?.ironMonarchExtraMove || forcedExtraMove?.piece?.madHorseSecondMove)) {
      actions.push({
        type: "fileSurgeSkip",
        color,
        from: { row: forcedExtraMove.row, col: forcedExtraMove.col }
      });
    }
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color && piece.type !== "football" || piece.type === "wall") return;
      if (forcedExtraMove && piece !== forcedExtraMove.piece) return;
      if (isFrozenPiece(piece)) return;
      if (isWorkerStakedPiece(piece)) return;
      if (isDiceLockedPiece(boardState, piece)) return;
      const fieldPromotion = isFieldPromotionRecyclingReady(piece, boardState.fieldPromotion, boardState.recycling);
      if (!forcedExtraMove && !forcedEnPassant && boardState.recycling && (isPromotionRow(piece, row, boardState) || fieldPromotion) && workerBestRecyclingPromotionType(boardState, piece.color)) {
        actions.push({ type: "promotion", color, from: { row, col } });
      }
      generateMovesForPiece(boardState, piece, row, col).filter((move) => isWorkerMoveAllowed(boardState, piece, row, col, move)).forEach((move) => {
        if (!isUsefulSpecialMove(boardState, move, piece, row, col, color)) return;
        actions.push({ type: "move", color, from: { row, col }, move });
      });
      if (!forcedExtraMove && !forcedEnPassant) generateSpecialPieceActions(boardState, piece, row, col, color).forEach((action) => actions.push(action));
    });
    if (!boardState.aiSearchNoCards && !boardState.draftDelete && !forcedExtraMove && !forcedEnPassant) {
      deck(boardState, color).forEach((card) => {
        if (!card || card.emptySlot || card.used || card.recovering || isWorkerCardPendingNextTurn(card)) return;
        if (isWorkerTurnExclusiveCardBlocked(boardState, card, color)) return;
        generateCardTargets(boardState, card, color).forEach((target) => {
          actions.push({
            type: "card",
            color,
            cardId: card.id,
            cardInstanceId: card.instanceId,
            target
          });
        });
      });
    }
    return actions;
  }
  function workerMissionaryMoves(boardState, row, col, piece) {
    return diagonals().flatMap(([dr, dc]) => {
      const targetRow2 = row + dr;
      const targetCol2 = col + dc;
      if (!inBounds(targetRow2, targetCol2, boardState) || isWorkerCollapsedSquare(boardState, targetRow2, targetCol2)) return [];
      const target = get(boardState, targetRow2, targetCol2);
      if (!target) return [{ row: targetRow2, col: targetCol2 }];
      if (target.color !== piece.color && COLORS.includes(target.color) && !["wall", "football", "monster", "blackHole"].includes(target.type) && !isDesperadoRoyalCaptureBlocked(piece, target)) {
        return [{ row: targetRow2, col: targetCol2, missionaryConvert: true }];
      }
      return [];
    });
  }
  function isWorkerPendingPortalReservedSquare(boardState, row, col) {
    return normalizePendingPortals(boardState.pendingPortals, boardRowCount(boardState), boardColCount(boardState)).some((entry) => entry.cells.some((cell) => cell.row === row && cell.col === col));
  }
  function isWorkerPortalMovementReservedSquare(boardState, row, col) {
    return normalizePendingPortals(boardState.pendingPortals, boardRowCount(boardState), boardColCount(boardState)).some((entry) => entry.blocksMovement === true && entry.cells.some((cell) => cell.row === row && cell.col === col));
  }
  function isWorkerPendingSpawnReservedSquare(boardState, row, col) {
    return [...boardState.pendingScarecrows || [], ...boardState.pendingLobsters || []].some((entry) => !entry?.pieceId && entry?.row === row && entry?.col === col);
  }
  function workerLoyalistMoves(boardState, piece) {
    if (!piece?.loyalist || !COLORS.includes(piece.color)) return [];
    const anchors = piecesMatching(boardState, (candidate) => candidate.color === piece.color && (isWorkerKingRole(boardState, candidate) || candidate.type === "merchant"));
    const seen = /* @__PURE__ */ new Set();
    return anchors.flatMap((anchor) => queenDirections().map(([dr, dc]) => ({ row: anchor.row + dr, col: anchor.col + dc, loyalistMove: true }))).filter(({ row, col }) => {
      const key = `${row}:${col}`;
      if (!workerOpenPlacementSquare(boardState, row, col) || isWorkerCollapsedSquare(boardState, row, col) || isBlackHoleCell(boardState, row, col) || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function workerDarkMagicCircleCenter(piece, fallbackRow, fallbackCol) {
    const stored = piece?.darkMagicCircle;
    return {
      row: Number.isInteger(stored?.centerRow) ? stored.centerRow : fallbackRow,
      col: Number.isInteger(stored?.centerCol) ? stored.centerCol : fallbackCol
    };
  }
  function workerDarkMagicCircleContains(boardState, piece, fallbackRow, fallbackCol, row, col) {
    const center = workerDarkMagicCircleCenter(piece, fallbackRow, fallbackCol);
    return inBounds(row, col, boardState) && Math.abs(row - center.row) <= 1 && Math.abs(col - center.col) <= 1;
  }
  function workerDarkWizardMoveStaysInCircle(boardState, piece, row, col, move) {
    const cells = [workerPortalMoveDestination(move)];
    if (move?.portalEntry) cells.push(move.portalEntry);
    if (move?.portalExit) cells.push(move.portalExit);
    const destinations = uniqueCells(cells.filter((cell) => Number.isInteger(cell?.row) && Number.isInteger(cell?.col)));
    return destinations.length > 0 && destinations.every((cell) => workerDarkMagicCircleContains(boardState, piece, row, col, cell.row, cell.col));
  }
  function workerUniqueAlliedPieceCount(boardState, color) {
    // Perf (2026-09-16): read-only counter (never mutates the board via
    // set() while iterating), so it's safe to use the cached piece-list
    // scan instead of a fresh uncached one -- called from 3 move-generation
    // hot paths (berserker tier lookups) per candidate move.
    const seen = /* @__PURE__ */ new Set();
    forEachPieceCached(boardState, (item, row, col) => {
      if (item?.color === color) seen.add(item.id || `${row}:${col}`);
    });
    return seen.size;
  }
  function workerSiegeRamMoves(boardState, piece, row, col) {
    const moves = [];
    const portalRule = workerPortalRule(boardState);
    orthogonals().forEach(([dr, dc]) => {
      let cursor = { row, col };
      let portalTransit = null;
      const cells = [];
      for (let distance = 1; distance <= 2; distance += 1) {
        cursor = { row: cursor.row + dr, col: cursor.col + dc };
        if (!inBounds(cursor.row, cursor.col, boardState)) break;
        cells.push({ ...cursor });
        const exit = !portalTransit && portalRule ? workerPortalExitAt(boardState, cursor.row, cursor.col) : null;
        if (exit) {
          if (isWorkerCollapsedSquare(boardState, cursor.row, cursor.col) || isWorkerCollapsedSquare(boardState, exit.row, exit.col)) break;
          portalTransit = { entry: { ...cursor }, exit: { ...exit } };
          cells.push({ ...exit });
          moves.push({
            row: cursor.row,
            col: cursor.col,
            siegeRamMove: true,
            portalLanding: true,
            portalEntry: { ...cursor },
            portalExit: { ...exit },
            highlightCells: uniqueCells(cells)
          });
          cursor = { ...exit };
          continue;
        }
        moves.push({
          row: cursor.row,
          col: cursor.col,
          siegeRamMove: true,
          ...portalTransit ? {
            portalThrough: true,
            portalEntry: portalTransit.entry,
            portalExit: portalTransit.exit
          } : {},
          highlightCells: uniqueCells(cells)
        });
      }
    });
    return moves;
  }
  function workerSlimeMoves(boardState, piece, row, col) {
    return orthogonals().flatMap(([dr, dc]) => {
      const targetRow2 = row + dr * 3;
      const targetCol2 = col + dc * 3;
      if (!inBounds(targetRow2, targetCol2, boardState)) return [];
      const target = get(boardState, targetRow2, targetCol2);
      if (target && !canWorkerCaptureTarget(piece.color, target, piece, boardState)) return [];
      return [{ row: targetRow2, col: targetCol2, slimeMove: true }];
    });
  }
  function workerSirenMoves(boardState, piece, row, col) {
    return leapMoves(boardState, row, col, piece.color, kingDeltas()).map((move) => ({ ...move, sirenMove: true }));
  }
  function isWorkerSiegeRamForcedRemovalTarget(target) {
    return Boolean(target && (target.type === "wall" || !COLORS.includes(target.color)));
  }
  function canWorkerSiegeRamAffectTarget(boardState, attacker, target, row, col, move = {}) {
    if (!attacker || !target || target === attacker) return true;
    if (isWorkerSiegeRamForcedRemovalTarget(target)) return true;
    return true;
  }
  function workerSiegeRamPotentialCaptureCount(entries, boardState = null) {
    const hits = /* @__PURE__ */ new Map();
    (entries || []).forEach(({ item }) => {
      if (!item) return;
      const key = item.id || item;
      const entry = hits.get(key) || { item, count: 0 };
      entry.count += 1;
      hits.set(key, entry);
    });
    let captures = 0;
    hits.forEach(({ item, count }) => {
      if (!legacyAttackRulesStates.has(boardState) || !isHpPiece(item) || Number(item.hp ?? item.maxHp ?? 1) <= count) captures += 1;
    });
    return captures;
  }
  function workerBerserkerMoves(boardState, piece, row, col) {
    const tier = berserkerMovementTier(workerUniqueAlliedPieceCount(boardState, piece.color));
    const kingStep = leapMoves(boardState, row, col, piece.color, queenDirections());
    if (tier === "king-step") return kingStep;
    const rook = rayMoves(boardState, row, col, piece.color, orthogonals(), 8);
    if (tier === "rook-king-step") return uniqueMoves([...kingStep, ...rook]);
    return uniqueMoves([
      ...rayMoves(boardState, row, col, piece.color, queenDirections(), 8),
      ...leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color))
    ]);
  }
  function workerRelayMoves(boardState, piece, row, col) {
    if (!boardState.relay?.[piece?.color] || isSlimeSpecialMovementLocked(piece) || workerIsLargePiece(piece) || ["wall", "football", "blackHole", "monster", "coffin"].includes(piece?.type)) return [];
    const moves = [];
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (target, targetRow2, targetCol2) => {
      if (!target || target === piece || target.color !== piece.color || isSlimeSpecialMovementLocked(target) || workerIsLargePiece(target)) return;
      if (["wall", "football", "blackHole", "monster", "coffin"].includes(target.type)) return;
      if (targetRow2 !== row && targetCol2 !== col) return;
      const key = `${targetRow2}:${targetCol2}`;
      if (seen.has(key)) return;
      seen.add(key);
      moves.push({ row: targetRow2, col: targetCol2, relaySwap: true });
    });
    return moves;
  }
  function generateMovesForPiece(boardState, piece, row, col, options = {}) {
    if (piece?.type === "scarecrow") return [];
    if (boardState.democracy?.[piece.color] && boardState.zugzwang?.[piece.color] && !findWorkerKingRole(boardState, piece.color)) boardState.zugzwang[piece.color] = false;
    if (workerIdolEncoreRepeatBlocked(boardState, piece)) return [];
    if (isWorkerUndergroundBunkerKing(piece) || isPoisonStunned(piece) || isExhaustionMoveBlocked(boardState.exhaustion, piece)) return [];
    if ((septemberCounterLimit(piece.type) > 0 || pieceHasAbility(piece, "hedgehog")) && (Number(boardState.turnsTaken?.[piece.color]) || 0) < (Number(piece.bearMoveLockedUntilTurn) || 0)) return [];
    if (piece.type === "clockwork" && !clockworkHasNeighbor(piece, row, col, (r, c) => get(boardState, r, c))) return [];
    if (piece.type === "babyBear") return [];
    const type = renderType(piece);
    if (boardState.zugzwang?.[piece.color] && !isWorkerZugzwangTargetKing(boardState, piece)) return [];
    let moves = [];
    let handled = true;
    if (type === "football") moves = footballMoves(boardState, row, col, boardState.turn || piece.color);
    else if (type === "fanatic") moves = fanaticMoves(boardState, row, col, piece.color);
    else if (["pawn", "squire", "standardBearer"].includes(type)) moves = pawnMoves(boardState, row, col, piece.color, piece);
    else if (isWorkerCheckerType(type)) moves = checkerMoves(boardState, row, col, piece.color, type);
    else if (type === "assassin") moves = assassinMoves(boardState, row, col, piece.color);
    else if (type === "pegasus") moves = pegasusMoves(boardState, row, col, piece.color);
    else if (["knight", "unicorn"].includes(type)) {
      moves = knightLikeMoves(boardState, row, col, piece);
      if (type === "knight" && boardState.cornerKick?.[piece.color] && isBoardCorner(row, col, boardRowCount(boardState), boardColCount(boardState))) {
        moves = uniqueMoves([...moves, ...rayMoves(boardState, row, col, piece.color, diagonals())]);
      }
    } else if (type === "royalKnight") moves = uniqueMoves([
      ...leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color)),
      ...boardState.royalKnightKing?.[piece.color] ? leapMoves(boardState, row, col, piece.color, queenDirections()) : [],
      ...boardState.hillKing?.[piece.color] && isCenterTwoByTwoCell(row, col, boardRowCount(boardState), boardColCount(boardState)) ? rayMoves(boardState, row, col, piece.color, queenDirections(), 8) : []
    ]);
    else if (["man", "guard", "reaper"].includes(type)) moves = leapMoves(boardState, row, col, piece.color, queenDirections());
    else if (type === "recruiter") moves = rayMoves(boardState, row, col, piece.color, queenDirections(), 1);
    else if (type === "knightmaster") moves = leapMoves(boardState, row, col, piece.color, [[1, 1], [1, -1], [-1, 1], [-1, -1]]);
    else if (type === "camel") moves = leapMoves(boardState, row, col, piece.color, [[3, 1], [3, -1], [-3, 1], [-3, -1], [1, 3], [1, -3], [-1, 3], [-1, -3]]);
    else if (type === "eagle") moves = leapMoves(boardState, row, col, piece.color, eagleDeltas());
    else if (type === "alibaba") moves = leapMoves(boardState, row, col, piece.color, eagleDeltas());
    else if (type === "alfil") moves = leapMoves(boardState, row, col, piece.color, [[2, 2], [2, -2], [-2, 2], [-2, -2]]);
    else if (type === "ferz") moves = leapMoves(boardState, row, col, piece.color, diagonals());
    else if (type === "vampireLord") moves = vampireLordMoves(boardState, row, col, piece.color);
    else if (type === "bat") moves = batMoves(boardState, row, col, piece.color);
    else if (type === "coffin") moves = [];
    else if (type === "log") moves = logMoves(boardState, row, col);
    else if (type === "missionary") moves = workerMissionaryMoves(boardState, row, col, piece);
    else if (type === "bishop") moves = boardState.bishopSnipe?.[piece.color] ? uniqueMoves([...rayMoves(boardState, row, col, piece.color, boardState.reversal?.[piece.color] ? orthogonals() : diagonals()), ...bishopSnipeMoves(boardState, row, col, piece.color)]) : rayMoves(boardState, row, col, piece.color, boardState.reversal?.[piece.color] ? orthogonals() : diagonals());
    else if (type === "protestant") moves = protestantMoves(boardState, row, col, piece.color);
    else if (type === "cardinal") moves = cardinalMoves(boardState, row, col, piece.color);
    else if (type === "rook") moves = rayMoves(boardState, row, col, piece.color, boardState.reversal?.[piece.color] ? diagonals() : orthogonals(), 8);
    else if (type === "cannon") moves = cannonMoves(boardState, row, col, piece.color);
    else if (type === "timeTraveler") moves = timeTravelerMoves(boardState, row, col, piece);
    else if (type === "herald") moves = heraldMoves(boardState, row, col, piece);
    else if (type === "dragon") moves = dragonMoves(boardState, row, col, piece);
    else if (type === "wizard") moves = wizardPieceMoves(boardState, row, col, piece.color);
    else if (type === "paladin") moves = leapMoves(boardState, row, col, piece.color, INTERNAL_KNIGHT_DELTAS);
    else if (type === "octopus") moves = leapMoves(boardState, row, col, piece.color, queenDirections());
    else if (type === "brutus") moves = hookMoves(boardState, row, col, piece.color);
    else if (type === "clockwork") moves = rayMoves(boardState, row, col, piece.color, queenDirections());
    else if (type === "parrot") moves = parrotBaseMoves({ state: boardState, memory: boardState.parrotMovement?.[piece.color], row, col, color: piece.color, moved: piece.moved, rows: boardRowCount(boardState), cols: boardColCount(boardState), at: (r, c) => get(boardState, r, c), canCapture: (target) => target.color !== piece.color && canWorkerCaptureTarget(piece.color, target, piece, boardState) });
    else if (type === "thief") moves = leapMoves(boardState, row, col, piece.color, thiefOffsets()).filter((to) => thiefBaseMoveAllowed({ row, col }, to, (r, c) => boardState.board[r]?.[c], boardState)).map((to) => ({ ...to, ...thiefMoveFlags({ row, col }, to, (r, c) => boardState.board[r]?.[c], boardState) }));
    else if (type === "princess") moves = septemberPrincessHasQueenMovementCached(boardState.board, piece.color) ? rayMoves(boardState, row, col, piece.color, queenDirections()) : leapMoves(boardState, row, col, piece.color, diagonals());
    else if (type === "campfire") moves = wizardPieceMoves(boardState, row, col, piece.color).filter((move) => move.row === row || move.col === col);
    else if (type === "hedgehog") moves = leapMoves(boardState, row, col, piece.color, queenDirections());
    else if (type === "darkWizard") moves = rayMoves(
      boardState,
      row,
      col,
      piece.color,
      piece.darkMagicCircle ? orthogonals() : queenDirections(),
      1
    );
    else if (type === "idol") moves = rayMoves(boardState, row, col, piece.color, queenDirections(), 8).filter((move) => inBounds(move.row, move.col, boardState) && !workerMovementOccupant(boardState, row, col, move.row, move.col, piece.color));
    else if (type === "lobster") {
      const dir = pawnDir(boardState, piece.color);
      moves = leapMoves(boardState, row, col, piece.color, [[dir, -1], [dir, 0], [dir, 1]]);
    } else if (type === "siegeRam") moves = workerSiegeRamMoves(boardState, piece, row, col);
    else if (type === "magicGirl") moves = boardState.magicGirlSurge?.[piece.color] ? uniqueMoves([
      ...rayMoves(boardState, row, col, piece.color, queenDirections(), 8),
      ...leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color))
    ]) : leapMoves(boardState, row, col, piece.color, queenDirections());
    else if (type === "berserker") moves = workerBerserkerMoves(boardState, piece, row, col);
    else if (type === "slime") moves = workerSlimeMoves(boardState, piece, row, col);
    else if (type === "siren") moves = workerSirenMoves(boardState, piece, row, col);
    else if (type === "undead") moves = leapMoves(boardState, row, col, piece.color, queenDirections());
    else if (type === "trickster") {
      const movementType = TRICKSTER_MOVEMENT_TYPES.includes(piece.tricksterMoveType) && piece.tricksterMoveType !== "trickster" ? piece.tricksterMoveType : TRICKSTER_MOVEMENT_TYPES[0] || "queen";
      moves = generateMovesForPiece(boardState, { ...piece, type: movementType }, row, col, { ...options, tricksterProjection: true }).map((move) => ({ ...move, tricksterMove: true }));
    } else if (type === "primeMinister") moves = primeMinisterMoves(boardState, row, col, piece.color);
    else if (type === "grasshopper") moves = grasshopperMoves(boardState, row, col, piece.color);
    else if (type === "jester") moves = jesterMoves(boardState, row, col, piece.color);
    else if (type === "hook") moves = hookMoves(boardState, row, col, piece.color);
    else if (type === "queen") {
      moves = isWorkerRegencyRoyalHeir(boardState, piece) ? uniqueMoves([
        ...rayMoves(boardState, row, col, piece.color, queenDirections(), 8),
        ...boardState.kingKnight?.[piece.color] ? leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color)) : [],
        ...workerSwitcherooMoves(boardState, row, col, piece.color)
      ]) : rayMoves(boardState, row, col, piece.color, queenDirections(), 8);
    } else if (type === "bear") moves = rayMoves(boardState, row, col, piece.color, queenDirections(), 8);
    else if (type === "babyBear") moves = [];
    else handled = false;
    if (type === "amazon") {
      handled = true;
      moves = [
        ...rayMoves(boardState, row, col, piece.color, queenDirections(), 8),
        ...leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color))
      ];
    } else if (["vip", "crown"].includes(type)) {
      handled = true;
      moves = rayMoves(boardState, row, col, piece.color, queenDirections(), 1);
    } else if (type === "king") {
      handled = true;
      moves = uniqueMoves([
        ...rayMoves(boardState, row, col, piece.color, queenDirections(), 1),
        ...boardState.hillKing?.[piece.color] && isCenterTwoByTwoCell(row, col, boardRowCount(boardState), boardColCount(boardState)) ? rayMoves(boardState, row, col, piece.color, queenDirections(), 8) : [],
        ...boardState.kingKnight?.[piece.color] ? leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color)) : [],
        ...workerSwitcherooMoves(boardState, row, col, piece.color)
      ]);
    } else if (type === "merchant") {
      handled = true;
      moves = merchantMoves(boardState, row, col, piece);
    } else if (type === "shotgunKing") {
      handled = true;
      moves = shotgunKingMoves(boardState, row, col, piece);
    } else if (type === "colossus") {
      handled = true;
      moves = colossusMoves(boardState, row, col, piece.color);
    } else if (["bigRook", "bigBishop"].includes(type)) {
      handled = true;
      moves = bigRookMoves(boardState, row, col, piece.color);
    }
    if (!handled) moves = rayMoves(boardState, row, col, piece.color, queenDirections(), 1);
    const slimeSpecialMovementLocked = isSlimeSpecialMovementLocked(piece);
    if (!slimeSpecialMovementLocked && !options.skipImperialStudies && isWorkerKingRole(boardState, piece)) {
      moves = uniqueMoves([...moves, ...workerImperialStudyMoves(boardState, row, col, piece)]);
    }
    if (!slimeSpecialMovementLocked) moves = uniqueMoves([...moves, ...workerKillerKingMoves(boardState, row, col, piece)]);
    if (!slimeSpecialMovementLocked && boardState.socialism?.[piece.color] > 0 && (type === "merchant" && !usesSeptember18Balance(boardState) || !isWorkerRoyalIdentityPiece(boardState, piece)) && type !== "crown") {
      moves = type === "colossus" ? socialismColossusMoves(boardState, row, col, piece.color) : ["bigRook", "bigBishop"].includes(type) ? socialismBigRookMoves(boardState, row, col, piece.color) : isWorkerNativePawnMover(piece) ? pawnMoves(boardState, row, col, piece.color) : socialistPawnMoves(boardState, row, col, piece.color);
      if (type === "merchant") moves = uniqueMoves([...merchantMoves(boardState, row, col, piece), ...moves]);
    }
    if (!slimeSpecialMovementLocked && hasWorkerBasicTrainingMove(piece)) moves = uniqueMoves([...moves, ...workerBasicTrainingPawnMoves(boardState, row, col, piece)]);
    if (!slimeSpecialMovementLocked && (piece.crownBearer || !workerSocialismSuppressesRoyalCommand(boardState, piece) && (piece.royalCommand || piece.color && !["wall", "football", "blackHole", "colossus", "coffin"].includes(piece.type) && isWorkerRoyalCommandTurnActive(boardState, piece.color)))) {
      const kingMoves = workerIsLargePiece(piece) ? workerCrownLargePieceMoves(boardState, row, col, piece) : rayMoves(boardState, row, col, piece.color, queenDirections(), 1);
      moves = uniqueMoves([...moves, ...kingMoves]);
    }
    if (!slimeSpecialMovementLocked && canUseWorkerHighwayMove(boardState, piece, row, col)) {
      moves = uniqueMoves([...moves, ...workerHighwayMoves(boardState, row, col, piece)]);
    }
    if (!slimeSpecialMovementLocked && isPromotionRushActive(piece, boardState.turnsTaken?.[piece.color] || 0)) {
      moves = uniqueMoves([...moves, ...workerPromotionRushMoves(boardState, piece, row, col)]);
    }
    if (boardState.locustSwarm?.[piece.color] && locustReady(piece, row, col, boardState)) moves = uniqueMoves([...moves, ...grasshopperMoves(boardState, row, col, piece.color)]);
    const availableSubstitutions = slimeSpecialMovementLocked ? [] : workerSubstitutionMoves(boardState, piece);
    if (availableSubstitutions.length) {
      moves = uniqueMoves([...availableSubstitutions, ...moves]);
    }
    if (!slimeSpecialMovementLocked) moves = uniqueMoves([...workerSymmetryMoves(boardState, piece, row, col), ...moves]);
    const availableRelays = slimeSpecialMovementLocked ? [] : workerRelayMoves(boardState, piece, row, col);
    if (availableRelays.length) moves = uniqueMoves([...availableRelays, ...moves]);
    if (!slimeSpecialMovementLocked && isWorkerRegencyRoyalHeir(boardState, piece)) {
      moves = uniqueMoves([
        ...moves,
        ...workerSwitcherooMoves(boardState, row, col, piece.color)
      ]);
    }
    if (!slimeSpecialMovementLocked && piece.loyalist) moves = uniqueMoves([...moves, ...workerLoyalistMoves(boardState, piece)]);
    moves = applyWorkerPortalMoves(boardState, piece, moves).flatMap((move) => {
      const destination = workerPortalMoveDestination(move);
      if (!destination || !isWorkerCrownGroundSquare(boardState, destination.row, destination.col)) return [move];
      if (!workerPawnCanCollectGroundCrown(boardState, piece, row, col, destination)) return [];
      return [{ ...move, crownGroundCapture: true }];
    });
    if (type === "darkWizard" && piece.darkMagicCircle) {
      moves = moves.filter((move) => workerDarkWizardMoveStaysInCircle(boardState, piece, row, col, move));
    }
    if (piece.rookLiftSecondMove && piece.rookLiftChain) {
      const blockedCornerRow = piece.rookLiftChain.blockedCornerRow;
      const blockedCornerCol = piece.rookLiftChain.blockedCornerCol;
      if (Number.isInteger(blockedCornerRow) && Number.isInteger(blockedCornerCol)) {
        moves = moves.filter((move) => {
          const landing = move.portalLanding && move.portalExit ? move.portalExit : move;
          return landing.row !== blockedCornerRow || landing.col !== blockedCornerCol;
        });
      }
    }
    if (workerSeveranceRemaining(boardState, piece) > 0) {
      moves = moves.filter((move) => !Number.isInteger(move.row) || !Number.isInteger(move.col) || oneStepRestrictedMoveDistance(pieceHasAbility(piece, "parrot") ? boardState.parrotMovement?.[piece.color]?.type : pieceAbilityType(piece), row, col, move) <= 1);
    }
    if (piece.inertia) {
      moves = moves.filter((move) => !workerIsIceSheetMovementMove(move) || !isInertiaOneStepMove(piece.type, row, col, move));
    }
    if (workerIceSheetRemaining(piece) > 0) {
      moves = applyWorkerIceSheetRestrictions(boardState, piece, row, col, moves);
    }
    moves = moves.filter((move) => !isWorkerFianchettoBlockedMove(boardState, piece, row, col, move));
    moves = filterWorkerQuantumCounterpartMoves(boardState, piece, moves);
    moves = applyWorkerCheckerCaptureRules(boardState, piece, row, col, moves, options);
    if (boardState.zugzwang?.[piece.color] && isWorkerZugzwangTargetKing(boardState, piece)) {
      moves = moves.filter(isWorkerZugzwangKingMove);
    }
    moves = moves.filter((move) => !crossesReservedScarecrow({ row, col }, move, [...boardState.pendingScarecrows || [], ...usesSeptember18Balance(boardState) ? piecesMatching(boardState, (p) => p.type === "scarecrow").map(({ row: row2, col: col2 }) => ({ row: row2, col: col2, solid: true })) : []]));
    moves = moves.filter((move) => !workerMoveLandingCellsForQuantum(move).some((cell) => isWorkerPortalMovementReservedSquare(boardState, cell.row, cell.col) || isWorkerPendingSpawnReservedSquare(boardState, cell.row, cell.col)));
    moves = moves.filter((move) => thiefMoveAllowed(piece, move, boardState, { row, col }));
    moves = moves.filter((move) => threeMoveAllowed(boardState.board, piece, row, col, move, { allSquaresRadiance: usesSeptember18Balance(boardState), enemyOnlyRadiance: usesEnemyOnlyRadiance(boardState), movementType: pieceHasAbility(piece, "parrot") ? boardState.parrotMovement?.[piece.color]?.type : pieceAbilityType(piece) }));
    moves = applyWorkerScarecrowCaptureRules(boardState, piece, row, col, moves, options);
    return moves.flatMap((move) => {
      const destination = workerPortalMoveDestination(move);
      if (!destination || !isWorkerCrownGroundSquare(boardState, destination.row, destination.col)) return [move];
      if (!workerPawnCanCollectGroundCrown(boardState, piece, row, col, destination)) return [];
      return [{ ...move, crownGroundCapture: true }];
    });
  }
  function workerMoveCapturesScarecrow(boardState, piece, move) {
    if (!piece?.color || piece.type === "log") return false;
    return workerMoveCaptureCells(move).some(({ row, col }) => {
      const target = get(boardState, row, col);
      return target?.type === "scarecrow" && target.color !== piece.color && canWorkerCaptureTarget(piece.color, target, piece, boardState, {
        allowBasicTrainingCapture: Boolean(move?.basicTrainingCapture)
      });
    });
  }
  function applyWorkerScarecrowCaptureRules(boardState, piece, row, col, moves, options = {}) {
    if (options.ignoreScarecrowForce || options.ignoreGlobalCaptureForce || !["white", "black"].includes(piece?.color) || !boardState.board.some((line) => line.some((target) => target?.type === "scarecrow" && target.color !== piece.color))) return moves;
    const captures = moves.filter((move) => workerMoveCapturesScarecrow(boardState, piece, move) && isWorkerBaseMoveAllowed(boardState, piece, row, col, move));
    if (captures.length) return captures;
    let forced = false;
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (other, r, c) => {
      if (forced || !other || other === piece || other.color !== piece.color || seen.has(other.id || other)) return;
      seen.add(other.id || other);
      forced = generateMovesForPiece(boardState, other, r, c, { ignoreScarecrowForce: true }).some((move) => workerMoveCapturesScarecrow(boardState, other, move) && isWorkerBaseMoveAllowed(boardState, other, r, c, move));
    });
    return forced ? [] : moves;
  }
  function workerPawnCanCollectGroundCrown(boardState, piece, row, col, destination) {
    if (piece?.type !== "pawn") return true;
    return destination.row === row + pawnDir(boardState, piece.color) && destination.col === col;
  }
  function workerMoveLandingCellsForQuantum(move) {
    if (!move || move.colossusBody || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.setLogDirection) return [];
    if (move.castle) return uniqueCells([{ row: move.row, col: move.col }, move.rookTo].filter((cell) => inBounds(cell?.row, cell?.col)));
    if (Array.isArray(move.highlightCells) && (move.colossusMove || move.bigRookMove)) return move.highlightCells;
    if (move.portalLanding && Number.isInteger(move.portalExit?.row) && Number.isInteger(move.portalExit?.col)) return [{ ...move.portalExit }];
    if (Number.isInteger(move.row) && Number.isInteger(move.col)) return [{ row: move.row, col: move.col }];
    return [];
  }
  function workerMoveLandsOnQuantumCounterpart(boardState, piece, move, counterpart = piece?.quantum) {
    if (!piece || !counterpart) return false;
    const counterpartCells = workerQuantumCellsForItemAt(boardState, piece, counterpart.row, counterpart.col);
    if (!counterpartCells.length) return false;
    const landingCells = workerMoveLandingCellsForQuantum(move);
    return landingCells.some((landing) => counterpartCells.some((cell) => cell.row === landing.row && cell.col === landing.col));
  }
  function filterWorkerQuantumCounterpartMoves(boardState, piece, moves, counterpart = piece?.quantum) {
    if (!counterpart) return moves || [];
    return (moves || []).filter((move) => !workerMoveLandsOnQuantumCounterpart(boardState, piece, move, counterpart));
  }
  function isWorkerNativePawnMover(piece) {
    return ["pawn", "squire", "standardBearer"].includes(piece?.type);
  }
  function hasWorkerBasicTrainingMove(piece) {
    return Boolean(
      piece?.basicTraining && piece?.color && (piece.potionBasicTraining || !BASIC_TRAINING_FORBIDDEN_TYPES.has(piece.type)) && !["wall", "football", "blackHole", "colossus", "bigRook", "bigBishop"].includes(piece.type)
    );
  }
  function isWorkerBasicTrainingPawnCapture(boardState, piece, row, col, targetRow2, targetCol2) {
    if (!hasWorkerBasicTrainingMove(piece)) return false;
    if (pieceHasAbility(piece, "missionary")) return false;
    const dr = targetRow2 - row;
    const dc = targetCol2 - col;
    const dir = pawnDir(boardState, piece.color);
    return (dr === dir || boardState.retreat?.[piece.color] && dr === -dir) && Math.abs(dc) === 1;
  }
  function workerBasicTrainingPawnMoves(boardState, row, col, piece) {
    const moves = [];
    addWorkerBasicTrainingDirectionalMoves(boardState, moves, row, col, piece, pawnDir(boardState, piece.color));
    if (boardState.retreat?.[piece.color]) {
      addWorkerBasicTrainingDirectionalMoves(boardState, moves, row, col, piece, -pawnDir(boardState, piece.color));
    }
    return moves;
  }
  function addWorkerBasicTrainingDirectionalMoves(boardState, moves, row, col, piece, dir) {
    const movementTarget = (targetRow2, targetCol2) => {
      const target = get(boardState, targetRow2, targetCol2);
      return isStealthTransparentFor(piece, target, boardState) ? null : target;
    };
    const one = row + dir;
    if (!inBounds(one, col, boardState)) return;
    if (!movementTarget(one, col)) {
      moves.push({ row: one, col, ...isWorkerCrownGroundSquare(boardState, one, col) ? { crownGroundCapture: true } : {} });
    }
    if (pieceHasAbility(piece, "missionary")) return;
    [-1, 1].forEach((dc) => {
      const targetCol2 = col + dc;
      if (!inBounds(one, targetCol2, boardState)) return;
      const target = movementTarget(one, targetCol2);
      if (isWorkerCrownGroundSquare(boardState, one, targetCol2) && piece?.type !== "pawn") {
        moves.push({ row: one, col: targetCol2, basicTrainingCapture: true, crownGroundCapture: true });
      } else if (canWorkerCaptureTarget(piece.color, target, piece, boardState, { allowBasicTrainingCapture: true })) {
        moves.push({ row: one, col: targetCol2, basicTrainingCapture: true });
      }
    });
  }
  function workerKnightJumpSquare(row, col, dr, dc) {
    if (Math.abs(dr) > Math.abs(dc)) {
      return { row: row + Math.sign(dr), col };
    }
    return { row, col: col + Math.sign(dc) };
  }
  function workerKnightDeltasForMove(boardState, row, col, color) {
    if (!boardState.knightInjury?.[color]) return knightDeltas();
    return knightDeltas().filter(([dr, dc]) => {
      const jump = workerKnightJumpSquare(row, col, dr, dc);
      if (!inBounds(jump.row, jump.col, boardState)) return true;
      const blocker = get(boardState, jump.row, jump.col);
      return !blocker || isGhostTransparentFor(
        get(boardState, row, col),
        blocker,
        "rook",
        boardState
      );
    });
  }
  function knightLikeMoves(boardState, row, col, piece) {
    return workerKnightDeltasForMove(boardState, row, col, piece.color).map(([dr, dc]) => {
      const move = { row: row + dr, col: col + dc };
      const target = get(boardState, move.row, move.col);
      if (isWorkerMadHorseFriendlyTarget(boardState, piece, target)) move.madHorseCapture = true;
      if (piece.brutalKnight || boardState.radicalCharge?.[piece.color]) {
        const jump = workerKnightJumpSquare(row, col, dr, dc);
        const jumped = get(boardState, jump.row, jump.col);
        if (canWorkerRadicalChargeCaptureTarget(boardState, piece.color, jumped, piece)) {
          move.jumpCapture = jump;
          if (isHpPiece(jumped)) {
            move.jumpCaptureHits = Math.max(1, Math.min(2, radicalChargeHpHitCount(boardState.board, jumped, row, col, dr, dc)));
          }
        }
      }
      return move;
    }).filter((move) => {
      if (!inBounds(move.row, move.col)) return false;
      const target = get(boardState, move.row, move.col);
      return !target || move.madHorseCapture || canWorkerCaptureTarget(piece.color, target, piece, boardState);
    });
  }
  function isWorkerMadHorseFriendlyTarget(boardState, knight, target) {
    if (!knight || renderType(knight) !== "knight" || !target || target.color !== knight.color || !boardState.madHorse?.[knight.color]) return false;
    if (isCritical(target) || isWorkerRegencyRoyalHeir(boardState, target) || workerIsLargePiece(target) || isHpPiece(target)) return false;
    if (["merchant", "timeTraveler", "vampireLord", "guard", "wall", "football", "blackHole", "coffin"].includes(target.type)) return false;
    if (target.protected || target.shielded || isFrozenPiece(target) || isWorkerEncouragedTarget(boardState, target)) return false;
    if (isWorkerMannerCaptureLocked(boardState, knight) || isWorkerSaturationCaptureLocked(boardState, knight) || isWorkerInitiativeCaptureLocked(boardState, knight.color) || knight.repositionSecondMove) return false;
    return canTimePhaseInteract(boardState, knight, target);
  }
  function pegasusMoves(boardState, row, col, color) {
    const knightTargets = new Set(workerKnightDeltasForMove(boardState, row, col, color).map(([dr, dc]) => `${row + dr}:${col + dc}`));
    const moves = [];
    for (let targetRow2 = 0; targetRow2 < boardRowCount(boardState); targetRow2 += 1) {
      for (let targetCol2 = 0; targetCol2 < boardColCount(boardState); targetCol2 += 1) {
        if (targetRow2 === row && targetCol2 === col) continue;
        const target = get(boardState, targetRow2, targetCol2);
        if (!target || isStealthTransparentFor(get(boardState, row, col) || { color }, target, boardState)) {
          moves.push({ row: targetRow2, col: targetCol2 });
        } else if (knightTargets.has(`${targetRow2}:${targetCol2}`) && canWorkerCaptureTarget(color, target, get(boardState, row, col) || { type: "pegasus", color }, boardState)) {
          moves.push({ row: targetRow2, col: targetCol2 });
        }
      }
    }
    return moves;
  }
  function workerKillerKingMoves(boardState, row, col, piece) {
    if (!boardState.killerKing?.[piece?.color] || !isWorkerKingRole(boardState, piece) || isSlimeSpecialMovementLocked(piece)) return [];
    return septemberKillerKingTargets({
      from: { row, col },
      color: piece.color,
      at: (r, c) => get(boardState, r, c),
      isRoyal: (target) => isWorkerRoyalIdentityPiece(boardState, target),
      rowCount: boardRowCount(boardState),
      colCount: boardColCount(boardState)
    }).filter((cell) => canWorkerCaptureTarget(piece.color, get(boardState, cell.row, cell.col), piece, boardState));
  }
  function assassinMoves(boardState, row, col, color) {
    const piece = get(boardState, row, col) || { type: "assassin", color };
    const moves = [...knightLikeMoves(boardState, row, col, piece)];
    queenDirections().forEach(([dr, dc]) => {
      let nextRow = row + dr;
      let nextCol = col + dc;
      while (inBounds(nextRow, nextCol, boardState)) {
        const target = get(boardState, nextRow, nextCol);
        if (target) {
          if (target.color !== color && (isRoyalPiece(target) || isWorkerRegencyRoyalHeir(boardState, target)) && canWorkerCaptureTarget(color, target, piece, boardState)) {
            moves.push({ row: nextRow, col: nextCol });
          }
          break;
        }
        nextRow += dr;
        nextCol += dc;
      }
    });
    return uniqueMoves(moves);
  }
  function dragonMoves(boardState, row, col, piece) {
    const moves = knightLikeMoves(boardState, row, col, piece);
    forEachPiece(boardState, (target, targetRow2, targetCol2) => {
      if (!target || target.color !== piece.color || isSlimeSpecialMovementLocked(target) || target.type === "wall" || workerIsLargePiece(target)) return;
      if (targetRow2 === row && targetCol2 === col) return;
      const move = { row: targetRow2, col: targetCol2, dragonSwap: true };
      if (isWorkerMonochromeMoveAllowed(boardState, piece, row, col, move)) moves.push(move);
    });
    return uniqueMoves(moves);
  }
  function isWorkerMoveAllowed(boardState, piece, row, col, move) {
    if (!move) return false;
    if (workerIdolEncoreRepeatBlocked(boardState, piece)) return false;
    const forcedExtraMove = piece?.color ? activeWorkerForcedExtraMove(boardState, piece.color) : null;
    if (forcedExtraMove && forcedExtraMove.piece !== piece) return false;
    const forcedEnPassant = piece?.color ? workerHasForcedEnPassant(boardState, piece.color) : false;
    if (forcedEnPassant && !move.enPassant) return false;
    if (isWorkerCheckerPiece(piece)) {
      const hasCapture = workerLegalCheckerCaptureMoves(boardState, piece, row, col).length > 0;
      if ((piece.checkerChainCapture || hasCapture) && !move.checkerCapture) return false;
    } else if (!forcedEnPassant && piece?.color && workerHasForcedCheckerCapture(boardState, piece.color)) {
      return false;
    }
    return isWorkerBaseMoveAllowed(boardState, piece, row, col, move);
  }
  function isWorkerBaseMoveAllowed(boardState, piece, row, col, move) {
    if (isWorkerSeptemberMajestyBlockedMove(boardState, piece, row, col, move)) return false;
    if (!piece || !move) return false;
    if (piece.type === "football" && isWorkerCrownGroundSquare(boardState, move.row, move.col)) return false;
    if (workerMoveCapturesDesperadoRoyal(boardState, piece, move)) return false;
    if (Array.isArray(move.highlightCells) && move.highlightCells.some((cell) => isWorkerCollapsedSquare(boardState, cell.row, cell.col))) return false;
    if ((move.portalLanding || move.portalThrough) && [move.portalEntry, move.portalExit, workerPortalMoveDestination(move)].filter(Boolean).some((cell) => isWorkerCollapsedSquare(boardState, cell.row, cell.col))) return false;
    if (!move.colossusBody && Number.isInteger(move.row) && Number.isInteger(move.col) && isWorkerCollapsedSquare(boardState, move.row, move.col)) return false;
    if (isWorkerFianchettoBlockedMove(boardState, piece, row, col, move)) return false;
    if (isWorkerUndergroundBunkerKing(piece)) return false;
    if (isWorkerStakedPiece(piece)) return false;
    if (isDiceLockedPiece(boardState, piece)) return false;
    if (move.dragonSwap) {
      const target = get(boardState, move.row, move.col);
      if (!canWorkerResolveDragonSwap(boardState, piece, target, move)) return false;
    }
    if (move.substitutionSwap) {
      const target = get(boardState, move.row, move.col);
      if (!canWorkerResolveSubstitutionMove(boardState, piece, target)) return false;
    }
    if (move.crownGroundCapture) {
      const occupant = get(boardState, move.row, move.col);
      if (occupant && !workerCrownEligiblePiece(occupant)) return false;
    }
    if (!isWorkerChainMoveAllowed(boardState, piece, row, col, move)) return false;
    if (isWorkerHighGroundCaptureBlocked(boardState, piece, row, col, move)) return false;
    if (isWorkerMachoMoveBlocked(boardState, piece, row, col, move)) return false;
    if (piece.type === "pawn" && boardState.effects?.pawnReverse?.[piece.color] > 0) {
      const to = workerPortalMoveDestination(move);
      if (to && (to.row - row) * (piece.color === "white" ? -1 : 1) > 0) return false;
    }
    if (isWorkerTauntBackwardMove(boardState, piece, row, col, move)) return false;
    if (isWorkerBloodCurseMoveBlocked(boardState, piece, row, col, move)) return false;
    if (isWorkerPalaceMoveBlocked(boardState, piece, row, col, move)) return false;
    if (isWorkerMannerCaptureLocked(boardState, piece) && isWorkerCaptureMove(boardState, piece, move)) return false;
    if (isWorkerSaturationCaptureLocked(boardState, piece) && isWorkerCaptureMove(boardState, piece, move)) return false;
    if (isWorkerInitiativeCaptureLocked(boardState, piece?.color) && isWorkerCaptureMove(boardState, piece, move)) return false;
    if (isWorkerFreeMoveCaptureLocked(boardState, piece?.color) && isWorkerCaptureMove(boardState, piece, move)) return false;
    if (isChaosChessCaptureBlocked(boardState.chaosNoCaptureUntilHalfTurn, boardState.turnsTaken) && isWorkerCaptureMove(boardState, piece, move)) return false;
    if (piece?.repositionSecondMove && isWorkerRepositionCaptureMove(boardState, piece, move)) return false;
    return isWorkerMonochromeMoveAllowed(boardState, piece, row, col, move);
  }
  function isWorkerMonochromeMoveAllowed(boardState, piece, row, col, move) {
    if (!boardState.monochromeChess || piece?.type === "wall") return true;
    if (move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.setLogDirection) return true;
    const destination = workerPortalMoveDestination(move);
    if (!destination) return true;
    const shade = squareShade(row, col);
    if (piece) piece.monoShade = shade;
    return squareShade(destination.row, destination.col) === shade;
  }
  function hasWorkerImperialDragonMovement(boardState, piece) {
    return Boolean(
      piece?.color && boardState?.imperialStudies?.[piece.color] && isWorkerKingRole(boardState, piece) && Array.isArray(piece.imperialMoves) && piece.imperialMoves.map(String).includes("dragon")
    );
  }
  function canWorkerUseImperialDragonSwap(boardState, piece, move = {}) {
    return Boolean(move?.dragonSwap && hasWorkerImperialDragonMovement(boardState, piece));
  }
  function canWorkerResolveDragonSwap(boardState, piece, target, move = {}) {
    return Boolean(
      piece && target && target.color === piece.color && !workerIsLargePiece(target) && (pieceHasAbility(piece, "dragon") || canWorkerUseImperialDragonSwap(boardState, piece, move))
    );
  }
  function isWorkerTauntBackwardMove(boardState, piece, row, col, move) {
    if (!piece?.color || piece.type === "football" || (Number(boardState?.taunt?.[piece.color]) || 0) <= 0) return false;
    if (move.colossusBody || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.setLogDirection) return false;
    const targetRow2 = workerPortalMoveDestination(move)?.row;
    if (!Number.isInteger(targetRow2)) return false;
    return (targetRow2 - row) * pawnDir(boardState, piece.color) < 0;
  }
  function applyWorkerIceSheetRestrictions(boardState, piece, row, col, moves) {
    if (["hook", "brutus"].includes(pieceAbilityType(piece))) {
      return filterHookIceSheetTerminalMoves(
        moves,
        (move) => workerIsIceSheetMovementMove(move) && isWorkerBaseMoveAllowed(boardState, piece, row, col, move)
      );
    }
    if (piece?.type === "cardinal") {
      const terminalKeys = new Set(
        diagonals().map(([dr, dc]) => workerCardinalTerminalMove(boardState, row, col, piece.color, dr, dc)).filter(Boolean).map((move) => `${move.row}:${move.col}`)
      );
      if (terminalKeys.size) {
        return moves.filter((move) => !workerIsIceSheetMovementMove(move) || !isWorkerBaseMoveAllowed(boardState, piece, row, col, move) || terminalKeys.has(`${move.row}:${move.col}`));
      }
    }
    const movable = moves.filter((move) => workerIsIceSheetMovementMove(move) && isWorkerBaseMoveAllowed(boardState, piece, row, col, move));
    if (!movable.length) return moves;
    const maxDistanceByDirection = /* @__PURE__ */ new Map();
    movable.forEach((move) => {
      const direction = workerIceSheetMoveDirectionKey(row, col, move);
      if (!direction) return;
      const distance = workerIceSheetMoveDistance(row, col, move);
      maxDistanceByDirection.set(direction, Math.max(maxDistanceByDirection.get(direction) || 0, distance));
    });
    if (!maxDistanceByDirection.size) return moves;
    return moves.filter((move) => {
      if (!workerIsIceSheetMovementMove(move)) return true;
      if (!isWorkerBaseMoveAllowed(boardState, piece, row, col, move)) return true;
      const direction = workerIceSheetMoveDirectionKey(row, col, move);
      if (!direction) return true;
      return workerIceSheetMoveDistance(row, col, move) >= maxDistanceByDirection.get(direction);
    });
  }
  function workerIsIceSheetMovementMove(move) {
    if (!move || move.colossusBody || move.castle || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.setLogDirection) return false;
    return Number.isInteger(move.row) && Number.isInteger(move.col);
  }
  function workerIceSheetMoveDistance(row, col, move) {
    const targetRow2 = Number.isInteger(move.anchorRow) ? move.anchorRow : move.row;
    const targetCol2 = Number.isInteger(move.anchorCol) ? move.anchorCol : move.col;
    return Math.max(Math.abs(targetRow2 - row), Math.abs(targetCol2 - col));
  }
  function workerIceSheetMoveDirectionKey(row, col, move) {
    const targetRow2 = Number.isInteger(move.anchorRow) ? move.anchorRow : move.row;
    const targetCol2 = Number.isInteger(move.anchorCol) ? move.anchorCol : move.col;
    const dr = targetRow2 - row;
    const dc = targetCol2 - col;
    if (!dr && !dc) return "";
    const divisor = workerGreatestCommonDivisor(Math.abs(dr), Math.abs(dc));
    return `${dr / divisor}:${dc / divisor}`;
  }
  function workerGreatestCommonDivisor(a, b) {
    let left = Math.max(0, Math.trunc(a));
    let right = Math.max(0, Math.trunc(b));
    while (right) {
      const next = left % right;
      left = right;
      right = next;
    }
    return left || 1;
  }
  function knightDeltas() {
    return [[2, 1], [2, -1], [-2, 1], [-2, -1], [1, 2], [1, -2], [-1, 2], [-1, -2]];
  }
  const WORKER_IMPERIAL_STUDIES_EXCLUDED_TYPES = /* @__PURE__ */ new Set([
    "",
    "king",
    "royalKnight",
    "shotgunKing",
    "merchant",
    "recruiter",
    "wall",
    "scarecrow",
    "football",
    "blackHole",
    "colossus",
    "bigRook",
    "bigBishop",
    "coffin",
    "log",
    "timeTraveler",
    "wizard"
  ]);
  function workerImperialStudyLearnableType(piece) {
    const type = piece?.type === "windmill" ? piece.windmillMode === "rook" ? "rook" : "bishop" : piece?.type;
    return WORKER_IMPERIAL_STUDIES_EXCLUDED_TYPES.has(type) ? "" : type;
  }
  function workerImperialStudyMoveTypes(piece) {
    if (!Array.isArray(piece?.imperialMoves)) return [];
    return [...new Set(piece.imperialMoves.map(String).filter((type) => type && !WORKER_IMPERIAL_STUDIES_EXCLUDED_TYPES.has(type)))];
  }
  function workerImperialStudyMoves(boardState, row, col, piece) {
    if (!piece?.color || !boardState.imperialStudies?.[piece.color] || !isWorkerKingRole(boardState, piece)) return [];
    return uniqueMoves(workerImperialStudyMoveTypes(piece).flatMap((type) => {
      const learnedPiece = { ...piece, type };
      switch (type) {
        case "pawn":
        case "squire":
        case "standardBearer":
          return pawnMoves(boardState, row, col, piece.color);
        case "checker":
        case "checkerKing":
          return checkerMoves(boardState, row, col, piece.color, type);
        case "fanatic":
          return fanaticMoves(boardState, row, col, piece.color);
        case "knight":
          return knightLikeMoves(boardState, row, col, learnedPiece);
        case "knightmaster":
          return leapMoves(boardState, row, col, piece.color, diagonals());
        case "assassin":
          return assassinMoves(boardState, row, col, piece.color);
        case "man":
          return leapMoves(boardState, row, col, piece.color, queenDirections());
        case "camel":
          return leapMoves(boardState, row, col, piece.color, [[3, 1], [3, -1], [-3, 1], [-3, -1], [1, 3], [1, -3], [-1, 3], [-1, -3]]);
        case "alfil":
          return leapMoves(boardState, row, col, piece.color, [[2, 2], [2, -2], [-2, 2], [-2, -2]]);
        case "ferz":
          return leapMoves(boardState, row, col, piece.color, diagonals());
        case "eagle":
        case "alibaba":
          return leapMoves(boardState, row, col, piece.color, eagleDeltas());
        case "vampireLord":
          return vampireLordMoves(boardState, row, col, piece.color);
        case "bat":
          return batMoves(boardState, row, col, piece.color);
        case "grasshopper":
          return grasshopperMoves(boardState, row, col, piece.color);
        case "bishop":
          return rayMoves(boardState, row, col, piece.color, diagonals());
        case "cardinal":
          return cardinalMoves(boardState, row, col, piece.color);
        case "protestant":
          return protestantMoves(boardState, row, col, piece.color);
        case "rook":
          return rayMoves(boardState, row, col, piece.color, orthogonals(), 8);
        case "hook":
          return hookMoves(boardState, row, col, piece.color);
        case "herald":
          return heraldMoves(boardState, row, col, learnedPiece);
        case "cannon":
          return cannonMoves(boardState, row, col, piece.color);
        case "queen":
          return rayMoves(boardState, row, col, piece.color, queenDirections(), 8);
        case "amazon":
          return uniqueMoves([
            ...rayMoves(boardState, row, col, piece.color, queenDirections(), 8),
            ...leapMoves(boardState, row, col, piece.color, workerKnightDeltasForMove(boardState, row, col, piece.color))
          ]);
        case "pegasus":
          return pegasusMoves(boardState, row, col, piece.color);
        case "dragon":
          return dragonMoves(boardState, row, col, learnedPiece);
        case "jester":
          return jesterMoves(boardState, row, col, piece.color);
        case "primeMinister":
          return primeMinisterMoves(boardState, row, col, piece.color);
        case "missionary":
          return workerMissionaryMoves(boardState, row, col, learnedPiece);
        default:
          return [];
      }
    }).map((move) => ({ ...move, imperialStudy: true })));
  }
  function workerLearnImperialStudyMovement(boardState, attacker, capturedPieces) {
    if (!attacker?.color || !boardState.imperialStudies?.[attacker.color] || !isWorkerKingRole(boardState, attacker)) return false;
    const learned = new Set(workerImperialStudyMoveTypes(attacker));
    const before = learned.size;
    (capturedPieces || []).forEach((captured) => {
      const type = workerImperialStudyLearnableType(captured);
      if (type) learned.add(type);
    });
    if (learned.size === before) return false;
    attacker.imperialMoves = [...learned];
    return true;
  }
  function kingDeltas() {
    return [[-1, -1], [-1, 0], [-1, 1], [0, -1], [0, 1], [1, -1], [1, 0], [1, 1]];
  }
  function generateSpecialPieceActions(boardState, piece, row, col, color) {
    if (piece.type === "shotgunKing" && (piece.ammo ?? 0) < (piece.maxAmmo ?? 3)) {
      return [{ type: "shotgunReload", color, from: { row, col } }];
    }
    if (pieceHasAbility(piece, "wizard")) return wizardActions(boardState, piece, row, col, color);
    return [];
  }
  function logMoves(boardState, row, col) {
    return queenDirections().filter(([dr, dc]) => !boardState.monochromeChess || dr !== 0 && dc !== 0).map(([dr, dc]) => ({ row: row + dr, col: col + dc, setLogDirection: { dr, dc } })).filter((move) => inBounds(move.row, move.col));
  }
  function isUsefulSpecialMove(boardState, move, piece, row, col, color) {
    if (move?.colossusAttack || move?.shotgunBlast) {
      const cells = move.sectorCells || (move.shotgunBlast ? shotgunBlastCells(row, col, move.shotgunDirection) : []);
      return cellsContainEnemy(boardState, cells, color, piece);
    }
    if ((move?.colossusMove || move?.bigRookMove) && (move.highlightCells || colossusCells(move.anchorRow ?? move.row, move.anchorCol ?? move.col)).some((cell) => isBlackHoleCell(boardState, cell.row, cell.col))) return false;
    const destination = workerPortalMoveDestination(move);
    if (destination && isBlackHoleCell(boardState, destination.row, destination.col)) return false;
    return true;
  }
  function cellsContainEnemy(boardState, cells, color, attacker = null) {
    return (cells || []).some((cell) => {
      const item = get(boardState, cell.row, cell.col);
      return !isIndirectAttackImmunePiece(item) && canWorkerCaptureTarget(color, item, attacker || { color }, boardState);
    });
  }
  function workerColossusSectorTargetAllowed(boardState, target, attacker) {
    return Boolean(target && target.color === opponent(attacker.color) && !target.shielded && !isIndirectAttackImmunePiece(target) && canWorkerCaptureTarget(attacker.color, target, attacker, boardState));
  }
  function activeWorkerForcedExtraMove(boardState, color) {
    let found = null;
    forEachPiece(boardState, (piece, row, col) => {
      if (!found && piece?.color === color && (piece.thiefSecondMove || piece.fileSurgeSecondMove || piece.rookLiftSecondMove || piece.ironMonarchExtraMove || piece.underpromotionSecondMove || piece.checkerChainCapture || piece.madHorseSecondMove || piece.repositionSecondMove?.used || piece.repositionSecondMove?.forced || piece.frenzyExtraMove || piece.platformExtraMove || piece.desperado)) {
        found = { piece, row, col };
      }
    });
    return found;
  }
  function isWorkerNativeKing(piece) {
    return Boolean(piece && ["king", "royalKnight", "shotgunKing", "darkWizard"].includes(piece.type));
  }
  function isWorkerZugzwangTargetKing(boardState, piece) {
    return Boolean(piece && (usesSeptember18Balance(boardState) || piece.type !== "merchant") && isWorkerRoyalIdentityPiece(boardState, piece));
  }
  function isWorkerZugzwangKingMove(move) {
    return Boolean(move && Number.isInteger(move.row) && Number.isInteger(move.col) && !move.castle && !move.switcherooMove && !move.colossusAttack && !move.shotgunBlast && !move.shotgunSnipe && !move.merchantBuy && !move.setLogDirection && !move.dragonSwap && !move.substitutionSwap);
  }
  function workerZugzwangKingMoves(boardState, color) {
    const active = Boolean(boardState.zugzwang?.[color]);
    if (active) boardState.zugzwang[color] = false;
    try {
      return piecesMatching(boardState, (piece) => piece.color === color && isWorkerZugzwangTargetKing(boardState, piece)).flatMap(({ piece, row, col }) => generateMovesForPiece(boardState, piece, row, col).filter(isWorkerZugzwangKingMove).filter((move) => isWorkerMoveAllowed(boardState, piece, row, col, move)));
    } finally {
      if (active) boardState.zugzwang[color] = true;
    }
  }
  function workerCanStartFrenzyExtraMove(boardState, piece, row, col, capturedSomething) {
    if (!capturedSomething || piece?.type !== "pawn" || !piece.frenzy || boardState.mode === "gameover" || get(boardState, row, col) !== piece) return false;
    piece.frenzyExtraMove = true;
    const canContinue = generateMovesForPiece(boardState, piece, row, col).some((move) => isWorkerMoveAllowed(boardState, piece, row, col, move));
    if (!canContinue) delete piece.frenzyExtraMove;
    return canContinue;
  }
  function workerTryStartPlatformExtraMove(boardState, piece) {
    const cell = workerPlatformCells(boardState).find((entry) => get(boardState, entry.row, entry.col) === piece);
    if (!boardState?.platformRule?.enabled || boardState.mode === "gameover" || !piece?.id || !Number.isInteger(cell?.row) || !Number.isInteger(cell?.col) || get(boardState, cell.row, cell.col) !== piece) return false;
    if (!Array.isArray(boardState.platformRule.triggeredIds)) boardState.platformRule.triggeredIds = [];
    if (legacyReusablePlatformStates.has(boardState) && boardState.platformRule.triggeredIds.includes(piece.id)) return false;
    if (!boardState.platformRule.triggeredIds.includes(piece.id)) boardState.platformRule.triggeredIds.push(piece.id);
    piece.platformExtraMove = true;
    const origin = findWorkerPieceByRef(boardState, { id: piece.id });
    const canContinue = Boolean(origin) && generateMovesForPiece(boardState, piece, origin.row, origin.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, origin.row, origin.col, candidate));
    if (!canContinue) delete piece.platformExtraMove;
    else boardState.enPassant = null;
    return canContinue;
  }
  function workerPlatformCells(boardState) {
    const rule = boardState?.platformRule;
    if (!rule?.enabled) return [];
    const candidates = Array.isArray(rule.cells) && rule.cells.length ? rule.cells : [rule.cell];
    const seen = /* @__PURE__ */ new Set();
    return candidates.filter((cell) => {
      if (!Number.isInteger(cell?.row) || !Number.isInteger(cell?.col)) return false;
      const key = `${cell.row}:${cell.col}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function clearWorkerRepositionMarks(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color) delete piece.repositionSecondMove;
    });
  }
  function workerSwitcherooMoves(boardState, row, col, color) {
    const item = get(boardState, row, col);
    if (!boardState.switcheroo?.[color] || !item || item.type !== "king" || item.color !== color || isWorkerUndergroundBunkerKing(item)) return [];
    return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn").map(({ row: targetRow2, col: targetCol2 }) => ({ row: targetRow2, col: targetCol2, switcherooMove: true }));
  }
  function workerPromotionRushMoves(boardState, piece, row, col) {
    if (!isPromotionRushActive(piece, boardState.turnsTaken?.[piece?.color] || 0)) return [];
    if (workerIsLargePiece(piece)) return workerPromotionRushLargeMoves(boardState, piece, row, col);
    return rayMoves(boardState, row, col, piece.color, queenDirections(), Math.max(boardRowCount(boardState), boardColCount(boardState))).filter((move) => !workerMovementOccupant(boardState, row, col, move.row, move.col, piece.color)).map((move) => ({ ...move, promotionRushMove: true }));
  }
  function workerPromotionRushLargeMoves(boardState, piece, row, col) {
    const moves = [];
    queenDirections().forEach(([dr, dc]) => {
      for (let distance = 1; ; distance += 1) {
        const anchorRow = row + dr * distance;
        const anchorCol = col + dc * distance;
        const cells = colossusCells(anchorRow, anchorCol);
        if (cells.length !== 4 || cells.some((cell) => {
          const occupant = get(boardState, cell.row, cell.col);
          return occupant && occupant !== piece;
        })) break;
        moves.push({
          row: anchorRow,
          col: anchorCol,
          anchorRow,
          anchorCol,
          highlightCells: cells,
          promotionRushMove: true,
          ...["bigRook", "bigBishop"].includes(piece.type) ? { bigRookMove: true, bigRookLandingCaptures: [] } : { colossusMove: true, colossusLandingCaptures: [] }
        });
      }
    });
    return moves;
  }
  function workerSubstitutionMoves(boardState, piece) {
    if (!piece?.color || !boardState.substitution?.[piece.color]) return [];
    return piecesMatching(boardState, (target) => canSubstitutePieces(piece, target, boardState.substitution[piece.color])).map(({ piece: target, row, col }) => ({
      row: workerIsLargePiece(target) ? target.anchorRow : row,
      col: workerIsLargePiece(target) ? target.anchorCol : col,
      substitutionSwap: true
    })).filter((move) => Number.isInteger(move.row) && Number.isInteger(move.col));
  }
  function workerHasSubstitutionCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color).some(({ piece }) => piecesMatching(boardState, (target) => canSubstitutePieces(piece, target, true)).length > 0);
  }
  function canWorkerResolveSubstitutionMove(boardState, piece, target) {
    return canSubstitutePieces(piece, target, Boolean(boardState.substitution?.[piece?.color]));
  }
  function workerChainTargetEntries(boardState, color) {
    return piecesMatching(boardState, (piece) => piece?.color === opponent(color) && !workerIsLargePiece(piece) && !["wall", "football", "blackHole"].includes(piece.type));
  }
  function workerChainPairCandidates(boardState, color) {
    const entries = workerChainTargetEntries(boardState, color);
    const existing = new Set(normalizeChainBonds(boardState.chainBonds).map((bond) => [bond.aId, bond.bId].sort().join("\0")));
    const pairs = [];
    for (let firstIndex = 0; firstIndex < entries.length; firstIndex += 1) {
      for (let secondIndex = firstIndex + 1; secondIndex < entries.length; secondIndex += 1) {
        const first = entries[firstIndex];
        const second = entries[secondIndex];
        if (!isWithinChainRange(first, second)) continue;
        if (first.piece.id && second.piece.id && existing.has([first.piece.id, second.piece.id].sort().join("\0"))) continue;
        pairs.push([first, second]);
        if (pairs.length >= 24) return pairs;
      }
    }
    return pairs;
  }
  function workerEnsureChainPieceId(boardState, piece, row, col) {
    if (!piece) return "";
    if (!piece.id) {
      piece.id = `${piece.color || "piece"}-${piece.type || "unknown"}-chain-${Number(boardState.moveCount) || 0}-${row}-${col}`;
    }
    return piece.id;
  }
  function workerChainMoveProjection(boardState, piece, row, col, move) {
    const projected = /* @__PURE__ */ new Map();
    if (!piece?.id || !move) return projected;
    const source = workerIsLargePiece(piece) ? { row: piece.anchorRow, col: piece.anchorCol } : { row, col };
    const staysInPlace = move.colossusBody || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.setLogDirection;
    const destination = staysInPlace ? source : workerPortalMoveDestination(move);
    projected.set(piece.id, destination);
    if (move.dragonSwap || move.substitutionSwap) {
      const target = get(boardState, move.row, move.col);
      if (target?.id) projected.set(target.id, source);
    }
    if (move.switcherooMove) {
      const target = get(boardState, move.row, move.col);
      if (target?.id) projected.set(target.id, null);
    }
    if (move.castle && move.rookFrom && move.rookTo) {
      const rook = get(boardState, move.rookFrom.row, move.rookFrom.col);
      if (rook?.id) projected.set(rook.id, { row: move.rookTo.row, col: move.rookTo.col });
    }
    return projected;
  }
  function isWorkerChainMoveAllowed(boardState, piece, row, col, move) {
    const bonds = normalizeChainBonds(boardState.chainBonds);
    if (!bonds.length) return true;
    const projected = workerChainMoveProjection(boardState, piece, row, col, move);
    return bonds.every((bond) => {
      const first = findWorkerPieceByRef(boardState, { id: bond.aId });
      const second = findWorkerPieceByRef(boardState, { id: bond.bId });
      if (!first || !second) return true;
      const firstPosition = projected.has(bond.aId) ? projected.get(bond.aId) : first;
      const secondPosition = projected.has(bond.bId) ? projected.get(bond.bId) : second;
      if (!firstPosition || !secondPosition) return true;
      return isWithinChainRange(firstPosition, secondPosition);
    });
  }
  function isWorkerChainDestinationAllowed(boardState, piece, destination) {
    if (!piece?.id || !destination) return true;
    return normalizeChainBonds(boardState.chainBonds).every((bond) => {
      if (bond.aId !== piece.id && bond.bId !== piece.id) return true;
      const partnerId = bond.aId === piece.id ? bond.bId : bond.aId;
      const partner = findWorkerPieceByRef(boardState, { id: partnerId });
      return !partner || isWithinChainRange(destination, partner);
    });
  }
  function canWorkerResolveSwitcherooMove(boardState, piece, target) {
    return Boolean(
      piece && target && boardState.switcheroo?.[piece.color] && isWorkerKingRole(boardState, piece) && !isWorkerUndergroundBunkerKing(piece) && target.color === piece.color && target.type === "pawn"
    );
  }
  function merchantMoves(boardState, row, col, merchant) {
    const moves = [];
    const gold = merchant.gold ?? 0;
    forEachPiece(boardState, (item, targetRow2, targetCol2) => {
      if (item.color !== opponent(merchant.color) || ["merchant", "wall", "football", "monster", "blackHole"].includes(item.type)) return;
      if (isFrozenPiece(item)) return;
      if (item.type !== "scarecrow" && item.shielded) return;
      if (isArmisticeCaptureBlocked(boardState.armistice, merchant, item)) return;
      const cost = merchantCost(item);
      if (Number.isFinite(cost) && cost <= gold) moves.push({ row: targetRow2, col: targetCol2, merchantBuy: true, cost });
    });
    return moves;
  }
  function merchantCost(item) {
    return merchantPurchasePrice(item);
  }
  function shotgunKingMoves(boardState, row, col, piece) {
    const moves = rayMoves(boardState, row, col, piece.color, queenDirections(), 1).filter((move) => !workerMovementOccupant(boardState, row, col, move.row, move.col, piece.color));
    const ammo = piece.ammo ?? 0;
    if (ammo >= SHOTGUN_BLAST_AMMO_COST) {
      queenDirections().forEach(([dr, dc]) => {
        const cells = shotgunBlastCells(row, col, [dr, dc]);
        if (cellsContainEnemy(boardState, cells, piece.color, piece)) {
          const first = cells.find((cell) => {
            const target = get(boardState, cell.row, cell.col);
            return !isIndirectAttackImmunePiece(target) && canWorkerCaptureTarget(piece.color, target, piece, boardState);
          }) || cells[0];
          moves.push({ row: first.row, col: first.col, shotgunBlast: true, shotgunDirection: [dr, dc], sectorCells: cells });
        }
      });
    }
    if (ammo >= SHOTGUN_SNIPE_AMMO_COST) {
      queenDirections().forEach(([dr, dc]) => {
        let r = row + dr;
        let c = col + dc;
        while (inBounds(r, c)) {
          const target = get(boardState, r, c);
          if (target) {
            if (!isIndirectAttackImmunePiece(target) && canWorkerCaptureTarget(piece.color, target, piece, boardState)) moves.push({ row: r, col: c, shotgunSnipe: true });
            break;
          }
          r += dr;
          c += dc;
        }
      });
    }
    return moves;
  }
  function shotgunBlastCells(row, col, facing) {
    const [dr, dc] = facing;
    const side = dr === 0 ? [[-1, 0], [0, 0], [1, 0]] : dc === 0 ? [[0, -1], [0, 0], [0, 1]] : [[0, 0], [-dr, 0], [0, -dc]];
    const cells = [];
    const depth = dr === 0 || dc === 0 ? 3 : 2;
    for (let distance = 1; distance <= depth; distance += 1) {
      const offsets = (dr === 0 || dc === 0) && distance === 3 ? [[0, 0]] : side;
      offsets.forEach(([sr, sc]) => {
        const r = row + dr * distance + sr;
        const c = col + dc * distance + sc;
        if (inBounds(r, c)) cells.push({ row: r, col: c });
      });
    }
    return uniqueCells(cells);
  }
  function colossusMoves(boardState, row, col, color) {
    const item = get(boardState, row, col);
    const moves = [];
    orthogonals().forEach(([dr, dc]) => {
      const anchorRow = row + dr;
      const anchorCol = col + dc;
      const cells = colossusCells(anchorRow, anchorCol);
      if (cells.length !== 4) return;
      const landingCaptures = workerColossusLandingCaptures(boardState, cells, item, color);
      if (!landingCaptures) return;
      moves.push({ row: anchorRow, col: anchorCol, anchorRow, anchorCol, highlightCells: cells, colossusLandingCaptures: landingCaptures, colossusMove: true });
    });
    colossusAttackSectors(boardState, row, col, color).forEach((sectorCells) => {
      if (!sectorCells.some((cell) => workerColossusSectorTargetAllowed(boardState, get(boardState, cell.row, cell.col), item))) return;
      sectorCells.forEach((cell) => moves.push({ row: cell.row, col: cell.col, sectorCells, colossusAttack: true }));
    });
    return moves;
  }
  function workerColossusLandingCaptures(boardState, cells, item, color, maxCaptures = Infinity, options = {}) {
    const captures = [];
    const seen = /* @__PURE__ */ new Set();
    const captureLimit = Math.min(maxCaptures, workerSaturationCapturesRemaining(boardState, item));
    for (const cell of cells || []) {
      const target = get(boardState, cell.row, cell.col);
      if (!target || target === item || item?.id && target.id === item.id) continue;
      if (target.protected || isWorkerEncouragedTarget(boardState, target)) return null;
      if (options.allowFriendly) {
        if (["wall", "football"].includes(target.type)) return null;
        if (target.color !== color && !canWorkerCaptureTarget(color, target, item, boardState)) return null;
      } else if (!canWorkerCaptureTarget(color, target, item, boardState)) {
        return null;
      }
      const key = target.id || `${cell.row}:${cell.col}`;
      if (seen.has(key)) continue;
      seen.add(key);
      captures.push({ row: cell.row, col: cell.col });
      if (captures.length > captureLimit) return null;
    }
    return captures;
  }
  function workerBigRookLandingCaptures(boardState, cells, item, color) {
    return workerColossusLandingCaptures(boardState, cells, item, color, septemberLargeCaptureLimit(item.type));
  }
  function workerCrownLargePieceMoves(boardState, row, col, piece) {
    if (!workerIsLargePiece(piece)) return [];
    const moves = [];
    queenDirections().forEach(([dr, dc]) => {
      const anchorRow = row + dr;
      const anchorCol = col + dc;
      const cells = colossusCells(anchorRow, anchorCol).filter(({ row: cellRow, col: cellCol }) => inBounds(cellRow, cellCol, boardState));
      if (cells.length !== 4) return;
      const landingCaptures = workerColossusLandingCaptures(
        boardState,
        cells,
        piece,
        piece.color,
        septemberLargeCaptureLimit(piece.type)
      );
      if (!landingCaptures) return;
      moves.push({
        row: anchorRow,
        col: anchorCol,
        anchorRow,
        anchorCol,
        highlightCells: cells,
        ...["bigRook", "bigBishop"].includes(piece.type) ? { bigRookLandingCaptures: landingCaptures, bigRookMove: true } : { colossusLandingCaptures: landingCaptures, colossusMove: true }
      });
    });
    return uniqueMoves(moves);
  }
  function bigRookMoves(boardState, row, col, color) {
    const item = get(boardState, row, col);
    const moves = [];
    (item.type === "bigBishop" ? diagonals() : orthogonals()).forEach(([dr, dc]) => {
      let distance = 1;
      while (true) {
        const anchorRow = row + dr * distance;
        const anchorCol = col + dc * distance;
        const cells = colossusCells(anchorRow, anchorCol);
        if (cells.length !== 4) break;
        const occupants = cells.map((cell) => get(boardState, cell.row, cell.col)).filter((target) => target && target !== item && (!item?.id || target.id !== item.id));
        const transparentGhosts = occupants.filter((target) => isGhostTransparentFor(item, target, item.type, boardState));
        if (transparentGhosts.length) {
          if (occupants.length !== transparentGhosts.length) break;
          distance += 1;
          continue;
        }
        const landingCaptures = workerBigRookLandingCaptures(boardState, cells, item, color);
        if (!landingCaptures) break;
        moves.push({ row: anchorRow, col: anchorCol, anchorRow, anchorCol, highlightCells: cells, bigRookLandingCaptures: landingCaptures, bigRookMove: true });
        if (landingCaptures.length) break;
        distance += 1;
      }
    });
    return moves;
  }
  function workerBigRookAttacksSquare(boardState, piece, row, col, targetRow2, targetCol2) {
    return bigRookMoves(boardState, row, col, piece.color).some((move) => Array.isArray(move.highlightCells) && move.highlightCells.some((cell) => cell.row === targetRow2 && cell.col === targetCol2));
  }
  function vampireLordMoves(boardState, row, col, color) {
    if (usesBloodMoonNightMovement(boardState, color)) {
      return uniqueMoves([
        ...rayMoves(boardState, row, col, color, queenDirections(), 8),
        ...leapMoves(boardState, row, col, color, workerKnightDeltasForMove(boardState, row, col, color))
      ]);
    }
    return rayMoves(boardState, row, col, color, queenDirections(), 1);
  }
  function batMoves(boardState, row, col, color) {
    return usesBloodMoonNightMovement(boardState, color) ? primeMinisterMoves(boardState, row, col, color) : rayMoves(boardState, row, col, color, orthogonals(), 2);
  }
  function primeMinisterMoves(boardState, row, col, color) {
    const moves = [];
    const piece = get(boardState, row, col) || { type: "primeMinister", color };
    queenDirections().forEach(([dr1, dc1]) => {
      const midRow = row + dr1;
      const midCol = col + dc1;
      if (!inBounds(midRow, midCol, boardState) || isWorkerCollapsedSquare(boardState, midRow, midCol)) return;
      const midTarget = get(boardState, midRow, midCol);
      const portalExit = workerPortalExitAt(boardState, midRow, midCol);
      if (portalExit) {
        const portalEntry = { row: midRow, col: midCol };
        if (workerCanPortalLandOn(boardState, piece, portalEntry) && workerCanPortalLandOn(boardState, piece, portalExit)) {
          moves.push({
            row: midRow,
            col: midCol,
            primeMinisterMove: true,
            primeMinisterPortalEntry: true,
            portalLanding: true,
            portalEntry,
            portalExit
          });
        }
        if (!midTarget && !get(boardState, portalExit.row, portalExit.col)) {
          queenDirections().forEach(([dr2, dc2]) => {
            const nextRow = portalExit.row + dr2;
            const nextCol = portalExit.col + dc2;
            if (!inBounds(nextRow, nextCol, boardState) || isWorkerCollapsedSquare(boardState, nextRow, nextCol)) return;
            const target = get(boardState, nextRow, nextCol);
            if (!target || canWorkerCaptureTarget(color, target, piece, boardState)) {
              moves.push({
                row: nextRow,
                col: nextCol,
                primeMinisterMove: true,
                primeMinisterPortalExit: true,
                portalThrough: true,
                portalEntry,
                portalExit
              });
            }
          });
        }
        return;
      }
      if (!midTarget || canWorkerCaptureTarget(color, midTarget, piece, boardState)) {
        moves.push({ row: midRow, col: midCol, primeMinisterMove: true });
      }
      if (midTarget) return;
      queenDirections().forEach(([dr2, dc2]) => {
        const nextRow = midRow + dr2;
        const nextCol = midCol + dc2;
        if (!inBounds(nextRow, nextCol, boardState) || isWorkerCollapsedSquare(boardState, nextRow, nextCol)) return;
        if (nextRow === row && nextCol === col) return;
        const target = get(boardState, nextRow, nextCol);
        const secondPortalExit = workerPortalExitAt(boardState, nextRow, nextCol);
        if (secondPortalExit) {
          if (workerCanPortalLandOn(boardState, piece, { row: nextRow, col: nextCol }) && workerCanPortalLandOn(boardState, piece, secondPortalExit)) {
            moves.push({
              row: nextRow,
              col: nextCol,
              primeMinisterMove: true,
              primeMinisterPortalSecondEntry: true,
              portalLanding: true,
              portalEntry: { row: nextRow, col: nextCol },
              portalExit: secondPortalExit
            });
          }
          return;
        }
        if (!target || canWorkerCaptureTarget(color, target, piece, boardState)) {
          moves.push({ row: nextRow, col: nextCol, primeMinisterMove: true });
        }
      });
    });
    return uniqueMoves(moves);
  }
  function eagleDeltas() {
    return [[-2, -2], [-2, 0], [-2, 2], [0, -2], [0, 2], [2, -2], [2, 0], [2, 2]];
  }
  function wizardPieceMoves(boardState, row, col, color) {
    return queenDirections().map(([dr, dc]) => ({ row: row + dr, col: col + dc })).filter((move) => {
      if (!inBounds(move.row, move.col, boardState)) return false;
      return !workerMovementOccupant(boardState, row, col, move.row, move.col, color);
    });
  }
  function workerMovementOccupant(boardState, fromRow, fromCol, row, col, color) {
    const target = get(boardState, row, col);
    return isStealthTransparentFor(get(boardState, fromRow, fromCol) || { color }, target, boardState) ? null : target;
  }
  function protestantMoves(boardState, row, col, color) {
    const piece = get(boardState, row, col) || { type: "protestant", color };
    return protestantPathMoves({
      row,
      col,
      rowCount: boardRowCount(boardState),
      colCount: boardColCount(boardState),
      portalRule: workerPortalRule(boardState),
      directions: diagonals(),
      getOccupant: (targetRow2, targetCol2) => get(boardState, targetRow2, targetCol2),
      canCapture: (target) => canWorkerCaptureTarget(color, target, piece, boardState),
      isPortalBlocked: (targetRow2, targetCol2) => isWorkerCollapsedSquare(boardState, targetRow2, targetCol2)
    });
  }
  function bishopSnipeMoves(boardState, row, col, color) {
    const moves = [];
    const attacker = get(boardState, row, col) || { type: "bishop", color };
    (boardState.reversal?.[color] && attacker.type === "bishop" ? orthogonals() : diagonals()).forEach(([dr, dc]) => {
      let nextRow = row + dr;
      let nextCol = col + dc;
      let jumpedPieces = 0;
      while (inBounds(nextRow, nextCol, boardState)) {
        const target = get(boardState, nextRow, nextCol);
        if (isGhostTransparentFor(attacker, target, "bishop", boardState)) {
          nextRow += dr;
          nextCol += dc;
          continue;
        }
        if (target && jumpedPieces === 1 && canWorkerCaptureTarget(color, target, get(boardState, row, col) || { type: "bishop", color }, boardState)) {
          moves.push({ row: nextRow, col: nextCol, bishopSnipe: true });
        }
        if (target) {
          jumpedPieces += 1;
          if (jumpedPieces > 1) break;
        }
        nextRow += dr;
        nextCol += dc;
      }
    });
    return uniqueMoves(moves);
  }
  function bishopSnipeAttacksSquare(boardState, row, col, targetRow2, targetCol2) {
    const dr = Math.sign(targetRow2 - row);
    const dc = Math.sign(targetCol2 - col);
    const reversed = get(boardState, row, col)?.type === "bishop" && boardState.reversal?.[get(boardState, row, col)?.color];
    const distanceRow = Math.abs(targetRow2 - row), distanceCol = Math.abs(targetCol2 - col);
    if (reversed ? !((dr === 0 || dc === 0) && distanceRow + distanceCol > 0) : !dr || !dc || distanceRow !== distanceCol) return false;
    let jumpedPieces = 0;
    const attacker = get(boardState, row, col) || null;
    let nextRow = row + dr;
    let nextCol = col + dc;
    while (inBounds(nextRow, nextCol, boardState)) {
      if (nextRow === targetRow2 && nextCol === targetCol2) return jumpedPieces === 1;
      if (get(boardState, nextRow, nextCol) && !isGhostTransparentFor(attacker, get(boardState, nextRow, nextCol), "bishop", boardState)) {
        jumpedPieces += 1;
        if (jumpedPieces > 1) return false;
      }
      nextRow += dr;
      nextCol += dc;
    }
    return false;
  }
  function heraldMoves(boardState, row, col, piece) {
    const moves = [];
    orthogonals().forEach(([dr, dc]) => {
      for (let distance = 1; distance <= 3; distance += 1) {
        const nextRow = row + dr * distance;
        const nextCol = col + dc * distance;
        if (!inBounds(nextRow, nextCol, boardState)) break;
        const target = workerMovementOccupant(boardState, row, col, nextRow, nextCol, piece.color);
        if (!target) {
          moves.push({ row: nextRow, col: nextCol });
        } else if (isGhostTransparentFor(piece, target, "herald", boardState)) {
          continue;
        } else if (isWorkerHeraldJumpLocked(boardState, piece, piece?.color)) {
          break;
        }
      }
    });
    return uniqueMoves(moves);
  }
  function cannonMoves(boardState, row, col, color) {
    const attacker = get(boardState, row, col) || { type: "cannon", color };
    return uniqueMoves(cannonPathMoves({
      row,
      col,
      rowCount: boardRowCount(boardState),
      colCount: boardColCount(boardState),
      portalRule: workerPortalRule(boardState),
      directions: orthogonals(),
      getOccupant: (targetRow2, targetCol2) => workerMovementOccupant(boardState, row, col, targetRow2, targetCol2, color),
      canCapture: (target) => canWorkerCaptureTarget(color, target, attacker, boardState),
      isCannon: (target) => target?.type === "cannon",
      canServeAsScreen: (target) => canCannonPieceServeAsScreen(target, boardState),
      isTransparent: (target) => isGhostTransparentFor(attacker, target, "cannon", boardState),
      isPortalBlocked: (targetRow2, targetCol2) => isWorkerCollapsedSquare(boardState, targetRow2, targetCol2)
    }));
  }
  function cardinalMoves(boardState, row, col, color) {
    const moves = [];
    const attacker = get(boardState, row, col) || { type: "cardinal", color };
    const portalRule = workerPortalRule(boardState);
    diagonals().forEach(([startDr, startDc]) => {
      let dr = startDr;
      let dc = startDc;
      let currentRow = row;
      let currentCol = col;
      let portalTransit = null;
      const seen = /* @__PURE__ */ new Set();
      const stepLimit = Math.max(24, boardRowCount(boardState) * boardColCount(boardState) * 4);
      for (let step = 0; step < stepLimit; step += 1) {
        let nextRow = currentRow + dr;
        let nextCol = currentCol + dc;
        if (!inBounds(nextRow, nextCol, boardState)) {
          if (nextRow < 0 || nextRow >= boardRowCount(boardState)) dr *= -1;
          if (nextCol < 0 || nextCol >= boardColCount(boardState)) dc *= -1;
          nextRow = currentRow + dr;
          nextCol = currentCol + dc;
        }
        if (!inBounds(nextRow, nextCol, boardState)) break;
        const key = `${nextRow}:${nextCol}:${dr}:${dc}:${portalTransit ? 1 : 0}`;
        if (seen.has(key)) break;
        seen.add(key);
        const target = get(boardState, nextRow, nextCol);
        const portalExit = portalRule ? workerPortalExitAt(boardState, nextRow, nextCol) : null;
        if (portalExit && portalTransit) break;
        if (target?.type === "wall") {
          dr *= -1;
          dc *= -1;
          continue;
        }
        if (!target) {
          moves.push({
            row: nextRow,
            col: nextCol,
            ...portalTransit ? {
              portalThrough: true,
              portalEntry: portalTransit.entry,
              portalExit: portalTransit.exit
            } : {}
          });
          if (portalExit) {
            const exitTarget = get(boardState, portalExit.row, portalExit.col);
            if ((!exitTarget || isGhostTransparentFor(attacker, exitTarget, "cardinal", boardState)) && !isWorkerCollapsedSquare(boardState, nextRow, nextCol) && !isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col)) {
              portalTransit = {
                entry: { row: nextRow, col: nextCol },
                exit: { ...portalExit }
              };
              currentRow = portalExit.row;
              currentCol = portalExit.col;
              continue;
            }
            break;
          }
          currentRow = nextRow;
          currentCol = nextCol;
          continue;
        }
        if (isGhostTransparentFor(attacker, target, "cardinal", boardState)) {
          if (portalExit) {
            moves.push({ row: nextRow, col: nextCol, portalTransparentEntry: true });
            const exitTarget = get(boardState, portalExit.row, portalExit.col);
            if ((!exitTarget || isGhostTransparentFor(attacker, exitTarget, "cardinal", boardState)) && !isWorkerCollapsedSquare(boardState, nextRow, nextCol) && !isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col)) {
              portalTransit = {
                entry: { row: nextRow, col: nextCol },
                exit: { ...portalExit }
              };
              currentRow = portalExit.row;
              currentCol = portalExit.col;
              continue;
            }
            break;
          }
          currentRow = nextRow;
          currentCol = nextCol;
          continue;
        }
        if (canWorkerCaptureTarget(color, target, get(boardState, row, col) || { type: "cardinal", color }, boardState)) {
          moves.push({
            row: nextRow,
            col: nextCol,
            ...portalTransit ? {
              portalThrough: true,
              portalEntry: portalTransit.entry,
              portalExit: portalTransit.exit
            } : {}
          });
        }
        break;
      }
    });
    return uniqueMoves(moves);
  }
  function workerCardinalTerminalMove(boardState, row, col, color, startDr, startDc) {
    let dr = startDr;
    let dc = startDc;
    let currentRow = row;
    let currentCol = col;
    let portalTransit = null;
    let terminalMove = null;
    const seen = /* @__PURE__ */ new Set();
    const attacker = get(boardState, row, col) || { type: "cardinal", color };
    const portalRule = workerPortalRule(boardState);
    const stepLimit = Math.max(24, boardRowCount(boardState) * boardColCount(boardState) * 4);
    for (let step = 0; step < stepLimit; step += 1) {
      const stateKey = `${currentRow}-${currentCol}-${dr}-${dc}-${portalTransit ? 1 : 0}`;
      if (seen.has(stateKey)) return terminalMove;
      seen.add(stateKey);
      let nextRow = currentRow + dr;
      let nextCol = currentCol + dc;
      if (!inBounds(nextRow, nextCol, boardState)) {
        if (nextRow < 0 || nextRow >= boardRowCount(boardState)) dr *= -1;
        if (nextCol < 0 || nextCol >= boardColCount(boardState)) dc *= -1;
        nextRow = currentRow + dr;
        nextCol = currentCol + dc;
      }
      if (!inBounds(nextRow, nextCol, boardState)) return terminalMove;
      const target = get(boardState, nextRow, nextCol);
      const portalExit = portalRule ? workerPortalExitAt(boardState, nextRow, nextCol) : null;
      if (portalExit && portalTransit) return terminalMove;
      if (target?.type === "wall") {
        dr *= -1;
        dc *= -1;
        continue;
      }
      if (isGhostTransparentFor(attacker, target, "cardinal", boardState)) {
        if (portalExit) {
          terminalMove = { row: nextRow, col: nextCol, portalTransparentEntry: true };
          const exitTarget = get(boardState, portalExit.row, portalExit.col);
          if ((!exitTarget || isGhostTransparentFor(attacker, exitTarget, "cardinal", boardState)) && !isWorkerCollapsedSquare(boardState, nextRow, nextCol) && !isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col)) {
            portalTransit = {
              entry: { row: nextRow, col: nextCol },
              exit: { ...portalExit }
            };
            currentRow = portalExit.row;
            currentCol = portalExit.col;
            continue;
          }
          return terminalMove;
        }
        currentRow = nextRow;
        currentCol = nextCol;
        continue;
      }
      if (target) {
        return canWorkerCaptureTarget(color, target, get(boardState, row, col) || { type: "cardinal", color }, boardState) ? {
          row: nextRow,
          col: nextCol,
          ...portalTransit ? {
            portalThrough: true,
            portalEntry: portalTransit.entry,
            portalExit: portalTransit.exit
          } : {}
        } : terminalMove;
      }
      terminalMove = {
        row: nextRow,
        col: nextCol,
        ...portalTransit ? {
          portalThrough: true,
          portalEntry: portalTransit.entry,
          portalExit: portalTransit.exit
        } : {}
      };
      if (portalExit) {
        const exitTarget = get(boardState, portalExit.row, portalExit.col);
        if ((!exitTarget || isGhostTransparentFor(attacker, exitTarget, "cardinal", boardState)) && !isWorkerCollapsedSquare(boardState, nextRow, nextCol) && !isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col)) {
          portalTransit = {
            entry: { row: nextRow, col: nextCol },
            exit: { ...portalExit }
          };
          currentRow = portalExit.row;
          currentCol = portalExit.col;
          continue;
        }
        return terminalMove;
      }
      currentRow = nextRow;
      currentCol = nextCol;
    }
    return terminalMove;
  }
  function grasshopperMoves(boardState, row, col, color) {
    const moves = [];
    queenDirections().forEach(([dr, dc]) => {
      let nextRow = row + dr;
      let nextCol = col + dc;
      while (inBounds(nextRow, nextCol, boardState) && !get(boardState, nextRow, nextCol)) {
        nextRow += dr;
        nextCol += dc;
      }
      const landingRow = nextRow + dr;
      const landingCol = nextCol + dc;
      if (!inBounds(landingRow, landingCol, boardState)) return;
      const target = get(boardState, landingRow, landingCol);
      if (!target || canWorkerCaptureTarget(color, target, get(boardState, row, col) || { type: "grasshopper", color }, boardState)) {
        moves.push({ row: landingRow, col: landingCol });
      }
    });
    return uniqueMoves(moves);
  }
  function jesterMoves(boardState, row, col, color) {
    const attacker = get(boardState, row, col) || { type: "jester", color };
    return uniqueMoves(rayMoves(boardState, row, col, color, queenDirections()).filter(({ row: nextRow, col: nextCol }) => {
      const target = workerMovementOccupant(boardState, row, col, nextRow, nextCol, color);
      return !target || target.color !== color && target.type !== "jester" && (isRoyalPiece(target) || isWorkerRegencyRoyalHeir(boardState, target) || target.type === "merchant") && canWorkerCaptureTarget(color, target, attacker, boardState);
    }));
  }
  function hookMoves(boardState, row, col, color) {
    const attacker = get(boardState, row, col) || { type: "hook", color };
    return uniqueMoves(hookPathMoves({
      row,
      col,
      rowCount: boardRowCount(boardState),
      colCount: boardColCount(boardState),
      portalRule: workerPortalRule(boardState),
      directions: orthogonals(),
      getOccupant: (targetRow2, targetCol2) => {
        if (boardState.pendingScarecrows?.some((e) => e.reserved && e.row === targetRow2 && e.col === targetCol2)) return { type: "wall", color };
        const target = get(boardState, targetRow2, targetCol2);
        return isStealthTransparentFor(attacker, target, boardState) ? null : target;
      },
      canCapture: (target) => canWorkerCaptureTarget(
        color,
        target,
        attacker,
        boardState
      ),
      isTransparent: (target) => isGhostTransparentFor(attacker, target, "hook", boardState),
      isPortalBlocked: (targetRow2, targetCol2) => isWorkerCollapsedSquare(boardState, targetRow2, targetCol2)
    }));
  }
  function timeTravelerMoves(boardState, row, col, piece) {
    const data = timeTravelerState(boardState);
    const canAttack = Boolean(data?.attackEnabledFor === piece.color);
    return queenDirections().map(([dr, dc]) => ({ row: row + dr, col: col + dc })).filter((move) => {
      if (!inBounds(move.row, move.col, boardState)) return false;
      const target = get(boardState, move.row, move.col);
      if (!target) return true;
      if (!canTimePhaseInteract(boardState, piece, target)) return false;
      return canAttack && canWorkerCaptureTarget(piece.color, target, piece, boardState);
    });
  }
  function footballMoves(boardState, row, col, color) {
    const moves = [];
    footballCornerSteps(boardState, row, col).forEach((square) => {
      if (!get(boardState, square.row, square.col) && !isWorkerCrownGroundSquare(boardState, square.row, square.col)) {
        moves.push({ row: square.row, col: square.col, footballCornerMove: true });
      }
    });
    queenDirections().forEach(([dr, dc]) => {
      const kickerRow = row - dr;
      const kickerCol = col - dc;
      const kicker = get(boardState, kickerRow, kickerCol);
      if (!kicker || kicker.color !== color || ["football", "wall"].includes(kicker.type) || isFrozenPiece(kicker) || isDiceLockedPiece(boardState, kicker)) return;
      let nextRow = row + dr;
      let nextCol = col + dc;
      while (inBounds(nextRow, nextCol, boardState)) {
        if (isWorkerCrownGroundSquare(boardState, nextRow, nextCol)) break;
        const target = get(boardState, nextRow, nextCol);
        if (!target) {
          moves.push({ row: nextRow, col: nextCol, footballKick: true, kicker: { row: kickerRow, col: kickerCol } });
        } else {
          if (canWorkerCaptureTarget(color, target, get(boardState, row, col) || { type: "football", color }, boardState)) {
            moves.push({ row: nextRow, col: nextCol, footballKick: true, footballCapture: true, kicker: { row: kickerRow, col: kickerCol } });
          }
          break;
        }
        nextRow += dr;
        nextCol += dc;
      }
    });
    return uniqueMoves(moves);
  }
  function isCornerSquare(boardState, row, col) {
    return (row === 0 || row === boardRowCount(boardState) - 1) && (col === 0 || col === boardColCount(boardState) - 1);
  }
  function footballCornerSteps(boardState, row, col) {
    if (!isCornerSquare(boardState, row, col)) return [];
    const rowStep = row === 0 ? 1 : -1;
    const colStep = col === 0 ? 1 : -1;
    return [
      { row: row + rowStep, col },
      { row, col: col + colStep },
      { row: row + rowStep, col: col + colStep }
    ].filter((square) => inBounds(square.row, square.col, boardState));
  }
  function socialistPawnMoves(boardState, row, col, color) {
    const moves = [];
    addWorkerPawnDirectionalMoves(boardState, moves, row, col, color, pawnDir(boardState, color), null);
    return moves;
  }
  function socialismColossusMoves(boardState, row, col, color) {
    const item = get(boardState, row, col);
    const dir = pawnDir(boardState, color);
    const moves = [];
    const anchorRow = row + dir;
    const forwardCells = colossusCells(anchorRow, col);
    if (forwardCells.length === 4) {
      const landingCaptures = workerColossusLandingCaptures(boardState, forwardCells, item, color);
      if (landingCaptures) {
        moves.push({ row: anchorRow, col, anchorRow, anchorCol: col, highlightCells: forwardCells, colossusLandingCaptures: landingCaptures, colossusMove: true });
      }
    }
    [-1, 1].forEach((dc) => {
      const attackCell = { row: row + (dir < 0 ? -1 : 2), col: col + (dc < 0 ? -1 : 2) };
      if (!inBounds(attackCell.row, attackCell.col, boardState)) return;
      const target = get(boardState, attackCell.row, attackCell.col);
      if (!canWorkerCaptureTarget(color, target, item, boardState)) return;
      const landingCells = colossusCells(row + dir, col + dc);
      if (landingCells.length !== 4) return;
      const landingCaptures = workerColossusLandingCaptures(boardState, landingCells, item, color);
      if (!landingCaptures) return;
      moves.push({ row: row + dir, col: col + dc, anchorRow: row + dir, anchorCol: col + dc, highlightCells: landingCells, sectorCells: [attackCell], colossusLandingCaptures: landingCaptures, colossusMove: true });
    });
    return uniqueMoves(moves);
  }
  function socialismBigRookMoves(boardState, row, col, color) {
    const item = get(boardState, row, col);
    if (!item || !["bigRook", "bigBishop"].includes(item.type)) return [];
    const dir = pawnDir(boardState, color);
    const moves = [];
    const forwardCells = colossusCells(row + dir, col);
    if (forwardCells.length === 4) {
      const landingCaptures = workerColossusLandingCaptures(boardState, forwardCells, item, color, septemberLargeCaptureLimit(item.type));
      if (landingCaptures && landingCaptures.length === 0) {
        moves.push({
          row: row + dir,
          col,
          anchorRow: row + dir,
          anchorCol: col,
          highlightCells: forwardCells,
          bigRookLandingCaptures: landingCaptures,
          bigRookMove: true
        });
      }
    }
    [-1, 1].forEach((dc) => {
      const attackCell = { row: row + (dir < 0 ? -1 : 2), col: col + (dc < 0 ? -1 : 2) };
      if (!inBounds(attackCell.row, attackCell.col, boardState)) return;
      const target = get(boardState, attackCell.row, attackCell.col);
      if (!canWorkerCaptureTarget(color, target, item, boardState)) return;
      const anchorRow = row + dir;
      const anchorCol = col + dc;
      const landingCells = colossusCells(anchorRow, anchorCol);
      if (landingCells.length !== 4) return;
      const landingCaptures = workerColossusLandingCaptures(boardState, landingCells, item, color, septemberLargeCaptureLimit(item.type));
      if (!landingCaptures || !landingCaptures.some((cell) => cell.row === attackCell.row && cell.col === attackCell.col)) return;
      moves.push({
        row: anchorRow,
        col: anchorCol,
        anchorRow,
        anchorCol,
        highlightCells: landingCells,
        sectorCells: [attackCell],
        bigRookLandingCaptures: landingCaptures,
        bigRookMove: true
      });
    });
    return uniqueMoves(moves);
  }
  function colossusAttackSectors(boardState, anchorRow, anchorCol, color) {
    const rowsLimit = Math.max(0, boardRowCount(boardState) - 2);
    const colsLimit = Math.max(0, boardColCount(boardState) - 2);
    const clampRowStart = (value) => Math.max(0, Math.min(rowsLimit, value));
    const clampColStart = (value) => Math.max(0, Math.min(colsLimit, value));
    const rowStart = clampRowStart(color === "white" ? anchorRow - 3 : anchorRow + 3);
    const rows = color === "white" ? [rowStart + 1, rowStart] : [rowStart, rowStart + 1];
    const leftStart = clampColStart(anchorCol - 3);
    const rightStart = clampColStart(anchorCol + 3);
    return [[rightStart, rightStart + 1], [leftStart, leftStart + 1]].map((cols) => [
      { row: rows[0], col: cols[0] },
      { row: rows[0], col: cols[1] },
      { row: rows[1], col: cols[0] },
      { row: rows[1], col: cols[1] }
    ]);
  }
  function wizardActions(boardState, wizard, row, col, color) {
    if (boardState.zugzwang?.[color]) return [];
    const mana = wizard.mana ?? 0;
    const actions = forcedRoyalSpellActions(boardState, wizard, row, col, color);
    if (mana >= 3) {
      const choices = [];
      for (let r = 0; r < Math.max(1, boardRowCount(boardState) - 1); r += 1) {
        for (let c = 0; c < Math.max(1, boardColCount(boardState) - 1); c += 1) {
          let hitsRoyal = false;
          const score = meteorCells(boardState, r, c).reduce((sum, cell) => {
            const item = get(boardState, cell.row, cell.col);
            if (!item || isFrozenPiece(item)) return sum;
            if (item.color === opponent(color) && isRoyalPiece(item)) hitsRoyal = true;
            return sum + (item.color === opponent(color) ? pieceValue(item) : -pieceValue(item) * 0.8);
          }, 0);
          if (hitsRoyal || score >= 650) choices.push({ row: r, col: c, score });
        }
      }
      choices.sort((a, b) => b.score - a.score).slice(0, 4).forEach((target) => {
        actions.push({ type: "wizardSpell", color, from: { row, col }, spellId: "meteor", target: { row: target.row, col: target.col } });
      });
    }
    if (mana >= 1) {
      enemyPieces(boardState, color).filter(({ piece }) => shouldWorkerLightningTarget(boardState, piece, wizard, mana, color)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece)).slice(0, 3).forEach((target) => {
        actions.push({ type: "wizardSpell", color, from: { row, col }, spellId: "lightning", target: { row: target.row, col: target.col } });
      });
    }
    if (mana >= 2) {
      friendlyPieces(boardState, color).filter((target) => shouldWorkerShieldTarget(boardState, target, color)).sort((a, b) => workerShieldTargetScore(boardState, b, color) - workerShieldTargetScore(boardState, a, color)).slice(0, 4).forEach((target) => {
        actions.push({ type: "wizardSpell", color, from: { row, col }, spellId: "shield", target: { row: target.row, col: target.col } });
      });
    }
    if (mana >= 5 && !boardState.skipTurn?.[opponent(color)]) {
      actions.push({ type: "wizardSpell", color, from: { row, col }, spellId: "timeStop", target: { row, col } });
    }
    return uniqueWizardActions(actions);
  }
  function forcedRoyalSpellActions(boardState, wizard, row, col, color) {
    const mana = wizard.mana ?? 0;
    if (mana < 1) return [];
    const actions = [];
    criticalPieces(boardState, opponent(color)).filter(({ piece }) => isRoyalPiece(piece)).forEach(({ piece, row: royalRow, col: royalCol }) => {
      const escapeSquares = royalEscapeSquares(boardState, piece, royalRow, royalCol, color);
      if (mana >= 1 && escapeSquares.length === 1) {
        actions.push({
          type: "wizardSpell",
          color,
          from: { row, col },
          spellId: "lightning",
          target: { row: escapeSquares[0].row, col: escapeSquares[0].col },
          forcedRoyalSpell: true
        });
      }
      if (mana >= 3) {
        meteorTrapTargets(boardState, escapeSquares, color).slice(0, 1).forEach((target) => {
          actions.push({
            type: "wizardSpell",
            color,
            from: { row, col },
            spellId: "meteor",
            target: { row: target.row, col: target.col },
            forcedRoyalSpell: true
          });
        });
      }
    });
    return uniqueWizardActions(actions);
  }
  function royalEscapeSquares(boardState, royal, row, col, attackerColor) {
    const squares = [{ row, col }];
    generateMovesForPiece(boardState, royal, row, col).filter((move) => Number.isInteger(move?.row) && Number.isInteger(move?.col)).filter((move) => !move.shotgunBlast && !move.shotgunSnipe && !move.colossusAttack && !move.merchantBuy && !move.setLogDirection).filter((move) => isWorkerMoveAllowed(boardState, royal, row, col, move)).forEach((move) => {
      if (!royalCanStandAt(boardState, royal, row, col, move.row, move.col, attackerColor)) return;
      squares.push({ row: move.row, col: move.col });
    });
    return uniqueSquares(squares);
  }
  function royalCanStandAt(boardState, royal, fromRow, fromCol, row, col, attackerColor) {
    if (!inBounds(row, col, boardState)) return false;
    const target = get(boardState, row, col);
    if (target?.color === royal.color) return false;
    if (target && !canWorkerCaptureTarget(royal.color, target, royal, boardState)) return false;
    const originalFrom = get(boardState, fromRow, fromCol);
    const originalTo = get(boardState, row, col);
    set(boardState, fromRow, fromCol, null);
    set(boardState, row, col, royal);
    const attacked = isSquareAttacked(boardState, row, col, attackerColor);
    set(boardState, fromRow, fromCol, originalFrom);
    set(boardState, row, col, originalTo);
    return !attacked;
  }
  function meteorTrapTargets(boardState, escapeSquares, color) {
    if (!escapeSquares.length || escapeSquares.length > 4) return [];
    const targets = [];
    for (let r = 0; r < Math.max(1, boardRowCount(boardState) - 1); r += 1) {
      for (let c = 0; c < Math.max(1, boardColCount(boardState) - 1); c += 1) {
        const cells = meteorCells(boardState, r, c);
        if (!cellsCoverSquares(cells, escapeSquares)) continue;
        if (cells.some((cell) => {
          const item = get(boardState, cell.row, cell.col);
          return item?.color === color && isRoyalPiece(item);
        })) continue;
        const score = cells.reduce((sum, cell) => {
          const item = get(boardState, cell.row, cell.col);
          if (!item || isFrozenPiece(item)) return sum;
          return sum + (item.color === opponent(color) ? pieceValue(item) : -pieceValue(item) * 0.8);
        }, 0);
        targets.push({ row: cells[0].row, col: cells[0].col, score });
      }
    }
    return targets.sort((a, b) => b.score - a.score);
  }
  function cellsCoverSquares(cells, squares) {
    const covered = new Set((cells || []).map((cell) => `${cell.row}:${cell.col}`));
    return (squares || []).every((square) => covered.has(`${square.row}:${square.col}`));
  }
  function uniqueSquares(squares) {
    const seen = /* @__PURE__ */ new Set();
    return (squares || []).filter((square) => {
      const key = `${square.row}:${square.col}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function uniqueWizardActions(actions) {
    const seen = /* @__PURE__ */ new Set();
    return (actions || []).filter((action) => {
      const key = `${action.spellId}:${action.from?.row},${action.from?.col}:${action.target?.row},${action.target?.col}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function shouldWorkerLightningTarget(boardState, piece, wizard, mana, color) {
    if (!piece || isFrozenPiece(piece) || !canWorkerCaptureTarget(color, piece, wizard, boardState)) return false;
    if (isCritical(piece)) return true;
    const value = pieceValue(piece);
    if (mana >= 5) return value >= 900;
    if (mana >= 3) return value >= 760;
    return value >= 650;
  }
  function shouldWorkerShieldTarget(boardState, target, color) {
    const piece = target.piece;
    if (!piece || piece.shielded || piece.type === "wall" || piece.type === "football" || isFrozenPiece(piece)) return false;
    if (isCritical(piece)) return true;
    return pieceValue(piece) >= 650 || isSquareAttacked(boardState, target.row, target.col, opponent(color));
  }
  function workerShieldTargetScore(boardState, target, color) {
    const piece = target.piece;
    let score = pieceValue(piece);
    if (isCritical(piece)) score += 1e4;
    if (isSquareAttacked(boardState, target.row, target.col, opponent(color))) score += 950;
    return score;
  }
  function wizardSpellCost(spellId) {
    return { lightning: 1, shield: 2, meteor: 3, timeStop: 5 }[spellId] || 99;
  }
  function wizardActionScore(boardState, action, color) {
    if (action.forcedRoyalSpell) return 8e4;
    if (action.spellId === "meteor") {
      return 700 + meteorCells(boardState, action.target.row, action.target.col).reduce((sum, cell) => {
        const item = get(boardState, cell.row, cell.col);
        if (!canWorkerCaptureTarget(color, item, { color }, boardState)) return sum;
        return sum + (item.color === opponent(color) ? pieceValue(item) * 1.1 : -pieceValue(item) * 0.75);
      }, 0);
    }
    const target = get(boardState, action.target?.row, action.target?.col);
    if (action.spellId === "lightning") return 520 + (target?.color === opponent(color) ? pieceValue(target) * 0.75 : -200);
    if (action.spellId === "shield") return 620 + (target?.color === color ? pieceValue(target) * 0.45 : 0) + (isCritical(target) ? 1100 : 0);
    if (action.spellId === "timeStop") return 1350 + royalPressureScore(boardState, color) * 0.25;
    return 80;
  }
  function meteorCells(boardState, row, col) {
    const startRow = Math.min(row, Math.max(0, boardRowCount(boardState) - 2));
    const startCol = Math.min(col, Math.max(0, boardColCount(boardState) - 2));
    return [
      { row: startRow, col: startCol },
      { row: startRow, col: startCol + 1 },
      { row: startRow + 1, col: startCol },
      { row: startRow + 1, col: startCol + 1 }
    ];
  }
  function enemyPieces(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === opponent(color));
  }
  function friendlyPieces(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color);
  }
  // Migrated to forEachPieceCached (grafted from engine.optimized.js): this
  // is the single shared chokepoint for a large number of call sites
  // (findWorkerKingRole, enemyPieces/friendlyPieces, etc.), so caching here
  // benefits all of them without auditing each caller individually. Purely
  // a perf swap -- forEachPieceCached iterates the exact same
  // (piece, row, col) triples as forEachPiece, just from a cached list.
  function piecesMatching(boardState, predicate) {
    const pieces = [];
    forEachPieceCached(boardState, (piece, row, col) => {
      if (predicate(piece, row, col)) pieces.push({ piece, row, col });
    });
    return pieces;
  }
  function workerImmediateCastlingPlans(boardState, color) {
    if (!["white", "black"].includes(color) || boardState.castlingCanceled?.[color]) return [];
    const king = findWorkerKingRole(boardState, color);
    if (!king) return [];
    const plans = [];
    queenDirections().forEach(([dr, dc]) => {
      for (let distance = 3; ; distance += 1) {
        const row = king.row + dr * distance;
        const col = king.col + dc * distance;
        if (!inBounds(row, col, boardState)) break;
        const target = get(boardState, row, col);
        if (target?.color !== color || target.type !== "rook") continue;
        plans.push({
          king,
          rook: { piece: target, row, col },
          kingTo: { row: king.row + dr * 2, col: king.col + dc * 2 },
          rookTo: { row: king.row + dr, col: king.col + dc }
        });
      }
    });
    return plans;
  }
  function workerEvasionEligiblePieces(boardState, color, options = {}) {
    const requireUnmarked = options.requireUnmarked !== false;
    return piecesMatching(boardState, (piece) => piece.color === color && !["wall", "football", "blackHole", "scarecrow"].includes(piece.type) && (!requireUnmarked || !piece.evasion));
  }
  function strongestFriendly(boardState, color) {
    return friendlyPieces(boardState, color).filter(({ piece }) => piece.type !== "wall" && piece.type !== "football").sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece))[0] || null;
  }
  function weakestEnemyPiece(boardState, color) {
    const eligibleEnemies = enemyPieces(boardState, color).filter(({ piece }) => !usesSeptember18Balance(boardState) || piece.moved === true);
    const ordinary = eligibleEnemies.filter(({ piece }) => !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "football", "colossus"].includes(piece.type));
    if (!ordinary.length) return eligibleEnemies.find(({ piece }) => piece.type === "king") || eligibleEnemies.find(({ piece }) => isWorkerRoyalIdentityPiece(boardState, piece)) || null;
    return ordinary.filter(({ piece }) => Number.isFinite(pieceCombatValue(piece, boardState))).sort((a, b) => {
      const pawnBias = (a.piece.type === "pawn" ? -1e3 : 0) - (b.piece.type === "pawn" ? -1e3 : 0);
      return pawnBias || pieceCombatValue(a.piece, boardState) - pieceCombatValue(b.piece, boardState);
    })[0] || null;
  }
  function bestWorkerDiceLock(boardState, color) {
    const groups = /* @__PURE__ */ new Map();
    piecesMatching(boardState, (piece) => piece.color === color && !["wall", "football"].includes(piece.type)).forEach(({ piece }) => {
      const type = isCritical(piece) ? "king" : piece.type;
      const entry = groups.get(type) || { type, value: 0, count: 0 };
      entry.value += pieceValue(piece);
      entry.count += 1;
      groups.set(type, entry);
    });
    return [...groups.values()].sort((a, b) => b.value - a.value || b.count - a.count)[0] || null;
  }
  function uniqueCells(cells) {
    const seen = /* @__PURE__ */ new Set();
    return cells.filter((cell) => {
      const key = `${cell.row},${cell.col}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function isWorkerVanguardPawn(boardState, piece, row) {
    return Boolean(boardState.vanguard?.[piece?.color] && septemberVanguardPawn(piece, row, boardState.board.flatMap((line, r) => line.filter((item) => item?.type === "pawn").map((item) => ({ color: item.color, row: r })))));
  }
  function addWorkerInternalPawnMoves(boardState, moves, pawn, row, col, color) {
    if (pawn?.type !== "pawn") return;
    const direction = pawnDir(boardState, color), to = row + direction, target = get(boardState, to, col);
    if (boardState.proficiency?.[color] && row === (color === "white" ? 1 : boardRowCount(boardState) - 2) && target && canWorkerCaptureTarget(color, target, pawn, boardState) && !moves.some((move) => move.row === to && move.col === col)) moves.push({ row: to, col, capture: true });
    if (boardState.longEnPassant?.[color]) {
      const rights = enPassantRightEntries(boardState.enPassant);
      const extra = longEnPassantCandidates({
        from: { row, col },
        color,
        direction,
        rights,
        at: (r, c) => get(boardState, r, c),
        canCapture: (victim) => canWorkerCaptureTarget(color, victim, pawn, boardState)
      });
      for (const move of extra) {
        for (let i = moves.length - 1; i >= 0; i--) if (moves[i].row === move.row && moves[i].col === move.col) moves.splice(i, 1);
        moves.push(move);
      }
    }
  }
  function pawnMoves(boardState, row, col, color, movementPiece = null) {
    const moves = basePawnMoves(boardState, row, col, color, movementPiece);
    const piece = movementPiece || get(boardState, row, col);
    if (piece?.type === "pawn" && !boardState.pawnConversion?.[color] && !hasWorkerAdjacentKnightmaster(boardState, row, col, color)) {
      const landingRow = row + pawnDir(boardState, color);
      for (const dc of [-1, 1]) {
        const landingCol = col + dc;
        if (!inBounds(landingRow, landingCol, boardState)) continue;
        const right = enPassantRightEntries(boardState.enPassant).find((entry) => entry.color !== color && entry.row === landingRow && entry.col === landingCol && get(boardState, entry.capturedRow, entry.capturedCol)?.type === "pawn" && get(boardState, entry.capturedRow, entry.capturedCol).color === opponent(color));
        const side = get(boardState, row, landingCol), frenzy = !right && boardState.enPassantFrenzy?.[color] && side?.color === opponent(color);
        if (!right && !frenzy) continue;
        const capturedRow = right?.capturedRow ?? row, capturedCol = right?.capturedCol ?? landingCol;
        for (let i = moves.length - 1; i >= 0; i--) if (moves[i].row === landingRow && moves[i].col === landingCol) moves.splice(i, 1);
        const landing = get(boardState, landingRow, landingCol);
        if (!canWorkerCaptureTarget(color, get(boardState, capturedRow, capturedCol), piece, boardState) || landing && !canWorkerCaptureTarget(color, landing, piece, boardState)) continue;
        moves.push({ row: landingRow, col: landingCol, enPassant: true, capturedRow, capturedCol, ...frenzy ? { enPassantFrenzy: true } : {} });
      }
    }
    addWorkerInternalPawnMoves(boardState, moves, piece, row, col, color);
    if (!isWorkerVanguardPawn(boardState, piece, row)) return moves;
    const dir = pawnDir(boardState, color);
    for (const extra of leapMoves(boardState, row, col, color, [[dir, -1], [dir, 0], [dir, 1]])) {
      if (moves.some((move) => move.row === extra.row && move.col === extra.col)) continue;
      const adjacent = get(boardState, row, extra.col);
      const specialCapture = extra.col !== col && adjacent?.color === opponent(color) && (boardState.enPassantFrenzy?.[color] || adjacent.type === "pawn" && enPassantRightEntries(boardState.enPassant).some((entry) => entry.color !== color && entry.row === extra.row && entry.col === extra.col));
      if (!specialCapture) moves.push(extra);
    }
    return moves;
  }
  function basePawnMoves(boardState, row, col, color, movementPiece = null) {
    const moves = [];
    const piece = movementPiece || get(boardState, row, col);
    const movementTarget = (targetRow2, targetCol2) => {
      const target = get(boardState, targetRow2, targetCol2);
      return isStealthTransparentFor(piece, target, boardState) ? null : target;
    };
    const dir = pawnDir(boardState, color);
    if (piece?.type === "pawn" && hasWorkerAdjacentKnightmaster(boardState, row, col, color)) {
      const knightmasterMoves = leapMoves(boardState, row, col, color, workerKnightDeltasForMove(boardState, row, col, color));
      if (hasWorkerCaptureReadyAdjacentKnightmaster(boardState, row, col, color)) return knightmasterMoves;
      return knightmasterMoves.filter((move) => !isWorkerCaptureMove(boardState, piece, move));
    }
    if (piece?.type === "pawn" && boardState.pawnConversion?.[color]) {
      addWorkerConvertedPawnDirectionalMoves(boardState, moves, row, col, color, dir, workerPawnDoubleStepRows(boardState, color));
      if (boardState.retreat?.[color]) addWorkerConvertedPawnDirectionalMoves(boardState, moves, row, col, color, -dir, []);
      if (pieceHasAbility(piece, "standardBearer") || piece?.type === "pawn" && hasSameRankStandardBearer(boardState, row, color)) {
        const captureReady = hasWorkerCaptureReadySameRankStandardBearer(boardState, row, color);
        [-1, 1].forEach((dc) => {
          const nextCol = col + dc;
          const target = movementTarget(row, nextCol);
          if (inBounds(row, nextCol) && (!target || captureReady && canWorkerCaptureTarget(color, target, piece, boardState))) moves.push({ row, col: nextCol });
        });
      }
      return moves;
    }
    const one = row + dir;
    const two = row + dir * 2;
    if (inBounds(one, col)) {
      const forwardTarget = movementTarget(one, col);
      const forwardCrown = isWorkerCrownGroundSquare(boardState, one, col);
      if (!forwardTarget) {
        moves.push({ row: one, col, ...forwardCrown ? { crownGroundCapture: true } : {} });
        if (!forwardCrown) {
          const doubleStepRows = workerPawnDoubleStepRows(boardState, color);
          const firstStepPortalExit = workerPortalExitAt(boardState, one, col);
          if (!piece?.moved && doubleStepRows.includes(row) && firstStepPortalExit) {
            const portalDestination = { row: firstStepPortalExit.row + dir, col: firstStepPortalExit.col };
            if (!movementTarget(firstStepPortalExit.row, firstStepPortalExit.col) && inBounds(portalDestination.row, portalDestination.col, boardState) && !isWorkerCollapsedSquare(boardState, portalDestination.row, portalDestination.col) && !movementTarget(portalDestination.row, portalDestination.col) && !isWorkerCrownGroundSquare(boardState, portalDestination.row, portalDestination.col)) {
              moves.push({ row: portalDestination.row, col: portalDestination.col, pawnPortalDoubleStep: true, portalThrough: true, portalEntry: { row: one, col }, portalExit: firstStepPortalExit });
            }
          } else if (!piece?.moved && doubleStepRows.includes(row) && inBounds(two, col, boardState)) {
            const twoTarget = movementTarget(two, col);
            if (!twoTarget && !isWorkerCrownGroundSquare(boardState, two, col)) {
              moves.push({ row: two, col, standardPawnDoubleStep: true });
              const three = row + dir * 3;
              if (boardState.pawnSprint?.[color] && piece?.type === "pawn" && inBounds(three, col, boardState) && !movementTarget(three, col) && !isWorkerCrownGroundSquare(boardState, three, col)) {
                moves.push({ row: three, col, pawnSprintTripleStep: true });
              }
            }
          }
          if (boardState.pawnSprint?.[color] && piece?.type === "pawn" && piece.londonSystemPawn && row === (color === "white" ? boardRowCount(boardState) - 2 : 1)) {
            for (let distance = 2; distance <= 3; distance += 1) {
              const targetRow2 = row + dir * distance;
              if (!inBounds(targetRow2, col, boardState) || movementTarget(targetRow2, col) || isWorkerCrownGroundSquare(boardState, targetRow2, col)) break;
              if (!moves.some((move) => move.row === targetRow2 && move.col === col)) {
                moves.push({
                  row: targetRow2,
                  col,
                  ...distance === 3 ? { pawnSprintTripleStep: true } : {}
                });
              }
            }
          }
        }
      } else {
        if (boardState.breakthroughPawns?.[color] && canWorkerCaptureTarget(color, forwardTarget, piece, boardState)) {
          moves.push({ row: one, col });
        }
        if (boardState.pawnLeap?.[color] && piece?.type === "pawn" && forwardTarget?.color === opponent(color) && forwardTarget.type === "pawn" && inBounds(two, col, boardState) && !movementTarget(two, col) && !isWorkerCrownGroundSquare(boardState, two, col)) {
          moves.push({ row: two, col, pawnLeap: true });
        }
      }
    }
    if (pieceHasAbility(piece, "missionary")) return;
    [-1, 1].forEach((dc) => {
      const nextCol = col + dc;
      const target = movementTarget(one, nextCol);
      if (!inBounds(one, nextCol)) return;
      if (isWorkerCrownGroundSquare(boardState, one, nextCol) && piece?.type !== "pawn") moves.push({ row: one, col: nextCol, crownGroundCapture: true });
      else if (canWorkerCaptureTarget(color, target, piece, boardState)) moves.push({ row: one, col: nextCol });
    });
    if (boardState.retreat?.[color]) {
      const back = row - dir;
      if (inBounds(back, col, boardState)) {
        const backTarget = movementTarget(back, col);
        if (!backTarget && !isWorkerCrownGroundSquare(boardState, back, col)) {
          moves.push({ row: back, col });
        } else if (boardState.breakthroughPawns?.[color] && canWorkerCaptureTarget(color, backTarget, piece, boardState)) {
          moves.push({ row: back, col });
        }
      }
      [-1, 1].forEach((dc) => {
        const targetCol2 = col + dc;
        const target = movementTarget(back, targetCol2);
        if (!inBounds(back, targetCol2, boardState)) return;
        if (isWorkerCrownGroundSquare(boardState, back, targetCol2) && piece?.type !== "pawn") {
          moves.push({ row: back, col: targetCol2, crownGroundCapture: true });
        } else if (canWorkerCaptureTarget(color, target, piece, boardState)) {
          moves.push({ row: back, col: targetCol2 });
        }
      });
    }
    if (pieceHasAbility(piece, "standardBearer") || piece?.type === "pawn" && hasSameRankStandardBearer(boardState, row, color)) {
      const captureReady = hasWorkerCaptureReadySameRankStandardBearer(boardState, row, color);
      [-1, 1].forEach((dc) => {
        const nextCol = col + dc;
        const target = movementTarget(row, nextCol);
        if (inBounds(row, nextCol) && (!target || captureReady && canWorkerCaptureTarget(color, target, piece, boardState))) moves.push({ row, col: nextCol });
      });
    }
    workerEnPassantMoves(boardState, row, col, color, piece).forEach((move) => moves.push(move));
    workerEnPassantFrenzyMoves(boardState, row, col, color).forEach((move) => moves.push(move));
    if (piece?.chargeRush && piece.type === "pawn") {
      const two2 = row + dir * 2;
      const three = row + dir * 3;
      if (inBounds(two2, col, boardState) && !movementTarget(two2, col) && !isWorkerCrownGroundSquare(boardState, two2, col) && inBounds(three, col, boardState) && !movementTarget(three, col) && !isWorkerCrownGroundSquare(boardState, three, col) && !moves.some((move) => move.row === three && move.col === col)) {
        moves.push({ row: three, col, chargeRush: true });
      }
    }
    return moves;
  }
  function fanaticMoves(boardState, row, col, color) {
    const moves = [];
    const piece = get(boardState, row, col);
    const dir = pawnDir(boardState, color);
    const firstRow = row + dir;
    if (!inBounds(firstRow, col, boardState)) return moves;
    const firstTarget = get(boardState, firstRow, col);
    if (firstTarget) {
      if (canWorkerCaptureTarget(color, firstTarget, piece, boardState)) moves.push({ row: firstRow, col });
      return moves;
    }
    moves.push({ row: firstRow, col });
    const continuation = portalRayContinuation(
      workerPortalRule(boardState),
      firstRow,
      col,
      dir,
      0,
      boardRowCount(boardState),
      boardColCount(boardState)
    );
    if (continuation) {
      const { entry, exit, next } = continuation;
      const target = get(boardState, next.row, next.col);
      if (inBounds(next.row, next.col, boardState) && !get(boardState, exit.row, exit.col) && !isWorkerCollapsedSquare(boardState, entry.row, entry.col) && !isWorkerCollapsedSquare(boardState, exit.row, exit.col) && !isWorkerCollapsedSquare(boardState, next.row, next.col) && (!target || canWorkerCaptureTarget(color, target, piece, boardState))) {
        moves.push({
          row: next.row,
          col: next.col,
          portalThrough: true,
          portalEntry: entry,
          portalExit: exit
        });
      }
      return moves;
    }
    const secondRow = row + dir * 2;
    if (inBounds(secondRow, col, boardState)) {
      const target = get(boardState, secondRow, col);
      if (!target || canWorkerCaptureTarget(color, target, piece, boardState)) {
        moves.push({ row: secondRow, col });
      }
    }
    return moves;
  }
  function addWorkerConvertedPawnDirectionalMoves(boardState, moves, row, col, color, dir, startRows = []) {
    const piece = get(boardState, row, col) || { color, type: "pawn" };
    const movementTarget = (targetRow2, targetCol2) => {
      const target = get(boardState, targetRow2, targetCol2);
      return isStealthTransparentFor(piece, target, boardState) ? null : target;
    };
    const one = row + dir;
    if (!inBounds(one, col, boardState)) return;
    const doubleStepRows = Array.isArray(startRows) ? startRows : [startRows].filter((entry) => entry !== null && entry !== void 0);
    [-1, 1].forEach((dc) => {
      const oneCol = col + dc;
      if (!inBounds(one, oneCol, boardState)) return;
      if (isWorkerCrownGroundSquare(boardState, one, oneCol)) return;
      if (movementTarget(one, oneCol)) return;
      moves.push({ row: one, col: oneCol });
      if (!piece?.moved && doubleStepRows.includes(row)) {
        const two = row + dir * 2;
        const twoCol = col + dc * 2;
        if (inBounds(two, twoCol, boardState) && !movementTarget(two, twoCol) && !isWorkerCrownGroundSquare(boardState, two, twoCol)) {
          moves.push({ row: two, col: twoCol });
          const three = row + dir * 3;
          const threeCol = col + dc * 3;
          if (boardState.pawnSprint?.[color] && piece?.type === "pawn" && inBounds(three, threeCol, boardState) && !movementTarget(three, threeCol) && !isWorkerCrownGroundSquare(boardState, three, threeCol)) {
            moves.push({ row: three, col: threeCol });
          }
        }
      }
      if (boardState.pawnSprint?.[color] && piece?.type === "pawn" && piece.londonSystemPawn && row === (color === "white" ? boardRowCount(boardState) - 2 : 1) && dir === pawnDir(boardState, color)) {
        for (let distance = 2; distance <= 3; distance += 1) {
          const targetRow2 = row + dir * distance;
          const targetCol2 = col + dc * distance;
          if (!inBounds(targetRow2, targetCol2, boardState) || movementTarget(targetRow2, targetCol2) || isWorkerCrownGroundSquare(boardState, targetRow2, targetCol2)) break;
          if (!moves.some((move) => move.row === targetRow2 && move.col === targetCol2)) {
            moves.push({ row: targetRow2, col: targetCol2 });
          }
        }
      }
    });
    const forwardTarget = movementTarget(one, col);
    if (isWorkerCrownGroundSquare(boardState, one, col)) {
      moves.push({ row: one, col, crownGroundCapture: true });
    } else if (canWorkerCaptureTarget(color, forwardTarget, piece, boardState)) {
      moves.push({ row: one, col });
    }
  }
  function workerEnPassantMoves(boardState, row, col, color, movementPiece = null) {
    const enPassantStates = enPassantRightEntries(boardState?.enPassant).filter((entry) => entry.color !== color);
    if (!enPassantStates.length) return [];
    const piece = movementPiece || get(boardState, row, col);
    const dir = pawnDir(boardState, color);
    const targetRow2 = row + dir;
    const moves = [];
    [-1, 1].forEach((dc) => {
      const targetCol2 = col + dc;
      if (!inBounds(targetRow2, targetCol2, boardState)) return;
      const enPassant = enPassantStates.find((entry) => entry.row === targetRow2 && entry.col === targetCol2 && entry.capturedCol === targetCol2);
      if (!enPassant) return;
      if (get(boardState, targetRow2, targetCol2) && !canWorkerCaptureTarget(color, get(boardState, targetRow2, targetCol2), piece, boardState)) return;
      const captured = get(boardState, enPassant.capturedRow, enPassant.capturedCol);
      if (!captured || captured.type !== "pawn" || !canWorkerCaptureTarget(color, captured, piece, boardState)) return;
      moves.push({
        row: targetRow2,
        col: targetCol2,
        enPassant: true,
        capturedRow: enPassant.capturedRow,
        capturedCol: enPassant.capturedCol
      });
    });
    return moves;
  }
  function workerEnPassantFrenzyMoves(boardState, row, col, color) {
    if (!boardState?.enPassantFrenzy?.[color]) return [];
    const piece = get(boardState, row, col);
    if (!piece || piece.type !== "pawn") return [];
    const dir = pawnDir(boardState, color);
    const landingRow = row + dir;
    const moves = [];
    [-1, 1].forEach((dc) => {
      const capturedCol = col + dc;
      if (!inBounds(row, capturedCol, boardState) || !inBounds(landingRow, capturedCol, boardState)) return;
      if (get(boardState, landingRow, capturedCol) && !canWorkerCaptureTarget(color, get(boardState, landingRow, capturedCol), piece, boardState)) return;
      const target = get(boardState, row, capturedCol);
      if (!canWorkerCaptureTarget(color, target, piece, boardState)) return;
      moves.push({
        row: landingRow,
        col: capturedCol,
        enPassant: true,
        enPassantFrenzy: true,
        capturedRow: row,
        capturedCol
      });
    });
    return moves;
  }
  function workerHasForcedEnPassant(boardState, color) {
    if (!boardState?.machoChess || boardState.pawnConversion?.[color] || !boardState.enPassant || boardState.enPassant.color === color) return false;
    let forced = false;
    forEachPiece(boardState, (piece, row, col) => {
      if (forced || piece?.color !== color || piece.type !== "pawn") return;
      forced = workerEnPassantMoves(boardState, row, col, color).some((move) => isWorkerBaseMoveAllowed(boardState, piece, row, col, move));
    });
    return forced;
  }
  function updateWorkerEnPassantAfterMove(boardState, piece, from, move, movedAsType) {
    const nextEnPassant = pawnEnPassantRightAfterAdvance(
      piece,
      from,
      { row: move.row ?? from.row, col: move.col ?? from.col },
      move,
      movedAsType
    );
    boardState.enPassant = nextEnPassant || retainSameTurnEnPassant(boardState.enPassant, piece.color);
  }
  function workerPawnDoubleStepRows(boardState, color) {
    const rows = color === "white" ? [boardRowCount(boardState) - 2, boardRowCount(boardState) - 1] : [1, 0];
    return [...new Set(rows)].filter((row) => inBounds(row, 0, boardState));
  }
  function pawnDir(boardState, color) {
    const reverse = Number(boardState?.effects?.pawnReverse?.[color]) > 0;
    return (color === "white" ? -1 : 1) * (reverse ? -1 : 1);
  }
  function addWorkerPawnDirectionalMoves(boardState, moves, row, col, color, dir, startRow = null) {
    const pawn = get(boardState, row, col) || { color, type: "pawn" };
    const one = row + dir;
    const two = row + dir * 2;
    if (inBounds(one, col, boardState)) {
      const forwardTarget = workerMovementOccupant(boardState, row, col, one, col, color);
      const forwardCrown = isWorkerCrownGroundSquare(boardState, one, col);
      if (!forwardTarget && (!forwardCrown || pawn?.type === "pawn")) {
        moves.push({ row: one, col, ...forwardCrown ? { crownGroundCapture: true } : {} });
        if (startRow !== null && row === startRow && inBounds(two, col, boardState) && !workerMovementOccupant(boardState, row, col, two, col, color) && !isWorkerCrownGroundSquare(boardState, two, col)) {
          if (!forwardCrown) moves.push({ row: two, col });
        }
      } else if (boardState.breakthroughPawns?.[color] && canWorkerCaptureTarget(color, forwardTarget, pawn, boardState)) {
        moves.push({ row: one, col });
      }
    }
    [-1, 1].forEach((dc) => {
      const nextCol = col + dc;
      if (!inBounds(one, nextCol, boardState)) return;
      const target = workerMovementOccupant(boardState, row, col, one, nextCol, color);
      if (isWorkerCrownGroundSquare(boardState, one, nextCol) && pawn?.type !== "pawn") moves.push({ row: one, col: nextCol, crownGroundCapture: true });
      else if (canWorkerCaptureTarget(color, target, pawn, boardState)) moves.push({ row: one, col: nextCol });
    });
  }
  // Grafted from engine.optimized.js (our-only addition): caches the
  // per-color rank->standardBearer-pieces mapping once per boardState
  // (safe: every place that changes a position clones a new state object
  // first, so a given boardState's aura-relevant pieces never change after
  // this cache is built). Pure perf swap, same as forEachPieceCached above
  // -- profiling found forEachPiece a major cost partly from repeated scans
  // like the one hasSameRankStandardBearer used to run inline here.
  const STANDARD_BEARER_RANK_CACHE = /* @__PURE__ */ new WeakMap();
  function standardBearerRanksByColor(boardState) {
    let cached = STANDARD_BEARER_RANK_CACHE.get(boardState);
    if (cached) return cached;
    cached = { white: /* @__PURE__ */ new Map(), black: /* @__PURE__ */ new Map() };
    forEachPiece(boardState, (piece, row) => {
      if (!piece || !COLORS.includes(piece.color) || !pieceHasAbility(piece, "standardBearer")) return;
      const byRank = cached[piece.color];
      const list = byRank.get(row);
      if (list) list.push(piece);
      else byRank.set(row, [piece]);
    });
    STANDARD_BEARER_RANK_CACHE.set(boardState, cached);
    return cached;
  }
  function hasSameRankStandardBearer(boardState, row, color) {
    return Boolean(standardBearerRanksByColor(boardState)[color]?.get(row)?.length);
  }
  function workerPortalRule(boardState) {
    return normalizePortalRule(boardState?.portalRule, boardRowCount(boardState), boardColCount(boardState));
  }
  function workerPortalExitAt(boardState, row, col) {
    return portalCounterpartCell(boardState?.portalRule, row, col, boardRowCount(boardState), boardColCount(boardState));
  }
  function workerPortalMoveDestination(move) {
    if (move?.portalLanding && Number.isInteger(move.portalExit?.row) && Number.isInteger(move.portalExit?.col)) {
      return { row: move.portalExit.row, col: move.portalExit.col };
    }
    if (Number.isInteger(move?.anchorRow) && Number.isInteger(move?.anchorCol)) {
      return { row: move.anchorRow, col: move.anchorCol };
    }
    return Number.isInteger(move?.row) && Number.isInteger(move?.col) ? { row: move.row, col: move.col } : null;
  }
  function workerActionDestination(action) {
    return workerPortalMoveDestination(action?.move || {});
  }
  function workerPortalCaptureCells(move) {
    const cells = [];
    if (move?.portalLanding && Number.isInteger(move.portalEntry?.row) && Number.isInteger(move.portalEntry?.col)) cells.push(move.portalEntry);
    const destination = workerPortalMoveDestination(move);
    if (destination) cells.push(destination);
    return uniqueCells(cells);
  }
  function workerMistakeCaptureCandidate(boardState, piece, move, portalEntryTarget, target) {
    if (!(boardState.mistakeRule || boardState.mistakeCard?.[piece?.color]) || !piece || move?.merchantBuy || move?.colossusAttack || move?.shotgunBlast || move?.shotgunSnipe) return null;
    const candidates = [];
    if (portalEntryTarget && move.portalEntry) {
      candidates.push({ piece: portalEntryTarget, row: move.portalEntry.row, col: move.portalEntry.col, kind: "portal" });
    }
    if (target && Number.isInteger(move?.row) && Number.isInteger(move?.col)) {
      candidates.push({ piece: target, row: move.row, col: move.col, kind: "landing" });
    }
    if (Number.isInteger(move?.capturedRow) && Number.isInteger(move?.capturedCol)) {
      candidates.push({ piece: get(boardState, move.capturedRow, move.capturedCol), row: move.capturedRow, col: move.capturedCol, kind: "en-passant" });
    }
    if (move?.jumpCapture) {
      candidates.push({ piece: get(boardState, move.jumpCapture.row, move.jumpCapture.col), row: move.jumpCapture.row, col: move.jumpCapture.col, kind: "jump" });
    }
    return candidates.find((candidate) => {
      const targetPiece = candidate.piece;
      if (!isMistakeReversalTarget(targetPiece) || targetPiece.color === piece.color) return false;
      if (candidate.kind === "jump" && !move.checkerCapture) {
        return canWorkerRadicalChargeCaptureTarget(boardState, piece.color, targetPiece, piece);
      }
      return canWorkerCaptureTarget(piece.color, targetPiece, piece, boardState, {
        allowBasicTrainingCapture: Boolean(move.basicTrainingCapture)
      });
    }) || null;
  }
  function workerMistakeTriggers(boardState, piece, from, move, candidate) {
    const seed = [
      boardState.moveCount || 0,
      boardState.fullMove || 1,
      piece?.id || piece?.type || "piece",
      from?.row,
      from?.col,
      move?.row,
      move?.col,
      candidate?.piece?.id || candidate?.piece?.type || "target",
      candidate?.row,
      candidate?.col
    ].join(":");
    const chance = boardState.mistakeCard?.[piece?.color] ? MISTAKE_CARD_CHANCE : void 0;
    return mistakeRollTriggers(workerStableIndex(seed, 1e4) / 1e4, chance);
  }
  function resolveWorkerMistakeReversal(boardState, piece, from, candidate, color, aiColor) {
    const counter = candidate?.piece;
    if (!piece || !counter || get(boardState, candidate.row, candidate.col) !== counter) return { ok: false, score: 0 };
    const counterMovedAsType = counter.type;
    let score = (counter.color === aiColor ? 1 : -1) * pieceValue(piece) * 1.8;
    if (isWorkerDecisiveCaptureTarget(boardState, piece)) {
      score += workerCriticalCaptureScore(boardState, piece, counter.color, aiColor);
    }
    clearPieceCells(boardState, piece);
    recordWorkerCapturedPieces(boardState, counter.color, [piece]);
    recordWorkerDirectCaptures(boardState, counter, [piece]);
    workerLearnImperialStudyMovement(boardState, counter, [piece]);
    clearPieceCells(boardState, counter);
    set(boardState, from.row, from.col, counter);
    counter.moved = true;
    delete counter.quantum;
    if (boardState.monochromeChess) counter.monoShade = squareShade(from.row, from.col);
    trackWorkerMovingProgress(boardState, counter);
    noteWorkerUltimatumMovement(boardState, counter);
    score += resolveWorkerReaperNearbyDeaths(
      boardState,
      [{ item: piece, row: from.row, col: from.col }],
      aiColor,
      { activePiece: counter, capturedBy: counter.color }
    );
    const reaperExecution = consumeWorkerReaperExecution(boardState, counter);
    if (boardState.afterimageQueen?.[counter.color] && counterMovedAsType === "queen" && isWorkerQueenIdentity(counter, counter.color) && isWorkerQueenIdentity(piece, piece.color) && !get(boardState, candidate.row, candidate.col)) {
      const afterimage = workerCreatePiece(counter.color, "queen", boardState, candidate.row, candidate.col);
      afterimage.moved = true;
      set(boardState, candidate.row, candidate.col, afterimage);
    }
    if (counterMovedAsType === "vampireLord" && grantWorkerBloodCard(boardState, counter.color)) {
      score += counter.color === aiColor ? 260 : -260;
    }
    if (counter.type === "windmill") counter.windmillMode = counter.windmillMode === "rook" ? "bishop" : "rook";
    const counterPromotionDue = ["pawn", "squire", "standardBearer"].includes(counter.type) && isPromotionRow(counter, from.row, boardState);
    if (counter.type === "squire" && !counterPromotionDue) {
      counter.type = monochromePieceType("knight", boardState.monochromeChess);
    }
    if (counter.chameleon && !isWorkerRoyalIdentityPiece(boardState, piece) && !["wall", "colossus", "bigRook", "bigBishop"].includes(piece.type)) {
      if (isWorkerNativeKing(counter) || counter.editorRoyal) counter.crownRoyal = true;
      counter.type = monochromePieceType(piece.type, boardState.monochromeChess);
    }
    if (counterPromotionDue) {
      workerAutoPromotePawnStormPiece(boardState, counter, from.row);
    }
    workerCrownCheckerIfNeeded(boardState, counter, from.row);
    if (boardState.mode !== "gameover") {
      const trojanResult = resolveWorkerTrojanHorseRetaliation(
        boardState,
        counter,
        [{ item: piece, row: from.row, col: from.col }],
        aiColor
      );
      score += trojanResult.score;
    }
    if (boardState.mode !== "gameover" && piece.explosive) {
      score += applyWorkerExplosionsForEntries(boardState, [{ item: piece, row: from.row, col: from.col }], aiColor, /* @__PURE__ */ new Set([piece]));
    }
    boardState.enPassant = null;
    resolveWorkerRuleBombsUnderPieces(boardState, counter.color);
    if (boardState.mode !== "gameover" && !counterPromotionDue && get(boardState, from.row, from.col) === counter) {
      const previousValue = pieceValue(counter);
      if (applyWorkerTranscendence(boardState, counter, counterMovedAsType, from.row, from.col, `${boardState.moveCount || 0}:mistake`)) {
        const gain = Math.max(0, pieceValue(counter) - previousValue) * 0.8;
        score += counter.color === aiColor ? gain : -gain;
      }
    }
    if (boardState.mode !== "gameover") finishWorkerMove(boardState, color);
    pushRecentMove(boardState, {
      from: { row: candidate.row, col: candidate.col },
      to: reaperExecution || { row: from.row, col: from.col },
      pieceId: counter.id || "",
      pieceType: counter.type || "",
      color: counter.color
    });
    return { ok: true, score };
  }
  function isWorkerPortalEligibleMove(piece, move) {
    if (!piece || workerIsLargePiece(piece) || ["football", "wall", "blackHole"].includes(piece.type)) return false;
    return !(move?.portalThrough || move?.primeMinisterPortalEntry || move?.primeMinisterPortalSecondEntry || move?.castle || move?.dragonSwap || move?.switcherooMove || move?.substitutionSwap || move?.colossusBody || move?.colossusMove || move?.colossusAttack || move?.bigRookMove || move?.siegeRamMove || move?.shotgunBlast || move?.shotgunSnipe || move?.merchantBuy || move?.setLogDirection || move?.footballKick || move?.footballCornerMove);
  }
  function isWorkerPortalDirectCaptureAllowed(boardState, piece, target, options = {}) {
    const attackerType = pieceAbilityType(piece);
    if (!attackerType || attackerType === "missionary") return false;
    if (["recruiter", "guard"].includes(attackerType)) {
      return Boolean(
        options.allowBasicTrainingCapture && piece?.basicTraining || hasWorkerRoyalCommandCaptureAccess(boardState, piece)
      );
    }
    return isHighwayCaptureAllowed({ ...piece, type: attackerType }, target);
  }
  function workerCanPortalLandOn(boardState, piece, cell, options = {}) {
    if (!cell || isWorkerCollapsedSquare(boardState, cell.row, cell.col)) return false;
    const target = get(boardState, cell.row, cell.col);
    if (!target) return true;
    if (target === piece || target.color === piece.color) return false;
    if (!isWorkerPortalDirectCaptureAllowed(boardState, piece, target, options)) return false;
    return canWorkerCaptureTarget(piece.color, target, piece, boardState, options);
  }
  function applyWorkerPortalMoves(boardState, piece, moves) {
    if (!workerPortalRule(boardState) || !piece) return moves || [];
    return (moves || []).flatMap((move) => {
      if (!isWorkerPortalEligibleMove(piece, move)) return [move];
      const exit = workerPortalExitAt(boardState, move.row, move.col);
      if (!exit) return [move];
      const portalEntryTarget = get(boardState, move.row, move.col);
      const portalExitTarget = get(boardState, exit.row, exit.col);
      if (move.thiefQuietJump && portalExitTarget) return [];
      const madHorsePortalEntryCapture = Boolean(
        move.madHorseCapture && isWorkerMadHorseFriendlyTarget(boardState, piece, portalEntryTarget)
      );
      const madHorsePortalExitCapture = Boolean(
        isWorkerMadHorseFriendlyTarget(boardState, piece, portalExitTarget)
      );
      if (isWorkerCollapsedSquare(boardState, move.row, move.col) || !madHorsePortalExitCapture && !workerCanPortalLandOn(boardState, piece, exit, {
        allowBasicTrainingCapture: Boolean(move.basicTrainingCapture)
      })) return [];
      return [{
        ...move,
        madHorseCapture: madHorsePortalEntryCapture || madHorsePortalExitCapture,
        madHorsePortalEntryCapture,
        madHorsePortalExitCapture,
        portalLanding: true,
        portalEntry: { row: move.row, col: move.col },
        portalExit: exit
      }];
    });
  }
  function rayMoves(boardState, row, col, color, dirs, limit = 8, canCapture = true) {
    const moves = [];
    const maxSteps = limit === 8 ? boardRayLimit(boardState) : limit;
    const attacker = get(boardState, row, col) || { color };
    const portalRule = workerPortalRule(boardState);
    const isTransparent = (target) => septemberOvertakeCanCross(boardState.overtake?.[attacker.color], attacker, target) || isGhostTransparentFor(attacker, target, renderType(attacker), boardState);
    dirs.forEach(([dr, dc]) => {
      let nextRow = row + dr;
      let nextCol = col + dc;
      let step = 1;
      let portalTransit = null;
      while (step <= maxSteps) {
        if (!inBounds(nextRow, nextCol)) break;
        const target = get(boardState, nextRow, nextCol);
        const portalExit = portalRule ? workerPortalExitAt(boardState, nextRow, nextCol) : null;
        if (portalExit && portalTransit) break;
        if (!target) {
          moves.push({
            row: nextRow,
            col: nextCol,
            ...portalTransit ? { portalThrough: true, portalEntry: portalTransit.entry, portalExit: portalTransit.exit } : {}
          });
          if (portalExit) {
            const exitTarget = get(boardState, portalExit.row, portalExit.col);
            if ((!exitTarget || isTransparent(exitTarget)) && !isWorkerCollapsedSquare(boardState, nextRow, nextCol) && !isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col)) {
              portalTransit = portalRayContinuation(
                boardState.portalRule,
                nextRow,
                nextCol,
                dr,
                dc,
                boardRowCount(boardState),
                boardColCount(boardState)
              );
              nextRow = portalTransit.next.row;
              nextCol = portalTransit.next.col;
              step += 1;
              continue;
            }
            break;
          }
          nextRow += dr;
          nextCol += dc;
          step += 1;
          continue;
        }
        if (isTransparent(target)) {
          if (isStealthTransparentFor(attacker, target, boardState) && !portalExit) {
            moves.push({
              row: nextRow,
              col: nextCol,
              ...portalTransit ? { portalThrough: true, portalEntry: portalTransit.entry, portalExit: portalTransit.exit } : {}
            });
          }
          if (portalExit) {
            moves.push({ row: nextRow, col: nextCol, portalTransparentEntry: true });
            const exitTarget = get(boardState, portalExit.row, portalExit.col);
            if ((!exitTarget || isTransparent(exitTarget)) && !isWorkerCollapsedSquare(boardState, nextRow, nextCol) && !isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col)) {
              portalTransit = portalRayContinuation(
                boardState.portalRule,
                nextRow,
                nextCol,
                dr,
                dc,
                boardRowCount(boardState),
                boardColCount(boardState)
              );
              nextRow = portalTransit.next.row;
              nextCol = portalTransit.next.col;
              step += 1;
              continue;
            }
            break;
          }
          nextRow += dr;
          nextCol += dc;
          step += 1;
          continue;
        }
        if (canCapture && canWorkerCaptureTarget(color, target, attacker, boardState)) {
          moves.push({
            row: nextRow,
            col: nextCol,
            ...portalTransit ? { portalThrough: true, portalEntry: portalTransit.entry, portalExit: portalTransit.exit } : {}
          });
        }
        break;
      }
    });
    return moves;
  }
  function leapMoves(boardState, row, col, color, deltas) {
    const attacker = get(boardState, row, col) || { color };
    return deltas.flatMap(([dr, dc]) => {
      const nextRow = row + dr;
      const nextCol = col + dc;
      if (!inBounds(nextRow, nextCol)) return [];
      const target = get(boardState, nextRow, nextCol);
      if (target?.color === color) return [];
      if (target && !isStealthTransparentFor(attacker, target, boardState) && !canWorkerCaptureTarget(color, target, attacker, boardState)) return [];
      return [{ row: nextRow, col: nextCol }];
    });
  }
  function isWorkerCheckerType(type) {
    return type === "checker" || type === "checkerKing";
  }
  function isWorkerCheckerPiece(piece) {
    return Boolean(piece && isWorkerCheckerType(renderType(piece)));
  }
  function workerCheckerDirections(boardState, color, type = "checker") {
    if (type === "checkerKing") return diagonals();
    const dir = color === "white" ? -1 : 1;
    return [[dir, -1], [dir, 1]];
  }
  function workerCheckerKingRow(boardState, color) {
    return color === "white" ? 0 : boardRowCount(boardState) - 1;
  }
  function workerCrownCheckerIfNeeded(boardState, piece, row) {
    if (!pieceHasAbility(piece, "checker") || row !== workerCheckerKingRow(boardState, piece.color)) return false;
    if (campaignAuthorityV1States.has(boardState) && piece.type !== "checker") return false;
    piece.type = "checkerKing";
    delete piece.tricksterMoveType;
    delete piece.tricksterPreviousAbilityForTurn;
    piece.moved = true;
    return true;
  }
  function checkerMoves(boardState, row, col, color, type = renderType(get(boardState, row, col)) || "checker") {
    return uniqueMoves([
      ...checkerStepMoves(boardState, row, col, color, type),
      ...checkerCaptureMoves(boardState, row, col, color, type)
    ]);
  }
  function checkerStepMoves(boardState, row, col, color, type = "checker") {
    return workerCheckerDirections(boardState, color, type).map(([dr, dc]) => ({ row: row + dr, col: col + dc })).filter((move) => inBounds(move.row, move.col, boardState) && !workerMovementOccupant(boardState, row, col, move.row, move.col, color));
  }
  function checkerCaptureMoves(boardState, row, col, color, type = renderType(get(boardState, row, col)) || "checker") {
    const attacker = get(boardState, row, col) || { color, type };
    const moves = [];
    workerCheckerDirections(boardState, color, type).forEach(([dr, dc]) => {
      const midRow = row + dr;
      const midCol = col + dc;
      const landingRow = row + dr * 2;
      const landingCol = col + dc * 2;
      if (!inBounds(midRow, midCol, boardState) || !inBounds(landingRow, landingCol, boardState)) return;
      if (workerMovementOccupant(boardState, row, col, landingRow, landingCol, color)) return;
      const target = workerMovementOccupant(boardState, row, col, midRow, midCol, color);
      if (!canWorkerCaptureTarget(color, target, attacker, boardState)) return;
      moves.push({
        row: landingRow,
        col: landingCol,
        jumpCapture: { row: midRow, col: midCol },
        checkerCapture: true
      });
    });
    return moves;
  }
  function workerLegalCheckerCaptureMoves(boardState, piece, row, col) {
    if (!isWorkerCheckerPiece(piece) || isFrozenPiece(piece) || isWorkerStakedPiece(piece) || isDiceLockedPiece(boardState, piece)) return [];
    return checkerCaptureMoves(boardState, row, col, piece.color, renderType(piece)).filter((move) => isWorkerBaseMoveAllowed(boardState, piece, row, col, move));
  }
  function workerHasForcedCheckerCapture(boardState, color) {
    let forced = false;
    forEachPiece(boardState, (piece, row, col) => {
      if (forced || piece?.color !== color || !isWorkerCheckerPiece(piece)) return;
      forced = workerLegalCheckerCaptureMoves(boardState, piece, row, col).length > 0;
    });
    return forced;
  }
  function applyWorkerCheckerCaptureRules(boardState, piece, row, col, moves, options = {}) {
    if (!options.ignoreGlobalCaptureForce && piece?.color && workerHasForcedEnPassant(boardState, piece.color)) return moves;
    if (isWorkerCheckerPiece(piece)) {
      const captures = moves.filter((move) => move.checkerCapture);
      if (piece.checkerChainCapture || captures.length) return captures;
    }
    if (!options.ignoreGlobalCaptureForce && piece?.color && workerHasForcedCheckerCapture(boardState, piece.color)) return [];
    return moves;
  }
  function workerSuicideBomberBlastProfile(boardState, row, col, color) {
    const seen = /* @__PURE__ */ new Set();
    let blastBalance = 0;
    let ownRoyalAtRisk = false;
    for (let dr = -1; dr <= 1; dr += 1) {
      for (let dc = -1; dc <= 1; dc += 1) {
        if (dr === 0 && dc === 0) continue;
        const targetRow2 = row + dr;
        const targetCol2 = col + dc;
        if (!inBounds(targetRow2, targetCol2, boardState)) continue;
        const item = get(boardState, targetRow2, targetCol2);
        if (!item || isFrozenPiece(item) || isIndirectAttackImmunePiece(item) || isBombImmuneNeutralPiece(item) || seen.has(item)) continue;
        seen.add(item);
        const value = pieceValue(item);
        if (item.color === color) {
          if (isWorkerDecisiveCaptureTarget(boardState, item)) ownRoyalAtRisk = true;
          else blastBalance -= value * (value >= 500 ? 1.35 : 1);
        } else if (item.color === opponent(color)) {
          blastBalance += isWorkerDecisiveCaptureTarget(boardState, item) ? Math.min(1200, value) : value * (value >= 500 ? 1.15 : 0.85);
        }
      }
    }
    return { blastBalance, ownRoyalAtRisk };
  }
  function workerSuicideBomberTargetScore(boardState, piece, row, col, color) {
    const valid = Boolean(
      piece && piece.color === color && ["pawn", "fanatic"].includes(piece.type) && !piece.explosive && !piece.feudalContractId && Number.isInteger(row) && Number.isInteger(col)
    );
    if (!valid) return scoreSuicideBomberTarget({ valid: false });
    const profile = workerSuicideBomberBlastProfile(boardState, row, col, color);
    const capturer = workerBestCaptureThreat(boardState, piece, row, col);
    const homeRow = color === "white" ? boardRowCount(boardState) - 2 : 1;
    const advance = color === "white" ? homeRow - row : row - homeRow;
    return scoreSuicideBomberTarget({
      valid,
      targetValue: pieceValue(piece),
      blastBalance: profile.blastBalance,
      capturerValue: pieceValue(capturer?.piece),
      capturable: Boolean(capturer),
      ownRoyalAtRisk: profile.ownRoyalAtRisk,
      advance
    });
  }
  function workerWitchTrialCaptureProfile(boardState, piece, row, col, color) {
    const disabled = Boolean(
      isFrozenPiece(piece) || isWorkerStakedPiece(piece) || piece?.disarmed || isDiceLockedPiece(boardState, piece) || isWorkerMannerCaptureLocked(boardState, piece) || isWorkerSaturationCaptureLocked(boardState, piece) || isWorkerInitiativeCaptureLocked(boardState, piece?.color)
    );
    const profile = {
      disabled,
      immediateCaptureCount: 0,
      maxCaptureValue: 0,
      totalCaptureValue: 0,
      decisiveCapture: false
    };
    if (disabled) return profile;
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (target, targetRow2, targetCol2) => {
      if (!target || target.color !== color || seen.has(target)) return;
      if (!canWorkerCaptureTarget(piece.color, target, piece, boardState)) return;
      if (!attacksSquare(boardState, piece, row, col, targetRow2, targetCol2)) return;
      seen.add(target);
      const value = pieceValue(target);
      profile.immediateCaptureCount += 1;
      profile.maxCaptureValue = Math.max(profile.maxCaptureValue, value);
      profile.totalCaptureValue += value;
      if (isWorkerDecisiveCaptureTarget(boardState, target)) profile.decisiveCapture = true;
    });
    return profile;
  }
  function workerWitchTrialTargetScore(boardState, piece, row, col, color) {
    const valid = Boolean(
      piece && piece.color === opponent(color) && !piece.witchTrial && !isWorkerRoyalIdentityPiece(boardState, piece) && !["vip", "merchant", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(piece.type) && Number.isInteger(row) && Number.isInteger(col)
    );
    if (!valid) return scoreWitchTrialTarget({ valid: false });
    const captureProfile = workerWitchTrialCaptureProfile(boardState, piece, row, col, color);
    return scoreWitchTrialTarget({
      valid,
      targetValue: pieceValue(piece),
      ...captureProfile,
      shielded: Boolean(piece.shielded),
      currentlyThreatened: Boolean(workerBestCaptureThreat(boardState, piece, row, col))
    });
  }
  function generateCardTargets(boardState, card, color) {
    if (card.effect === "bloodCard") {
      if (!hasVampireLord(boardState, color)) return [];
      const effectIds = card.bloodRevealed && card.bloodEffectId ? [card.bloodEffectId] : BLOOD_EFFECT_IDS;
      return effectIds.map((bloodEffectId) => ({ bloodEffectId }));
    }
    return generateWorkerCardTargetsV2(boardState, card, color);
  }
  function generateWorkerCardTargetsV2(boardState, card, color) {
    if (card.effect === "miracle") return workerMiracleTargets(boardState, color).length ? [null] : [];
    if (card.effect === "recurrence") return piecesMatching(boardState, (piece) => workerRecurrenceTarget(boardState, piece, color)).map(({ row, col }) => ({ row, col }));
    if (card.effect === "nullification") return piecesMatching(boardState, (piece) => workerNullificationTarget(piece, color)).map(({ row, col }) => ({ row, col }));
    if (card.effect === "outpost") return piecesMatching(boardState, (piece, row, col) => workerOutpostTarget(boardState, piece, row, col, color)).map(({ row, col }) => ({ row, col }));
    // Grafted from engine.optimized.js (our-only addition): "bribe" targets
    // any of this color's own knights (apply-time logic grafted into
    // applyCardActionUnchecked's "bribe" branch below).
    if (card.effect === "bribe") return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight").map(({ row, col }) => ({ row, col })).slice(0, 20);
    // 2026-09-15 patch: 16 new cards' targeting, grafted from the fresh
    // aiWorker.js's generateWorkerCardTargetsV2 (INTERNAL_THREE/FIVE/EIGHT).
    if (["symmetry", "mutation"].includes(card.id)) return boardState[card.id]?.[color] ? [] : [null];
    if (["paladin", "octopus"].includes(card.id)) return workerPositionsOf(boardState, (p) => p.color === color && p.type === (card.id === "paladin" ? "knight" : "rook") && !isWorkerKingRole(boardState, p));
    if (card.id === "metal") return workerPositionsOf(boardState, (p) => p.color === color && !p.metalized && !isWorkerKingRole(boardState, p) && workerIsIceSheetRangedPiece(boardState, p));
    if (card.id === "thief" && usesThiefRemake(boardState)) return thiefQueenCandidates(boardState.board, color, (item) => isWorkerKingRole(boardState, item)).length ? [null] : [];
    if (card.id === "brutus") return workerCountPiecesOf(boardState, color, "rook") > 0 ? [null] : [];
    if (["clockwork", "parrot"].includes(card.id)) return workerPositionsOf(boardState, (p) => p.color === color && (card.id === "parrot" ? parrotTransformTypes(boardState) : ["knight", "bishop", "camel"]).includes(p.type) && !isWorkerKingRole(boardState, p));
    if (INTERNAL_EIGHT_PASSIVE_EFFECTS.includes(card.effect)) return boardState[card.effect]?.[color] ? [] : [null];
    if (card.effect === "disassembly") return boardState.disassembly?.[color] ? [] : [null];
    if (card.effect === "extinction") return piecesMatching(boardState, (p) => p.color === color && !isWorkerRoyalIdentityPiece(boardState, p) && extinctionTargetTypeAllowed(p.type, boardState)).map(({ row, col }) => ({ row, col }));
    if (SEPTEMBER_PASSIVE_EFFECTS.includes(card.effect) && card.effect !== "bigBishop") return boardState[card.effect]?.[color] ? [] : [null];
    const effect = card.effect || "";
    const enemy = opponent(color);
    if (usesSeptember18Balance(boardState) && ["campfire", "reversal", "scarecrow"].includes(effect)) return piecesMatching(boardState, (piece) => piece.color === color && (effect === "scarecrow" ? !["wall", "football", "blackHole", "coffin"].includes(piece.type) : ["knight", "bishop", "camel"].includes(piece.type) && !isWorkerKingRole(boardState, piece))).map(({ row, col }) => ({ row, col }));
    if (["siegeRam", "magicGirl", "berserker", "slime", "trickster", "campfire", "princess"].includes(effect)) {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "rook" && !isWorkerRoyalIdentityPiece(boardState, piece)).sort((a, b) => pieceValue(a.piece) - pieceValue(b.piece)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (["siren", "undead", "hedgehog"].includes(effect)) {
      return piecesMatching(boardState, (piece) => isWorkerNonRoyalQueen(boardState, piece, color)).map(({ row, col }) => ({ row, col }));
    }
    if (effect === "hypocrisy") {
      const candidates = [];
      for (let row = 0; row < boardRowCount(boardState); row += 1) {
        for (let col = 0; col < boardColCount(boardState); col += 1) {
          if (!workerOpenPlacementSquare(boardState, row, col) || workerSeptember12PlacementCrownBlocked(boardState, row, col) || isWorkerCollapsedSquare(boardState, row, col) || isBlackHoleCell(boardState, row, col)) continue;
          candidates.push({ row, col, score: centerScore(row, col, boardState) });
        }
      }
      const selections = candidates.sort((a, b) => a.score - b.score).slice(0, 4).map(({ row, col }) => ({ row, col }));
      return selections.length === 4 ? [{ selections }] : [];
    }
    if (effect === "cleanupPieces") {
      const selections = piecesMatching(boardState, (piece) => piece.color === color && !isWorkerRoyalIdentityPiece(boardState, piece) && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)).sort((a, b) => pieceValue(a.piece) - pieceValue(b.piece)).slice(0, 1).map(({ row, col }) => ({ row, col }));
      return selections.length ? [{ selections }] : [];
    }
    if (effect === "suspiciousPotion") {
      return piecesMatching(boardState, (piece) => COLORS.includes(piece.color) && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)).sort((a, b) => (a.piece.color === color ? -1 : 1) * pieceValue(b.piece) - (b.piece.color === color ? -1 : 1) * pieceValue(a.piece)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "loyalist") {
      return piecesMatching(boardState, (piece) => piece.color === color && !piece.loyalist && !isSlimeSpecialMovementLocked(piece) && !isWorkerRoyalIdentityPiece(boardState, piece) && !workerIsLargePiece(piece) && !["merchant", "wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)).map(({ row, col }) => ({ row, col })).slice(0, 24);
    }
    if (effect === "parry") {
      return piecesMatching(boardState, (piece) => piece.color === color && !piece.parry && !["wall", "football", "blackHole", "monster", "scarecrow"].includes(piece.type)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece)).map(({ row, col }) => ({ row, col })).slice(0, 24);
    }
    if (effect === "emptyLunchbox") {
      return piecesMatching(boardState, (piece) => piece.color === enemy && !isWorkerRoyalIdentityPiece(boardState, piece) && !piece.emptyLunchbox && !["merchant", "wall", "football", "blackHole", "monster"].includes(piece.type)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece)).map(({ row, col }) => ({ row, col })).slice(0, 24);
    }
    if (effect === "switcheroo") return workerCanResolveUntargetedCard(boardState, card, color) ? [void 0] : [];
    if (effect === "freeCastling") {
      return workerImmediateCastlingPlans(boardState, color).map(({ rook }) => ({ row: rook.row, col: rook.col }));
    }
    if (effect === "poisonedPawn") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn" && !piece.poisonedPawn).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "missionary") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "bishop").map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "portalGun") {
      const candidates = [];
      for (let row = 0; row < boardRowCount(boardState); row += 1) {
        for (let col = 0; col < boardColCount(boardState); col += 1) {
          if (get(boardState, row, col) || workerSeptember12PlacementCrownBlocked(boardState, row, col) || isWorkerCollapsedSquare(boardState, row, col) || isBlackHoleCell(boardState, row, col) || isWorkerPendingPortalReservedSquare(boardState, row, col)) continue;
          candidates.push({ row, col });
        }
      }
      const preferred = candidates.sort((a, b) => centerScore(b.row, b.col, boardState) - centerScore(a.row, a.col, boardState)).slice(0, 8);
      const pairs = [];
      for (let first = 0; first < preferred.length; first += 1) {
        for (let second = first + 1; second < preferred.length; second += 1) {
          pairs.push({ selections: [preferred[first], preferred[second]] });
        }
      }
      return pairs.slice(0, 16);
    }
    if (effect === "promotionRush") {
      return piecesMatching(boardState, (piece) => isPromotionRushEligiblePiece(piece, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "randomRoulette") {
      return piecesMatching(boardState, (piece) => isRandomRouletteTargetPiece(piece)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "freeMove") {
      const candidates = [];
      piecesMatching(boardState, (piece) => piece.color === color && !["wall", "football", "blackHole", "coffin"].includes(piece.type)).forEach(({ piece, row, col }) => {
        generateMovesForPiece(boardState, piece, row, col).filter((move) => isWorkerMoveAllowed(boardState, piece, row, col, move)).filter((move) => Number.isInteger(move?.row) && Number.isInteger(move?.col) && !move.colossusBody && !move.colossusAttack && !move.shotgunBlast && !move.shotgunSnipe && !move.merchantBuy && !move.setLogDirection && !move.dragonSwap && !move.substitutionSwap).forEach((move) => {
          const target = get(boardState, move.row, move.col);
          const captureScore = target?.color === enemy ? pieceValue(target) * 12 : 0;
          candidates.push({
            pieceId: piece.id,
            from: { row, col },
            to: { row: move.row, col: move.col },
            score: captureScore + centerScore(move.row, move.col, boardState)
          });
        });
      });
      const selected = /* @__PURE__ */ new Set();
      const selections = candidates.sort((a, b) => b.score - a.score).filter((entry) => {
        if (!entry.pieceId || selected.has(entry.pieceId)) return false;
        selected.add(entry.pieceId);
        return true;
      }).slice(0, 3).map(({ from, to }) => ({ from, to }));
      return selections.length ? [{ selections }] : [];
    }
    if (effect === "chain") {
      return workerChainPairCandidates(boardState, color).map((pair) => ({
        selections: pair.map(({ row, col }) => ({ row, col }))
      }));
    }
    if (effect === "babyBear") {
      return piecesMatching(boardState, (piece) => isWorkerNonRoyalQueen(boardState, piece, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "frenzy") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn").map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "vip") {
      return piecesMatching(boardState, (piece) => ["white", "black"].includes(piece.color) && piece.type === "pawn").map(({ row, col }) => ({ row, col })).slice(0, 32);
    }
    if (effect === "twins") {
      const entries = piecesMatching(boardState, (piece) => piece.color === color && !isSlimeSpecialMovementLocked(piece) && !piece.twinBondId && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "coffin"].includes(piece.type));
      const pairs = [];
      for (let first = 0; first < entries.length; first += 1) {
        for (let second = first + 1; second < entries.length; second += 1) {
          pairs.push({ selections: [entries[first], entries[second]].map(({ row, col }) => ({ row, col })) });
        }
      }
      return pairs.slice(0, 20);
    }
    if (effect === "submerge") {
      return piecesMatching(boardState, (piece, row, col) => piece.color === color && !piece.submerged && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "coffin"].includes(piece.type) && !hasAdjacentEnemyPiece(boardState.board, row, col, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "homecoming") {
      return piecesMatching(boardState, (piece, row, col) => {
        if (piece.color !== color || isWorkerRoyalIdentityPiece(boardState, piece) || workerIsLargePiece(piece) || !piece.origin || ["wall", "football", "blackHole", "coffin"].includes(piece.type)) return false;
        const origin = parseWorkerSquare(boardState, piece.origin);
        return workerOpenPlacementSquare(boardState, origin.row, origin.col) && (origin.row !== row || origin.col !== col);
      }).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "judgment") {
      return workerJudgmentEntries(boardState).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "lobster") {
      const targets2 = [];
      for (let row = 0; row < boardRowCount(boardState); row += 1) {
        for (let col = 0; col < boardColCount(boardState); col += 1) {
          if (workerOpenPlacementSquare(boardState, row, col) && !workerSeptember12PlacementCrownBlocked(boardState, row, col)) targets2.push({ row, col });
        }
      }
      return targets2.slice(0, 32);
    }
    if (effect === "ruleTicket") {
      return workerRuleTicketCardPool(boardState).sort((a, b) => workerRuleTicketScore(b, color) - workerRuleTicketScore(a, color)).slice(0, 4).map((rule) => ({ ruleId: rule.id }));
    }
    if (effect === "joker") {
      return workerReusableActiveCards(boardState, color, card).sort((a, b) => cardValue(b) - cardValue(a)).slice(0, 6).map((usedCard) => ({ cardInstanceId: usedCard.instanceId }));
    }
    if (effect === "chameleonMutation") {
      const selections = piecesMatching(boardState, (piece) => piece.color === color && !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "colossus", "bigRook", "bigBishop"].includes(piece.type)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece)).slice(0, 3).map(({ row, col }) => ({ row, col }));
      return selections.length ? [{ selections }] : [];
    }
    if (effect === "othello" && usesSeptember18Balance(boardState)) return [null];
    if (effect === "othello") {
      return piecesMatching(boardState, (piece, row, col) => workerIsOthelloTarget(boardState, piece, row, col, color)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "amazon") {
      const queens = piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color));
      const knights = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight");
      return queens.flatMap((queen) => knights.map((knight) => ({
        row: queen.row,
        col: queen.col,
        knight: { row: knight.row, col: knight.col }
      }))).slice(0, 20);
    }
    if (!card.target) return workerCanResolveUntargetedCard(boardState, card, color) ? [void 0] : [];
    if (effect === "windmill") {
      const bishops = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "bishop");
      const rooks = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "rook");
      return bishops.flatMap((bishop) => rooks.map((rook) => ({ row: rook.row, col: rook.col, bishop: { row: bishop.row, col: bishop.col } }))).slice(0, 12);
    }
    if (effect === "hook") {
      const queens = piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color));
      const rooks = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "rook");
      return queens.flatMap((queen) => rooks.map((rook) => ({ row: queen.row, col: queen.col, rook: { row: rook.row, col: rook.col } }))).slice(0, 12);
    }
    if (effect === "feudalContract") {
      const pawns = piecesMatching(boardState, (piece) => piece.color === color && ["pawn", "fanatic"].includes(piece.type) && !piece.feudalContractId);
      const guardians = piecesMatching(boardState, (piece) => piece.color === color && !["pawn", "fanatic", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(piece.type));
      return pawns.flatMap((pawn) => guardians.map((guardian) => ({ row: guardian.row, col: guardian.col, pawn: { row: pawn.row, col: pawn.col } }))).slice(0, 16);
    }
    if (effect === "emergencyEvacuation") {
      const selections = piecesMatching(boardState, (piece, row, col) => piece.color === color && !["wall", "football", "colossus"].includes(piece.type) && !isWorkerUndergroundBunkerKing(piece)).filter(({ piece, row, col }) => {
        const dir = pawnDir(boardState, color);
        const destination = { row: row - dir, col };
        return inBounds(destination.row, col, boardState) && !get(boardState, destination.row, col) && !isWorkerFianchettoBlockedMove(boardState, piece, row, col, destination);
      }).slice(0, 3).map(({ row, col }) => ({ row, col }));
      return selections.length ? [{ selections }] : [];
    }
    if (effect === "panic") {
      const selections = workerPanicTargets(boardState, color).slice(0, 2).map(({ row, col }) => ({ row, col }));
      return selections.length >= 2 ? [{ selections }] : [];
    }
    if (effect === "spy") {
      const selections = piecesMatching(boardState, (piece) => piece.color === enemy && piece.type === "pawn").slice(0, 2).map(({ row, col }) => ({ row, col }));
      const required = Math.min(2, selections.length);
      return required > 0 ? [{ selections: selections.slice(0, required) }] : [];
    }
    if (effect === "pawnStorm") {
      const selections = workerPawnStormTargets(boardState, color).slice(0, 4).map(({ row, col }) => ({ row, col }));
      return selections.length ? [{ selections }] : [];
    }
    if (effect === "holdout") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn" && !piece.holdoutPromotion).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "charge") {
      return piecesMatching(boardState, (piece, row, col) => workerCanChargePawn(boardState, piece, row, col, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "witchTrial") {
      return piecesMatching(boardState, (piece) => piece.color === enemy && !isWorkerRoyalIdentityPiece(boardState, piece) && !["vip", "merchant", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(piece.type)).filter(({ piece }) => !piece.witchTrial).sort((a, b) => workerWitchTrialTargetScore(boardState, b.piece, b.row, b.col, color) - workerWitchTrialTargetScore(boardState, a.piece, a.row, a.col, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "exile") {
      return piecesMatching(boardState, (piece, row, col) => {
        if (piece.color !== enemy || isWorkerRoyalIdentityPiece(boardState, piece) || ["wall", "football", "colossus"].includes(piece.type)) return false;
        const origin = parseWorkerSquare(boardState, piece.origin || workerSquareName(boardState, row, col));
        return inBounds(origin.row, origin.col, boardState) && !get(boardState, origin.row, origin.col);
      }).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "suicideBomber") {
      return piecesMatching(boardState, (piece) => piece.color === color && ["pawn", "fanatic"].includes(piece.type) && !piece.explosive && !piece.feudalContractId).sort((a, b) => workerSuicideBomberTargetScore(boardState, b.piece, b.row, b.col, color) - workerSuicideBomberTargetScore(boardState, a.piece, a.row, a.col, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "chimera") {
      return piecesMatching(boardState, (piece) => piece.color === color && !piece.chimera && ["knight", "bishop"].includes(piece.type)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "stake") {
      return piecesMatching(boardState, (piece) => piece.color === color && !piece.staked && !["wall", "football", "colossus"].includes(piece.type)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "basicTraining") {
      return piecesMatching(boardState, (piece) => isWorkerBasicTrainingEligible(piece, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "localConscription") {
      return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "queensGambit") {
      if (!workerHasQueensGambitCandidate(boardState, color)) return [];
      return piecesMatching(boardState, (piece) => isWorkerQueensGambitQueen(boardState, piece, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "iceSheet") {
      return workerCanResolveUntargetedCard(boardState, card, color) ? [void 0] : [];
    }
    if (effect === "desperado") {
      return piecesMatching(boardState, (piece) => piece.color === color && !isCritical(piece) && !isWorkerRegencyRoyalHeir(boardState, piece) && !["merchant", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(piece.type)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "reposition") {
      return [null];
    }
    if (effect === "cleanupSacrifice") {
      return piecesMatching(boardState, (piece) => workerIsCleanupSacrificeTarget(boardState, piece, color)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "sacrifice") {
      return piecesMatching(boardState, (piece) => workerIsSacrificeTarget(boardState, piece, color)).sort((a, b) => pieceValue(a.piece) - pieceValue(b.piece)).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "deathSquad") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn").map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "trojanHorse") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight" && !piece.trojanHorse).map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    if (effect === "ordination") {
      const bishops = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "bishop");
      if (bishops.length < 2) return [];
      return bishops.map(({ row, col }) => ({ row, col })).slice(0, 20);
    }
    const spec = card.effect === "grasshopper" && legacyGrasshopperTargetStates.has(boardState) ? { side: "own", types: ["rook"] } : workerCardTargetSpec(card);
    const targets = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.type === "wall" || piece.type === "football" || piece.type === "colossus") return;
      if (spec.side === "enemy" && piece.color !== enemy) return;
      if (spec.side !== "enemy" && piece.color !== color) return;
      if (spec.nonKing && (isCritical(piece) || isWorkerRegencyRoyalHeir(boardState, piece))) return;
      if ((spec.slider || spec.ranged && !spec.iceSheetTarget || spec.types?.includes("queen")) && isWorkerRoyalIdentityPiece(boardState, piece)) return;
      if (spec.slider && !["rook", "bishop", "queen"].includes(piece.type)) return;
      if (spec.ranged && !(spec.iceSheetTarget ? workerIsIceSheetEffectTarget(boardState, piece) : workerIsIceSheetRangedPiece(boardState, piece))) return;
      if (Array.isArray(spec.types) && spec.types.length && !spec.types.includes(piece.type)) return;
      targets.push({ row, col });
    });
    return targets.slice(0, 20);
  }
  function workerCountPiecesOf(boardState, color, type) {
    return piecesMatching(boardState, (piece) => piece.color === color && piece.type === type).length;
  }
  function workerIsCleanupSacrificeExcluded(boardState, piece) {
    return !piece || isCritical(piece) || isWorkerRegencyRoyalHeir(boardState, piece) || ["wall", "football", "blackHole"].includes(piece.type);
  }
  function workerCleanupSacrificeEnemyCandidates(boardState, color, type) {
    const enemy = opponent(color);
    const seen = /* @__PURE__ */ new Set();
    const candidates = [];
    forEachPiece(boardState, (piece, row, col) => {
      const id = piece?.id || `${row},${col}`;
      if (!piece || seen.has(id) || piece.color !== enemy || piece.type !== type || workerIsCleanupSacrificeExcluded(boardState, piece)) return;
      seen.add(id);
      candidates.push({ piece, row, col });
    });
    return candidates;
  }
  function workerIsCleanupSacrificeTarget(boardState, piece, color) {
    return Boolean(piece && piece.color === color && !workerIsCleanupSacrificeExcluded(boardState, piece) && workerCleanupSacrificeEnemyCandidates(boardState, color, piece.type).length);
  }
  function workerHasCleanupSacrificeCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => workerIsCleanupSacrificeTarget(boardState, piece, color)).length > 0;
  }
  function workerIsSacrificeExcluded(boardState, piece) {
    return !piece || !Number.isFinite(pieceCombatValue(piece, boardState)) || isCritical(piece) || isWorkerRegencyRoyalHeir(boardState, piece) || ["wall", "football", "blackHole", "coffin"].includes(piece.type);
  }
  function workerSacrificeProtectionCandidates(boardState, source, color) {
    if (!source || source.color !== color || workerIsSacrificeExcluded(boardState, source)) return [];
    const sourceValue = pieceCombatValue(source, boardState);
    return piecesMatching(boardState, (piece) => piece !== source && piece.color === color && !workerIsSacrificeExcluded(boardState, piece) && pieceCombatValue(piece, boardState) < sourceValue);
  }
  function workerIsSacrificeTarget(boardState, piece, color) {
    return Boolean(piece && piece.color === color && !workerIsSacrificeExcluded(boardState, piece) && workerSacrificeProtectionCandidates(boardState, piece, color).length);
  }
  function workerHasSacrificeCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => workerIsSacrificeTarget(boardState, piece, color)).length > 0;
  }
  function workerReusableActiveCards(boardState, color, jokerCard = null) {
    return deck(boardState, color).filter((card) => card && !card.emptySlot && card !== jokerCard && card.effect !== "joker" && card.effect !== "bloodCard" && card.phase !== "RULE" && card.used && !card.recovering && !card.passiveApplied);
  }
  function workerHasKnightFamilyMovementCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => {
      if (!piece || piece.color !== color || piece.type === "wall") return false;
      if (["knight", "royalKnight", "assassin", "dragon", "pegasus", "unicorn", "amazon"].includes(renderType(piece))) return true;
      if (isWorkerKingRole(boardState, piece) && boardState.kingKnight?.[color]) return true;
      if (piece.type === "vampireLord" && usesBloodMoonNightMovement(boardState, color)) return true;
      return false;
    }).length > 0;
  }
  function workerHasEnemyNonRoyalPiece(boardState, color, options = {}) {
    const enemy = opponent(color);
    return piecesMatching(boardState, (piece) => {
      if (piece.color !== enemy || isWorkerRoyalIdentityPiece(boardState, piece) || ["wall", "colossus"].includes(piece.type)) return false;
      if (options.excludeMerchant && piece.type === "merchant") return false;
      if (options.excludeVip && piece.type === "vip") return false;
      if (options.excludeBigRook && ["bigRook", "bigBishop"].includes(piece.type)) return false;
      if (options.excludePawn && piece.type === "pawn") return false;
      if (options.excludeFootball && piece.type === "football") return false;
      return true;
    }).length > 0;
  }
  function workerHasUndergroundBunkerCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && isWorkerKingRole(boardState, piece) && !piece.undergroundBunker).length > 0;
  }
  function workerHasSwitcherooCandidate(boardState, color) {
    return Boolean(workerFindMobileKing(boardState, color)) && piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn").length > 0;
  }
  function workerFindMobileKing(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && isWorkerKingRole(boardState, piece) && !isWorkerUndergroundBunkerKing(piece))[0] || null;
  }
  function workerRangedPieceOptions(boardState, piece) {
    const memo = ATTACK_MEMO;
    if (memo !== null && memo.board === boardState && piece && typeof piece === "object") {
      let v = memo.ranged.get(piece);
      if (v === void 0) { v = workerRangedPieceOptionsRaw(boardState, piece); memo.ranged.set(piece, v); }
      return v;
    }
    return workerRangedPieceOptionsRaw(boardState, piece);
  }
  function workerRangedPieceOptionsRaw(boardState, piece) {
    return {
      princessQueenMovement: pieceAbilityType(piece) === "princess" && septemberPrincessHasQueenMovementCached(boardState?.board, piece.color),
      magicGirlAwakened: Boolean(boardState?.magicGirlSurge?.[piece?.color]),
      berserkerTier: boardState && piece?.color ? berserkerMovementTier(workerUniqueAlliedPieceCount(boardState, piece.color)) : "",
      tricksterMoveType: piece?.tricksterMoveType
    };
  }
  function workerIsIceSheetRangedPiece(boardState, piece) {
    return Boolean(piece && isRangedPiece(piece, workerRangedPieceOptions(boardState, piece)));
  }
  function workerIsIceSheetEffectTarget(boardState, piece) {
    return Boolean(piece && isIceSheetEffectTargetPiece(piece, workerRangedPieceOptions(boardState, piece)));
  }
  function isStealthTransparentFor(attacker, target, boardState) {
    return isStealthTransparentFor$1(attacker, target, {
      camouflageRule: boardState?.camouflageRule,
      board: boardState?.board,
      royal: Boolean(target?.editorRoyal || isWorkerKingRole(boardState, target))
    });
  }
  function isGhostTransparentFor(attacker, target, attackerType = attacker?.type, boardState = null) {
    return isStealthTransparentFor(attacker, target, boardState) || isGhostTransparentFor$1(
      attacker,
      target,
      attackerType,
      workerRangedPieceOptions(boardState, attacker)
    );
  }
  function workerHasSummonColossusCandidate(boardState, color) {
    const anchor = initialColossusAnchor(boardState, color);
    const cells = colossusCells(anchor.row, anchor.col);
    return cells.length === 4 && workerSummonColossusSacrificePawns(boardState, color).length === 6;
  }
  function workerHasMerchantGuildCandidate(boardState, color) {
    const king = criticalPieces(boardState, color).find(({ piece }) => piece.type === "king" && !isWorkerUndergroundBunkerKing(piece));
    if (!king) return false;
    const row = king.row + pawnDir(boardState, color) * 2;
    const col = king.col;
    if (!inBounds(row, col, boardState)) return false;
    const occupant = get(boardState, row, col);
    if (workerIsLargePiece(occupant)) return false;
    return !occupant || occupant.color === color;
  }
  function workerIsDiagonalChessActive(boardState) {
    return boardState?.appliedRuleCard?.id === "diagonal-chess" || (boardState?.additionalRuleCards || []).some((card) => card?.id === "diagonal-chess");
  }
  function workerHasQueenCavalryCandidate(boardState, color) {
    const targetFile = workerIsDiagonalChessActive(boardState) ? color === "black" ? boardColCount(boardState) - 1 : 0 : Math.max(0, Math.floor((boardColCount(boardState) - 2) / 2));
    return piecesMatching(boardState, (piece, row, col) => piece.color === color && piece.type === "pawn" && col === targetFile).length > 0;
  }
  function workerHasFeudalContractCandidate(boardState, color) {
    const hasPawn = piecesMatching(boardState, (piece) => piece.color === color && ["pawn", "fanatic"].includes(piece.type) && !piece.explosive && !piece.feudalContractId).length > 0;
    if (!hasPawn) return false;
    return piecesMatching(boardState, (piece) => piece.color === color && !["pawn", "fanatic", "wall", "colossus", "bigRook", "bigBishop"].includes(piece.type)).length > 0;
  }
  function workerHasExileCandidate(boardState, color) {
    const enemy = opponent(color);
    return piecesMatching(boardState, (piece, row, col) => {
      if (piece.color !== enemy || isWorkerRoyalIdentityPiece(boardState, piece) || ["wall", "football", "colossus"].includes(piece.type)) return false;
      const origin = parseWorkerSquare(boardState, piece.origin || workerSquareName(boardState, row, col));
      return inBounds(origin.row, origin.col, boardState) && !get(boardState, origin.row, origin.col);
    }).length > 0;
  }
  function workerVortexCandidateCount(boardState, color) {
    return workerVortexCandidates(boardState, color).length;
  }
  function workerIsPanicTargetCandidate(boardState, piece, color) {
    return Boolean(
      piece && piece.color === opponent(color) && !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(piece.type)
    );
  }
  function workerPanicTargets(boardState, color) {
    return piecesMatching(boardState, (piece) => workerIsPanicTargetCandidate(boardState, piece, color)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece) || a.row - b.row || a.col - b.col);
  }
  function workerIsTrolleyExcludedPiece(boardState, piece) {
    return !piece || !Number.isFinite(pieceCombatValue(piece, boardState)) || isWorkerRoyalIdentityPiece(boardState, piece) || ["wall", "football", "blackHole"].includes(piece.type);
  }
  function workerTrolleyPieceRef(piece, row, col, boardState) {
    return {
      id: String(piece?.id || `${row}:${col}`),
      type: piece?.type || "pawn",
      color: piece?.color || "white",
      row,
      col,
      value: pieceCombatValue(piece, boardState)
    };
  }
  function workerTrolleyBundleValue(pieces) {
    return (pieces || []).reduce((sum, piece) => sum + (Number(piece.value) || 0), 0);
  }
  function workerMakeTrolleyBundle(pieces, id) {
    const unique = [];
    const seen = /* @__PURE__ */ new Set();
    (pieces || []).forEach((piece) => {
      if (!piece?.id || seen.has(piece.id)) return;
      seen.add(piece.id);
      unique.push(piece);
    });
    return { id, pieces: unique, value: workerTrolleyBundleValue(unique) };
  }
  function workerTrolleyEligiblePieces(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && !workerIsTrolleyExcludedPiece(boardState, piece)).map(({ piece, row, col }) => workerTrolleyPieceRef(piece, row, col, boardState));
  }
  function workerAddTrolleyBundleToScoreMap(scoreMap, pieces) {
    const bundle = workerMakeTrolleyBundle(pieces, `score-${pieces.map((piece) => piece.id).join("-")}`);
    const value = Number(bundle.value) || 0;
    if (value < TROLLEY_LOW_SCORE_OPTIONS[0] || value > TROLLEY_MAX_HIGH_SCORE) return;
    const bucket = scoreMap.get(value) || [];
    if (bucket.length >= TROLLEY_MAX_BUNDLES_PER_SCORE) return;
    bucket.push(bundle);
    scoreMap.set(value, bucket);
  }
  function workerTrolleyBundleCandidatesByScore(boardState, color) {
    const pieces = workerTrolleyEligiblePieces(boardState, color).filter((piece) => Number(piece.value) > 0 && Number(piece.value) <= TROLLEY_MAX_HIGH_SCORE).slice(0, TROLLEY_MAX_ELIGIBLE_PIECES);
    const scoreMap = /* @__PURE__ */ new Map();
    const walk = (start, picked, score) => {
      if (score > TROLLEY_MAX_HIGH_SCORE) return;
      if (picked.length) workerAddTrolleyBundleToScoreMap(scoreMap, picked);
      if (picked.length >= TROLLEY_MAX_BUNDLE_PIECES) return;
      for (let index = start; index < pieces.length; index += 1) {
        const piece = pieces[index];
        const nextScore = score + (Number(piece.value) || 0);
        if (nextScore > TROLLEY_MAX_HIGH_SCORE) continue;
        picked.push(piece);
        walk(index + 1, picked, nextScore);
        picked.pop();
      }
    };
    walk(0, [], 0);
    return scoreMap;
  }
  function workerTrolleyBundlesDisjoint(a, b) {
    const ids = new Set((a?.pieces || []).map((piece) => piece.id));
    return (b?.pieces || []).every((piece) => !ids.has(piece.id));
  }
  function workerTrolleyScorePairIsAllowed(lowScore, highScore) {
    return TROLLEY_LOW_SCORE_OPTIONS.includes(lowScore) && highScore >= lowScore && highScore <= TROLLEY_MAX_HIGH_SCORE && highScore - lowScore <= TROLLEY_MAX_SCORE_GAP;
  }
  function workerTrolleyPairsForScores(scoreMap, lowScore, highScore) {
    if (!workerTrolleyScorePairIsAllowed(lowScore, highScore)) return [];
    const lowBundles = scoreMap.get(lowScore) || [];
    const highBundles = scoreMap.get(highScore) || [];
    const pairs = [];
    if (lowScore === highScore) {
      for (let i = 0; i < lowBundles.length; i += 1) {
        for (let j = i + 1; j < lowBundles.length; j += 1) {
          const left = lowBundles[i];
          const right = lowBundles[j];
          if (workerTrolleyBundlesDisjoint(left, right)) pairs.push([left, right]);
        }
      }
      return pairs;
    }
    lowBundles.forEach((left) => {
      highBundles.forEach((right) => {
        if (!workerTrolleyBundlesDisjoint(left, right)) return;
        pairs.push([left, right]);
      });
    });
    return pairs;
  }
  function workerFindTrolleyBundlePair(boardState, color) {
    const scoreMap = workerTrolleyBundleCandidatesByScore(boardState, color);
    for (const lowScore of TROLLEY_LOW_SCORE_OPTIONS) {
      for (let highScore = lowScore; highScore <= Math.min(TROLLEY_MAX_HIGH_SCORE, lowScore + TROLLEY_MAX_SCORE_GAP); highScore += 1) {
        const pairs = workerTrolleyPairsForScores(scoreMap, lowScore, highScore);
        if (pairs.length) return pairs[0];
      }
    }
    return null;
  }
  function workerHasTrolleyCandidate(boardState, color) {
    return Boolean(workerFindTrolleyBundlePair(boardState, color));
  }
  function workerHasTauntCandidate(boardState, color) {
    const enemy = opponent(color);
    return piecesMatching(boardState, (piece) => piece.color === enemy && !["wall", "football", "blackHole"].includes(piece.type)).length > 0;
  }
  function workerHasBasicTrainingCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => isWorkerBasicTrainingEligible(piece, color)).length > 0;
  }
  function isWorkerBasicTrainingEligible(piece, color) {
    return Boolean(
      piece && piece.color === color && !piece.basicTraining && !BASIC_TRAINING_FORBIDDEN_TYPES.has(piece.type) && !["wall", "football", "blackHole", "colossus", "bigRook", "bigBishop"].includes(piece.type)
    );
  }
  function isWorkerMajorPiece(piece) {
    return Boolean(piece?.color && piece.regencyHeir !== true && MAJOR_PIECE_TYPES.has(piece.type));
  }
  function workerMajorPieces(boardState) {
    return piecesMatching(boardState, (piece) => isWorkerMajorPiece(piece));
  }
  function workerRandomRouletteLargeAnchors(boardState, piece, row, col) {
    const origin = workerIsLargePiece(piece) ? { row: piece.anchorRow, col: piece.anchorCol } : { row, col };
    const candidates = workerIsLargePiece(piece) ? [origin] : [
      origin,
      { row: origin.row - 1, col: origin.col },
      { row: origin.row, col: origin.col - 1 },
      { row: origin.row - 1, col: origin.col - 1 }
    ];
    const seen = /* @__PURE__ */ new Set();
    return candidates.filter((anchor) => {
      const key = `${anchor.row}:${anchor.col}`;
      if (seen.has(key)) return false;
      seen.add(key);
      const cells = colossusCells(anchor.row, anchor.col);
      if (cells.length !== 4) return false;
      return cells.every((cell) => {
        if (isBlackHoleCell(boardState, cell.row, cell.col)) return false;
        const occupant = get(boardState, cell.row, cell.col);
        return !occupant || occupant === piece || piece.id && occupant.id === piece.id;
      });
    });
  }
  function resetWorkerRandomRoulettePiece(piece, boardState) {
    clearWorkerObsoleteRestoration(boardState, piece);
    [
      "anchorRow",
      "anchorCol",
      "hp",
      "maxHp",
      "mana",
      "maxMana",
      "ammo",
      "maxAmmo",
      "facing",
      "gold",
      "windmillMode",
      "logDir",
      "logRollAfterTurn",
      "heraldJumpUnlocked",
      "bribed",
      "bribedRemaining",
      "quantum",
      "quantumNoCaptureUntil",
      "quantumFirstObservationFails",
      "babyBearGrowAtTurn",
      "babyBearGrowAtMove",
      "babyBearMoveAfterTurn",
      "bearRetaliationsRemaining",
      "tricksterMoveType",
      "tricksterPreviousAbilityForTurn",
      "reaperCaptures"
    ].forEach((key) => delete piece[key]);
  }
  function initializeWorkerRandomRoulettePiece(piece, type, boardState) {
    Object.assign(piece, randomRouletteInitialPieceState(type, sharedTurnCount(boardState)));
    if (type === "trickster") {
      const seed = `${piece.id || "trickster"}:${boardState.moveCount || 0}:roulette`;
      setWorkerTricksterAbilityType(boardState, piece, TRICKSTER_MOVEMENT_TYPES[workerStableIndex(seed, TRICKSTER_MOVEMENT_TYPES.length)] || "queen");
    }
  }
  function applyWorkerRandomRoulette(boardState, card, target, row, col, color, aiColor) {
    if (!isRandomRouletteTargetPiece(target)) return { ok: false, score: 0 };
    const largeAnchors = workerRandomRouletteLargeAnchors(boardState, target, row, col);
    const outcomeTypes = randomRouletteOutcomeTypes(target.type, largeAnchors.length > 0);
    if (!outcomeTypes.length) return { ok: false, score: 0 };
    const seed = `${card.instanceId || card.id}:${target.id || ""}:${boardState.moveCount || 0}:${row}:${col}`;
    const resultType = monochromePieceType(outcomeTypes[workerStableIndex(seed, outcomeTypes.length)], boardState.monochromeChess);
    const origin = workerIsLargePiece(target) ? { row: target.anchorRow, col: target.anchorCol } : { row, col };
    const destination = ["colossus", "bigRook", "bigBishop"].includes(resultType) ? largeAnchors[workerStableIndex(`${seed}:anchor`, largeAnchors.length)] : origin;
    if (!destination) return { ok: false, score: 0 };
    const previousValue = pieceValue(target);
    clearPieceCells(boardState, target);
    if (Array.isArray(boardState.temporaryQueens)) {
      boardState.temporaryQueens = boardState.temporaryQueens.filter((entry) => entry?.id !== target.id);
    }
    resetWorkerRandomRoulettePiece(target, boardState);
    target.type = resultType;
    target.moved = true;
    initializeWorkerRandomRoulettePiece(target, resultType, boardState);
    if (boardState.monochromeChess) target.monoShade = squareShade(destination.row, destination.col);
    if (["colossus", "bigRook", "bigBishop"].includes(resultType)) {
      target.anchorRow = destination.row;
      target.anchorCol = destination.col;
      colossusCells(destination.row, destination.col).forEach((cell) => set(boardState, cell.row, cell.col, target));
    } else {
      set(boardState, destination.row, destination.col, target);
    }
    const valueSwing = pieceValue(target) - previousValue;
    const score = (target.color === aiColor ? 1 : -1) * valueSwing * 0.84;
    boardState.turn = color;
    return { ok: true, score };
  }
  function clearWorkerObsoleteRestoration(boardState, piece) {
    if (!piece || typeof piece !== "object") return;
    if (boardState && piece.id && Array.isArray(boardState.necromancy)) {
      boardState.necromancy = boardState.necromancy.filter((entry) => entry?.id !== piece.id);
    }
    delete piece.necromancy;
    delete piece.necromancyRemaining;
    delete piece.quantum;
    delete piece.quantumNoCaptureUntil;
    delete piece.quantumFirstObservationFails;
  }
  function workerPawnStormDirection(boardState, color) {
    return usesSeptember18Balance(boardState) ? pawnDir(boardState, color) : color === "white" ? -1 : 1;
  }
  function workerCanPawnStormAdvance(boardState, piece, row, col, color) {
    if (!piece || piece.color !== color || piece.type !== "pawn") return false;
    if (isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return false;
    const nextRow = row + workerPawnStormDirection(boardState, color);
    return inBounds(nextRow, col, boardState) && !get(boardState, nextRow, col);
  }
  function markWorkerCardNoCaptureThisTurn(boardState, piece) {
    if (!piece?.color) return;
    piece.cardNoCaptureUntil = (Number(boardState?.turnsTaken?.[piece.color]) || 0) + 1;
  }
  function isWorkerCardNoCaptureActive(boardState, piece) {
    // Site parity (2026-09-20): a piece under promotionRush may only move, not capture, that turn
    // (the site's worker returns true here while the rush is active; without it the engine allowed
    // capturing queen-ray moves that the real worker never generates).
    if (isPromotionRushActive(piece, boardState?.turnsTaken?.[piece?.color] || 0)) return true;
    if (!piece?.cardNoCaptureUntil) return false;
    return (Number(boardState?.turnsTaken?.[piece.color]) || 0) < piece.cardNoCaptureUntil;
  }
  function isWorkerQuantumCaptureLocked(boardState, piece) {
    if (!piece?.color) return false;
    if (boardState?.quantumPending?.[piece.color]) return true;
    if (!piece.quantumNoCaptureUntil) return false;
    return (Number(boardState?.turnsTaken?.[piece.color]) || 0) < piece.quantumNoCaptureUntil;
  }
  function isWorkerMannerCaptureLocked(boardState, piece) {
    if (piece?.repositionSecondMove) return true;
    const turnsTaken = Number(boardState?.turnsTaken?.[piece?.color]) || 0;
    if (piece?.freshNoCaptureUntil && turnsTaken < piece.freshNoCaptureUntil) return true;
    if (piece?.type === "monster") return false;
    if (!piece?.color) return false;
    if (boardState?.freeMoveCaptureLock?.[piece.color]) return true;
    if (isWorkerCardNoCaptureActive(boardState, piece)) return true;
    const mannerActive = Boolean(boardState?.coolGuy || boardState?.manner?.[piece.color] || piece.potionManner);
    if (mannerActive && piece.coolGuyCapturedLast) return true;
    if (isWorkerCheckerPiece(piece)) return false;
    return isWorkerQuantumCaptureLocked(boardState, piece);
  }
  function workerPawnStormTargets(boardState, color) {
    return piecesMatching(boardState, (piece, row, col) => workerCanPawnStormAdvance(boardState, piece, row, col, color)).sort((a, b) => (color === "white" ? a.row - b.row : b.row - a.row) || a.col - b.col);
  }
  function workerCanChargePawn(boardState, piece, row, col, color) {
    if (!piece || piece.color !== color || piece.type !== "pawn") return false;
    if (isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return false;
    const dir = pawnDir(boardState, color);
    for (let step = 1; step <= 3; step += 1) {
      const nextRow = row + dir * step;
      if (!inBounds(nextRow, col, boardState) || get(boardState, nextRow, col)) return false;
    }
    return true;
  }
  function workerHasChargeCandidate(boardState, color) {
    return piecesMatching(boardState, (piece, row, col) => workerCanChargePawn(boardState, piece, row, col, color)).length > 0;
  }
  function clearWorkerChargeRush(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color) delete piece.chargeRush;
    });
  }
  function resolveWorkerChargeRushFailures(boardState, color) {
    const deaths = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || piece.chargeRush !== true) return;
      delete piece.chargeRush;
      if (piece.type !== "pawn") return;
      deaths.push({ item: piece, row, col });
      clearPieceCells(boardState, piece);
      if (boardState.captures?.[opponent(color)]) boardState.captures[opponent(color)].push(piece);
    });
    if (deaths.length) {
      resolveWorkerReaperNearbyDeaths(boardState, deaths, opponent(color));
      cancelWorkerPropheciesByCapture(boardState);
    }
  }
  function workerFindProphecyRoyal(boardState, color) {
    return findWorkerKingRole(boardState, color);
  }
  function workerHasProphecyCandidate(boardState, color) {
    return Boolean(workerFindProphecyRoyal(boardState, color)) && !boardState.prophecy?.[color];
  }
  function applyWorkerInsight(boardState, color) {
    const enemy = opponent(color);
    let removed = 0;
    forEachPiece(boardState, (piece) => {
      if (!piece) return;
      const result = clearInsightPieceEffects(piece, color, {
        coolGuy: boardState.coolGuy,
        saturationRule: boardState.saturationRule
      });
      if (result.evasionRemoved) resetWorkerMovingProgressAfterEvasionLoss(boardState, piece);
      removed += result.removed;
    });
    if (Array.isArray(boardState.feudalContracts)) {
      boardState.feudalContracts = boardState.feudalContracts.filter((entry) => entry?.color !== enemy);
    }
    if (Array.isArray(boardState.palaces)) {
      const beforePalaces = boardState.palaces.length;
      boardState.palaces = boardState.palaces.filter((palace) => palace?.color !== color);
      removed += beforePalaces - boardState.palaces.length;
    }
    const chainBonds = normalizeChainBonds(boardState.chainBonds);
    boardState.chainBonds = chainBonds.filter((bond) => !isChainBondHostileTo(bond, color));
    removed += chainBonds.length - boardState.chainBonds.length;
    if (boardState.hallucination?.[color]?.color === enemy) {
      boardState.hallucination[color] = null;
      removed += 1;
    }
    if (boardState.effects?.pawnReverse?.[color] > 0) {
      boardState.effects.pawnReverse[color] = 0;
      removed += 1;
    }
    if (boardState.taunt?.[color] > 0) {
      boardState.taunt[color] = 0;
      removed += 1;
    }
    if (boardState.initiative?.[color]) {
      boardState.initiative[color] = null;
      removed += 1;
    }
    if (boardState.diceLocks?.[color]) {
      boardState.diceLocks[color] = null;
      removed += 1;
    }
    if (boardState.knightInjury?.[color]) {
      boardState.knightInjury[color] = false;
      removed += 1;
    }
    const exhaustion = normalizeExhaustionState(boardState.exhaustion);
    if (exhaustion[color].enabled) {
      exhaustion[color] = { enabled: false, pieceId: "", count: 0 };
      boardState.exhaustion = exhaustion;
      removed += 1;
    }
    if (boardState.mistakeCard?.[color]) {
      boardState.mistakeCard[color] = false;
      removed += 1;
    }
    if (boardState.zugzwang?.[color]) {
      boardState.zugzwang[color] = false;
      removed += 1;
    }
    if (boardState.socialism?.[color] > 0) {
      boardState.socialism[color] = 0;
      removed += 1;
    }
    if (boardState.royalCommand?.[enemy]) {
      boardState.royalCommand[enemy] = null;
      removed += 1;
    }
    if (boardState.winterKingdom?.frozenIds?.length) {
      boardState.winterKingdom.frozenIds = boardState.winterKingdom.frozenIds.filter((id) => {
        const found = findWorkerPieceByRef(boardState, { id });
        return found?.piece?.color !== color;
      });
    }
    return removed;
  }
  function workerWinterEligiblePieces(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && !isWorkerRoyalIdentityPiece(boardState, piece) && !["wall", "football", "blackHole", "scarecrow"].includes(piece.type));
  }
  function workerFreezeCardAvailability(boardState, color) {
    const eligible = workerWinterEligiblePieces(boardState, opponent(color));
    return {
      blockedByLastWarmth: isLastWarmthActive(eligible.length),
      candidates: eligible.filter(({ piece }) => !isFrozenPiece(piece))
    };
  }
  function workerQueensGambitPawns(boardState, color, queenCol, randomCol = queenCol) {
    return piecesMatching(boardState, (piece, row, col) => isQueensGambitProtectedPawn(piece, col, color, queenCol, randomCol));
  }
  function isWorkerQueensGambitQueen(boardState, piece, color) {
    return !boardState.regency?.[color] && isWorkerNonRoyalQueen(boardState, piece, color);
  }
  function workerHasQueensGambitCandidate(boardState, color) {
    return piecesMatching(boardState, (piece) => isWorkerQueensGambitQueen(boardState, piece, color)).length > 0;
  }
  function workerQxe1UsurpationPlan(boardState, color) {
    if (!["white", "black"].includes(color)) return null;
    const kings = piecesMatching(boardState, (piece) => piece.color === color && isWorkerNativeKing(piece));
    const queens = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "queen");
    const plans = [];
    kings.forEach((king) => {
      queens.forEach((queen) => {
        const dr = king.row - queen.row;
        const dc = king.col - queen.col;
        if (!(dr === 0 || dc === 0 || Math.abs(dr) === Math.abs(dc))) return;
        const stepRow = Math.sign(dr);
        const stepCol = Math.sign(dc);
        let nextRow = queen.row + stepRow;
        let nextCol = queen.col + stepCol;
        while (nextRow !== king.row || nextCol !== king.col) {
          if (get(boardState, nextRow, nextCol)) return;
          nextRow += stepRow;
          nextCol += stepCol;
        }
        plans.push({ throne: { row: king.row, col: king.col }, king: king.piece, queen });
      });
    });
    plans.sort((first, second) => {
      const firstDistance = Math.max(Math.abs(first.throne.row - first.queen.row), Math.abs(first.throne.col - first.queen.col));
      const secondDistance = Math.max(Math.abs(second.throne.row - second.queen.row), Math.abs(second.throne.col - second.queen.col));
      return firstDistance - secondDistance || first.throne.row - second.throne.row || first.throne.col - second.throne.col || first.queen.row - second.queen.row || first.queen.col - second.queen.col;
    });
    return plans[0] || null;
  }
  function workerReplayCapturePrefixMatches(current, expected) {
    if (!Array.isArray(current) || !Array.isArray(expected) || current.length < expected.length) return false;
    return expected.every((piece, index) => String(current[index]?.id || `${current[index]?.color || ""}:${current[index]?.type || ""}`) === String(piece?.id || `${piece?.color || ""}:${piece?.type || ""}`));
  }
  function workerReplayResurrectionPrefixMatches(current, expected) {
    if (!Array.isArray(current) || !Array.isArray(expected) || current.length < expected.length) return false;
    return expected.every((entry, index) => String(current[index]?.id || "") === String(entry?.id || "") && Number(current[index]?.dueMoveCount) === Number(entry?.dueMoveCount));
  }
  function canWorkerReplayLastMove(boardState, color) {
    const replay = boardState.moveReplay?.[color];
    return Boolean(
      replay?.delta?.length && replayDeltaMatchesBoard(boardState.board, replay.delta) && workerReplayCapturePrefixMatches(boardState.captures?.[color] || [], replay.capturesAfter || [])
    );
  }
  function restoreWorkerReplayBoardDelta(boardState, delta) {
    (delta || []).forEach((entry) => {
      if (inBounds(entry.row, entry.col, boardState)) set(boardState, entry.row, entry.col, null);
    });
    const restored = /* @__PURE__ */ new Map();
    (delta || []).forEach((entry) => {
      if (!entry.before || !inBounds(entry.row, entry.col, boardState)) return;
      const id = entry.before.id || "";
      let item = id ? restored.get(id) : null;
      if (!item) item = clonePlain(entry.before);
      if (id) restored.set(id, item);
      set(boardState, entry.row, entry.col, item);
    });
  }
  function workerCanResolveUntargetedCard(boardState, card, color) {
    const effect = card?.effect || "";
    const enemy = opponent(color);
    if (activeWorkerForcedExtraMove(boardState, color)) return false;
    if (effect === "replayMove") return canWorkerReplayLastMove(boardState, color);
    if (effect === "thief" && usesThiefRemake(boardState)) return thiefQueenCandidates(boardState.board, color, (item) => isWorkerKingRole(boardState, item)).length > 0;
    if (effect === "brutus") return workerCountPiecesOf(boardState, color, "rook") > 0;
    // Grafted from engine.optimized.js (our-only addition): "collapse"
    // (one-shot, distinct from the existing RULE card periodicCollapse) is
    // genuinely absent from aiWorker-raw.js entirely -- confirmed by
    // grepping the whole real file, found nowhere but generic scoring
    // bucket lists. Untargeted, always resolvable; apply/tick logic
    // grafted into applyCardActionUnchecked and resolveWorkerOneShotCollapse.
    if (effect === "collapse") return true;
    if (effect === "frontlineResponse") return !boardState.frontlineResponse?.[color];
    if (effect === "relay") return !boardState.relay?.[color];
    if (effect === "fieldPromotion") return !boardState.fieldPromotion?.[color] && workerCountPiecesOf(boardState, color, "pawn") > 0;
    if (effect === "hypocrisy") {
      let open = 0;
      for (let row = 0; row < boardRowCount(boardState); row += 1) for (let col = 0; col < boardColCount(boardState); col += 1) {
        if (workerOpenPlacementSquare(boardState, row, col) && !isWorkerCollapsedSquare(boardState, row, col) && !isBlackHoleCell(boardState, row, col)) open += 1;
      }
      return open >= 4;
    }
    if (effect === "cleanupPieces") return piecesMatching(boardState, (piece) => piece.color === color && !isWorkerRoyalIdentityPiece(boardState, piece) && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)).length > 0;
    if (usesSeptember18Balance(boardState) && ["campfire", "reversal"].includes(effect)) return piecesMatching(boardState, (piece) => piece.color === color && ["knight", "bishop", "camel"].includes(piece.type) && !isWorkerKingRole(boardState, piece)).length > 0;
    if (["siegeRam", "magicGirl", "berserker", "slime", "trickster", "campfire", "princess"].includes(effect)) return workerCountPiecesOf(boardState, color, "rook") > 0;
    if (["siren", "undead", "hedgehog"].includes(effect)) return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0;
    if (effect === "suspiciousPotion") return piecesMatching(boardState, (piece) => COLORS.includes(piece.color) && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)).length > 0;
    if (effect === "kingOfTheHill") return !boardState.hillKing?.[color];
    if (effect === "genevaConvention") return !boardState.genevaConvention?.[color];
    if (effect === "blackMagic") return Boolean(findWorkerKingRole(boardState, color));
    if (effect === "fleetingDream") return piecesMatching(boardState, (piece) => piece.color === enemy && piece.promotedFromPawn).length > 0;
    if (effect === "platformRule") return !boardState.platformRule?.enabled;
    if (effect === "qxe1") return Boolean(workerQxe1UsurpationPlan(boardState, color));
    if (effect === "imperialStudies") return !boardState.imperialStudies?.[color] && Boolean(findWorkerKingRole(boardState, color));
    if (effect === "religiousVictory") return !boardState.religiousVictory?.[color];
    if (effect === "callingCard") return piecesMatching(boardState, (piece) => piece.color === enemy && piece.type !== "pawn" && !["guard", "jester"].some((type) => pieceHasAbility(piece, type)) && !isWorkerNativeKing(piece) && !isWorkerRegencyRoyalHeir(boardState, piece)).length > 0;
    if (effect === "zugzwang") return workerZugzwangKingMoves(boardState, enemy).length > 0;
    if (effect === "armistice") return true;
    if (effect === "exhaustion") return !normalizeExhaustionState(boardState.exhaustion)[enemy].enabled;
    if (effect === "mistakeCard") return !boardState.mistakeCard?.[enemy];
    if (effect === "democracy") return !boardState.democracy?.[color] && workerCountPiecesOf(boardState, color, "pawn") > 0;
    if (effect === "icbm") return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0 && piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, enemy)).length > 0;
    if (effect === "guard") return Boolean(workerGuardPawnSquare(boardState, color));
    if (effect === "reaper") return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0;
    if (effect === "freeze") {
      const availability = workerFreezeCardAvailability(boardState, color);
      return !availability.blockedByLastWarmth && availability.candidates.length > 0;
    }
    if (effect === "sacrifice") return workerHasSacrificeCandidate(boardState, color);
    if (effect === "joker") return workerReusableActiveCards(boardState, color, card).length > 0;
    if (["clonePassive", "vanish"].includes(effect)) return true;
    if (effect === "trojanHorse") {
      return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight" && !piece.trojanHorse).length > 0;
    }
    if (effect === "madHorse") return true;
    if (effect === "evasion") return workerEvasionEligiblePieces(boardState, color).length > 0;
    if (effect === "moving") {
      const movingState = normalizeMovingState(boardState.moving);
      return !movingState[color].enabled && workerEvasionEligiblePieces(boardState, color, { requireUnmarked: false }).length > 0;
    }
    if (effect === "substitution") return workerHasSubstitutionCandidate(boardState, color);
    if (effect === "cornerKick") return !boardState.cornerKick?.[color] && workerCountPiecesOf(boardState, color, "knight") > 0;
    if (effect === "conversion") return workerCountPiecesOf(boardState, color, "knight") > 0;
    if (effect === "summonColossus") return workerHasSummonColossusCandidate(boardState, color);
    if (effect === "bigRook") return boardRowCount(boardState) === 8 && boardColCount(boardState) === 8 && workerCountPiecesOf(boardState, color, "king") > 0;
    if (effect === "merchantGuild") return workerHasMerchantGuildCandidate(boardState, color);
    if (["constitutionalMonarchy", "jester", "wizard"].includes(effect)) return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0;
    if (effect === "reformation") return workerCountPiecesOf(boardState, color, "bishop") > 0;
    if (effect === "fianchetto") return !boardState.fianchetto?.[color] && workerCountPiecesOf(boardState, color, "bishop") > 0;
    if (effect === "dutch") return ["rook", "bishop", "knight"].some((type) => workerCountPiecesOf(boardState, color, type) > 0);
    if (effect === "pawnConversion") return !boardState.pawnConversion?.[color] && workerCountPiecesOf(boardState, color, "pawn") > 0;
    if (["lastStand", "earlyPromotion", "fastGrowth", "retreat", "breakthroughOrder", "enPassantBang"].includes(effect)) return workerCountPiecesOf(boardState, color, "pawn") > 0;
    if (effect === "martyrdom") return workerCountPiecesOf(boardState, color, "bishop") > 0 && workerCountPiecesOf(boardState, color, "pawn") > 0;
    if (effect === "queensGambit") return workerHasQueensGambitCandidate(boardState, color);
    if (effect === "taunt") return workerHasTauntCandidate(boardState, color);
    if (effect === "hallucination") return piecesMatching(boardState, (piece) => piece.color === color && !["wall", "football", "blackHole"].includes(piece.type)).length > 0;
    if (effect === "basicTraining") return workerHasBasicTrainingCandidate(boardState, color);
    if (effect === "blueJeans") return workerMajorPieces(boardState).length > 0;
    if (["shotgunKing", "horseRiding", "knightmate", "horde", "encouragement"].includes(effect)) return Boolean(findWorkerKingRole(boardState, color));
    if (effect === "racingKing") return Boolean(findWorkerRacingKingRole(boardState, color));
    if (effect === "switcheroo") return workerHasSwitcherooCandidate(boardState, color);
    if (effect === "undergroundBunker") return workerHasUndergroundBunkerCandidate(boardState, color);
    if (effect === "reversePawns") return workerCountPiecesOf(boardState, enemy, "pawn") > 0;
    if (effect === "queenCavalry") return workerHasQueenCavalryCandidate(boardState, color);
    if (effect === "eagle") return workerCountPiecesOf(boardState, color, "knight") > 0;
    if (effect === "amazon") return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0 && workerCountPiecesOf(boardState, color, "knight") >= 1;
    if (["radicalCharge", "backwardKnight"].includes(effect)) return workerCountPiecesOf(boardState, color, "knight") > 0;
    if (effect === "bishopSnipe") return workerCountPiecesOf(boardState, color, "bishop") > 0;
    if (effect === "palace") return Boolean(findWorkerKingRole(boardState, enemy));
    if (effect === "feudalContract") return workerHasFeudalContractCandidate(boardState, color);
    if (effect === "ironMonarch") return Boolean(findWorkerKingRole(boardState, color));
    if (effect === "lastResistance") return criticalPieces(boardState, color).length > 0;
    if (effect === "binaMate") return !boardState.binaMate?.[color];
    if (effect === "overwhelm") return !boardState.overwhelm?.[color];
    if (effect === "exile") return workerHasExileCandidate(boardState, color);
    if (effect === "royalShield") return piecesMatching(boardState, (piece) => piece.color === color && !["wall", "scarecrow"].includes(piece.type) && !piece.shielded).length > 0;
    if (effect === "witchTrial") return workerHasEnemyNonRoyalPiece(boardState, color, { excludeMerchant: true, excludeVip: true, excludeBigRook: true, excludeFootball: true });
    if (effect === "disarm") return workerHasEnemyNonRoyalPiece(boardState, color, { excludeFootball: true });
    if (["severance", "inertia"].includes(effect)) return piecesMatching(boardState, (piece) => piece.color === enemy && !isWorkerRoyalIdentityPiece(boardState, piece) && workerIsIceSheetRangedPiece(boardState, piece)).length > 0;
    if (effect === "injury") return workerHasKnightFamilyMovementCandidate(boardState, enemy);
    if (effect === "insight") return true;
    if (effect === "prophecy") return workerHasProphecyCandidate(boardState, color);
    if (effect === "ruleTicket") return workerRuleTicketCardPool(boardState).length > 0;
    if (effect === "cleanupSacrifice") return workerHasCleanupSacrificeCandidate(boardState, color);
    if (effect === "wizard") return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0;
    if (effect === "shotgunKing") return Boolean(findWorkerKingRole(boardState, color));
    if (effect === "fileSurge") return !boardState.fileSurge?.[color] && piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight").length > 0;
    if (effect === "rookLift") return !boardState.rookLift?.[color] && piecesMatching(boardState, (piece) => piece.color === color && piece.type === "rook").length > 0;
    if (effect === "iceSheet") return piecesMatching(boardState, (piece) => piece.color === enemy && workerIsIceSheetEffectTarget(boardState, piece)).length > 0;
    if (effect === "underpromotion") return !boardState.underpromotion?.[color] && piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn").length > 0;
    if (effect === "finalWeapon") return !boardState.finalWeapon?.[color] && piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn").length > 0;
    if (effect === "queenAfterimage") return !boardState.afterimageQueen?.[color] && piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length > 0;
    if (effect === "whiteBox" || effect === "blackBox") return true;
    if (effect === "checker") return workerHasCheckerCandidate(boardState, color);
    if (effect === "recycling") return !boardState.recycling;
    if (effect === "canceling") return canWorkerCancelingAffect(boardState, enemy);
    if (effect === "fanaticalRitual") return workerHasEnemyNonRoyalPiece(boardState, color, { excludeMerchant: true });
    if (effect === "traitor") return Boolean(weakestEnemyPiece(boardState, color));
    if (effect === "vortex") return workerVortexCandidateCount(boardState, color) >= 2 && !workerVortexHasUnsafeShuffle(boardState, color);
    if (effect === "panic") return workerPanicTargets(boardState, color).length >= 2;
    if (effect === "trolley") return workerHasTrolleyCandidate(boardState, enemy);
    if (effect === "mongolianGambit") return piecesMatching(boardState, (piece) => piece.color === color && !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "football"].includes(piece.type)).length > 0;
    if (effect === "alehkineMachineGun" || effect === "alekhineMachineGun") return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length && piecesMatching(boardState, (piece) => piece.color === color && piece.type === "rook").length >= 2;
    if (effect === "charge") return workerHasChargeCandidate(boardState, color);
    return true;
  }
  function findWorkerRoyalKing(boardState, color) {
    let found = null;
    forEachPiece(boardState, (piece, row, col) => {
      if (!found && piece?.color === color && isWorkerKingRole(boardState, piece)) found = { piece, row, col };
    });
    return found;
  }
  function workerFileLabels(count) {
    const labels = [];
    for (let index = 0; index < count; index += 1) {
      labels.push(index < 26 ? String.fromCharCode(97 + index) : `x${index + 1}`);
    }
    return labels;
  }
  function parseWorkerSquare(boardState, square) {
    const match = String(square || "").match(/^([a-z]+)(\d+)$/i);
    if (!match) return { row: -1, col: -1 };
    const rank = Number(match[2]);
    return {
      row: boardRowCount(boardState) - rank,
      col: workerFileLabels(boardColCount(boardState)).indexOf(match[1].toLowerCase())
    };
  }
  function workerOriginalKingSquare(boardState, king, color) {
    const origin = king?.origin ? parseWorkerSquare(boardState, king.origin) : null;
    if (origin && inBounds(origin.row, origin.col, boardState)) return origin;
    return {
      row: color === "white" ? boardRowCount(boardState) - 1 : 0,
      col: Math.min(boardColCount(boardState) - 1, Math.floor(boardColCount(boardState) / 2))
    };
  }
  function canWorkerCancelingAffect(boardState, color) {
    if (!boardState?.castled?.[color]) return false;
    const kingSquare = findWorkerRoyalKing(boardState, color);
    if (!kingSquare) return false;
    const origin = workerOriginalKingSquare(boardState, kingSquare.piece, color);
    if (!inBounds(origin.row, origin.col, boardState)) return false;
    if (kingSquare.row === origin.row && kingSquare.col === origin.col) return false;
    return true;
  }
  function forceWorkerRemovePieceAt(boardState, row, col, capturerColor, aiColor) {
    const captured = get(boardState, row, col);
    if (!captured) return 0;
    if (captured.type === "colossus") clearPieceCells(boardState, captured);
    else set(boardState, row, col, null);
    recordWorkerCapturedPieces(boardState, capturerColor, [captured]);
    if (!isWorkerDecisiveCaptureTarget(boardState, captured)) return 0;
    const attackerColor = captured.color === capturerColor ? opponent(capturerColor) : capturerColor;
    return workerCriticalCaptureScore(boardState, captured, attackerColor, aiColor);
  }
  function applyWorkerCanceling(boardState, color, capturerColor, aiColor) {
    if (!canWorkerCancelingAffect(boardState, color)) return false;
    const kingSquare = findWorkerRoyalKing(boardState, color);
    const origin = workerOriginalKingSquare(boardState, kingSquare.piece, color);
    const occupant = get(boardState, origin.row, origin.col);
    if (occupant && occupant !== kingSquare.piece) {
      forceWorkerRemovePieceAt(boardState, origin.row, origin.col, capturerColor, aiColor);
    }
    set(boardState, kingSquare.row, kingSquare.col, null);
    set(boardState, origin.row, origin.col, kingSquare.piece);
    kingSquare.piece.moved = true;
    if (boardState.monochromeChess) kingSquare.piece.monoShade = squareShade(origin.row, origin.col);
    if (!boardState.castled) boardState.castled = { white: false, black: false };
    boardState.castled[color] = false;
    boardState.enPassant = null;
    if (isWorkerCollapsedSquare(boardState, origin.row, origin.col)) {
      forceWorkerRemovePieceAt(boardState, origin.row, origin.col, capturerColor, aiColor);
    }
    return true;
  }
  function isWorkerSeptemberMajestyDestinationBlocked(boardState, item, destinations) {
    if (!item?.color || !boardState.majesty?.[opponent(item.color)] || !isWorkerMajorPiece(item)) return false;
    const royalCells = piecesMatching(boardState, (other) => other.color === opponent(item.color) && isWorkerKingRole(boardState, other));
    return septemberMajestyBlocks({ enabled: true, major: true, destinations, royalCells });
  }
  function isWorkerSeptemberMajestyBlockedMove(boardState, piece, row, col, move) {
    if (!piece?.color || !move || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.setLogDirection) return false;
    const blocked = (item, destinations2) => isWorkerSeptemberMajestyDestinationBlocked(boardState, item, destinations2);
    const destinations = workerFianchettoMoveDestinationCells(move);
    if (blocked(piece, destinations)) return true;
    const swapTarget = move.dragonSwap || move.substitutionSwap || move.relaySwap ? get(boardState, move.row, move.col) : null;
    if (swapTarget && blocked(swapTarget, workerFianchettoPieceCells(swapTarget, row, col))) return true;
    const twin = workerFianchettoTwinPartner(boardState, piece);
    return Boolean(twin && twin.piece !== swapTarget && blocked(twin.piece, destinations));
  }
  function workerFianchettoDiagonalKey(boardState, row, col) {
    const size = Math.min(boardRowCount(boardState), boardColCount(boardState));
    if (row === col) return "h1-a8";
    if (row + col === size - 1) return "a1-h8";
    return "";
  }
  function workerHasFianchettoBishop(boardState, color, diagonalKey) {
    if (!boardState.fianchetto?.[color] || !diagonalKey) return false;
    return piecesMatching(boardState, (piece, row, col) => piece.color === color && piece.type === "bishop" && workerFianchettoDiagonalKey(boardState, row, col) === diagonalKey).length > 0;
  }
  function workerFianchettoTwinPartner(boardState, piece) {
    if (!piece?.twinBondId || !piece.twinPartnerId) return null;
    return piecesMatching(boardState, (candidate) => candidate.id === piece.twinPartnerId && candidate.color === piece.color)[0] || null;
  }
  function workerFianchettoPieceCells(piece, row, col) {
    return workerIsLargePiece(piece) ? colossusCells(row, col) : [{ row, col }];
  }
  function workerFianchettoMoveDestinationCells(move) {
    if (Array.isArray(move?.highlightCells) && (move.colossusMove || move.bigRookMove)) return move.highlightCells;
    const destination = workerPortalMoveDestination(move);
    return destination ? [destination] : [];
  }
  function isWorkerFianchettoDestinationBlockedForPiece(boardState, piece, row, col, destinations) {
    if (isWorkerSeptemberMajestyDestinationBlocked(boardState, piece, Array.isArray(destinations) ? destinations : [destinations].filter(Boolean))) return true;
    if (!["white", "black"].includes(piece?.color) || !fianchettoRestrictedPieceType(piece.type)) return false;
    const targetCells = Array.isArray(destinations) ? destinations : [destinations].filter(Boolean);
    if (!targetCells.length) return false;
    const originKeys = new Set(workerFianchettoPieceCells(piece, row, col).map(({ row: originRow, col: originCol }) => workerFianchettoDiagonalKey(boardState, originRow, originCol)).filter(Boolean));
    return targetCells.some((destination) => {
      const targetKey = workerFianchettoDiagonalKey(boardState, destination.row, destination.col);
      return Boolean(targetKey && !originKeys.has(targetKey) && workerHasFianchettoBishop(boardState, opponent(piece.color), targetKey));
    });
  }
  function isWorkerFianchettoBlockedMove(boardState, piece, row, col, move) {
    if (!piece?.color || !move) return false;
    if (move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.setLogDirection) return false;
    let swapTarget = null;
    if (move.dragonSwap || move.substitutionSwap || move.relaySwap) {
      swapTarget = get(boardState, move.row, move.col);
      if (swapTarget) {
        const swapDestinations = workerIsLargePiece(swapTarget) ? colossusCells(row, col) : [{ row, col }];
        if (isWorkerFianchettoDestinationBlockedForPiece(boardState, swapTarget, move.row, move.col, swapDestinations)) return true;
      }
    }
    const destinations = workerFianchettoMoveDestinationCells(move);
    if (!destinations.length) return false;
    if (isWorkerFianchettoDestinationBlockedForPiece(boardState, piece, row, col, destinations)) return true;
    const partner = workerFianchettoTwinPartner(boardState, piece);
    if (partner && partner.piece !== swapTarget && isWorkerFianchettoDestinationBlockedForPiece(boardState, partner.piece, partner.row, partner.col, destinations)) return true;
    return false;
  }
  function workerActiveRuleIds(boardState) {
    const ids = /* @__PURE__ */ new Set();
    if (boardState?.appliedRuleCard?.id) ids.add(boardState.appliedRuleCard.id);
    (boardState?.additionalRuleCards || []).forEach((card) => {
      if (card?.id) ids.add(card.id);
    });
    (boardState?.pendingRuleTickets || []).forEach((entry) => {
      if (entry?.ruleId) ids.add(entry.ruleId);
    });
    return ids;
  }
  function workerRuleTicketCardPool(boardState) {
    const active = workerActiveRuleIds(boardState);
    return WORKER_RULE_TICKET_CANDIDATES.filter((card) => card && !WORKER_RULE_TICKET_EXCLUDED_RULE_IDS.has(card.id) && !active.has(card.id) && !workerRuleTicketHasConflict(boardState, card, active));
  }
  function workerRuleTicketHasConflict(boardState, cardOrId, activeIds = workerActiveRuleIds(boardState)) {
    return false;
  }
  function workerRuleTicketScore(card, color) {
    if (!card) return 0;
    const base = (CARD_EFFECT_VALUES[card.effect] || 0) + (Number(card.stars) || 0) * 120;
    if (card.effect === "monochromeChess") return base + 80;
    if (card.effect === "highGround") return base + 60;
    if (card.effect === "highway") return base + 55;
    if (card.effect === "blackHole") return base + 45;
    if (card.effect === "winterKingdom") return base + 35;
    if (card.effect === "recycling") return base + 30;
    return base + (color === "white" ? 1 : 0);
  }
  function applyWorkerAdditionalRuleCard(boardState, ruleId, color) {
    const card = WORKER_RULE_TICKET_CANDIDATES.find((entry) => entry.id === ruleId);
    if (!card || WORKER_RULE_TICKET_EXCLUDED_RULE_IDS.has(card.id)) return false;
    if (workerRuleTicketHasConflict(boardState)) ;
    if (!Array.isArray(boardState.additionalRuleCards)) boardState.additionalRuleCards = [];
    if (!boardState.additionalRuleCards.some((entry) => entry?.id === card.id)) {
      boardState.additionalRuleCards.push({ ...card, instanceId: `worker-rule-ticket-${card.id}` });
    }
    if (card.effect === "monochromeChess") {
      boardState.monochromeChess = true;
      forEachPiece(boardState, (piece, row, col) => {
        if (!piece || piece.type === "wall" || piece.type === "football") return;
        piece.monoShade = squareShade(row, col);
        if (piece.type === "knight") piece.type = "camel";
      });
    } else if (card.effect === "football") {
      workerPlaceRuleFootball(boardState);
    } else if (card.effect === "monsterRule") {
      workerPlaceRuleMonster(boardState);
    } else if (card.effect === "blackHole") {
      boardState.blackHole = workerCenterCells(boardState, 2);
      const removed = [];
      boardState.blackHole.forEach((cell) => {
        const item = get(boardState, cell.row, cell.col);
        if (item && item.type !== "blackHole") {
          removed.push({ item, row: targetRow, col: targetCol });
          recordWorkerCapturedPieces(boardState, opponent(item.color), [item]);
          clearPieceCells(boardState, item);
        }
        set(boardState, cell.row, cell.col, { id: `worker-black-hole-${cell.row}-${cell.col}`, color: null, type: "blackHole" });
      });
      removed.forEach((item) => {
        const captureOwner = opponent(item.color);
        if (isWorkerDecisiveCaptureTarget(boardState, item)) {
          workerCriticalCaptureScore(boardState, item, captureOwner, color);
        }
      });
      resolveWorkerBatchRoyalDefeats(boardState, removed);
    } else if (card.effect === "ruleBombs") {
      workerInstallRuleBombs(boardState);
    } else if (card.effect === "saturation") {
      boardState.saturationRule = true;
    } else if (card.effect === "periodicCollapse") {
      const currentTurn = sharedTurnCount(boardState);
      boardState.periodicCollapse = {
        enabled: true,
        interval: PERIODIC_COLLAPSE_INTERVAL,
        nextAt: nextPeriodicCollapseTurn(currentTurn)
      };
    } else if (card.effect === "platformRule") {
      const currentTurn = platformTurnCount(boardState.turnsTaken, { countUnit: "ply" });
      boardState.platformRule = normalizePlatformRule({ enabled: true, countUnit: "ply", cadence: "full-turn", nextAt: currentTurn + PLATFORM_INTERVAL_TURNS * 2 }, currentTurn, boardRowCount(boardState), boardColCount(boardState));
      spawnWorkerPlatformAtTurn(boardState, currentTurn);
    } else if (card.effect === "portal") {
      boardState.portalRule = normalizePortalRule(true, boardRowCount(boardState), boardColCount(boardState));
    } else if (card.effect === "crownRule") {
      workerEnableCrownRule(boardState);
    } else if (card.effect === "conveyorRule") {
      boardState.conveyorRule = true;
    } else if (card.effect === "mistake") {
      boardState.mistakeRule = true;
    } else if (card.effect === "transcendence") {
      boardState.transcendenceRule = true;
    } else if (card.effect === "camouflageRule") {
      boardState.camouflageRule = true;
    } else if (card.effect === "winterKingdom") {
      boardState.winterKingdom = normalizeWinterKingdom({ ...boardState.winterKingdom || {}, enabled: true });
    } else if (card.effect === "machoChess") {
      boardState.machoChess = true;
    } else if (card.effect === "highway") {
      boardState.highway = true;
    } else if (card.effect === "recycling") {
      boardState.recycling = true;
    } else if (card.effect === "highGround") {
      boardState.highGround = workerCenterCells(boardState, 2).concat(workerCenterCells(boardState, 4).slice(0, 4));
    } else if (card.effect === "revelation") {
      boardState.revelation = true;
    }
    return true;
  }
  function findWorkerKingRole(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && isWorkerKingRole(boardState, piece))[0] || null;
  }
  function findWorkerRacingKingRole(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && isWorkerKingRole(boardState, piece))[0] || null;
  }
  function workerInstallRuleBombs(boardState) {
    const candidates = ruleBombCandidateCells(boardRowCount(boardState), boardColCount(boardState));
    const empty = candidates.filter(({ row, col }) => !get(boardState, row, col));
    const occupied = candidates.filter(({ row, col }) => get(boardState, row, col));
    const ordered = [...empty, ...occupied];
    const offset = workerStableIndex(`rule-bombs:${boardState.moveCount || 0}`, Math.max(1, ordered.length));
    const rotated = ordered.length ? [...ordered.slice(offset), ...ordered.slice(0, offset)] : [];
    boardState.ruleBombs = rotated.slice(0, RULE_BOMB_COUNT).map(({ row, col }, index) => {
      const occupant = get(boardState, row, col);
      return {
        id: `worker-rule-bomb-${index}-${row}-${col}`,
        row,
        col,
        ...occupant?.id ? { ignorePieceId: occupant.id } : {}
      };
    });
    return boardState.ruleBombs.length;
  }
  function workerCenterCells(boardState, span) {
    const rows = boardRowCount(boardState);
    const cols = boardColCount(boardState);
    const startRow = Math.max(0, Math.floor((rows - span) / 2));
    const startCol = Math.max(0, Math.floor((cols - span) / 2));
    const cells = [];
    for (let row = startRow; row < Math.min(rows, startRow + span); row += 1) {
      for (let col = startCol; col < Math.min(cols, startCol + span); col += 1) {
        cells.push({ row, col });
      }
    }
    return cells;
  }
  function workerPlaceRuleFootball(boardState) {
    const candidates = workerCenterCells(boardState, 2);
    const target = candidates.find((cell) => workerSeptember12OpenSpawnSquare(boardState, cell.row, cell.col) && !isWorkerCrownGroundSquare(boardState, cell.row, cell.col)) || candidates.concat(workerCenterCells(boardState, 4)).find((cell) => workerSeptember12OpenSpawnSquare(boardState, cell.row, cell.col) && !isWorkerCrownGroundSquare(boardState, cell.row, cell.col));
    if (!target) return false;
    set(boardState, target.row, target.col, workerCreatePiece("neutral", "football", boardState, target.row, target.col));
    return true;
  }
  function workerPlaceRuleMonster(boardState) {
    const center = workerCenterCells(boardState, 2);
    const direct = center.filter(({ row, col }) => workerSeptember12OpenSpawnSquare(boardState, row, col) && !isWorkerCrownGroundSquare(boardState, row, col));
    let candidates = direct;
    if (!candidates.length) {
      const centerRow = center.reduce((sum, cell) => sum + cell.row, 0) / Math.max(1, center.length);
      const centerCol = center.reduce((sum, cell) => sum + cell.col, 0) / Math.max(1, center.length);
      candidates = [];
      for (let row = 0; row < boardRowCount(boardState); row += 1) {
        for (let col = 0; col < boardColCount(boardState); col += 1) {
          if (!workerSeptember12OpenSpawnSquare(boardState, row, col) || isWorkerCrownGroundSquare(boardState, row, col)) continue;
          candidates.push({
            row,
            col,
            distance: Math.abs(row - centerRow) + Math.abs(col - centerCol)
          });
        }
      }
      candidates.sort((a, b) => a.distance - b.distance || a.row - b.row || a.col - b.col);
      if (candidates.length) {
        const nearestDistance = candidates[0].distance;
        candidates = candidates.filter(({ distance }) => distance === nearestDistance);
      }
    }
    if (!candidates.length) return false;
    const target = candidates[workerStableIndex(`monster-spawn:${boardState.moveCount || 0}`, candidates.length)];
    set(boardState, target.row, target.col, workerCreatePiece("neutral", "monster", boardState, target.row, target.col));
    return true;
  }
  function moveWorkerRuleMonsters(boardState, movingColor = "black") {
    const monsters = [];
    const seen = /* @__PURE__ */ new Set();
    forEachPieceCached(boardState, (item, row, col) => {
      if (item?.type !== "monster" || seen.has(item.id)) return;
      seen.add(item.id);
      monsters.push({ item, row, col });
    });
    let moved = 0;
    monsters.forEach(({ item, row, col }) => {
      if (get(boardState, row, col) !== item || isPoisonStunned(item)) return;
      const candidates = monsterStepCandidates(
        row,
        col,
        boardRowCount(boardState),
        boardColCount(boardState),
        (nextRow, nextCol) => {
          if (isWorkerCollapsedSquare(boardState, nextRow, nextCol) || isWorkerCrownGroundSquare(boardState, nextRow, nextCol) || !legacySeptember12RulesStates.has(boardState) && (isWorkerPendingSpawnReservedSquare(boardState, nextRow, nextCol) || isWorkerPortalMovementReservedSquare(boardState, nextRow, nextCol))) return false;
          const target = get(boardState, nextRow, nextCol);
          return !target || canWorkerCaptureTarget("neutral", target, item, boardState, { ignoreSaturation: true });
        }
      );
      if (!candidates.length) return;
      const destination = candidates[workerStableIndex(`${item.id}:${boardState.moveCount || 0}`, candidates.length)];
      const captured = get(boardState, destination.row, destination.col);
      if (captured) {
        const captureOwner = opponent(captured.color);
        clearPieceCells(boardState, captured);
        recordWorkerCapturedPieces(boardState, captureOwner, [captured]);
        workerCriticalCaptureScore(boardState, captured, captureOwner, captureOwner);
        resolveWorkerReaperNearbyDeaths(
          boardState,
          [{ item: captured, row: destination.row, col: destination.col }],
          captureOwner,
          {}
        );
        if (captured.poisonedPawn) {
          item.poisonStunTurns = Math.max(Number(item.poisonStunTurns) || 0, 3);
          item.poisonStunColor = movingColor;
        }
      }
      set(boardState, row, col, null);
      set(boardState, destination.row, destination.col, item);
      item.moved = true;
      noteWorkerUltimatumMovement(boardState, item);
      moved += 1;
    });
    return moved;
  }
  function tickWorkerPendingRuleTicketsForTurnStart(boardState, color = boardState.turn) {
    if (!Array.isArray(boardState.pendingRuleTickets) || !boardState.pendingRuleTickets.length) return 0;
    const due = [];
    const next = [];
    boardState.pendingRuleTickets.forEach((entry) => {
      const owner = entry?.color === "black" ? "black" : "white";
      const startTurnCount = Number(entry?.startTurnCount);
      if (Number.isFinite(startTurnCount)) {
        const ownerTurnsTaken = Math.max(0, Number(boardState.turnsTaken?.[owner]) || 0);
        if (owner === color && ownerTurnsTaken > Math.max(0, Math.floor(startTurnCount))) due.push(entry);
        else next.push(entry);
        return;
      }
      const remaining = Math.max(0, Number(entry?.remainingHalfTurns) || 0) - 1;
      if (owner === color && remaining <= 0) due.push(entry);
      else next.push({ ...entry, remainingHalfTurns: remaining });
    });
    boardState.pendingRuleTickets = next;
    due.forEach((entry) => applyWorkerAdditionalRuleCard(boardState, entry.ruleId, entry.color || boardState.turn));
    return due.length;
  }
  function cancelWorkerPropheciesByCapture(boardState) {
    if (!boardState?.prophecy) return false;
    let canceled = false;
    ["white", "black"].forEach((color) => {
      if (!boardState.prophecy[color]) return;
      boardState.prophecy[color] = null;
      canceled = true;
    });
    return canceled;
  }
  function tickWorkerPropheciesAfterTurn(boardState) {
    if (!boardState?.prophecy || boardState.mode === "gameover") return false;
    const currentMoveCount = Number(boardState.moveCount) || 0;
    for (const color of ["white", "black"]) {
      const entry = boardState.prophecy[color];
      if (!entry) continue;
      if (Number(entry.skipMoveCount) === currentMoveCount) {
        delete entry.skipMoveCount;
        continue;
      }
      entry.remainingHalfTurns = Math.max(0, (Number(entry.remainingHalfTurns) || 0) - 1);
      if (entry.remainingHalfTurns <= 0) {
        boardState.prophecy[color] = null;
        boardState.mode = "gameover";
        boardState.winner = color;
        return true;
      }
    }
    return false;
  }
  function workerCardTargetSpec(card) {
    const effect = card?.effect || "";
    const target = card?.target || "";
    if (["campfire", "princess"].includes(effect)) return { side: "own", types: ["rook"], nonKing: true };
    if (effect === "hedgehog") return { side: "own", types: ["queen"], nonKing: true };
    if (["disarm", "witchTrial", "exile"].includes(effect)) return { side: "enemy", nonKing: true };
    if (effect === "spy") return { side: "enemy", types: ["pawn"] };
    if (["severance", "inertia"].includes(effect)) return { side: "enemy", ranged: true };
    if (effect === "iceSheet") return { side: "enemy", ranged: true, iceSheetTarget: true };
    if (effect === "reposition") return null;
    if (effect === "desperado") return { side: "own", nonKing: true };
    if (effect === "basicTraining") return { side: "own", nonKing: false };
    if (effect === "promotionRush") return { side: "own", nonKing: false };
    if (effect === "deathSquad") return { side: "own", types: ["pawn"] };
    if (["standardBearer", "log", "suicideBomber"].includes(effect)) return { side: "own", types: ["pawn"] };
    if (["pegasus", "unicorn"].includes(effect)) return { side: "own", types: ["rook"] };
    if (["assassin", "knightmaster"].includes(effect)) return { side: "own", types: ["knight"] };
    if (["ordination", "stealth"].includes(effect)) return { side: "own", types: ["bishop"] };
    if (effect === "missionary") return { side: "own", types: ["bishop"] };
    if (effect === "poisonedPawn") return { side: "own", types: ["pawn"] };
    if (effect === "freeCastling") return { side: "own", types: ["rook"] };
    if (effect === "grasshopper") return { side: "own", types: ["knight", "bishop", "camel"] };
    if (["herald", "dragon"].includes(effect)) return { side: "own", types: ["rook"] };
    if (["localConscription", "queensGambit", "babyBear"].includes(effect)) return { side: "own", types: ["queen"] };
    if (["wizard", "merchantGuild", "jester", "constitutionalMonarchy", "reaper", "amazon", "idol"].includes(effect)) return { side: "own", types: ["queen"] };
    if (target === "enemy-slider") return { side: "enemy", slider: true };
    if (target === "enemy-ranged") return { side: "enemy", ranged: true };
    if (target === "own-nonking-piece") return { side: "own", nonKing: true };
    if (target === "own-pawn") return { side: "own", types: ["pawn"] };
    if (String(target).includes("enemy")) return { side: "enemy", nonKing: true };
    return { side: "own", nonKing: true };
  }
  function workerPieceTypeSnapshot(boardState) {
    const types = /* @__PURE__ */ new Map();
    forEachPiece(boardState, (piece) => {
      if (piece) {
        const identity = piece.id || piece;
        if (types.has(identity)) return;
        types.set(identity, {
          type: piece.type,
          promotedFromPawn: piece.promotedFromPawn === true
        });
      }
    });
    return types;
  }
  function reconcileWorkerQueensGambitProtection(boardState, beforeTypes, promotionPieceIdentity = null) {
    forEachPiece(boardState, (piece) => {
      const identity = piece?.id || piece;
      const before = beforeTypes.get(identity);
      if (!before) return;
      const promotedNow = identity === promotionPieceIdentity || !before.promotedFromPawn && piece.promotedFromPawn === true;
      reconcileQueensGambitProtectionAfterTypeChange(piece, before.type, {
        promotion: promotedNow
      });
    });
  }
  // Board-keyed caches (THREE_CACHE paladin radiance, princess queen-movement,
  // standard-bearer rank, piece list) assume a board is never mutated in
  // place, but applyAction does mutate in place (card:paladin/thief convert a
  // piece, swaps move pieces) -- so anything cached while generating this
  // node's actions is stale afterwards. Found via tools/site-parity playout:
  // extra substitutionSwap moves (paladin radiance missing) and princess
  // divergences after card:thief.
  function invalidateBoardCaches(boardState) {
    if (!boardState) return;
    if (boardState.board) { THREE_CACHE.delete(boardState.board); PRINCESS_QUEEN_MOVEMENT_CACHE.delete(boardState.board); }
    STANDARD_BEARER_RANK_CACHE.delete(boardState);
    ALL_PIECES_LIST_CACHE.delete(boardState);
    if (HANG_MEMO) HANG_MEMO.delete(boardState);
  }
  function applyAction(boardState, action, aiColor) {
    invalidateBoardCaches(boardState);
    try {
      return applyActionInner(boardState, action, aiColor);
    } finally {
      invalidateBoardCaches(boardState);
    }
  }
  function applyActionInner(boardState, action, aiColor) {
    if (!action) return { ok: false, score: 0 };
    septemberBeginBoardAction(boardState);
    const beforeTypes = workerPieceTypeSnapshot(boardState);
    const promotionPiece = action.type === "promotion" ? get(boardState, action.from?.row, action.from?.col) : null;
    const promotionPieceIdentity = promotionPiece?.id || promotionPiece || null;
    const actingColor = COLORS.includes(action.color) ? action.color : boardState.turn;
    let result = { ok: false, score: 0 };
    if (action.type === "fileSurgeSkip") result = applyFileSurgeSkipAction(boardState, action, aiColor);
    else if (action.type === "promotion") result = applyWorkerPromotionAction(boardState, action, aiColor);
    else if (action.type === "shotgunReload") result = applyShotgunReloadAction(boardState, action, aiColor);
    else if (action.type === "wizardSpell") result = applyWizardSpellAction(boardState, action, aiColor);
    else if (action.type === "card") result = applyCardAction(boardState, action, aiColor);
    else if (action.type === "move") result = applyMoveAction(boardState, action, aiColor);
    if (result.ok) reconcileWorkerQueensGambitProtection(boardState, beforeTypes, promotionPieceIdentity);
    if (result.ok && ["move", "promotion"].includes(action.type)) {
      workerBreakInitiativeByCheck(boardState, actingColor);
    }
    if (result.ok && action.type === "card") {
      for (const color of COLORS) workerBreakInitiativeByCheck(boardState, color);
    }
    boardState.chainBonds = partitionChainBondsByRange(
      boardState.chainBonds,
      (id) => findWorkerPieceByRef(boardState, { id })
    ).active;
    return result;
  }
  function workerActionLimit(boardState) {
    return boardState?.acceleration ? 2 : 1;
  }
  function resolveWorkerPendingFreeMovesAfterTurn(boardState, movingColor) {
    if (!Array.isArray(boardState.pendingFreeMoves) || !boardState.pendingFreeMoves.length) return 0;
    const due = boardState.pendingFreeMoves.filter((entry) => isFreeMovePlanDue(entry, movingColor, boardState.turnsTaken));
    if (!due.length) return 0;
    const dueSet = new Set(due);
    boardState.pendingFreeMoves = boardState.pendingFreeMoves.filter((entry) => !dueSet.has(entry));
    if (!boardState.freeMoveCaptureLock) boardState.freeMoveCaptureLock = { white: false, black: false };
    let executed = 0;
    due.forEach((entry) => {
      if (!COLORS.includes(entry?.color)) return;
      (entry.moves || []).forEach((plan) => {
        const from = plan?.from;
        if (!Number.isInteger(from?.row) || !Number.isInteger(from?.col)) return;
        const piece = get(boardState, from.row, from.col);
        const located = { item: piece, row: from.row, col: from.col };
        if (!isFreeMoveSourceIntact(plan, located, entry.color)) return;
        const legal = findFreeMoveLegalCandidate(plan, generateMovesForPiece(boardState, piece, from.row, from.col).filter((move) => isWorkerMoveAllowed(boardState, piece, from.row, from.col, move)));
        if (!legal) return;
        const destination = workerPortalMoveDestination(legal);
        if (!destination) return;
        const captured = get(boardState, destination.row, destination.col);
        if (captured?.color === entry.color) return;
        if (captured) {
          clearPieceCells(boardState, captured);
          recordWorkerDirectCaptures(boardState, piece, [captured]);
          recordWorkerCapturedPieces(boardState, entry.color, [captured], piece);
          workerCriticalCaptureScore(boardState, captured, entry.color, entry.color);
        }
        clearPieceCells(boardState, piece);
        set(boardState, destination.row, destination.col, piece);
        piece.moved = true;
        executed += 1;
      });
    });
    due.forEach((entry) => {
      if (COLORS.includes(entry?.color)) boardState.freeMoveCaptureLock[entry.color] = true;
    });
    return executed;
  }
  function resolveWorkerPendingIcbmForTurn(boardState, color) {
    const pending = normalizePendingIcbm(boardState.pendingIcbm);
    const due = pending.filter((entry) => isPendingIcbmDue(entry, color, boardState.turnsTaken));
    if (!due.length) {
      boardState.pendingIcbm = pending;
      return 0;
    }
    const dueIds = new Set(due.map((entry) => entry.id));
    boardState.pendingIcbm = pending.filter((entry) => !dueIds.has(entry.id));
    let resolved = 0;
    due.forEach((entry) => {
      const source = findWorkerPieceByRef(boardState, { id: entry.sourceQueenId });
      const target = findWorkerPieceByRef(boardState, { id: entry.targetQueenId });
      if (!source || !isWorkerQueenIdentity(source.piece, color)) return;
      if (!target || !isWorkerQueenIdentity(target.piece, opponent(color))) return;
      const center = { row: target.row, col: target.col };
      if (isWorkerDecisiveCaptureTarget(boardState, source.piece)) {
        workerCriticalCaptureScore(boardState, source.piece, opponent(color), opponent(color));
      }
      clearPieceCells(boardState, source.piece);
      recordWorkerCapturedPieces(boardState, opponent(color), [source.piece]);
      const victims = [];
      const seen = /* @__PURE__ */ new Set();
      for (let dr = -1; dr <= 1; dr += 1) {
        for (let dc = -1; dc <= 1; dc += 1) {
          const row = center.row + dr;
          const col = center.col + dc;
          if (!inBounds(row, col, boardState)) continue;
          const piece = get(boardState, row, col);
          if (!piece || isFrozenPiece(piece)) continue;
          const key = piece.id || `${row}:${col}`;
          if (seen.has(key)) continue;
          seen.add(key);
          victims.push(piece);
        }
      }
      victims.forEach((piece) => {
        const capturerColor = opponent(piece.color);
        if (isWorkerDecisiveCaptureTarget(boardState, piece)) {
          workerCriticalCaptureScore(boardState, piece, capturerColor, capturerColor);
        }
        clearPieceCells(boardState, piece);
        recordWorkerCapturedPieces(boardState, capturerColor, [piece]);
      });
      resolved += 1;
    });
    return resolved;
  }
  function tickWorkerCardFrozenPieces(boardState, color) {
    const winterFrozenIds = new Set(boardState.winterKingdom?.frozenIds || []);
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!piece?.id || seen.has(piece.id) || (piece.frozenByCard?.countBy && usesSeptember18Balance(boardState) ? opponent(piece.frozenByCard.countBy) : piece.frozenByCard?.countBy ?? piece.color) !== color || !piece.frozenByCard) return;
      seen.add(piece.id);
      piece.frozenByCard.remaining = Math.max(0, Number(piece.frozenByCard.remaining) - 1);
      if (piece.frozenByCard.remaining > 0) return;
      delete piece.frozenByCard;
      if (!winterFrozenIds.has(piece.id)) delete piece.frozen;
    });
  }
  function tickWorkerSacrificeProtection(boardState, color) {
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!piece?.sacrificeProtection || piece.color !== color) return;
      const key = piece.id || piece;
      if (seen.has(key)) return;
      seen.add(key);
      piece.sacrificeProtection.remaining = Math.max(0, Number(piece.sacrificeProtection.remaining) || 0) - 1;
      if (piece.sacrificeProtection.remaining > 0) return;
      const keepProtected = Boolean(piece.sacrificeProtection.previousProtected || piece.coronationProtection || piece.lastResistance || piece.queensGambitProtection);
      delete piece.sacrificeProtection;
      if (!keepProtected) delete piece.protected;
    });
  }
  function workerVanishingPieceCandidates(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && !["wall", "football", "blackHole", "coffin"].includes(piece.type));
  }
  const campaignAuthorityV1States = /* @__PURE__ */ new WeakSet();
  const legacyCompletedTurnEffectsStates = /* @__PURE__ */ new WeakSet();
  const legacyReusablePlatformStates = /* @__PURE__ */ new WeakSet();
  const legacyWindmillMovementStates = /* @__PURE__ */ new WeakSet();
  const legacyAttackRulesStates = /* @__PURE__ */ new WeakSet();
  const legacySeptember12RulesStates = /* @__PURE__ */ new WeakSet();
  function workerSeptember12PlacementCrownBlocked(state, row, col) {
    return !legacySeptember12RulesStates.has(state) && isWorkerCrownGroundSquare(state, row, col);
  }
  function workerSeptember12OpenSpawnSquare(state, row, col) {
    return legacySeptember12RulesStates.has(state) ? !get(state, row, col) : workerOpenPlacementSquare(state, row, col);
  }
  const legacyWitchTrialCaptureStates = /* @__PURE__ */ new WeakSet();
  const legacyRacingKingPriorityStates = /* @__PURE__ */ new WeakSet();
  const legacyTimedCardUnitsStates = /* @__PURE__ */ new WeakSet();
  const legacyBloodMoonLifecycleStates = /* @__PURE__ */ new WeakSet();
  const legacyGrasshopperTargetStates = /* @__PURE__ */ new WeakSet();
  function resolveWorkerVanishingForTurnStart(boardState, color) {
    if (boardState.skipRandomVanishing) return 0;
    if (!boardState.vanishing?.white && !boardState.vanishing?.black) return 0;
    if (!["white", "black"].includes(color)) return 0;
    const pool = workerVanishingPieceCandidates(boardState, color);
    if (!pool.length) return 0;
    const chosen = pool[workerStableIndex(`vanish:${boardState.moveCount}:${color}:turn-start`, pool.length)];
    const piece = chosen?.piece;
    if (!piece) return 0;
    const captureColor = opponent(piece.color);
    if (boardState.mode !== "gameover") workerCriticalCaptureScore(boardState, piece, captureColor, captureColor);
    clearPieceCells(boardState, piece);
    recordWorkerCapturedPieces(boardState, captureColor, [piece]);
    resolveWorkerBatchRoyalDefeats(boardState, [piece]);
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
    return 1;
  }
  function workerRuleBombNormalizationLimit(value) {
    return Math.max(RULE_BOMB_COUNT, Array.isArray(value) ? value.length : 0);
  }
  function resolveWorkerRuleBombsUnderPieces(boardState, causeColor) {
    const bombs = normalizeRuleBombs(boardState.ruleBombs, boardRowCount(boardState), boardColCount(boardState), workerRuleBombNormalizationLimit(boardState.ruleBombs));
    if (!bombs.length) return 0;
    const triggered = [];
    const retained = [];
    bombs.forEach((bomb) => {
      const occupant = get(boardState, bomb.row, bomb.col);
      if (bomb.ignorePieceId && occupant?.id === bomb.ignorePieceId) {
        retained.push(bomb);
        return;
      }
      delete bomb.ignorePieceId;
      if (occupant) triggered.push(bomb);
      else retained.push(bomb);
    });
    boardState.ruleBombs = retained;
    if (!triggered.length) return 0;
    const cells = /* @__PURE__ */ new Map();
    triggered.forEach((bomb) => {
      for (let row = 0; row < boardRowCount(boardState); row += 1) cells.set(`${row}:${bomb.col}`, { row, col: bomb.col });
      for (let col = 0; col < boardColCount(boardState); col += 1) cells.set(`${bomb.row}:${col}`, { row: bomb.row, col });
    });
    const removed = [];
    const seen = /* @__PURE__ */ new Set();
    cells.forEach(({ row, col }) => {
      const piece = get(boardState, row, col);
      if (!piece || isBombImmuneNeutralPiece(piece)) return;
      const key = piece.id || piece;
      if (seen.has(key)) return;
      seen.add(key);
      removed.push(piece);
    });
    removed.forEach((piece) => clearPieceCells(boardState, piece));
    removed.forEach((piece) => {
      if (!["white", "black"].includes(piece.color)) return;
      const capturer = opponent(piece.color);
      recordWorkerCapturedPieces(boardState, capturer, [piece]);
      workerCriticalCaptureScore(boardState, piece, capturer, causeColor);
    });
    const defeatedColors = new Set(removed.filter(isCritical).map((piece) => piece.color).filter((color) => ["white", "black"].includes(color)));
    if (defeatedColors.size > 1) {
      boardState.mode = "gameover";
      boardState.winner = "";
    }
    return triggered.length;
  }
  function workerCollapseOneRing(boardState, causeColor) {
    const depth = normalizeCollapseDepth(
      boardState.collapseDepth,
      boardRowCount(boardState),
      boardColCount(boardState),
      Boolean(boardState.collapsed)
    );
    const cells = collapseRingCells(boardRowCount(boardState), boardColCount(boardState), depth);
    if (!cells.length) return false;
    const cellKeys = new Set(cells.map(({ row, col }) => `${row}:${col}`));
    const removed = [];
    const seen = /* @__PURE__ */ new Set();
    cells.forEach(({ row, col }) => {
      const piece = get(boardState, row, col);
      if (!piece) return;
      const key = piece.id || piece;
      if (seen.has(key)) return;
      seen.add(key);
      removed.push(piece);
    });
    boardState.collapseDepth = depth + 1;
    boardState.collapsed = true;
    boardState.ruleBombs = normalizeRuleBombs(boardState.ruleBombs, boardRowCount(boardState), boardColCount(boardState), workerRuleBombNormalizationLimit(boardState.ruleBombs)).filter((bomb) => !cellKeys.has(`${bomb.row}:${bomb.col}`));
    removed.forEach((piece) => clearPieceCells(boardState, piece));
    removed.forEach((piece) => {
      if (!["white", "black"].includes(piece.color)) return;
      const capturer = piece.color === causeColor ? opponent(causeColor) : causeColor;
      recordWorkerCapturedPieces(boardState, capturer, [piece]);
      workerCriticalCaptureScore(boardState, piece, capturer, causeColor);
    });
    const defeatedColors = new Set(removed.filter(isCritical).map((piece) => piece.color).filter((color) => ["white", "black"].includes(color)));
    if (defeatedColors.size > 1) {
      boardState.mode = "gameover";
      boardState.winner = "";
    }
    return true;
  }
  // Grafted from engine.optimized.js (our-only addition, paired with the
  // "collapse" targeting/apply branches grafted above): the one-shot
  // "collapse" card's pending flag, set by applyCardActionUnchecked's
  // "collapse" branch, resolves here on the very next finishWorkerMove call
  // after the card is used (cards don't end the turn, so this naturally
  // lands "다음 턴에"). Reuses workerCollapseOneRing directly, same as the
  // existing periodicCollapse RULE card below.
  function resolveWorkerOneShotCollapse(boardState, color) {
    if (!(Number(boardState.pendingOneShotCollapse) > 0)) return;
    boardState.pendingOneShotCollapse = 0;
    workerCollapseOneRing(boardState, color);
  }
  function resolveWorkerPeriodicCollapseAfterTurn(boardState, movingColor) {
    const periodic = boardState.periodicCollapse?.enabled ? boardState.periodicCollapse : null;
    if (!periodic) return false;
    const currentTurn = sharedTurnCount(boardState);
    periodic.interval = PERIODIC_COLLAPSE_INTERVAL;
    periodic.nextAt = Math.max(1, Math.floor(Number(periodic.nextAt) || nextPeriodicCollapseTurn(currentTurn)));
    if (currentTurn < periodic.nextAt) return false;
    let triggered = false;
    while (boardState.mode !== "gameover" && currentTurn >= periodic.nextAt) {
      const collapsed = workerCollapseOneRing(boardState, movingColor);
      triggered = collapsed || triggered;
      periodic.nextAt += periodic.interval;
      if (!collapsed) {
        periodic.nextAt = nextPeriodicCollapseTurn(currentTurn, periodic.interval);
        break;
      }
    }
    return triggered;
  }
  function removeWorkerCapturedRecordById(boardState, color, pieceId) {
    const captures = boardState.captures?.[color];
    if (!Array.isArray(captures) || !pieceId) return false;
    const index = captures.findIndex((piece) => piece?.id === pieceId);
    if (index < 0) return false;
    captures.splice(index, 1);
    return true;
  }
  function prepareWorkerParryRetaliations(boardState, attacker, capturedEntries, options = {}) {
    const ordinaryEntries = [];
    const parryEntries = [];
    (capturedEntries || []).forEach((entry) => {
      const item = entry?.item;
      if (options.skip || item?.type === "scarecrow" || !item?.parry || item.color !== opponent(attacker?.color)) {
        ordinaryEntries.push(entry);
        return;
      }
      const chance = Number(item.parry.chance) || PARRY_CHANCE;
      delete item.parry;
      const roll = workerStableIndex(`parry:${boardState.moveCount || 0}:${item.id || "defender"}:${attacker?.id || "attacker"}`, 1e4) / 1e4;
      if (roll < chance) {
        const restoreCells = workerIsLargePiece(item) ? colossusCells(item.anchorRow ?? entry.row, item.anchorCol ?? entry.col).map((cell) => ({ row: cell.row, col: cell.col })) : [];
        clearPieceCells(boardState, item);
        parryEntries.push({ ...entry, parryTriggered: true, restoreCells });
      } else ordinaryEntries.push(entry);
    });
    recordWorkerSaturationCaptureAttempts(boardState, attacker, parryEntries.map(({ item }) => item));
    return {
      ordinaryEntries,
      retaliationEntries: [...ordinaryEntries, ...parryEntries],
      parryCount: parryEntries.length
    };
  }
  function resolveWorkerBearRetaliation(boardState, attacker, capturedEntries, aiColor, counterDestination = null) {
    const victims = (capturedEntries || []).filter(({ item, parryTriggered }) => {
      if (!item || item.color !== opponent(attacker?.color)) return false;
      if (parryTriggered) return true;
      return Boolean(septemberCounterLimit(pieceAbilityType(item))) && (Number(item.bearRetaliationsRemaining) || 0) > 0;
    });
    if (!attacker || !victims.length) return { triggered: false, attackerRemoved: false, score: 0 };
    const defender = victims[0].item.color;
    let score = 0;
    const attackerSquare = findWorkerPieceByRef(boardState, { id: attacker.id });
    const attackerOnBoard = Boolean(attackerSquare?.piece === attacker || workerPieceRemainsOnBoard(boardState, attacker));
    const requestedDestination = counterDestination && inBounds(Number(counterDestination.row), Number(counterDestination.col), boardState) ? { row: Number(counterDestination.row), col: Number(counterDestination.col) } : attackerSquare ? { row: attackerSquare.row, col: attackerSquare.col } : null;
    if (attackerOnBoard) {
      score += attacker.color === aiColor ? -pieceValue(attacker) * 1.8 : pieceValue(attacker) * 1.8;
      if (isWorkerDecisiveCaptureTarget(boardState, attacker)) {
        score += workerCriticalCaptureScore(boardState, attacker, defender, aiColor);
      }
      clearPieceCells(boardState, attacker);
      recordWorkerCapturedPieces(boardState, defender, [attacker]);
      if (attackerSquare) {
        score += resolveWorkerReaperNearbyDeaths(
          boardState,
          [{ item: attacker, row: attackerSquare.row, col: attackerSquare.col }],
          aiColor,
          {}
        );
      }
    }
    victims.forEach(({ item, row, col, parryTriggered }, index) => {
      removeWorkerCapturedRecordById(boardState, attacker.color, item.id);
      const originalRestoreCells = parryTriggered && workerIsLargePiece(item) ? (victims[index].restoreCells || []).filter((cell) => inBounds(Number(cell?.row), Number(cell?.col), boardState)) : [];
      const parryDestination = index === 0 ? requestedDestination : null;
      const destinationRestoreCells = originalRestoreCells.length === 4 && parryDestination ? colossusCells(parryDestination.row, parryDestination.col).filter((cell) => inBounds(cell.row, cell.col, boardState)) : [];
      const restoreCells = destinationRestoreCells.length === 4 && destinationRestoreCells.every((cell) => !get(boardState, cell.row, cell.col)) ? destinationRestoreCells : originalRestoreCells;
      if (restoreCells.length === 4 && restoreCells.every((cell) => !get(boardState, cell.row, cell.col))) {
        const restoredPiece = clonePiece(item);
        restoredPiece.anchorRow = Math.min(...restoreCells.map((cell) => cell.row));
        restoredPiece.anchorCol = Math.min(...restoreCells.map((cell) => cell.col));
        restoredPiece.totalCaptures = Math.max(0, Number(restoredPiece.totalCaptures) || 0) + 1;
        restoredPiece.moved = true;
        if (!usesInternalSixFixes(boardState)) noteWorkerTwinMovement(restoredPiece);
        restoreCells.forEach((cell) => set(boardState, cell.row, cell.col, restoredPiece));
        if (parryTriggered) workerLearnImperialStudyMovement(boardState, restoredPiece, [attacker]);
        return;
      }
      let destination = index === 0 ? requestedDestination : parryTriggered ? { row, col } : null;
      if ((!destination || get(boardState, destination.row, destination.col)) && !get(boardState, row, col)) {
        destination = { row, col };
      }
      if (!destination || get(boardState, destination.row, destination.col)) return;
      const parried = Boolean(parryTriggered);
      const bear = clonePiece(item);
      if (!parried && !["trickster", "hedgehog"].includes(bear.type)) bear.type = "bear";
      bear.totalCaptures = Math.max(0, Number(bear.totalCaptures) || 0) + 1;
      if (!parried) {
        bear.bearRetaliationsRemaining = Math.max(0, (Number(item.bearRetaliationsRemaining) || 0) - 1);
        bear.bearMoveLockedUntilTurn = (Number(boardState.turnsTaken?.[bear.color]) || 0) + 1;
      }
      bear.moved = true;
      if (!usesInternalSixFixes(boardState)) noteWorkerTwinMovement(bear);
      set(boardState, destination.row, destination.col, bear);
      if (parried) workerLearnImperialStudyMovement(boardState, bear, [attacker]);
    });
    victims.forEach(({ item, parryTriggered }) => {
      if (parryTriggered || workerPieceIdRemainsOnBoard(boardState, item)) return;
      recordWorkerNewCardCaptureReactions(boardState, attacker.color, item);
    });
    return { triggered: true, attackerRemoved: attackerOnBoard, score };
  }
  function resolveWorkerTrojanHorseRetaliation(boardState, attacker, capturedEntries, aiColor) {
    const victims = (capturedEntries || []).filter(({ item }) => item?.type === "knight" && item.color === opponent(attacker?.color) && item.trojanHorse);
    if (!attacker || !victims.length) return { triggered: false, attackerRemoved: false, score: 0 };
    victims.forEach(({ item }) => {
      delete item.trojanHorse;
    });
    const attackerRemoved = canTrojanHorseRecaptureAttacker(attacker);
    let score = 0;
    const defender = victims[0].item.color;
    const attackerSquare = attackerRemoved ? findWorkerPieceByRef(boardState, { id: attacker.id }) : null;
    if (attackerRemoved) {
      score = attacker.color === aiColor ? -pieceValue(attacker) * 1.7 : pieceValue(attacker) * 1.7;
      if (isWorkerDecisiveCaptureTarget(boardState, attacker)) {
        score += workerCriticalCaptureScore(boardState, attacker, defender, aiColor);
      }
      clearPieceCells(boardState, attacker);
      recordWorkerCapturedPieces(boardState, defender, [attacker]);
      if (attackerSquare) {
        score += resolveWorkerReaperNearbyDeaths(
          boardState,
          [{ item: attacker, row: attackerSquare.row, col: attackerSquare.col }],
          aiColor,
          {}
        );
      }
    }
    const occupied = /* @__PURE__ */ new Set();
    victims.forEach(({ item, row, col }) => {
      const key = `${row}:${col}`;
      if (occupied.has(key) || get(boardState, row, col)) return;
      occupied.add(key);
      const pawn = workerCreatePiece(item.color, "pawn", boardState, row, col);
      pawn.moved = true;
      set(boardState, row, col, pawn);
    });
    return { triggered: true, attackerRemoved, score };
  }
  function workerFriendlyIdolAuraSources(boardState, piece, row, col) {
    if (!piece || pieceHasAbility(piece, "idol") || !["white", "black"].includes(piece.color)) return [];
    return queenDirections().map(([dr, dc]) => {
      const idol = get(boardState, row + dr, col + dc);
      return idol && idol !== piece && idol.color === piece.color && pieceHasAbility(idol, "idol") ? idol : null;
    }).filter(Boolean);
  }
  function workerFirstAvailableFriendlyIdolAuraSource(boardState, piece, row, col) {
    return workerFriendlyIdolAuraSources(boardState, piece, row, col).find((idol) => workerIdolEncoreAvailable(boardState, idol.id, piece.color)) || null;
  }
  function workerIdolEncoreAvailable(boardState, idolId, color) {
    if (!idolId || !["white", "black"].includes(color)) return false;
    const turnNumber = Number(boardState.turnsTaken?.[color]) || 0;
    return Number(boardState.idolEncoreUsedByPiece?.[`turn:${color}`]) !== turnNumber;
  }
  function workerIdolEncoreRepeatBlocked(boardState, piece) {
    const color = piece?.color;
    if (!piece || !["white", "black"].includes(color) || boardState?.turn !== color) return false;
    const restTurn = Number(piece.idolEncoreRestTurn);
    return Number.isFinite(restTurn) && restTurn === (Number(boardState.turnsTaken?.[color]) || 0);
  }
  function clearWorkerIdolEncoreRepeatBlocks(boardState, color) {
    forEachPieceCached(boardState, (piece) => {
      if (piece?.color === color) delete piece.idolEncoreRestTurn;
    });
  }
  function markWorkerIdolEncoreRepeatBlock(boardState, pieceId, color) {
    let moved = null;
    forEachPieceCached(boardState, (piece) => {
      if (!moved && piece?.id === pieceId && piece.color === color) moved = piece;
    });
    if (!moved) return false;
    moved.idolEncoreRestTurn = Number(boardState.turnsTaken?.[color]) || 0;
    return true;
  }
  function resolveWorkerRecurrences(boardState) {
    return septemberResolveBoardRecurrences(boardState, {
      isOpen: (row, col) => workerOpenPlacementSquare(boardState, row, col),
      choose: (pool, entry) => pool[workerStableIndex(`recurrence:${entry.piece.id}:${boardState.moveCount || 0}`, pool.length)]
    });
  }
  function resolveWorkerSubmergedPieces(boardState) {
    resolveWorkerRecurrences(boardState);
    septemberResolveBoardInfiltration(boardState);
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece?.submerged || seen.has(piece)) return;
      seen.add(piece);
      const enemyAdjacent = queenDirections().some(([dr, dc]) => {
        const neighbor = get(boardState, row + dr, col + dc);
        return neighbor && neighbor !== piece && neighbor.color === opponent(piece.color);
      });
      if (enemyAdjacent) delete piece.submerged;
    });
  }
  function resolveWorkerTwinSwaps(boardState) {
    const bonds = /* @__PURE__ */ new Map();
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece?.twinBondId || seen.has(piece)) return;
      seen.add(piece);
      const entries = bonds.get(piece.twinBondId) || [];
      entries.push({ piece, row, col });
      bonds.set(piece.twinBondId, entries);
    });
    bonds.forEach((entries) => {
      if (entries.length !== 2) {
        entries.forEach(({ piece }) => {
          delete piece.twinBondId;
          delete piece.twinPartnerId;
          delete piece.twinSwapPending;
        });
        return;
      }
      const [first, second] = entries;
      if (isSlimeSpecialMovementLocked(first.piece) || isSlimeSpecialMovementLocked(second.piece)) {
        entries.forEach(({ piece }) => {
          delete piece.twinBondId;
          delete piece.twinPartnerId;
          delete piece.twinSwapPending;
        });
        return;
      }
      const pending = (Number(first.piece.twinSwapPending) || 0) + (Number(second.piece.twinSwapPending) || 0);
      delete first.piece.twinSwapPending;
      delete second.piece.twinSwapPending;
      if (pending % 2 === 0) return;
      set(boardState, first.row, first.col, second.piece);
      set(boardState, second.row, second.col, first.piece);
      first.piece.moved = true;
      second.piece.moved = true;
    });
    boardState.chainBonds = partitionChainBondsByRange(
      boardState.chainBonds,
      (id) => findWorkerPieceByRef(boardState, { id })
    ).active;
  }
  function workerPieceHasAdjacentAlly(boardState, piece) {
    let adjacent = false;
    forEachPiece(boardState, (candidate, row, col) => {
      if (adjacent || candidate !== piece) return;
      adjacent = queenDirections().some(([dr, dc]) => {
        const neighbor = get(boardState, row + dr, col + dc);
        return neighbor && neighbor !== piece && neighbor.color === piece.color;
      });
    });
    return adjacent;
  }
  function resolveWorkerOthelloAll(boardState, color) {
    const targets = piecesMatching(boardState, (piece, row, col) => workerIsOthelloTarget(boardState, piece, row, col, color));
    for (const { piece } of targets) {
      const decisive = isWorkerDecisiveCaptureTarget(boardState, piece);
      piece.color = color;
      piece.moved = true;
      piece.defected = true;
      if (decisive) {
        boardState.mode = "gameover";
        boardState.winner = color;
      }
    }
  }
  function tickWorkerReservedScarecrows(boardState, color) {
    const due = [];
    boardState.pendingScarecrows = (boardState.pendingScarecrows || []).filter((entry) => {
      if (!entry.reserved) return true;
      if (entry.by === opponent(color)) entry.remainingOwnTurns--;
      if (entry.remainingOwnTurns > 0) return true;
      due.push(entry);
      return false;
    });
    for (const entry of due) {
      if (!workerOpenPlacementSquare(boardState, entry.row, entry.col)) continue;
      const p = workerCreatePiece(entry.color, "scarecrow", boardState, entry.row, entry.col);
      p.moved = true;
      p.origin = workerSquareName(boardState, entry.row, entry.col);
      set(boardState, entry.row, entry.col, p);
    }
  }
  function resolveWorkerPendingGalesAfterTurn(boardState, color) {
    if (!Array.isArray(boardState.pendingGales) || !boardState.pendingGales.length) return 0;
    const currentTurn = Number(boardState.turnsTaken?.[color]) || 0;
    const due = boardState.pendingGales.filter((entry) => {
      if (entry.remainingOwnTurns !== void 0) {
        if (entry.color === opponent(color)) entry.remainingOwnTurns--;
        return entry.remainingOwnTurns <= 0;
      }
      return entry?.color === color && currentTurn >= (Number(entry.triggerTurn) || 0);
    });
    if (!due.length) return 0;
    const dueIds = new Set(due.map((entry) => entry.id));
    boardState.pendingGales = boardState.pendingGales.filter((entry) => !dueIds.has(entry.id));
    const doomed = [];
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!piece || !["white", "black"].includes(piece.color) || seen.has(piece)) return;
      if (usesSeptember18Balance(boardState)) {
        if (!isWorkerMajorPiece(piece)) return;
        const cells = workerCrownPieceCells(boardState, piece);
        if (piecesMatching(boardState, (other) => other !== piece && isWorkerMajorPiece(other)).some(({ piece: other }) => cells.some((a) => workerCrownPieceCells(boardState, other).some((b) => Math.max(Math.abs(a.row - b.row), Math.abs(a.col - b.col)) <= 1)))) return;
      } else if (workerPieceHasAdjacentAlly(boardState, piece)) return;
      seen.add(piece);
      doomed.push(piece);
    });
    const defeated = /* @__PURE__ */ new Set();
    doomed.forEach((piece) => {
      if (isWorkerDecisiveCaptureTarget(boardState, piece)) defeated.add(piece.color);
      clearPieceCells(boardState, piece);
      recordWorkerCapturedPieces(boardState, opponent(piece.color), [piece]);
    });
    if (defeated.size > 1) {
      boardState.mode = "gameover";
      boardState.winner = null;
    } else if (defeated.size === 1) {
      const loser = [...defeated][0];
      boardState.mode = "gameover";
      boardState.winner = opponent(loser);
    }
    return doomed.length;
  }
  function resolveWorkerPendingOtherworldAfterMove(boardState, aiColor) {
    if (!Array.isArray(boardState.pendingOtherworld) || !boardState.pendingOtherworld.length) return 0;
    const advanced = boardState.pendingOtherworld.map((entry) => advanceScheduledHalfTurn(entry));
    const due = advanced.filter((entry, index) => entry !== boardState.pendingOtherworld[index] ? undeadResurrectionRemainingHalfMoves(entry, boardState.moveCount) <= 0 : (Number(boardState.moveCount) || 0) >= (Number(entry.dueMoveCount) || 0));
    if (!due.length) {
      boardState.pendingOtherworld = advanced;
      return 0;
    }
    const dueIds = new Set(due.map((entry) => entry.id));
    boardState.pendingOtherworld = advanced.filter((entry) => !dueIds.has(entry.id));
    let returned = 0;
    due.forEach((entry) => {
      const row = Number(entry.row);
      const col = Number(entry.col);
      if (!inBounds(row, col, boardState)) return;
      const blocker = get(boardState, row, col);
      if (blocker) forceWorkerRemovePieceAt(boardState, row, col, entry.color, aiColor);
      const wizard = workerCreatePiece(entry.color, "wizard", boardState, row, col);
      if (blocker) wizard.totalCaptures = Math.max(0, Number(wizard.totalCaptures) || 0) + 1;
      wizard.id = entry.pieceId || wizard.id;
      wizard.origin = entry.origin || workerSquareName(boardState, row, col);
      wizard.mana = 0;
      wizard.maxMana = 5;
      if (isWorkerCollapsedSquare(boardState, row, col)) {
        recordWorkerCapturedPieces(boardState, opponent(entry.color), [wizard]);
        return;
      }
      set(boardState, row, col, wizard);
      returned += 1;
    });
    return returned;
  }
  function workerJudgmentEntries(boardState) {
    const entries = [];
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece || !["white", "black"].includes(piece.color) || isWorkerRoyalIdentityPiece(boardState, piece) || seen.has(piece) || ["wall", "football", "blackHole"].includes(piece.type)) return;
      seen.add(piece);
      entries.push({ piece, row, col });
    });
    if (!entries.length) return [];
    const count = (piece) => Math.max(0, Number(piece.totalCaptures ?? piece.capturesMade) || 0);
    const eligible = entries.filter((entry) => count(entry.piece) >= 2);
    if (!eligible.length) return [];
    const maximum = Math.max(...eligible.map((entry) => count(entry.piece)));
    return eligible.filter((entry) => count(entry.piece) === maximum);
  }
  function workerOpenPlacementSquare(boardState, row, col) {
    return inBounds(row, col, boardState) && !get(boardState, row, col) && !workerFindQuantumAt(boardState, row, col) && !isWorkerCollapsedSquare(boardState, row, col) && !isWorkerPendingPortalReservedSquare(boardState, row, col) && !isWorkerPendingSpawnReservedSquare(boardState, row, col);
  }
  function resolveWorkerVipInvitationsForTurn(boardState, color) {
    piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn" && piece.vipInvitation && (Number(boardState.turnsTaken?.[color]) || 0) >= (Number(piece.vipInvitation.triggerTurn) || Infinity)).forEach(({ piece }) => {
      delete piece.vipInvitation;
      delete piece.holdoutPromotion;
      piece.type = "vip";
      piece.moved = true;
    });
  }
  const WORKER_CROWN_HOLD_WIN_MOVES = 10;
  function isWorkerCrownGroundSquare(boardState, row, col) {
    const rule = boardState?.crownRule;
    const entries = Array.isArray(rule?.crowns) && rule.crowns.length ? rule.crowns : [rule];
    return entries.some((entry) => {
      const ground = entry === true ? workerCrownCenterCells(boardState)[0] || null : entry?.ground;
      return Boolean(entry && !entry?.removed && Number.isInteger(ground?.row) && Number.isInteger(ground?.col) && ground.row === row && ground.col === col);
    });
  }
  function workerNormalizeCrownRule(value, boardState) {
    if (!value) return null;
    const rows = boardRowCount(boardState);
    const cols = boardColCount(boardState);
    const currentSharedTurn = sharedTurnCount(boardState);
    const sources = Array.isArray(value?.crowns) && value.crowns.length ? value.crowns : [value];
    const crowns = sources.map((source, index) => {
      const legacyGround = source === true ? workerCrownCenterCells(boardState)[0] || null : null;
      const rawGround = source === true ? legacyGround : source?.ground;
      const ground = Number.isInteger(rawGround?.row) && Number.isInteger(rawGround?.col) && rawGround.row >= 0 && rawGround.row < rows && rawGround.col >= 0 && rawGround.col < cols ? { row: rawGround.row, col: rawGround.col } : null;
      const rawPendingCells = Array.isArray(source?.pendingTransfer?.preferredCells) ? source.pendingTransfer.preferredCells : [];
      const pendingCells = rawPendingCells.filter((cell) => Number.isInteger(cell?.row) && Number.isInteger(cell?.col) && cell.row >= 0 && cell.row < rows && cell.col >= 0 && cell.col < cols).map((cell) => ({ row: cell.row, col: cell.col }));
      const pendingAttackerId = typeof source?.pendingTransfer?.attackerId === "string" ? source.pendingTransfer.attackerId : "";
      const fullTurnCounting = source?.countUnit === "full-turn";
      return {
        id: typeof source?.id === "string" && source.id ? source.id : `crown-${index + 1}`,
        crownGroupSize: sources.length,
        enabled: true,
        holderId: typeof source?.holderId === "string" ? source.holderId : "",
        ground,
        removed: Boolean(source?.removed),
        pendingTransfer: pendingAttackerId ? { attackerId: pendingAttackerId, preferredCells: pendingCells } : null,
        holdingColor: ["white", "black"].includes(source?.holdingColor) ? source.holdingColor : "",
        countUnit: "full-turn",
        heldMoves: {
          white: Math.max(0, Math.floor((Number(source?.heldMoves?.white) || 0) / (fullTurnCounting ? 1 : 2))),
          black: Math.max(0, Math.floor((Number(source?.heldMoves?.black) || 0) / (fullTurnCounting ? 1 : 2)))
        },
        lastCountedMove: Number.isFinite(Number(source?.lastCountedMove)) ? Math.min(currentSharedTurn, Math.max(0, Math.floor(Number(source.lastCountedMove) / (fullTurnCounting ? 1 : 2)))) : currentSharedTurn
      };
    });
    return crowns.length === 1 ? crowns[0] : { ...crowns[0], crowns };
  }
  function workerCrownRuleEntries(rule) {
    if (!rule) return [];
    return Array.isArray(rule.crowns) && rule.crowns.length ? rule.crowns : [rule];
  }
  function workerStoreCrownRuleEntries(boardState, entries) {
    boardState.crownRule = workerNormalizeCrownRule({ crowns: entries }, boardState);
    return boardState.crownRule;
  }
  function workerCrownEligiblePiece(piece) {
    return Boolean(piece && ["white", "black"].includes(piece.color) && !["wall", "football", "blackHole", "monster"].includes(piece.type));
  }
  function workerIsCrownPiece(piece) {
    return Boolean(workerCrownEligiblePiece(piece) && piece.type === "crown");
  }
  function workerFindCrownHolder(boardState, rule = boardState.crownRule) {
    let holder = null;
    let firstCrown = null;
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece || seen.has(piece) || !workerIsCrownPiece(piece)) return;
      seen.add(piece);
      const square = { piece, row, col };
      if (!holder && rule?.holderId && piece.id === rule.holderId) holder = square;
      if (!holder && Array.isArray(piece.crownTokenIds) && piece.crownTokenIds.includes(rule?.id)) holder = square;
      if (!firstCrown && Number(rule?.crownGroupSize) <= 1) firstCrown = square;
    });
    return holder || firstCrown;
  }
  function workerFindLegacyCrownBearer(boardState, rule = boardState.crownRule) {
    let holder = null;
    let flagged = null;
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece || seen.has(piece) || workerIsCrownPiece(piece) || !workerCrownEligiblePiece(piece)) return;
      seen.add(piece);
      const square = { piece, row, col };
      if (!holder && rule?.holderId && piece.id === rule.holderId) holder = square;
      if (!flagged && piece.crownBearer) flagged = square;
    });
    return holder || flagged;
  }
  function workerCrownPieceCells(boardState, piece) {
    const cells = [];
    if (!piece) return cells;
    for (let row = 0; row < boardRowCount(boardState); row += 1) {
      for (let col = 0; col < boardColCount(boardState); col += 1) {
        const occupant = get(boardState, row, col);
        if (occupant === piece || piece.id && occupant?.id === piece.id) cells.push({ row, col });
      }
    }
    return cells;
  }
  function workerCrownReplacementSquare(boardState, piece, preferredCells = []) {
    const occupied = workerCrownPieceCells(boardState, piece);
    if (!occupied.length) return null;
    const preferred = preferredCells.filter((cell) => occupied.some((current) => sameSquare(cell, current)));
    const pool = preferred.length ? preferred : occupied;
    return pool[workerStableIndex(`${piece.id || piece.type}:${boardState.moveCount || 0}:crown`, pool.length)] || pool[0] || null;
  }
  function workerResetRemovedCrownRule(boardState, rule) {
    if (!rule) return null;
    rule.holderId = "";
    rule.ground = null;
    rule.pendingTransfer = null;
    rule.removed = true;
    rule.holdingColor = "";
    rule.heldMoves.white = 0;
    rule.heldMoves.black = 0;
    rule.lastCountedMove = sharedTurnCount(boardState);
    return rule;
  }
  function workerSetCrownHolder(boardState, piece, options = {}) {
    const normalized = workerNormalizeCrownRule(boardState.crownRule, boardState);
    if (!normalized || !workerCrownEligiblePiece(piece)) return false;
    const entries = workerCrownRuleEntries(normalized);
    const preferredCells = Array.isArray(options.preferredCells) ? options.preferredCells : [];
    const rule = entries.find((entry) => entry.id === options.ruleId) || entries.find((entry) => entry.ground && preferredCells.some((cell) => sameSquare(cell, entry.ground))) || entries.find((entry) => !entry.removed && !entry.holderId) || entries[0];
    if (!rule) return false;
    const chosen = workerCrownReplacementSquare(boardState, piece, preferredCells);
    if (!chosen) return false;
    const color = piece.color;
    if (rule.holdingColor !== color) {
      rule.holdingColor = color;
      rule.heldMoves.white = 0;
      rule.heldMoves.black = 0;
      rule.lastCountedMove = sharedTurnCount(boardState);
    }
    clearPieceCells(boardState, piece);
    const originalId = piece.id || "";
    const existingTokenIds = Array.isArray(piece.crownTokenIds) ? [...piece.crownTokenIds] : [];
    const alreadyCrown = piece.type === "crown";
    const regencyKing = isWorkerRegencyRoyalHeir(boardState, piece);
    const preservedRegencyState = regencyKing ? {
      regencyHeir: true,
      undergroundBunker: piece.undergroundBunker,
      hp: piece.hp,
      maxHp: piece.maxHp,
      lastResistance: clonePlain(piece.lastResistance),
      protected: piece.protected,
      imperialMoves: Array.isArray(piece.imperialMoves) ? [...piece.imperialMoves] : void 0
    } : null;
    const crownRoyal = isWorkerKingRole(boardState, piece);
    if (!alreadyCrown) {
      const crown = workerCreatePiece(color, "crown", boardState, chosen.row, chosen.col);
      Object.keys(piece).forEach((key) => delete piece[key]);
      Object.assign(piece, crown, {
        id: originalId || crown.id,
        type: "crown",
        moved: true,
        crownBearer: true,
        origin: workerSquareName(boardState, chosen.row, chosen.col)
      });
      if (crownRoyal) piece.crownRoyal = true;
      if (preservedRegencyState) {
        Object.entries(preservedRegencyState).forEach(([key, value]) => {
          if (value !== void 0) piece[key] = value;
        });
      }
    }
    piece.crownBearer = true;
    piece.crownTokenIds = [.../* @__PURE__ */ new Set([...existingTokenIds, rule.id])];
    set(boardState, chosen.row, chosen.col, piece);
    rule.holderId = piece.id || "";
    rule.ground = null;
    rule.pendingTransfer = null;
    rule.removed = false;
    workerStoreCrownRuleEntries(boardState, entries);
    workerSyncCrownFlags(boardState, boardState.crownRule);
    return true;
  }
  function workerCrownCenterCells(boardState) {
    return workerCenterCells(boardState, 2);
  }
  function workerEnableCrownRule(boardState) {
    const center = workerCrownCenterCells(boardState);
    if (!center.length) return false;
    const open = center.filter(({ row, col }) => !get(boardState, row, col));
    const pool = open.length ? open : center;
    const chosen = pool[workerStableIndex(`crown:${boardState.moveCount || 0}`, pool.length)];
    boardState.crownRule = workerNormalizeCrownRule({
      enabled: true,
      holderId: "",
      ground: chosen,
      removed: false,
      pendingTransfer: null,
      holdingColor: "",
      countUnit: "full-turn",
      heldMoves: { white: 0, black: 0 },
      lastCountedMove: sharedTurnCount(boardState)
    }, boardState);
    workerReconcileCrownRule(boardState);
    return true;
  }
  function workerReconcileCrownRule(boardState, options = {}) {
    let normalized = workerNormalizeCrownRule(boardState.crownRule, boardState);
    if (!normalized) return null;
    boardState.crownRule = normalized;
    const ruleIds = workerCrownRuleEntries(normalized).map((entry) => entry.id);
    let firstHolder = null;
    for (const ruleId of ruleIds) {
      normalized = workerNormalizeCrownRule(boardState.crownRule, boardState);
      const entries = workerCrownRuleEntries(normalized);
      const rule = entries.find((entry) => entry.id === ruleId);
      if (!rule) continue;
      if (rule.removed) {
        workerResetRemovedCrownRule(boardState, rule);
        workerStoreCrownRuleEntries(boardState, entries);
        continue;
      }
      const holder = workerFindCrownHolder(boardState, rule);
      if (holder) {
        rule.holderId = holder.piece.id || "";
        rule.ground = null;
        rule.pendingTransfer = null;
        if (rule.holdingColor !== holder.piece.color) {
          rule.holdingColor = holder.piece.color;
          rule.heldMoves.white = 0;
          rule.heldMoves.black = 0;
          rule.lastCountedMove = sharedTurnCount(boardState);
        }
        workerStoreCrownRuleEntries(boardState, entries);
        firstHolder ||= holder;
        continue;
      }
      if (rule.pendingTransfer) {
        const pending = rule.pendingTransfer;
        const attacker = piecesMatching(boardState, (piece) => piece.id === pending.attackerId)[0]?.piece || null;
        if (workerCrownEligiblePiece(attacker) && workerSetCrownHolder(boardState, attacker, {
          ruleId,
          countCurrentMove: options.countCurrentMove,
          preferredCells: pending.preferredCells
        })) {
          const updatedRule = workerCrownRuleEntries(boardState.crownRule).find((entry) => entry.id === ruleId);
          firstHolder ||= workerFindCrownHolder(boardState, updatedRule);
          continue;
        }
        workerResetRemovedCrownRule(boardState, rule);
        workerStoreCrownRuleEntries(boardState, entries);
        continue;
      }
      const legacyHolder = ruleIds.length === 1 ? workerFindLegacyCrownBearer(boardState, rule) : null;
      if (legacyHolder && workerSetCrownHolder(boardState, legacyHolder.piece, {
        ruleId,
        countCurrentMove: options.countCurrentMove,
        preferredCells: [{ row: legacyHolder.row, col: legacyHolder.col }]
      })) {
        firstHolder ||= workerFindCrownHolder(boardState, workerCrownRuleEntries(boardState.crownRule)[0]);
        continue;
      }
      if (rule.ground) {
        const occupant = get(boardState, rule.ground.row, rule.ground.col);
        if (workerCrownEligiblePiece(occupant)) {
          workerSetCrownHolder(boardState, occupant, {
            ruleId,
            countCurrentMove: options.countCurrentMove,
            preferredCells: [rule.ground]
          });
          const updatedRule = workerCrownRuleEntries(boardState.crownRule).find((entry) => entry.id === ruleId);
          firstHolder ||= workerFindCrownHolder(boardState, updatedRule);
        }
        continue;
      }
      workerResetRemovedCrownRule(boardState, rule);
      workerStoreCrownRuleEntries(boardState, entries);
    }
    workerSyncCrownFlags(boardState, boardState.crownRule);
    return firstHolder;
  }
  function workerTransferCrownAfterCapture(boardState, captured, attacker, options = {}) {
    const normalized = workerNormalizeCrownRule(boardState.crownRule, boardState);
    if (!normalized) return false;
    const entries = workerCrownRuleEntries(normalized);
    const tokenIds = Array.isArray(captured?.crownTokenIds) ? captured.crownTokenIds : [];
    let capturedRules = entries.filter((rule) => rule.holderId && captured?.id === rule.holderId || tokenIds.includes(rule.id));
    const capturedCrown = workerIsCrownPiece(captured) || captured?.crownBearer || capturedRules.length > 0;
    if (!capturedCrown) return false;
    if (!capturedRules.length) capturedRules = entries.slice(0, 1);
    delete captured.crownBearer;
    delete captured.crownTokenIds;
    const preferredCells = Array.isArray(options.attackerLandingCells) ? options.attackerLandingCells : options.attackerLanding ? [options.attackerLanding] : [];
    capturedRules.forEach((rule) => {
      rule.holderId = "";
      rule.ground = null;
      rule.pendingTransfer = null;
      rule.removed = false;
      if (workerCrownEligiblePiece(attacker) && attacker.id) {
        rule.pendingTransfer = {
          attackerId: attacker.id,
          preferredCells: preferredCells.map((cell) => ({ row: cell.row, col: cell.col }))
        };
      } else {
        workerResetRemovedCrownRule(boardState, rule);
      }
    });
    workerStoreCrownRuleEntries(boardState, entries);
    workerSyncCrownFlags(boardState, boardState.crownRule);
    return true;
  }
  function resolveWorkerCrownRuleAfterMove(boardState) {
    workerReconcileCrownRule(boardState, { countCurrentMove: true });
    const normalized = workerNormalizeCrownRule(boardState.crownRule, boardState);
    if (!normalized) return false;
    const entries = workerCrownRuleEntries(normalized);
    const currentSharedTurn = sharedTurnCount(boardState);
    entries.forEach((rule, index) => {
      const holder = workerFindCrownHolder(boardState, rule);
      if (!holder || !["white", "black"].includes(holder.piece.color)) return;
      entries[index] = advanceCrownHoldEntry(rule, holder.piece.color, currentSharedTurn, WORKER_CROWN_HOLD_WIN_MOVES);
    });
    workerStoreCrownRuleEntries(boardState, entries);
    const color = crownHoldWinningColor(workerCrownRuleEntries(boardState.crownRule), WORKER_CROWN_HOLD_WIN_MOVES);
    if (!color) return false;
    boardState.mode = "gameover";
    boardState.winner = color;
    return true;
  }
  function workerBishopCount(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "bishop").length;
  }
  function resolveWorkerReligiousVictory(boardState) {
    if (resolveInternalHighlander(boardState)) return boardState.winner;
    if (boardState.mode === "gameover") return boardState.winner || "";
    for (const color of ["white", "black"]) {
      if (!boardState.religiousVictory?.[color]) continue;
      if (workerBishopCount(boardState, color) - workerBishopCount(boardState, opponent(color)) < 3) continue;
      boardState.mode = "gameover";
      boardState.winner = color;
      return color;
    }
    return "";
  }
  function workerHeraldHasVictory(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && workerPieceHasActiveOrPreviousTricksterAbility(piece, "herald") && !piece.kingThreatSuppressed).some(({ row, col }) => kingDeltas().some(([dr, dc]) => {
      const target = get(boardState, row + dr, col + dc);
      return Boolean(target?.color === opponent(color) && (isWorkerNativeKing(target) || isWorkerRegencyRoyalHeir(boardState, target) || target.type === "merchant"));
    }));
  }
  function resolveWorkerHeraldThreats(boardState, actorColor) {
    if (boardState.mode === "gameover") return boardState.winner || "";
    const orderedColors = [actorColor, opponent(actorColor)].filter((color, index, colors) => ["white", "black"].includes(color) && colors.indexOf(color) === index);
    for (const color of orderedColors) {
      if (!workerHeraldHasVictory(boardState, color)) continue;
      boardState.mode = "gameover";
      boardState.winner = color;
      return color;
    }
    return "";
  }
  function workerSyncCrownFlags(boardState, rule = boardState.crownRule) {
    const entries = workerCrownRuleEntries(rule).filter((entry) => !entry.removed && entry.holderId);
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!piece || seen.has(piece)) return;
      seen.add(piece);
      const ids = entries.filter((entry) => entry.holderId === piece.id).map((entry) => entry.id);
      if (ids.length) {
        piece.crownBearer = true;
        piece.crownTokenIds = ids;
      } else {
        delete piece.crownBearer;
        delete piece.crownTokenIds;
      }
    });
  }
  function resolveWorkerRacingKingVictory(boardState, actorColor) {
    if (boardState.mode === "gameover") return boardState.winner || "draw";
    const winners = ["white", "black"].filter((color) => {
      const king = findWorkerRacingKingRole(boardState, color);
      return king && workerRacingKingWins(boardState, king.piece, king.row);
    });
    if (!winners.length) return "";
    boardState.mode = "gameover";
    boardState.winner = winners.length === 2 ? legacyRacingKingPriorityStates.has(boardState) ? actorColor : null : winners[0];
    return boardState.winner || "draw";
  }
  function resolveWorkerSpecialObjectiveVictories(boardState, actorColor) {
    if (boardState.mode === "gameover") return boardState.winner || "";
    return resolveWorkerRacingKingVictory(boardState, actorColor) || resolveWorkerReligiousVictory(boardState) || resolveWorkerHeraldThreats(boardState, actorColor);
  }
  function workerCrownStakeValue(boardState, piece) {
    const rule = workerNormalizeCrownRule(boardState?.crownRule, boardState);
    if (!piece) return 0;
    const tokenIds = Array.isArray(piece?.crownTokenIds) ? piece.crownTokenIds : [];
    return workerCrownRuleEntries(rule).filter((entry) => entry.holderId && piece?.id === entry.holderId || tokenIds.includes(entry.id)).reduce((score, heldRule) => score + scoreCrownObjective({
      holderColor: piece.color,
      perspectiveColor: piece.color,
      heldMoves: heldRule.heldMoves?.[piece.color]
    }), 0);
  }
  function workerStrategicPieceValue(boardState, piece) {
    const baseValue = isWorkerDemocracyProtectedPiece(boardState, piece) ? Math.min(pieceValue(piece), 1400) : pieceValue(piece);
    return baseValue + workerCrownStakeValue(boardState, piece) + workerDemocracyCitizenStakeValue(boardState, piece);
  }
  function workerCrownGroundDistance(boardState, color, ground) {
    if (!ground || !["white", "black"].includes(color)) return Infinity;
    let best = Infinity;
    forEachPiece(boardState, (piece, row, col) => {
      if (!workerCrownEligiblePiece(piece) || piece.color !== color || isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return;
      best = Math.min(best, Math.max(Math.abs(ground.row - row), Math.abs(ground.col - col)));
    });
    return best;
  }
  function workerCrownStrategicScore(boardState, color) {
    const rule = workerNormalizeCrownRule(boardState?.crownRule, boardState);
    if (!rule) return 0;
    return workerCrownRuleEntries(rule).reduce((score, entry) => {
      const holder = workerFindCrownHolder(boardState, entry);
      if (holder) {
        if (holder.piece.color !== color) return score;
        return score + scoreCrownObjective({
          holderColor: holder.piece.color,
          perspectiveColor: color,
          heldMoves: entry.heldMoves?.[color],
          holderThreatened: Boolean(workerBestCaptureThreat(boardState, holder.piece, holder.row, holder.col))
        });
      }
      return score + scoreCrownObjective({
        perspectiveColor: color,
        groundDistance: workerCrownGroundDistance(boardState, color, entry.ground)
      });
    }, 0);
  }
  function workerVipInvitationStrategicScore(boardState, color) {
    let score = 0;
    const currentTurn = Number(boardState?.turnsTaken?.[color]) || 0;
    forEachPiece(boardState, (piece) => {
      if (piece?.color !== color || piece.type !== "pawn" || !piece.vipInvitation) return;
      const remaining = Math.max(0, Math.min(3, (Number(piece.vipInvitation.triggerTurn) || currentTurn) - currentTurn));
      score -= 420 + (3 - remaining) * 260;
    });
    return score;
  }
  function workerReligiousVictoryStrategicScore(boardState, color) {
    if (!boardState?.religiousVictory?.[color]) return 0;
    const difference = workerBishopCount(boardState, color) - workerBishopCount(boardState, opponent(color));
    let score = 180 + difference * 520;
    if (difference >= 2) score += 2600;
    return Math.max(-1200, score);
  }
  function workerRacingKingStrategicScore(boardState, color) {
    if (!workerHasRacingKingObjective(boardState, color)) return 0;
    const king = findWorkerRacingKingRole(boardState, color);
    if (!king) return 0;
    const goalRow = workerRacingKingGoalRow(boardState, color);
    const distance = Math.abs(king.row - goalRow);
    const progress = Math.max(0, boardRowCount(boardState) - 1 - distance);
    let score = progress * 620;
    if (distance <= 2) score += (3 - distance) * 1500;
    if (isSquareAttacked(boardState, king.row, king.col, opponent(color))) score -= 2400;
    return score;
  }
  function workerHeraldStrategicScore(boardState, color) {
    const targets = piecesMatching(boardState, (piece) => piece.color === opponent(color) && (isWorkerNativeKing(piece) || isWorkerRegencyRoyalHeir(boardState, piece) || piece.type === "merchant"));
    if (!targets.length) return 0;
    let best = Infinity;
    piecesMatching(boardState, (piece) => piece.color === color && piece.type === "herald" && !piece.kingThreatSuppressed).forEach((herald) => targets.forEach((target) => {
      best = Math.min(best, Math.max(Math.abs(herald.row - target.row), Math.abs(herald.col - target.col)));
    }));
    if (!Number.isFinite(best)) return 0;
    return Math.max(0, 720 - Math.max(0, best - 1) * 110);
  }
  function workerPersistentObjectiveScore(boardState, color) {
    return workerCrownStrategicScore(boardState, color) + scoreProphecyObjective({ remainingHalfTurns: boardState?.prophecy?.[color]?.remainingHalfTurns }) + workerVipInvitationStrategicScore(boardState, color) + workerReligiousVictoryStrategicScore(boardState, color) + workerRacingKingStrategicScore(boardState, color) + workerHeraldStrategicScore(boardState, color) + workerDemocracyStrategicScore(boardState, color);
  }
  function workerCrownMoveOrderingScore(boardState, action) {
    if (action?.type !== "move") return 0;
    const rule = workerNormalizeCrownRule(boardState?.crownRule, boardState);
    if (!rule?.ground || workerFindCrownHolder(boardState, rule)) return 0;
    const destination = workerActionDestination(action) || action.move || {};
    return workerCrownRuleEntries(rule).some((entry) => sameSquare(destination, entry.ground)) ? 2400 : 0;
  }
  function workerConveyorRingCells(boardState) {
    const rows = boardRowCount(boardState);
    const cols = boardColCount(boardState);
    if (rows < 2 || cols < 2) return [];
    const cells = [];
    for (let col = 0; col < cols; col += 1) cells.push({ row: 0, col });
    for (let row = 1; row < rows; row += 1) cells.push({ row, col: cols - 1 });
    for (let col = cols - 2; col >= 0; col -= 1) cells.push({ row: rows - 1, col });
    for (let row = rows - 2; row > 0; row -= 1) cells.push({ row, col: 0 });
    return cells;
  }
  function resolveWorkerConveyorAfterMove(boardState, movingColor) {
    if (!boardState.conveyorRule) return 0;
    const ring = workerConveyorRingCells(boardState);
    if (!ring.length) return 0;
    const enPassantPawn = boardState.enPassant ? get(boardState, boardState.enPassant.capturedRow, boardState.enPassant.capturedCol) : null;
    const occupants = ring.map(({ row, col }) => get(boardState, row, col));
    const fixed = occupants.map((piece) => Boolean(piece && workerIsLargePiece(piece)));
    const canMove = occupants.map(() => false);
    if (!fixed.some(Boolean)) {
      occupants.forEach((piece, index) => {
        canMove[index] = Boolean(piece);
      });
    } else {
      occupants.forEach((piece, index) => {
        if (!piece || fixed[index]) return;
        const next = (index + 1) % ring.length;
        if (!fixed[next] && !occupants[next]) canMove[index] = true;
      });
      let changed = true;
      while (changed) {
        changed = false;
        occupants.forEach((piece, index) => {
          if (!piece || fixed[index] || canMove[index]) return;
          const next = (index + 1) % ring.length;
          if (!fixed[next] && occupants[next] && canMove[next]) {
            canMove[index] = true;
            changed = true;
          }
        });
      }
    }
    canMove.forEach((moves, index) => {
      if (moves) set(boardState, ring[index].row, ring[index].col, null);
    });
    let moved = 0;
    canMove.forEach((moves, index) => {
      if (!moves) return;
      const piece = occupants[index];
      const destination = ring[(index + 1) % ring.length];
      piece.moved = true;
      noteWorkerUltimatumMovement(boardState, piece);
      if (isWorkerCollapsedSquare(boardState, destination.row, destination.col)) {
        if (isWorkerDecisiveCaptureTarget(boardState, piece)) {
          boardState.mode = "gameover";
          boardState.winner = opponent(piece.color);
        }
        recordWorkerCapturedPieces(boardState, opponent(piece.color), [piece]);
      } else {
        set(boardState, destination.row, destination.col, piece);
        if (["pawn", "squire", "standardBearer"].includes(piece.type) && isPromotionRow(piece, destination.row, boardState)) {
          piece.type = workerAutomaticPromotionType(boardState, piece, destination.row);
          clearWorkerPromotionTraits(piece, boardState);
          piece.promotedFromPawn = true;
        }
      }
      moved += 1;
    });
    boardState.pendingScarecrows = advanceConveyorReservations(
      boardState.pendingScarecrows,
      boardRowCount(boardState),
      boardColCount(boardState)
    );
    boardState.pendingLobsters = advanceConveyorReservations(
      boardState.pendingLobsters,
      boardRowCount(boardState),
      boardColCount(boardState)
    );
    if (boardState.enPassant) {
      let pawnSquare = null;
      forEachPiece(boardState, (piece, row, col) => {
        if (!pawnSquare && piece === enPassantPawn) pawnSquare = { row, col };
      });
      const nextEnPassant = conveyorEnPassantAfterMove(
        boardState.enPassant,
        enPassantPawn,
        pawnSquare,
        pawnDir(boardState, opponent(boardState.enPassant.color)),
        boardRowCount(boardState),
        boardColCount(boardState)
      );
      boardState.enPassant = nextEnPassant && !get(boardState, nextEnPassant.row, nextEnPassant.col) ? nextEnPassant : null;
    }
    if (moved) {
      boardState.chainBonds = partitionChainBondsByRange(
        boardState.chainBonds,
        (id) => findWorkerPieceByRef(boardState, { id })
      ).active;
      resolveWorkerRuleBombsUnderPieces(boardState, movingColor);
    }
    return moved;
  }
  function tickWorkerPoisonStunnedPieces(boardState, color) {
    forEachPieceCached(boardState, (piece) => {
      if (!shouldTickPoisonStun(piece, color)) return;
      piece.poisonStunTurns = Math.max(0, Number(piece.poisonStunTurns) - 1);
      if (!piece.poisonStunTurns) {
        delete piece.poisonStunTurns;
        delete piece.poisonStunColor;
      }
    });
  }
  function tickWorkerArmisticeAfterAction(boardState, movingColor) {
    const current = normalizeArmistice(boardState.armistice);
    if (!current) {
      boardState.armistice = null;
      return;
    }
    if (!COLORS.includes(movingColor)) return;
    current.actedColors = [.../* @__PURE__ */ new Set([...current.actedColors || [], movingColor])];
    if (current.actedColors.length >= COLORS.length) {
      current.remaining = Math.max(0, current.remaining - 1);
      current.actedColors = [];
    }
    boardState.armistice = current.remaining ? current : null;
  }
  function resolveWorkerPendingPortalsForTurn(boardState, color) {
    boardState.pendingPortals = normalizePendingPortals(boardState.pendingPortals, boardRowCount(boardState), boardColCount(boardState));
    const due = boardState.pendingPortals.filter((entry) => entry.color === color && (Number(boardState.turnsTaken?.[color]) || 0) >= entry.triggerTurn);
    if (!due.length) return false;
    const dueIds = new Set(due.map((entry) => entry.id));
    boardState.pendingPortals = boardState.pendingPortals.filter((entry) => !dueIds.has(entry.id));
    const selected = due[due.length - 1];
    if (selected.cells.some((cell) => selected.blocksMovement && get(boardState, cell.row, cell.col) || !inBounds(cell.row, cell.col, boardState) || isBlackHoleCell(boardState, cell.row, cell.col) || workerSeptember12PlacementCrownBlocked(boardState, cell.row, cell.col) || isWorkerCollapsedSquare(boardState, cell.row, cell.col))) return false;
    boardState.portalRule = normalizePortalRule({ enabled: true, cells: selected.cells }, boardRowCount(boardState), boardColCount(boardState));
    return Boolean(boardState.portalRule);
  }
  function workerDemocracyCitizenEntries(boardState, color) {
    return piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn");
  }
  function workerHasDemocracyPawn(boardState, color) {
    if (boardState.pendingRecurrences?.some((entry) => entry.piece.color === color && entry.piece.type === "pawn")) return true;
    return workerDemocracyCitizenEntries(boardState, color).length > 0;
  }
  function isWorkerDemocracyProtectedPiece(boardState, piece) {
    return Boolean(piece?.color && boardState?.democracy?.[piece.color] && isDemocracyProtectedRoyal(boardState.democracy, piece));
  }
  function workerNextJudgmentDraftPhase(boardState) {
    if (!boardState || boardState.draftDelete || boardState.gameStyle === "grand") return null;
    if (!boardState.middleDraftDone) return "MIDDLE";
    if (!boardState.endDraftDone) return "END";
    return null;
  }
  function workerDemocracyCitizenStakeValue(boardState, piece) {
    if (!piece?.color || !boardState?.democracy?.[piece.color] || piece.type !== "pawn") return 0;
    const citizenCount = workerDemocracyCitizenEntries(boardState, piece.color).length;
    if (citizenCount <= 1) return 12e3;
    if (citizenCount === 2) return 1800;
    if (citizenCount === 3) return 700;
    return 250;
  }
  function workerDemocracyStrategicScore(boardState, color) {
    if (!boardState?.democracy?.[color]) return 0;
    const citizens = workerDemocracyCitizenEntries(boardState, color);
    if (!citizens.length) return -1e5;
    const threatenedCount = citizens.filter(({ piece, row, col }) => Boolean(workerBestCaptureThreat(boardState, piece, row, col))).length;
    let score = 500 + Math.min(citizens.length, 4) * 220;
    if (citizens.length === 1) {
      score += 4200;
      if (threatenedCount) score -= 9e3;
    } else if (citizens.length === 2) {
      score += 1200 - threatenedCount * 850;
    } else {
      score -= threatenedCount * 180;
    }
    return score;
  }
  function resolveWorkerDemocracyDefeat(boardState) {
    for (const color of COLORS) {
      if (boardState.democracy?.[color] && boardState.zugzwang?.[color] && !findWorkerKingRole(boardState, color)) boardState.zugzwang[color] = false;
    }
    if (boardState.mode === "gameover") return false;
    const defeated = COLORS.filter((color) => boardState.democracy?.[color] && !workerHasDemocracyPawn(boardState, color));
    if (!defeated.length) return false;
    boardState.mode = "gameover";
    boardState.winner = defeated.length > 1 ? "draw" : opponent(defeated[0]);
    return true;
  }
  function resolveWorkerDelayedHazards(boardState, movingColor) {
    const hazards = Array.isArray(boardState.delayedHazards) ? boardState.delayedHazards : [];
    const due = hazards.filter((hazard) => hazard?.triggerAfter === movingColor);
    boardState.delayedHazards = hazards.filter((hazard) => hazard?.triggerAfter !== movingColor);
    let removed = 0;
    due.forEach((hazard) => {
      const seen = /* @__PURE__ */ new Set();
      const owner = COLORS.includes(hazard?.owner) ? hazard.owner : opponent(movingColor);
      let caster = null;
      if (hazard.casterId) forEachPiece(boardState, (piece) => {
        if (piece.id === hazard.casterId) caster = piece;
      });
      (hazard?.cells || []).forEach(({ row, col }) => {
        const item = get(boardState, row, col);
        const identity = item?.id || item;
        const repeatsPerCell = hazard?.type === "meteor" && isHpPiece(item);
        if (!item || item.type === "wall" || !repeatsPerCell && seen.has(identity) || isFrozenPiece(item)) return;
        if (item.nullification && septemberNullificationBlocks(item, caster)) return;
        if (!repeatsPerCell) seen.add(identity);
        if (item.shielded || item.protected || ["football", "monster"].includes(item.type)) {
          if (item.shielded) item.shielded = false;
          return;
        }
        if (isWorkerEncouragedTarget(boardState, item)) return;
        if (isHpPiece(item)) {
          const currentHp = item.hp ?? item.maxHp ?? 1;
          item.hp = Math.max(0, currentHp - 1);
          if (item.hp > 0) return;
        }
        const captureOwner = item.color === owner ? opponent(item.color) : owner;
        recordWorkerCapturedPieces(boardState, captureOwner, [item]);
        workerCriticalCaptureScore(boardState, item, captureOwner, captureOwner);
        clearPieceCells(boardState, item);
        removed += 1;
      });
    });
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
    return removed;
  }
  function moveWorkerBabyBears(boardState, color) {
    const babies = piecesMatching(boardState, (piece) => piece.color === color && pieceHasAbility(piece, "babyBear"));
    let moved = 0;
    babies.forEach(({ piece, row, col }) => {
      if (get(boardState, row, col) !== piece) return;
      const candidates = queenDirections().map(([dr, dc]) => ({ row: row + dr, col: col + dc })).filter((cell) => inBounds(cell.row, cell.col, boardState) && !get(boardState, cell.row, cell.col) && !isWorkerCollapsedSquare(boardState, cell.row, cell.col) && !isBlackHoleCell(boardState, cell.row, cell.col));
      if (!candidates.length) return;
      const destination = candidates[workerStableIndex(`${piece.id}:${Number(boardState.moveCount) || 0}`, candidates.length)];
      set(boardState, row, col, null);
      set(boardState, destination.row, destination.col, piece);
      piece.moved = true;
      if (piece.type === "trickster") {
        rerollWorkerTricksterAbility(boardState, piece, `${piece.id || "trickster"}:${boardState.moveCount || 0}:baby-bear`);
      }
      moved += 1;
    });
    return moved;
  }
  function workerBabyBearGrowthDue(boardState, piece) {
    const readyTurn = Number(piece?.babyBearGrowAtTurn);
    if (Number.isFinite(readyTurn)) return sharedTurnCount(boardState) >= readyTurn;
    const legacyReadyMove = Number(piece?.babyBearGrowAtMove);
    return Number.isFinite(legacyReadyMove) && (Number(boardState.moveCount) || 0) >= legacyReadyMove;
  }
  function resolveWorkerBabyBearGrowthForTurnStart(boardState, color) {
    let grown = 0;
    forEachPiece(boardState, (piece) => {
      if (piece?.color !== color || !pieceHasAbility(piece, "babyBear") || !workerBabyBearGrowthDue(boardState, piece)) return;
      piece.type = "bear";
      delete piece.tricksterMoveType;
      delete piece.tricksterPreviousAbilityForTurn;
      piece.bearRetaliationsRemaining = 2;
      delete piece.babyBearGrowAtTurn;
      delete piece.babyBearGrowAtMove;
      delete piece.babyBearMoveAfterTurn;
      piece.moved = true;
      grown += 1;
    });
    return grown;
  }
  function resolveWorkerEmptyLunchboxesAfterTurn(boardState, color) {
    const entries = piecesMatching(boardState, (piece) => piece.color === color && piece.emptyLunchbox);
    entries.forEach(({ piece, row, col }) => {
      const royal = findWorkerKingRole(boardState, color);
      if (royal && isAdjacentCell({ row, col }, royal)) {
        delete piece.emptyLunchbox;
        return;
      }
      if ((Number(boardState.turnsTaken?.[color]) || 0) < (Number(piece.emptyLunchbox.deadlineTurn) || 0)) return;
      const by = COLORS.includes(piece.emptyLunchbox.by) ? piece.emptyLunchbox.by : opponent(color);
      delete piece.emptyLunchbox;
      clearPieceCells(boardState, piece);
      recordWorkerCapturedPieces(boardState, by, [piece]);
      if (isWorkerDecisiveCaptureTarget(boardState, piece)) workerCriticalCaptureScore(boardState, piece, by, by);
    });
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
  }
  function spawnWorkerPlatformAtTurn(boardState, currentTurn) {
    if (!boardState.platformRule?.enabled) return false;
    const candidates = [];
    for (let row = 0; row < boardRowCount(boardState); row += 1) {
      for (let col = 0; col < boardColCount(boardState); col += 1) {
        if (!workerOpenPlacementSquare(boardState, row, col) || workerSeptember12PlacementCrownBlocked(boardState, row, col) || isWorkerCollapsedSquare(boardState, row, col) || isBlackHoleCell(boardState, row, col)) continue;
        candidates.push({ row, col });
      }
    }
    const index = workerStableIndex(`platform:${currentTurn}:${boardState.platformRule.nonce || 0}`, candidates.length);
    boardState.platformRule.cell = candidates[index] || null;
    boardState.platformRule.cells = boardState.platformRule.cell ? [boardState.platformRule.cell] : [];
    boardState.platformRule.fixed = false;
    boardState.platformRule.spawnedAt = currentTurn;
    boardState.platformRule.nextAt = currentTurn + platformInterval(boardState.platformRule);
    boardState.platformRule.triggeredIds = [];
    boardState.platformRule.nonce = (Number(boardState.platformRule.nonce) || 0) + 1;
    return Boolean(boardState.platformRule.cell);
  }
  function tickWorkerPlatformRuleAfterTurn(boardState) {
    if (!boardState.platformRule?.enabled) return false;
    const currentTurn = platformTurnCount(boardState.turnsTaken, boardState.platformRule);
    boardState.platformRule = normalizePlatformRule(boardState.platformRule, currentTurn, boardRowCount(boardState), boardColCount(boardState));
    if (!boardState.platformRule || boardState.platformRule.fixed || currentTurn < boardState.platformRule.nextAt) return false;
    return spawnWorkerPlatformAtTurn(boardState, currentTurn);
  }
  function workerSirenConvertible(boardState, piece) {
    return Boolean(
      piece && COLORS.includes(piece.color) && !workerIsLargePiece(piece) && !["wall", "football", "blackHole", "monster", "coffin", "crown"].includes(piece.type)
    );
  }
  const WORKER_SIREN_TURN_START_KEY = "__turnStartKey";
  function tickWorkerSirenExposureForTurnStart(boardState, color = boardState?.turn) {
    if (boardState?.mode !== "play" || boardState.turn !== color || !COLORS.includes(color)) return false;
    if (!boardState.sirenExposure || typeof boardState.sirenExposure !== "object") boardState.sirenExposure = {};
    const turnStartKey = `${color}:${Math.max(0, Number(boardState.turnsTaken?.[color]) || 0)}`;
    if (boardState.sirenExposure[WORKER_SIREN_TURN_START_KEY] === turnStartKey) return false;
    boardState.sirenExposure[WORKER_SIREN_TURN_START_KEY] = turnStartKey;
    const sirens = piecesMatching(boardState, (piece) => workerPieceHasActiveOrPreviousTricksterAbility(piece, "siren") && COLORS.includes(piece.color));
    const activeIds = new Set(sirens.map(({ piece }) => piece.id).filter(Boolean));
    Object.keys(boardState.sirenExposure).forEach((id) => {
      if (id === WORKER_SIREN_TURN_START_KEY) return;
      if (!activeIds.has(id)) delete boardState.sirenExposure[id];
    });
    sirens.forEach(({ piece: siren, row, col }) => {
      if (boardState.mode === "gameover") return;
      if (!siren.id || siren.color !== color) return;
      const nearby = [];
      forEachPiece(boardState, (target, targetRow2, targetCol2) => {
        if (!target?.id || target.color === siren.color || !workerSirenConvertible(boardState, target)) return;
        if (Math.abs(targetRow2 - row) <= 1 && Math.abs(targetCol2 - col) <= 1) nearby.push(target.id);
      });
      const advanced = nextSirenExposure(boardState.sirenExposure[siren.id], nearby);
      boardState.sirenExposure[siren.id] = advanced.counts;
      advanced.converted.forEach((pieceId) => {
        if (boardState.mode === "gameover") return;
        const target = piecesMatching(boardState, (candidate) => candidate.id === pieceId)[0];
        if (!target || target.piece.color === siren.color || !workerSirenConvertible(boardState, target.piece)) return;
        const converted = { ...target.piece };
        target.piece.color = siren.color;
        target.piece.moved = true;
        delete boardState.sirenExposure[siren.id][pieceId];
        if (workerPieceHasActiveOrPreviousTricksterAbility(target.piece, "siren")) delete boardState.sirenExposure[target.piece.id];
        workerCriticalCaptureScore(boardState, converted, siren.color, siren.color);
      });
    });
    return true;
  }
  function resolveWorkerUndeadResurrectionsAfterMove(boardState) {
    if (!Array.isArray(boardState.undeadResurrections) || !boardState.undeadResurrections.length) return;
    const remaining = [];
    boardState.undeadResurrections.forEach((entry) => {
      const advanced = advanceScheduledHalfTurn(entry);
      const usesBoundaryCountdown = advanced !== entry;
      if (usesBoundaryCountdown ? undeadResurrectionRemainingHalfMoves(advanced, boardState.moveCount) > 0 : (Number(boardState.moveCount) || 0) < (Number(entry?.dueMoveCount) || Number.POSITIVE_INFINITY)) {
        remaining.push(advanced);
        return;
      }
      const homeRow = entry.color === "white" ? boardRowCount(boardState) - 1 : 0;
      const cells = [];
      for (let col = 0; col < boardColCount(boardState); col += 1) {
        if (!get(boardState, homeRow, col) && workerOpenPlacementSquare(boardState, homeRow, col)) cells.push({ row: homeRow, col });
      }
      if (!cells.length) {
        remaining.push({ ...advanced, remainingHalfTurns: 1 });
        return;
      }
      const destination = cells[workerStableIndex(`${entry.id || entry.piece?.id}:${boardState.moveCount || 0}`, cells.length)];
      const revived = clonePiece(entry.piece || { color: entry.color, type: "undead" });
      if (!campaignAuthorityV1States.has(boardState)) clearTricksterUndeadResurrectionStatuses(revived);
      revived.color = entry.color;
      revived.type = entry.piece?.type === "trickster" ? "trickster" : "undead";
      if (revived.type === "trickster") setWorkerTricksterAbilityType(boardState, revived, "undead");
      revived.moved = true;
      revived.id = revived.id || `${entry.color}-undead-ai-${boardState.moveCount || 0}-${destination.col}`;
      set(boardState, destination.row, destination.col, revived);
      if (COLORS.includes(entry.capturedBy) && Array.isArray(boardState.captures?.[entry.capturedBy])) {
        const index = boardState.captures[entry.capturedBy].findIndex((piece) => piece?.id === revived.id);
        if (index >= 0) boardState.captures[entry.capturedBy].splice(index, 1);
      }
    });
    boardState.undeadResurrections = remaining;
  }
  function resolveWorkerGomokuVictory(boardState) {
    for (const color of COLORS) {
      if (!boardState.gomoku?.[color]) continue;
      if (verticalFiveColumns(boardState.board, color).length) {
        boardState.mode = "gameover";
        boardState.winner = color;
        return true;
      }
    }
    return false;
  }
  function resolveWorkerPendingLobstersAfterMove(boardState) {
    if (!Array.isArray(boardState.pendingLobsters) || !boardState.pendingLobsters.length) return;
    const moveCount = Number(boardState.moveCount) || 0;
    const due = [];
    boardState.pendingLobsters = boardState.pendingLobsters.map((entry) => advanceScheduledHalfTurn(entry)).filter((entry) => {
      if (!entry || !inBounds(entry.row, entry.col, boardState)) return false;
      const usesBoundaryCountdown = Number.isSafeInteger(Number(entry.remainingHalfTurns));
      if (usesBoundaryCountdown ? Number(entry.remainingHalfTurns) <= 0 : Number(entry.dueMoveCount) <= moveCount) {
        due.push(entry);
        return false;
      }
      return true;
    });
    for (const entry of due) {
      if (!workerOpenPlacementSquare(boardState, entry.row, entry.col) || workerSeptember12PlacementCrownBlocked(boardState, entry.row, entry.col)) continue;
      const lobster = workerCreatePiece(entry.color, "lobster", boardState, entry.row, entry.col);
      lobster.origin = workerSquareName(boardState, entry.row, entry.col);
      lobster.moved = true;
      markWorkerFreshNoCapture(boardState, lobster);
      set(boardState, entry.row, entry.col, lobster);
    }
  }
  function retainWorkerTimeStopAsSameTurn(boardState, color) {
    if (!boardState.skipTurn) boardState.skipTurn = { white: false, black: false };
    const stoppedColor = opponent(color);
    if (!boardState.skipTurn[stoppedColor]) return false;
    boardState.skipTurn[stoppedColor] = false;
    boardState.turn = color;
    boardState.enPassant = null;
    boardState.actionsRemaining = workerActionLimit(boardState);
    return true;
  }
  function updateWorkerCaptureFlags(boardState, actor, completed = false) {
    if (resolveInternalHighlander(boardState)) return true;
    if (!boardState.captureTheFlag || boardState.mode === "gameover") return false;
    const after = { ...boardState.turnsTaken }, before = { ...after };
    if (completed) before[actor] = Math.max(0, (Number(after[actor]) || 0) - 1);
    const result = septemberAdvanceFlags(boardState.captureTheFlag, { actor, beforeTurns: before, afterTurns: after, at: (row, col) => get(boardState, row, col) });
    boardState.captureTheFlag = result.rule;
    if (!result.defeated.length) return false;
    boardState.mode = "gameover";
    boardState.winner = result.defeated.length === 2 ? "draw" : opponent(result.defeated[0]);
    return true;
  }
  function finishWorkerMove(boardState, color, options = {}) {
    resolveWorkerRecurrences(boardState);
    if (!boardState.turnsTaken) boardState.turnsTaken = { white: 0, black: 0 };
    clearWorkerIdolEncoreRepeatBlocks(boardState, color);
    resolveWorkerRuleBombsUnderPieces(boardState, color);
    resolveWorkerDemocracyDefeat(boardState);
    if (boardState.mode === "gameover") return;
    resolveWorkerTwinSwaps(boardState);
    completeThreeTurn(boardState.board, color);
    resolveWorkerSubmergedPieces(boardState);
    tickThiefArrests(boardState.board, color, boardState);
    if (boardState.disassembly) boardState.disassembly[color] = false;
    if (updateWorkerCaptureFlags(boardState, color)) return;
    resolveWorkerDoubleCheckThreats(boardState, color);
    resolveWorkerSpecialObjectiveVictories(boardState, color);
    if (boardState.mode === "gameover") return;
    const idolEncorePending = boardState.idolEncorePending;
    if (idolEncorePending?.color === color) {
      delete boardState.idolEncorePending;
      if (workerIdolEncoreAvailable(boardState, idolEncorePending.idolId, color)) {
        let idolPiece = null;
        let movedPiece = null;
        forEachPiece(boardState, (piece) => {
          if (!idolPiece && piece?.id === idolEncorePending.idolId) idolPiece = piece;
          if (!movedPiece && piece?.id === idolEncorePending.pieceId) movedPiece = piece;
        });
        const validEncorePieces = pieceHasAbility(idolPiece, "idol") && idolPiece.color === color && (!movedPiece && usesSeptember18Balance(boardState) || movedPiece && movedPiece.color === color && !pieceHasAbility(movedPiece, "idol"));
        if (validEncorePieces && (!movedPiece || markWorkerIdolEncoreRepeatBlock(boardState, movedPiece.id, color))) {
          boardState.turn = color;
          const canMoveAgain = generateActions(boardState, color).some((action) => action?.type === "move");
          if (canMoveAgain) {
            if (!boardState.idolEncoreUsedByPiece) boardState.idolEncoreUsedByPiece = {};
            boardState.idolEncoreUsedByPiece[`turn:${color}`] = Number(boardState.turnsTaken?.[color]) || 0;
            return;
          }
          if (movedPiece) delete movedPiece.idolEncoreRestTurn;
        }
      }
    }
    if (septemberUseResolveCredit(boardState, color)) {
      boardState.turn = color;
      return;
    }
    const remaining = Math.max(1, Number(boardState.actionsRemaining) || 1);
    if (remaining > 1) {
      boardState.actionsRemaining = remaining - 1;
      boardState.turn = color;
      return;
    }
    if (retainWorkerTimeStopAsSameTurn(boardState, color)) return;
    tickWorkerTemporaryTransformations(boardState, color);
    resolveWorkerDelayedHazards(boardState, color);
    if (boardState.mode === "gameover") return;
    if (options.countMove !== false) {
      advanceWorkerCompletedMoveCount(boardState, color);
    } else {
      clearWorkerPreviousTricksterAbilities(boardState);
    }
    if (boardState.mode === "gameover") return;
    tickWorkerTimedTurnEffects(boardState, color);
    if (boardState.mode === "gameover") return;
    if (boardState.taunt?.[color] > 0) boardState.taunt[color] -= 1;
    tickWorkerStakedPieces(boardState, color);
    tickWorkerIceSheetPieces(boardState, color);
    tickWorkerBribedPieces(boardState, color);
    clearWorkerRepositionMarks(boardState, color);
    if (legacyCompletedTurnEffectsStates.has(boardState)) clearWorkerChargeRush(boardState, color);
    clearWorkerSameTurnMoveEffects(boardState, color);
    resolveWorkerDoubleCheckThreats(boardState, color);
    resolveWorkerSpecialObjectiveVictories(boardState, color);
    if (boardState.mode === "gameover") return;
    if (boardState.switcheroo) boardState.switcheroo[color] = false;
    if (boardState.enPassantFrenzy) boardState.enPassantFrenzy[color] = false;
    if (boardState.relay) boardState.relay[color] = false;
    boardState.zugzwang = expireZugzwangAfterCompletedTurn(boardState.zugzwang, color);
    if (boardState.substitution) boardState.substitution[color] = false;
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color) {
        delete piece.frenzy;
        delete piece.frenzyExtraMove;
      }
    });
    clearWorkerRoyalCommands(boardState, color);
    if (color === "black") {
      resolveWorkerConveyorAfterMove(boardState, color);
      if (boardState.mode === "gameover") return;
    }
    if (boardState.reversal) boardState.reversal[color] = false;
    septemberFinishResolveTurn(boardState, color);
    boardState.turnsTaken[color] = (Number(boardState.turnsTaken[color]) || 0) + 1;
    if (!boardState.freeMoveCaptureLock) boardState.freeMoveCaptureLock = { white: false, black: false };
    boardState.freeMoveCaptureLock[color] = false;
    resolveWorkerPendingFreeMovesAfterTurn(boardState, color);
    resolveWorkerEmptyLunchboxesAfterTurn(boardState, color);
    if (boardState.mode === "gameover") return;
    tickWorkerPlatformRuleAfterTurn(boardState);
    resolveWorkerCrownRuleAfterMove(boardState);
    if (boardState.mode === "gameover") return;
    boardState.mistakeCard = normalizeDemocracyState(boardState.mistakeCard);
    boardState.mistakeCard[color] = false;
    if (boardState.othelloPending?.[color]) {
      boardState.othelloPending[color] = false;
      resolveWorkerOthelloAll(boardState, color);
    }
    if (boardState.pendingScarecrows?.some((e) => e.pieceId)) {
      boardState.pendingScarecrows = boardState.pendingScarecrows.filter((entry) => {
        if (!entry.pieceId) return true;
        const target = piecesMatching(boardState, (piece) => piece.id === entry.pieceId)[0];
        if (!target) return false;
        if (entry.by === (usesSeptember18Balance(boardState) ? opponent(color) : color)) entry.remainingOwnTurns--;
        if (entry.remainingOwnTurns > 0) return true;
        const decisive = isWorkerDecisiveCaptureTarget(boardState, target.piece);
        clearPieceCells(boardState, target.piece);
        const identity = { id: target.piece.id, color: target.piece.color, type: "scarecrow", moved: true, origin: workerSquareName(boardState, target.row, target.col) };
        for (const key of Object.keys(target.piece)) delete target.piece[key];
        Object.assign(target.piece, identity);
        boardState.board[target.row][target.col] = target.piece;
        if (decisive) {
          boardState.mode = "gameover";
          boardState.winner = opponent(target.piece.color);
        }
        return false;
      });
    }
    tickWorkerReservedScarecrows(boardState, color);
    resolveWorkerPendingGalesAfterTurn(boardState, color);
    if (boardState.mode === "gameover") return;
    tickWorkerSacrificeProtection(boardState, color);
    tickWorkerCardFrozenPieces(boardState, color);
    tickWorkerPoisonStunnedPieces(boardState, color);
    if (!legacyCompletedTurnEffectsStates.has(boardState)) resolveWorkerChargeRushFailures(boardState, color);
    clearWorkerBloodMoonTurnEffects(boardState, color);
    if (color === "black") boardState.fullMove = (Number(boardState.fullMove) || 1) + 1;
    applyWorkerWinterFreezeCycle(boardState);
    resolveWorkerOneShotCollapse(boardState, color);
    resolveWorkerPeriodicCollapseAfterTurn(boardState, color);
    if (boardState.mode === "gameover") return;
    resolveWorkerDoubleCheckThreats(boardState, color);
    resolveWorkerSpecialObjectiveVictories(boardState, color);
    if (boardState.mode === "gameover") return;
    if (resolveWorkerUltimatumIfDue(boardState)) return;
    if (tickWorkerPropheciesAfterTurn(boardState)) return;
    applyWorkerPendingPawnStormForTurn(boardState, color);
    resolveWorkerDoubleCheckThreats(boardState, color);
    resolveWorkerSpecialObjectiveVictories(boardState, color);
    if (boardState.mode === "gameover") return;
    if (options.countMove !== false) tickWorkerArmisticeAfterAction(boardState, color);
    advanceWorkerTurn(boardState, color);
    if (boardState.mode === "gameover") return;
    tickWorkerSirenExposureForTurnStart(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    if (boardState.turn !== color) {
      resolveWorkerSpecialObjectiveVictories(boardState, boardState.turn);
      if (boardState.mode === "gameover") return;
    }
    resolveWorkerVanishingForTurnStart(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    resolveWorkerBabyBearGrowthForTurnStart(boardState, boardState.turn);
    moveWorkerBabyBears(boardState, boardState.turn);
    resolveWorkerTwinSwaps(boardState);
    resolveWorkerVipInvitationsForTurn(boardState, boardState.turn);
    resolveWorkerPendingPortalsForTurn(boardState, boardState.turn);
    resolveWorkerPendingIcbmForTurn(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    resolveWorkerDoubleCheckThreats(boardState, boardState.turn);
    resolveWorkerSpecialObjectiveVictories(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    tickWorkerPendingRuleTicketsForTurnStart(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    applyWorkerPendingPanicForTurn(boardState, boardState.turn);
    applyWorkerPendingTrolleyForTurn(boardState, boardState.turn);
    maybeGrantWorkerNightBloodForTurn(boardState, boardState.turn);
    resolveWorkerHoldoutPromotions(boardState, boardState.turn);
    resolveWorkerBrutus(boardState, boardState.turn);
    resolveWorkerDoubleCheckThreats(boardState, boardState.turn);
    resolveWorkerSpecialObjectiveVictories(boardState, boardState.turn);
  }
  function advanceWorkerCompletedMoveCount(boardState, color) {
    boardState.moveCount = (Number(boardState.moveCount) || 0) + 1;
    resolveWorkerPendingLobstersAfterMove(boardState);
    clearWorkerPreviousTricksterAbilities(boardState);
    resolveWorkerUndeadResurrectionsAfterMove(boardState);
    resolveWorkerGomokuVictory(boardState);
    if (boardState.mode === "gameover") return;
    resolveWorkerPendingOtherworldAfterMove(boardState, color);
    resolveWorkerSubmergedPieces(boardState);
    if (color === "black") moveWorkerRuleMonsters(boardState, color);
  }
  function retainWorkerTurn(boardState, color) {
    boardState.moveCount = (Number(boardState.moveCount) || 0) + 1;
    boardState.turn = color;
    boardState.actionsRemaining = workerActionLimit(boardState);
  }
  function workerGuardPawnSquare(boardState, color) {
    const kingSquare = findWorkerRoyalKing(boardState, color);
    if (!kingSquare) return null;
    const kingOrigin = workerOriginalKingSquare(boardState, kingSquare.piece, color);
    const candidates = piecesMatching(boardState, (piece) => piece.color === color && isGuardConvertiblePawnType(piece.type)).map((candidate) => ({
      ...candidate,
      origin: parseWorkerSquare(
        boardState,
        candidate.piece.origin || workerSquareName(boardState, candidate.row, candidate.col)
      )
    }));
    return findGuardPawnByOrigin(candidates, kingOrigin, pawnDir(boardState, color));
  }
  function isWorkerRoyalCommandTurnActive(boardState, color) {
    const turnsTaken = Number(boardState?.turnsTaken?.[color]) || 0;
    return isRoyalCommandWindowActive(boardState?.royalCommand?.[color], turnsTaken);
  }
  function hasWorkerRoyalCommandCaptureAccess(boardState, piece) {
    return Boolean(piece?.crownBearer || !workerSocialismSuppressesRoyalCommand(boardState, piece) && (piece?.royalCommand || piece?.color && isWorkerRoyalCommandTurnActive(boardState, piece.color)));
  }
  function workerSocialismSuppressesRoyalCommand(boardState, piece) {
    if (legacySeptember12RulesStates.has(boardState)) return false;
    return Boolean(boardState?.socialism?.[piece?.color] > 0 && piece.type !== "crown" && (piece.type === "merchant" && !usesSeptember18Balance(boardState) || !isWorkerRoyalIdentityPiece(boardState, piece)));
  }
  function clearWorkerRoyalCommands(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color) delete piece.royalCommand;
    });
    if (!boardState.royalCommand) boardState.royalCommand = { white: null, black: null };
    if (shouldExpireRoyalCommandWindowAfterTurn(
      boardState.royalCommand[color],
      Number(boardState.turnsTaken?.[color]) || 0
    )) {
      boardState.royalCommand[color] = null;
    }
  }
  function clearWorkerSameTurnMoveEffects(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color) delete piece.rookLiftChain;
    });
    if (boardState.freeCastling) boardState.freeCastling[color] = false;
    if (boardState.bishopSnipe) boardState.bishopSnipe[color] = false;
    if (boardState.breakthroughPawns) boardState.breakthroughPawns[color] = false;
    if (boardState.switcheroo) boardState.switcheroo[color] = false;
    if (boardState.enPassantFrenzy) boardState.enPassantFrenzy[color] = false;
    if (boardState.relay) boardState.relay[color] = false;
  }
  function tickWorkerTimedTurnEffects(boardState, color) {
    if (legacyCompletedTurnEffectsStates.has(boardState)) return;
    resolveWorkerWitchTrials(boardState, color);
    if (boardState.mode === "gameover") return;
    if (boardState.effects?.pawnQueen === color) boardState.effects.pawnQueen = null;
    if (boardState.effects?.pawnReverse?.[color] > 0) boardState.effects.pawnReverse[color] -= 1;
    if (boardState.socialism?.[color] > 0) boardState.socialism[color] -= 1;
    for (const key of ["hallucination", "diceLocks"]) {
      const effect = boardState[key]?.[color];
      if (effect && --effect.remaining <= 0) boardState[key][color] = null;
    }
    const traveler = timeTravelerState(boardState);
    if (traveler?.attackEnabledFor === color) traveler.attackEnabledFor = null;
    forEachPiece(boardState, (piece) => {
      if (piece.color !== color) return;
      for (const key of ["disarmed", "severed"]) {
        if (Number.isFinite(piece[key]?.remaining) && --piece[key].remaining <= 0) delete piece[key];
      }
      if (piece.lastResistance && --piece.lastResistance.remaining <= 0) {
        const keepProtected = Boolean(piece.lastResistance.previousProtected || piece.coronationProtection || piece.sacrificeProtection || piece.queensGambitProtection);
        delete piece.lastResistance;
        if (!keepProtected) delete piece.protected;
      }
      if (piece.type === "shotgunKing" && piece.snipeCooldown > 0) piece.snipeCooldown -= 1;
    });
  }
  function resolveWorkerWitchTrials(boardState, color) {
    if (legacyWitchTrialCaptureStates.has(boardState)) return;
    const removals = [];
    forEachPiece(boardState, (piece, row, col) => {
      if ((piece.witchTrial?.countBy && usesSeptember18Balance(boardState) ? opponent(piece.witchTrial.countBy) : piece.witchTrial?.countBy ?? piece.color) !== color || !piece.witchTrial) return;
      if (--piece.witchTrial.remaining <= 0) removals.push({ item: piece, row, col });
    });
    for (const { item, row, col } of removals) {
      if (!workerPieceRemainsOnBoard(boardState, item)) continue;
      const by = COLORS.includes(item.witchTrial.by) ? item.witchTrial.by : opponent(color);
      clearPieceCells(boardState, item);
      recordWorkerCapturedPieces(boardState, by, [item]);
      resolveWorkerReaperNearbyDeaths(boardState, [{ item, row, col }], by);
      if (isWorkerDecisiveCaptureTarget(boardState, item)) workerCriticalCaptureScore(boardState, item, by, by);
    }
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
  }
  function clearWorkerCoronationAtTurnStart(boardState, color) {
    if (legacyCompletedTurnEffectsStates.has(boardState)) return;
    forEachPiece(boardState, (piece) => {
      if (piece.color !== color || !piece.coronationProtection) return;
      const keepProtected = Boolean(piece.coronationProtection.previousProtected || piece.lastResistance || piece.sacrificeProtection || piece.queensGambitProtection);
      delete piece.coronationProtection;
      if (!keepProtected) delete piece.protected;
    });
  }
  function tickWorkerTemporaryTransformations(boardState, color) {
    if (legacyCompletedTurnEffectsStates.has(boardState)) return;
    if (boardState.temporaryQueens) boardState.temporaryQueens = boardState.temporaryQueens.filter((entry) => {
      if (entry.color !== color) return true;
      const piece = findWorkerPieceByRef(boardState, { id: entry.id })?.piece;
      entry.remaining -= 1;
      if (piece) piece.bribedRemaining = Math.max(0, entry.remaining);
      if (entry.remaining > 0) return true;
      if (piece?.type === "amazon" && piece.bribed) {
        piece.type = monochromePieceType("knight", boardState.monochromeChess);
        piece.bribed = false;
        piece.bribedRemaining = null;
      }
      return false;
    });
    if (boardState.necromancy) boardState.necromancy = boardState.necromancy.filter((entry) => {
      if (!entry || entry.color !== color) return true;
      const piece = findWorkerPieceByRef(boardState, { id: entry.id })?.piece;
      if (!piece) return false;
      const revivedType = entry.revivedType || piece.necromancy?.revivedType;
      if (piece.color !== entry.color || !piece.necromancy || typeof revivedType !== "string" || piece.type !== revivedType) {
        delete piece.necromancy;
        delete piece.necromancyRemaining;
        return false;
      }
      entry.remaining = Math.max(0, Number(entry.remaining) || 0) - 1;
      piece.necromancyRemaining = piece.necromancy.remaining = Math.max(0, entry.remaining);
      if (entry.remaining > 0) return true;
      piece.type = "pawn";
      delete piece.necromancy;
      delete piece.necromancyRemaining;
      return false;
    });
  }
  function applyWorkerPendingPawnStormForTurn(boardState, color) {
    if (!Array.isArray(boardState.pendingPawnStorm) || !boardState.pendingPawnStorm.length) return 0;
    const due = boardState.pendingPawnStorm.filter((entry) => entry?.color === color);
    if (!due.length) return 0;
    boardState.pendingPawnStorm = boardState.pendingPawnStorm.filter((entry) => entry?.color !== color);
    const refs = [];
    const seen = /* @__PURE__ */ new Set();
    due.forEach((entry) => {
      (entry.pieces || []).forEach((pieceRef) => {
        const id = String(pieceRef?.id || "");
        if (!id || seen.has(id)) return;
        seen.add(id);
        refs.push(pieceRef);
      });
    });
    const dir = pawnDir(boardState, color);
    const ordered = refs.map((pieceRef) => findWorkerPieceByRef(boardState, pieceRef)).filter((square) => square && workerCanPawnStormAdvance(boardState, square.piece, square.row, square.col, color)).sort((a, b) => (dir < 0 ? a.row - b.row : b.row - a.row) || a.col - b.col);
    let moved = 0;
    ordered.forEach(({ piece, row, col }) => {
      if (!workerCanPawnStormAdvance(boardState, piece, row, col, color)) return;
      const nextRow = row + dir;
      set(boardState, row, col, null);
      set(boardState, nextRow, col, piece);
      piece.moved = true;
      noteWorkerUltimatumMovement(boardState, piece);
      if (isPromotionRow(piece, nextRow, boardState)) workerAutoPromotePawnStormPiece(boardState, piece, nextRow);
      moved += 1;
    });
    return moved;
  }
  function workerAutoPromotePawnStormPiece(boardState, piece, promotionRow) {
    if (!piece || !["pawn", "squire", "standardBearer"].includes(piece.type)) return false;
    if (boardState.recycling) {
      const recycledType = workerBestRecyclingPromotionType(boardState, piece.color);
      if (recycledType && consumeWorkerRecycledPromotionPiece(boardState, piece.color, recycledType)) {
        piece.type = recycledType;
      } else {
        piece.noPromotion = true;
        return false;
      }
    } else {
      piece.type = workerAutomaticPromotionType(boardState, piece, promotionRow);
    }
    if (piece.type === "pawn") piece.noPromotion = true;
    else delete piece.noPromotion;
    delete piece.holdoutPromotion;
    clearWorkerPromotionTraits(piece, boardState);
    if (piece.type !== "pawn") piece.promotedFromPawn = true;
    if (boardState.coronation?.[piece.color]) {
      const previousProtected = Boolean(piece.protected);
      piece.protected = true;
      piece.coronationProtection = { remaining: 1, previousProtected };
    }
    resolveWorkerSpyPromotion(piece);
    return true;
  }
  function applyWorkerPendingPanicForTurn(boardState, color) {
    if (!Array.isArray(boardState.pendingPanic) || !boardState.pendingPanic.length) return 0;
    const due = boardState.pendingPanic.filter((entry) => entry?.color === color);
    if (!due.length) return 0;
    boardState.pendingPanic = boardState.pendingPanic.filter((entry) => entry?.color !== color);
    let moved = 0;
    due.forEach((entry) => {
      (entry.pieces || []).forEach((pieceRef) => {
        const square = findWorkerPieceByRef(boardState, pieceRef);
        if (!square || !workerIsPanicTargetCandidate(boardState, square.piece, entry.by)) return;
        const moves = workerPanicRelocationMoves(boardState, square.piece, square.row, square.col);
        if (!moves.length) return;
        const index = workerStableIndex(`${square.piece.id || ""}:${boardState.moveCount || 0}:${square.row}:${square.col}`, moves.length);
        const move = moves[index];
        relocateWorkerPanickedPiece(boardState, square.piece, square.row, square.col, move.row, move.col);
        moved += 1;
      });
    });
    return moved;
  }
  function applyWorkerPendingTrolleyForTurn(boardState, color) {
    if (boardState.activeTrolley || !Array.isArray(boardState.pendingTrolley) || !boardState.pendingTrolley.length) return 0;
    const pending = boardState.pendingTrolley.find((entry) => entry?.color === color);
    if (!pending) return 0;
    boardState.pendingTrolley = boardState.pendingTrolley.filter((entry) => entry !== pending);
    const pair = workerFindTrolleyBundlePair(boardState, color);
    if (!pair) return 0;
    const saveIndex = pair[1].value > pair[0].value ? 1 : 0;
    const doomed = pair[saveIndex === 0 ? 1 : 0];
    let removed = 0;
    (doomed.pieces || []).forEach((pieceRef) => {
      const square = findWorkerPieceByRef(boardState, pieceRef);
      if (!square || square.piece.color !== color || workerIsTrolleyExcludedPiece(boardState, square.piece)) return;
      clearPieceCells(boardState, square.piece);
      recordWorkerCapturedPieces(boardState, pending.by || opponent(color), [square.piece]);
      removed += 1;
    });
    return removed;
  }
  function findWorkerPieceByRef(boardState, ref) {
    const id = String(ref?.id || "");
    let found = null;
    if (id) {
      forEachPiece(boardState, (piece2, row2, col2) => {
        if (!found && String(piece2?.id || "") === id) found = { piece: piece2, row: row2, col: col2 };
      });
      if (found) return found;
    }
    const row = Number(ref?.row);
    const col = Number(ref?.col);
    const piece = get(boardState, row, col);
    return piece ? { piece, row, col } : null;
  }
  function workerPanicRelocationMoves(boardState, piece, row, col) {
    const previousTurn = boardState.turn;
    boardState.turn = piece.color;
    try {
      return generateMovesForPiece(boardState, piece, row, col).filter((move) => {
        if (!isWorkerMoveAllowed(boardState, piece, row, col, move)) return false;
        if (!Number.isInteger(move.row) || !Number.isInteger(move.col)) return false;
        if (move.castle || move.enPassant || move.jumpCapture || move.colossusAttack || move.colossusMove || move.colossusBody) return false;
        if (move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.dragonSwap || move.bigRookMove || move.setLogDirection) return false;
        if (isBlackHoleCell(boardState, move.row, move.col) || isPromotionRow(piece, move.row, boardState)) return false;
        return !get(boardState, move.row, move.col);
      }).sort((a, b) => a.row - b.row || a.col - b.col);
    } finally {
      boardState.turn = previousTurn;
    }
  }
  function relocateWorkerPanickedPiece(boardState, piece, fromRow, fromCol, toRow, toCol) {
    set(boardState, fromRow, fromCol, null);
    set(boardState, toRow, toCol, piece);
    piece.moved = true;
    delete piece.quantum;
    if (boardState.monochromeChess) piece.monoShade = squareShade(toRow, toCol);
    boardState.enPassant = null;
    pushRecentMove(boardState, {
      from: { row: fromRow, col: fromCol },
      to: { row: toRow, col: toCol },
      pieceId: piece.id || "",
      pieceType: piece.type || "",
      color: piece.color
    });
  }
  function workerStableIndex(seed, length) {
    if (length <= 1) return 0;
    let hash = 0;
    for (let index = 0; index < String(seed).length; index += 1) {
      hash = (hash << 5) - hash + String(seed).charCodeAt(index) | 0;
    }
    return Math.abs(hash) % length;
  }
  function applyWorkerTranscendence(boardState, piece, sourceType, row, col, seed = "") {
    if (!boardState.transcendenceRule || !piece) return "";
    const optionCount = sourceType === "pawn" ? 2 : 1;
    const roll = workerStableIndex(`transcendence:${seed}:${piece.id || ""}`, optionCount) / optionCount;
    const nextType = monochromePieceType(transcendenceUpgradeType(sourceType, roll), boardState.monochromeChess);
    if (!nextType) return "";
    delete piece.windmillMode;
    delete piece.logDir;
    delete piece.logRollAfterTurn;
    delete piece.mana;
    delete piece.maxMana;
    delete piece.ammo;
    delete piece.maxAmmo;
    delete piece.facing;
    piece.type = nextType;
    piece.moved = true;
    piece.origin = workerSquareName(boardState, row, col);
    piece.freshNoCaptureUntil = (Number(boardState.turnsTaken?.[piece.color]) || 0) + 1;
    if (boardState.monochromeChess) piece.monoShade = squareShade(row, col);
    return nextType;
  }
  function workerTryStartRepositionSecondMove(boardState, piece, row, col) {
    if (boardState.mode === "gameover" || !piece.repositionSecondMove || piece.repositionSecondMove.used) return false;
    clearWorkerRepositionMarks(boardState, piece.color);
    piece.repositionSecondMove = { used: true };
    if (generateMovesForPiece(boardState, piece, row, col).some((candidate) => isWorkerMoveAllowed(boardState, piece, row, col, candidate))) return true;
    delete piece.repositionSecondMove;
    return false;
  }
  function applyWorkerCastleMoveAction(boardState, action, aiColor, piece, color) {
    const from = action.from || {};
    const move = action.move || {};
    const rook = get(boardState, move.rookFrom?.row, move.rookFrom?.col);
    if (!rook || rook.color !== color || rook.type !== "rook") return { ok: false, score: 0 };
    if (!inBounds(move.row, move.col, boardState) || !inBounds(move.rookTo?.row, move.rookTo?.col, boardState)) return { ok: false, score: 0 };
    set(boardState, from.row, from.col, null);
    set(boardState, move.rookFrom.row, move.rookFrom.col, null);
    set(boardState, move.row, move.col, piece);
    set(boardState, move.rookTo.row, move.rookTo.col, rook);
    piece.moved = true;
    rook.moved = true;
    trackWorkerMovingProgress(boardState, piece);
    if (!boardState.castled) boardState.castled = { white: false, black: false };
    boardState.castled[color] = true;
    boardState.enPassant = null;
    noteWorkerUltimatumMovement(boardState, piece);
    noteWorkerUltimatumMovement(boardState, rook);
    if (workerTryStartRepositionSecondMove(boardState, piece, move.row, move.col) || workerTryStartPlatformExtraMove(boardState, piece) || workerTryStartPlatformExtraMove(boardState, rook)) {
      retainWorkerTurn(boardState, color);
    } else {
      finishWorkerMove(boardState, color);
    }
    pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
    return { ok: true, score: color === aiColor ? 120 : -120 };
  }
  function tickWorkerStakedPieces(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (!piece?.staked || piece.color !== color) return;
      piece.staked.remaining = Math.max(0, (Number(piece.staked.remaining) || 0) - 1);
      if (piece.staked.remaining > 0) return;
      delete piece.staked;
      piece.shielded = true;
    });
  }
  function tickWorkerIceSheetPieces(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (!piece?.iceSheet || piece.color !== color) return;
      piece.iceSheet.remaining = Math.max(0, (Number(piece.iceSheet.remaining) || 0) - 1);
      if (piece.iceSheet.remaining <= 0) delete piece.iceSheet;
    });
  }
  // Grafted from engine.optimized.js (our-only addition, paired with the
  // "bribe" targeting/apply branches grafted above): "bribe" temporarily
  // transforms an allied knight into an amazon for 3 of its own color's
  // turns, then reverts to knight. Mirrors tickWorkerIceSheetPieces exactly.
  function tickWorkerBribedPieces(boardState, color) {
    forEachPiece(boardState, (piece) => {
      if (!piece?.bribed || piece.color !== color) return;
      piece.bribed.remaining = Math.max(0, (Number(piece.bribed.remaining) || 0) - 1);
      if (piece.bribed.remaining > 0) return;
      piece.type = "knight";
      delete piece.bribed;
    });
  }
  function workerHoldoutReadyTurn(piece) {
    const readyTurn = Number(piece?.holdoutPromotion?.readyTurn);
    if (Number.isFinite(readyTurn)) return readyTurn;
    const legacyReadyMove = Number(piece?.holdoutPromotion?.readyMove);
    return Number.isFinite(legacyReadyMove) ? legacyReadyMove : 0;
  }
  function resolveWorkerHoldoutPromotions(boardState, color = boardState?.turn) {
    const dueTurn = sharedTurnCount(boardState);
    let promoted = 0;
    forEachPiece(boardState, (piece) => {
      if (piece?.type !== "pawn" || !piece.holdoutPromotion) return;
      if (color && piece.color !== color) return;
      const readyTurn = workerHoldoutReadyTurn(piece);
      if (dueTurn < readyTurn) return;
      if (boardState.recycling && !consumeWorkerRecycledPromotionPiece(boardState, piece.color, "queen")) return;
      delete piece.holdoutPromotion;
      piece.type = "queen";
      delete piece.noPromotion;
      piece.moved = true;
      clearWorkerPromotionTraits(piece, boardState);
      piece.promotedFromPawn = true;
      if (piece.type === "queen" && boardState.coronation?.[piece.color]) {
        const previousProtected = Boolean(piece.protected);
        piece.protected = true;
        piece.coronationProtection = { remaining: 1, previousProtected };
      }
      resolveWorkerSpyPromotion(piece);
      promoted += 1;
    });
    if (promoted > 0) resolveWorkerDemocracyDefeat(boardState);
    return promoted;
  }
  function clearWorkerPromotionTraits(piece, boardState = null) {
    if (!piece) return;
    if (piece.feudalContractId && Array.isArray(boardState?.feudalContracts)) {
      const contractId = piece.feudalContractId;
      const pawnId = piece.id;
      boardState.feudalContracts = boardState.feudalContracts.filter((entry) => entry.id !== contractId && entry.pawnId !== pawnId);
    }
    delete piece.feudalContractId;
    delete piece.shielded;
    delete piece.explosive;
    delete piece.chimera;
    delete piece.witchTrial;
    delete piece.severed;
    if (piece.potionBasicTraining) delete piece.basicTraining;
    if (piece.potionManner) piece.coolGuyCapturedLast = false;
    if (piece.potionSaturation) piece.capturesMade = 0;
    delete piece.potionBasicTraining;
    delete piece.potionManner;
    delete piece.potionSaturation;
    delete piece.chargeRush;
    delete piece.lastSprintPending;
    delete piece.trojanHorse;
    delete piece.vipInvitation;
    delete piece.holdoutPromotion;
  }
  function resolveWorkerSpyPromotion(piece) {
    if (!piece?.spyOwner || piece.spyOwner === piece.color) return false;
    piece.color = piece.spyOwner;
    delete piece.spyOwner;
    return true;
  }
  function forceWorkerTurnEnd(boardState, color) {
    if (!boardState.turnsTaken) boardState.turnsTaken = { white: 0, black: 0 };
    if (resolveWorkerDemocracyDefeat(boardState) || boardState.mode === "gameover") return;
    if (retainWorkerTimeStopAsSameTurn(boardState, color)) return;
    if (!legacyCompletedTurnEffectsStates.has(boardState)) {
      tickWorkerTemporaryTransformations(boardState, color);
      resolveWorkerDelayedHazards(boardState, color);
      if (boardState.mode === "gameover") return;
      advanceWorkerCompletedMoveCount(boardState, color);
      if (boardState.mode === "gameover") return;
      tickWorkerTimedTurnEffects(boardState, color);
      if (boardState.mode === "gameover") return;
      if (boardState.taunt?.[color] > 0) boardState.taunt[color] -= 1;
      tickWorkerStakedPieces(boardState, color);
      tickWorkerIceSheetPieces(boardState, color);
      clearWorkerRepositionMarks(boardState, color);
      clearWorkerSameTurnMoveEffects(boardState, color);
    }
    boardState.zugzwang = expireZugzwangAfterCompletedTurn(boardState.zugzwang, color);
    if (boardState.substitution) boardState.substitution[color] = false;
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color) {
        delete piece.frenzy;
        delete piece.frenzyExtraMove;
      }
    });
    clearWorkerRoyalCommands(boardState, color);
    if (color === "black") {
      resolveWorkerConveyorAfterMove(boardState, color);
      if (boardState.mode === "gameover") return;
    }
    if (!legacyCompletedTurnEffectsStates.has(boardState)) {
      if (boardState.reversal) boardState.reversal[color] = false;
      septemberFinishResolveTurn(boardState, color);
    }
    boardState.turnsTaken[color] = (Number(boardState.turnsTaken[color]) || 0) + 1;
    if (!boardState.freeMoveCaptureLock) boardState.freeMoveCaptureLock = { white: false, black: false };
    boardState.freeMoveCaptureLock[color] = false;
    resolveWorkerPendingFreeMovesAfterTurn(boardState, color);
    resolveWorkerEmptyLunchboxesAfterTurn(boardState, color);
    if (boardState.mode === "gameover") return;
    tickWorkerPlatformRuleAfterTurn(boardState);
    boardState.mistakeCard = normalizeDemocracyState(boardState.mistakeCard);
    boardState.mistakeCard[color] = false;
    if (!legacyCompletedTurnEffectsStates.has(boardState)) {
      tickWorkerSacrificeProtection(boardState, color);
      tickWorkerCardFrozenPieces(boardState, color);
    }
    tickWorkerPoisonStunnedPieces(boardState, color);
    resolveWorkerChargeRushFailures(boardState, color);
    if (!legacyCompletedTurnEffectsStates.has(boardState)) clearWorkerBloodMoonTurnEffects(boardState, color);
    if (color === "black") boardState.fullMove = (Number(boardState.fullMove) || 1) + 1;
    if (resolveWorkerUltimatumIfDue(boardState)) return;
    advanceWorkerTurn(boardState, color);
    tickWorkerSirenExposureForTurnStart(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    if (boardState.turn !== color) {
      resolveWorkerSpecialObjectiveVictories(boardState, boardState.turn);
      if (boardState.mode === "gameover") return;
    }
    resolveWorkerBabyBearGrowthForTurnStart(boardState, boardState.turn);
    moveWorkerBabyBears(boardState, boardState.turn);
    resolveWorkerTwinSwaps(boardState);
    resolveWorkerSpecialObjectiveVictories(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    resolveWorkerVipInvitationsForTurn(boardState, boardState.turn);
    if (resolveWorkerDemocracyDefeat(boardState)) return;
    resolveWorkerPendingPortalsForTurn(boardState, boardState.turn);
    resolveWorkerPendingIcbmForTurn(boardState, boardState.turn);
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
    if (boardState.mode === "gameover") return;
    tickWorkerPendingRuleTicketsForTurnStart(boardState, boardState.turn);
    if (boardState.mode === "gameover") return;
    applyWorkerPendingPanicForTurn(boardState, boardState.turn);
    applyWorkerPendingTrolleyForTurn(boardState, boardState.turn);
    if (resolveWorkerDemocracyDefeat(boardState)) return;
    resolveWorkerHoldoutPromotions(boardState, boardState.turn);
  }
  function advanceWorkerTurn(boardState, color) {
    if (updateWorkerCaptureFlags(boardState, color, true)) return;
    const nextColor = opponent(color);
    boardState.cardsUsedThisTurn = normalizeCardsUsedThisTurn(boardState.cardsUsedThisTurn);
    boardState.cardsUsedThisTurn[color] = 0;
    if (!boardState.magicGirlSurge) boardState.magicGirlSurge = { white: false, black: false };
    if (legacyCompletedTurnEffectsStates.has(boardState)) boardState.magicGirlSurge[color] = false;
    else {
      boardState.magicGirlSurge[color] = Boolean(boardState.magicGirlSurgeRefreshPending?.[color]);
      if (boardState.magicGirlSurgeRefreshPending) boardState.magicGirlSurgeRefreshPending[color] = false;
    }
    boardState.turn = nextColor;
    clearWorkerCoronationAtTurnStart(boardState, nextColor);
    boardState.actionsRemaining = workerActionLimit(boardState);
  }
  function applyFileSurgeSkipAction(boardState, action, aiColor) {
    const color = action.color || boardState.turn;
    const piece = get(boardState, action.from?.row, action.from?.col);
    if (piece?.thiefSecondMove && usesThiefRemake(boardState)) return { ok: false, score: 0 };
    if (!piece || piece.color !== color || !piece.thiefSecondMove && !piece.fileSurgeSecondMove && !piece.rookLiftSecondMove && !piece.ironMonarchExtraMove && !piece.madHorseSecondMove) return { ok: false, score: 0 };
    delete piece.thiefSecondMove;
    delete piece.fileSurgeSecondMove;
    delete piece.rookLiftSecondMove;
    delete piece.rookLiftChain;
    delete piece.ironMonarchExtraMove;
    delete piece.madHorseSecondMove;
    const queuedReasons = workerQueuedKnightExtraMoveReasons(piece);
    delete piece.queuedKnightExtraMoveReasons;
    const nextReason = queuedReasons.shift() ?? null;
    if (nextReason) {
      if (nextReason === "mad-horse") piece.madHorseSecondMove = true;
      if (nextReason === "file-surge") piece.fileSurgeSecondMove = true;
      if (queuedReasons.length) piece.queuedKnightExtraMoveReasons = queuedReasons;
      return { ok: true, score: color === aiColor ? 20 : -20 };
    }
    if (piece.queuedBackwardKnightTurn) {
      delete piece.queuedBackwardKnightTurn;
      retainWorkerTurn(boardState, color);
      return { ok: true, score: color === aiColor ? 35 : -35 };
    }
    finishWorkerMove(boardState, color, { countMove: false });
    return { ok: true, score: color === aiColor ? 35 : -35 };
  }
  function workerExplosionRemovalScore(boardState, item, aiColor) {
    const direction = item.color === aiColor ? -1 : 1;
    const value = pieceValue(item);
    if (isWorkerDecisiveCaptureTarget(boardState, item)) return direction * value * 0.18;
    if (value <= 130) return direction * value * 0.38;
    if (value < 500) return direction * value * 0.85;
    return direction * value * 1.25;
  }
  function applyWorkerExplosionsForEntries(boardState, entries, aiColor, alreadyCaptured = /* @__PURE__ */ new Set()) {
    if (!Array.isArray(entries) || !entries.length) return 0;
    const seen = /* @__PURE__ */ new Set();
    const removed = [];
    let score = 0;
    entries.forEach(({ row, col }) => {
      for (let dr = -1; dr <= 1; dr += 1) {
        for (let dc = -1; dc <= 1; dc += 1) {
          const targetRow2 = row + dr;
          const targetCol2 = col + dc;
          if (!inBounds(targetRow2, targetCol2, boardState)) continue;
          const item = get(boardState, targetRow2, targetCol2);
          if (!item || isFrozenPiece(item) || isIndirectAttackImmunePiece(item) || isBombImmuneNeutralPiece(item) || alreadyCaptured.has(item)) continue;
          const key = item.id || `${targetRow2}:${targetCol2}`;
          if (seen.has(key)) continue;
          seen.add(key);
          removed.push(item);
          const explosionCaptor = opponent(item.color);
          score += workerExplosionRemovalScore(boardState, item, aiColor);
          if (isWorkerDecisiveCaptureTarget(boardState, item)) {
            score += workerCriticalCaptureScore(boardState, item, explosionCaptor, aiColor);
          }
          clearPieceCells(boardState, item);
          recordWorkerCapturedPieces(boardState, explosionCaptor, [item]);
        }
      }
    });
    if (seen.size) cancelWorkerPropheciesByCapture(boardState);
    score += resolveWorkerReaperNearbyDeaths(boardState, removed, aiColor);
    resolveWorkerBatchRoyalDefeats(boardState, removed);
    return score;
  }
  function applyWorkerSubstitutionMoveAction(boardState, action, aiColor, piece, target, color) {
    const from = action.from || {};
    const move = action.move || {};
    if (!canWorkerResolveSubstitutionMove(boardState, piece, target)) return { ok: false, score: 0 };
    const largeSwap = workerIsLargePiece(piece) && workerIsLargePiece(target);
    if (largeSwap) {
      const sourceAnchor = { row: piece.anchorRow, col: piece.anchorCol };
      const targetAnchor = { row: target.anchorRow, col: target.anchorCol };
      clearPieceCells(boardState, piece);
      clearPieceCells(boardState, target);
      piece.anchorRow = targetAnchor.row;
      piece.anchorCol = targetAnchor.col;
      target.anchorRow = sourceAnchor.row;
      target.anchorCol = sourceAnchor.col;
      colossusCells(target.anchorRow, target.anchorCol).forEach((cell) => set(boardState, cell.row, cell.col, target));
      colossusCells(piece.anchorRow, piece.anchorCol).forEach((cell) => set(boardState, cell.row, cell.col, piece));
    } else {
      set(boardState, from.row, from.col, target);
      set(boardState, move.row, move.col, piece);
    }
    if (boardState.monochromeChess) {
      target.monoShade = squareShade(from.row, from.col);
      piece.monoShade = squareShade(move.row, move.col);
    }
    if (boardState.quantumPending?.[color]) boardState.quantumPending[color] = false;
    delete piece.quantum;
    delete target.quantum;
    piece.moved = true;
    target.moved = true;
    // Site parity (2026-09-19): swap paths also record thief movement and
    // disassemble a moved queen, like the plain-move path.
    disassembleMovedQueen(boardState, piece, from, move, piece.type, internalWorkerCallbacks(boardState));
    noteThiefMove(piece, from, move, boardState);
    if (usesThiefRemake(boardState)) noteThiefMove(target, move, from, boardState);
    if (piece.type === "trickster") {
      piece.tricksterPreviousAbilityForTurn = tricksterAbilityType(piece);
      rerollWorkerTricksterAbility(boardState, piece, `${piece.id || "trickster"}:${Number(boardState.moveCount) || 0}:${move.row}:${move.col}:substitution`);
    }
    trackWorkerMovingProgress(boardState, piece);
    noteWorkerUltimatumMovement(boardState, piece);
    noteWorkerUltimatumMovement(boardState, target);
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
    if (boardState.mode !== "gameover" && workerRacingKingWins(boardState, piece, move.row)) {
      resolveWorkerRacingKingVictory(boardState, color);
    }
    finishWorkerMove(boardState, color);
    pushRecentMove(boardState, {
      from: { row: from.row, col: from.col },
      to: { row: move.row, col: move.col },
      pieceId: piece.id || "",
      pieceType: piece.type || "",
      color
    });
    const positionalValue = Math.max(80, Math.min(420, Math.abs(pieceValue(piece) - pieceValue(target)) * 0.08 + 100));
    return { ok: true, score: color === aiColor ? positionalValue : -positionalValue };
  }
  function applyMoveAction(boardState, action, aiColor) {
    const piece0 = get(boardState, action.from?.row, action.from?.col), color0 = piece0?.color;
    const memory0 = piece0 ? rememberedBaseMovement(piece0, boardState.parrotMovement?.[color0]) : null;
    const metalBefore0 = metalPositions(boardState.board), turnBefore0 = boardState.turn;
    const result0 = applyMoveActionCore(boardState, action, aiColor);
    if (result0.ok) noteMetalRelocations(boardState.board, metalBefore0, boardState.turn !== turnBefore0 ? turnBefore0 : null);
    if (result0.ok && memory0 && ["white", "black"].includes(color0)) {
      boardState.parrotMovement ||= { white: null, black: null };
      boardState.parrotMovement[color0] = memory0;
    }
    return result0;
  }
  function applyMoveActionCore(boardState, action, aiColor) {
    const color = action.color || boardState.turn;
    const from = action.from || {};
    const selectedMove = action.move || {};
    const piece = get(boardState, from.row, from.col);
    if (!piece || piece.color !== color && piece.type !== "football") return { ok: false, score: 0 };
    if (selectedMove.enPassant && !canWorkerCaptureTarget(
      color,
      get(boardState, selectedMove.capturedRow, selectedMove.capturedCol),
      piece,
      boardState
    )) {
      return { ok: false, score: 0 };
    }
    if (workerIsLargePiece(piece) && !selectedMove.colossusAttack && !selectedMove.colossusBody && colossusCells(selectedMove.anchorRow ?? selectedMove.row, selectedMove.anchorCol ?? selectedMove.col).some((cell) => {
      const target2 = get(boardState, cell.row, cell.col);
      return target2 && target2 !== piece && (!piece.id || target2.id !== piece.id) && (target2.protected || isWorkerEncouragedTarget(boardState, target2));
    })) return { ok: false, score: 0 };
    const footballTarget = piece.type === "football" ? get(boardState, selectedMove.row, selectedMove.col) : null;
    if (footballTarget && (footballTarget.protected || isWorkerEncouragedTarget(boardState, footballTarget))) return { ok: false, score: 0 };
    const movedAbilityType = pieceAbilityType(piece);
    if (isFrozenPiece(piece) || isPoisonStunned(piece) || isExhaustionMoveBlocked(boardState.exhaustion, piece)) return { ok: false, score: 0 };
    if (!isWorkerMoveAllowed(boardState, piece, from.row, from.col, selectedMove)) return { ok: false, score: 0 };
    const idolEncoreSource = workerFirstAvailableFriendlyIdolAuraSource(boardState, piece, from.row, from.col);
    if (idolEncoreSource && !(boardState.idolEncorePending?.color === color && boardState.idolEncorePending.idolId)) {
      boardState.idolEncorePending = {
        color,
        idolId: idolEncoreSource.id,
        pieceId: piece.id || ""
      };
    }
    const move = { ...selectedMove };
    const portalEntry = (move.portalLanding || move.portalThrough) && Number.isInteger(move.portalEntry?.row) && Number.isInteger(move.portalEntry?.col) ? { ...move.portalEntry } : null;
    const portalExit = (move.portalLanding || move.portalThrough) && Number.isInteger(move.portalExit?.row) && Number.isInteger(move.portalExit?.col) ? { ...move.portalExit } : null;
    if (portalEntry && portalExit) {
      const expectedExit = workerPortalExitAt(boardState, portalEntry.row, portalEntry.col);
      if (!expectedExit || expectedExit.row !== portalExit.row || expectedExit.col !== portalExit.col) return { ok: false, score: 0 };
      if ([portalEntry, portalExit].some((cell) => isWorkerCollapsedSquare(boardState, cell.row, cell.col)) || move.portalThrough && [portalEntry, portalExit].some((cell) => {
        const blocker = get(boardState, cell.row, cell.col);
        return Boolean(blocker && !isGhostTransparentFor(piece, blocker, renderType(piece), boardState));
      })) return { ok: false, score: 0 };
      if (move.portalLanding) {
        move.row = portalExit.row;
        move.col = portalExit.col;
      }
    }
    const initialTarget = get(boardState, move.row, move.col);
    const occupiedPortalEntryTarget = move.portalLanding && portalEntry ? get(boardState, portalEntry.row, portalEntry.col) : null;
    const portalEntryTarget = move.portalTransparentEntry && isGhostTransparentFor(piece, occupiedPortalEntryTarget, renderType(piece), boardState) ? null : occupiedPortalEntryTarget;
    const madHorsePortalEntryCapture = Boolean(
      move.madHorsePortalEntryCapture && portalEntry && isWorkerMadHorseFriendlyTarget(boardState, piece, portalEntryTarget)
    );
    const madHorsePortalExitCapture = Boolean(
      (move.madHorsePortalExitCapture || !move.portalLanding && move.madHorseCapture) && isWorkerMadHorseFriendlyTarget(boardState, piece, initialTarget)
    );
    const madHorseFriendlyCapture = madHorsePortalEntryCapture || madHorsePortalExitCapture;
    if (move.substitutionSwap) return applyWorkerSubstitutionMoveAction(boardState, action, aiColor, piece, initialTarget, color);
    if (move.relaySwap) {
      const target2 = initialTarget;
      if (!boardState.relay?.[color] || !target2 || target2 === piece || target2.color !== color || from.row !== move.row && from.col !== move.col || [piece.type, target2.type].some((type) => ["colossus", "bigRook", "bigBishop"].includes(type))) return { ok: false, score: 0 };
      set(boardState, from.row, from.col, target2);
      set(boardState, move.row, move.col, piece);
      piece.moved = true;
      target2.moved = true;
      disassembleMovedQueen(boardState, piece, from, move, piece.type, internalWorkerCallbacks(boardState));
      noteThiefMove(piece, from, move, boardState, [portalEntry, portalExit]);
      if (usesThiefRemake(boardState)) noteThiefMove(target2, move, from, boardState);
      if (piece.type === "trickster") {
        piece.tricksterPreviousAbilityForTurn = movedAbilityType;
        rerollWorkerTricksterAbility(boardState, piece, `${piece.id || "trickster"}:${Number(boardState.moveCount) || 0}:${move.row}:${move.col}:relay`);
      }
      if (boardState.monochromeChess) {
        piece.monoShade = squareShade(move.row, move.col);
        target2.monoShade = squareShade(from.row, from.col);
      }
      trackWorkerMovingProgress(boardState, piece);
      noteWorkerUltimatumMovement(boardState, piece);
      noteWorkerUltimatumMovement(boardState, target2);
      boardState.enPassant = null;
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: color === aiColor ? 90 : -90 };
    }
    if (move.siegeRamMove) {
      const path = Array.isArray(move.highlightCells) ? uniqueCells(move.highlightCells.filter((cell) => inBounds(cell?.row, cell?.col, boardState))) : [];
      const landing = workerPortalMoveDestination(move) || move;
      if (!canResolveSiegeRamMove(piece, move) || path.length < 1 || path.length > 3 || path.some((cell) => !inBounds(cell.row, cell.col, boardState))) return { ok: false, score: 0 };
      const entries = path.map((cell) => ({ ...cell, item: get(boardState, cell.row, cell.col) })).filter((entry) => entry.item && entry.item !== piece);
      if (entries.some(({ item, row, col }) => !canWorkerSiegeRamAffectTarget(boardState, piece, item, row, col, move))) {
        return { ok: false, score: 0 };
      }
      if (workerSiegeRamPotentialCaptureCount(entries, boardState) > workerSaturationCapturesRemaining(boardState, piece)) {
        return { ok: false, score: 0 };
      }
      let score2 = 0;
      const captured2 = [];
      const capturedEntries2 = [];
      const seen = /* @__PURE__ */ new Set();
      path.forEach((cell) => {
        const item = get(boardState, cell.row, cell.col);
        if (!item || item === piece || seen.has(item.id || item)) return;
        if (legacyAttackRulesStates.has(boardState) && !isWorkerSiegeRamForcedRemovalTarget(item) && isHpPiece(item)) {
          item.hp = Math.max(0, Number(item.hp ?? item.maxHp ?? 1) - 1);
          if (item.hp > 0) return;
        }
        seen.add(item.id || item);
        captured2.push(item);
        capturedEntries2.push({ item, row: cell.row, col: cell.col });
        clearPieceCells(boardState, item);
        const favorable = item.color === opponent(color);
        score2 += (color === aiColor ? 1 : -1) * (favorable ? pieceValue(item) * 1.55 : -pieceValue(item) * 1.55);
        if (isWorkerDecisiveCaptureTarget(boardState, item)) {
          if (item.color === color) {
            boardState.mode = "gameover";
            boardState.winner = opponent(color);
            score2 += color === aiColor ? -1e5 : 1e5;
          } else {
            score2 += workerCriticalCaptureScore(boardState, item, color, aiColor);
          }
        }
      });
      recordWorkerCapturedPieces(boardState, color, captured2, piece, {
        attackerLanding: { row: landing.row, col: landing.col },
        deferRetaliatingBearReactions: true
      });
      recordWorkerDirectCaptures(boardState, piece, captured2);
      score2 += resolveWorkerReaperNearbyDeaths(boardState, capturedEntries2, aiColor, {
        activePiece: piece,
        activePieceLanding: { row: landing.row, col: landing.col },
        capturedBy: piece.color
      });
      const landingBlocked = Boolean(get(boardState, landing.row, landing.col) && get(boardState, landing.row, landing.col) !== piece);
      if (!landingBlocked) {
        set(boardState, from.row, from.col, null);
        set(boardState, landing.row, landing.col, piece);
        trackWorkerMovingProgress(boardState, piece);
        noteWorkerUltimatumMovement(boardState, piece);
      }
      piece.moved = true;
      if (!landingBlocked && move.tricksterMove && piece.type === "trickster") {
        piece.tricksterPreviousAbilityForTurn = movedAbilityType;
        rerollWorkerTricksterAbility(boardState, piece, `${piece.id || "trickster"}:${Number(boardState.moveCount) || 0}:${landing.row}:${landing.col}`);
      }
      boardState.enPassant = null;
      if (boardState.mode !== "gameover") finishWorkerMove(boardState, color);
      pushRecentMove(boardState, {
        from: { row: from.row, col: from.col },
        to: landingBlocked ? { row: from.row, col: from.col } : { row: landing.row, col: landing.col },
        pieceId: piece.id || "",
        pieceType: piece.type || "",
        color
      });
      return { ok: true, score: score2 };
    }
    if (move.castle) return applyWorkerCastleMoveAction(boardState, action, aiColor, piece, color);
    if (move.colossusMove) return applyColossusMoveAction(boardState, action, aiColor, piece, color);
    if (move.bigRookMove) return applyBigRookMoveAction(boardState, action, aiColor, piece, color);
    if (move.setLogDirection) {
      const nextRow = from.row + move.setLogDirection.dr;
      const nextCol = from.col + move.setLogDirection.dc;
      if (!inBounds(nextRow, nextCol)) return { ok: false, score: 0 };
      piece.logDir = { dr: move.setLogDirection.dr, dc: move.setLogDirection.dc };
      piece.moved = true;
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: from.row, col: from.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: color === aiColor ? 18 : -18 };
    }
    let target = initialTarget;
    if ((move.colossusAttack || move.shotgunBlast || move.shotgunSnipe) && isIndirectAttackImmunePiece(target)) target = null;
    if (move.colossusAttack && !workerColossusSectorTargetAllowed(boardState, target, piece)) target = null;
    if (move.switcherooMove) {
      if (!canWorkerResolveSwitcherooMove(boardState, piece, target)) return { ok: false, score: 0 };
      set(boardState, from.row, from.col, null);
      set(boardState, move.row, move.col, piece);
      piece.moved = true;
      trackWorkerMovingProgress(boardState, piece);
      if (boardState.switcheroo) boardState.switcheroo[color] = false;
      boardState.enPassant = null;
      noteWorkerUltimatumMovement(boardState, piece);
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: color === aiColor ? 110 : -110 };
    }
    if (move.dragonSwap) {
      if (!canWorkerResolveDragonSwap(boardState, piece, target, move) || !isWorkerMonochromeMoveAllowed(boardState, piece, from.row, from.col, move)) return { ok: false, score: 0 };
      const wasDesperadoMove2 = Boolean(piece.desperado);
      const quantumCandidates2 = workerQuantumCandidateMovesForMove(boardState, piece, from, move);
      set(boardState, from.row, from.col, target);
      set(boardState, move.row, move.col, piece);
      if (boardState.monochromeChess) {
        target.monoShade = squareShade(from.row, from.col);
        piece.monoShade = squareShade(move.row, move.col);
      }
      delete piece.quantum;
      const quantumTo = applyWorkerQuantumAfterMove(boardState, piece, move.row, move.col, quantumCandidates2);
      piece.moved = true;
      if (piece.type === "trickster") {
        piece.tricksterPreviousAbilityForTurn = "dragon";
        rerollWorkerTricksterAbility(boardState, piece, `${piece.id || "trickster"}:${boardState.moveCount || 0}:dragon-swap`);
      }
      trackWorkerMovingProgress(boardState, piece);
      target.moved = true;
      noteWorkerUltimatumMovement(boardState, piece);
      noteWorkerUltimatumMovement(boardState, target);
      let keepsTurnByDesperado2 = false;
      let score2 = color === aiColor ? 80 : -80;
      if (quantumTo) score2 += color === aiColor ? 120 : -120;
      if (wasDesperadoMove2 && boardState.mode !== "gameover" && get(boardState, move.row, move.col) === piece) {
        piece.desperado.remaining = Math.max(0, Number(piece.desperado.remaining) - 1);
        if (piece.desperado.remaining > 0) {
          keepsTurnByDesperado2 = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
        }
        if (!keepsTurnByDesperado2) {
          delete piece.desperado;
          delete piece.quantum;
          set(boardState, move.row, move.col, null);
          recordWorkerCapturedPieces(boardState, opponent(color), [piece]);
          score2 += color === aiColor ? -pieceValue(piece) * 0.45 : pieceValue(piece) * 0.45;
        }
      }
      if (keepsTurnByDesperado2 && boardState.mode !== "gameover") {
        boardState.moveCount = (Number(boardState.moveCount) || 0) + 1;
        boardState.turn = color;
        boardState.actionsRemaining = workerActionLimit(boardState);
        score2 += color === aiColor ? 420 : -420;
      } else {
        finishWorkerMove(boardState, color);
      }
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: score2 };
    }
    if (move.missionaryConvert) {
      if (!pieceHasAbility(piece, "missionary") || !target || target.color === color || !COLORS.includes(target.color) || isDesperadoRoyalCaptureBlocked(piece, target) || Math.abs(move.row - from.row) !== 1 || Math.abs(move.col - from.col) !== 1) {
        return { ok: false, score: 0 };
      }
      const formerTarget = { ...target };
      let conversionScore = (color === aiColor ? 1 : -1) * (pieceValue(target) * 1.05 + 160);
      if (isWorkerDecisiveCaptureTarget(boardState, formerTarget)) {
        conversionScore += workerCriticalCaptureScore(boardState, formerTarget, color, aiColor);
      }
      target.color = color;
      resetConvertedLargePieceHealth(target);
      target.moved = true;
      piece.moved = true;
      trackWorkerMovingProgress(boardState, piece);
      boardState.enPassant = null;
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: conversionScore };
    }
    if (target && target.color === color && !madHorsePortalExitCapture) return { ok: false, score: 0 };
    const basicTrainingCaptureOptions = { allowBasicTrainingCapture: Boolean(move.basicTrainingCapture) };
    if (portalEntryTarget && (portalEntryTarget.color === color ? !madHorsePortalEntryCapture : !canWorkerCaptureTarget(color, portalEntryTarget, piece, boardState, basicTrainingCaptureOptions))) {
      return { ok: false, score: 0 };
    }
    if (isFrozenPiece(target)) return { ok: false, score: 0 };
    const crushesStealthOnLanding = isStealthTransparentFor(piece, target, boardState) && !move.colossusAttack && !move.shotgunBlast && !move.shotgunSnipe;
    if (target && !move.merchantBuy && (target.color === color ? !madHorsePortalExitCapture : !crushesStealthOnLanding && !canWorkerCaptureTarget(color, target, piece, boardState, basicTrainingCaptureOptions))) return { ok: false, score: 0 };
    const mistakeCandidate = workerMistakeCaptureCandidate(boardState, piece, move, portalEntryTarget, target);
    if (mistakeCandidate && workerMistakeTriggers(boardState, piece, from, move, mistakeCandidate)) {
      return resolveWorkerMistakeReversal(boardState, piece, from, mistakeCandidate, color, aiColor);
    }
    let score = 0;
    const resolvedAction = { ...action, move };
    const repeatPenalty = oscillationPenalty(boardState, resolvedAction, color);
    score += color === aiColor ? -repeatPenalty : repeatPenalty;
    const ultimatumRelief = workerUltimatumMoveReliefScore(boardState, piece);
    score += color === aiColor ? ultimatumRelief : -ultimatumRelief;
    const quantumCandidates = workerQuantumCandidateMovesForMove(boardState, piece, from, move);
    const movedPieceType = piece.type;
    const internalThiefJump = pieceHasAbility(piece, "thief") && thiefJumpedPiece(from, usesThiefRemake(boardState) && portalEntry ? portalEntry : move, (row, col) => get(boardState, row, col));
    delete piece.thiefSecondMove;
    const movedAsType = renderType(piece);
    const wasFileSurgeSecondMove = Boolean(piece.fileSurgeSecondMove);
    const wasRookLiftSecondMove = Boolean(piece.rookLiftSecondMove);
    const rookLiftChainBeforeMove = piece.rookLiftChain ? clonePlain(piece.rookLiftChain) : null;
    const wasIronMonarchExtraMove = Boolean(piece.ironMonarchExtraMove);
    const wasUnderpromotionSecondMove = Boolean(piece.underpromotionSecondMove);
    const wasCheckerChainCapture = Boolean(piece.checkerChainCapture);
    const wasMadHorseSecondMove = Boolean(piece.madHorseSecondMove);
    const queuedKnightExtraMovesBeforeMove = workerQueuedKnightExtraMoveReasons(piece);
    const queuedBackwardKnightTurnBeforeMove = Boolean(piece.queuedBackwardKnightTurn);
    const wasRepositionSecondMove = Boolean(piece.repositionSecondMove);
    const wasFrenzyExtraMove = Boolean(piece.frenzyExtraMove);
    const wasPlatformExtraMove = Boolean(piece.platformExtraMove);
    const wasRepositionFirstMove = Boolean(piece.repositionSecondMove && !piece.repositionSecondMove.used);
    const wasDesperadoMove = Boolean(piece.desperado);
    const wasChargeRushPawnMove = Boolean(piece.chargeRush && movedAsType === "pawn");
    const clearPieceExtraMoveFlags = () => {
      if (wasFrenzyExtraMove || wasFileSurgeSecondMove || wasRookLiftSecondMove || wasIronMonarchExtraMove || wasUnderpromotionSecondMove || wasCheckerChainCapture || wasMadHorseSecondMove || wasPlatformExtraMove || wasRepositionSecondMove && !wasRepositionFirstMove) {
        delete piece.frenzyExtraMove;
        delete piece.thiefSecondMove;
        delete piece.fileSurgeSecondMove;
        delete piece.rookLiftSecondMove;
        delete piece.ironMonarchExtraMove;
        delete piece.underpromotionSecondMove;
        delete piece.checkerChainCapture;
        delete piece.madHorseSecondMove;
        delete piece.platformExtraMove;
        if (wasRepositionSecondMove && !wasRepositionFirstMove) delete piece.repositionSecondMove;
      }
      if (wasChargeRushPawnMove) delete piece.chargeRush;
    };
    if (portalEntryTarget?.type !== "scarecrow" && portalEntryTarget?.shielded) {
      delete portalEntryTarget.shielded;
      piece.moved = true;
      clearPieceExtraMoveFlags();
      noteWorkerUltimatumMovement(boardState, piece);
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: score + (color === aiColor ? pieceValue(portalEntryTarget) * 0.28 : -pieceValue(portalEntryTarget) * 0.28) };
    }
    if (isHpPiece(portalEntryTarget)) {
      const currentHp = portalEntryTarget.hp ?? portalEntryTarget.maxHp ?? 1;
      portalEntryTarget.hp = Math.max(0, currentHp - 1);
      if (portalEntryTarget.hp <= 0) {
        cancelWorkerPropheciesByCapture(boardState);
        recordWorkerCapturedPieces(boardState, color, [portalEntryTarget], piece);
        recordWorkerDirectCaptures(boardState, piece, [portalEntryTarget]);
        clearPieceCells(boardState, portalEntryTarget);
        if (isWorkerDecisiveCaptureTarget(boardState, portalEntryTarget)) {
          score += workerCriticalCaptureScore(boardState, portalEntryTarget, color, aiColor);
        }
        if (boardState.mode !== "gameover") {
          const previousValue = pieceValue(piece);
          if (applyWorkerTranscendence(boardState, piece, movedPieceType, from.row, from.col, `${boardState.moveCount || 0}:portal-hp`)) {
            const gain = Math.max(0, pieceValue(piece) - previousValue) * 0.8;
            score += color === aiColor ? gain : -gain;
          }
        }
      }
      piece.moved = true;
      clearPieceExtraMoveFlags();
      if (workerCanStartFrenzyExtraMove(boardState, piece, from.row, from.col, portalEntryTarget.hp <= 0)) retainWorkerTurn(boardState, color);
      else finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: score + (color === aiColor ? pieceValue(portalEntryTarget) * 0.55 : -pieceValue(portalEntryTarget) * 0.55) };
    }
    if (target?.type !== "scarecrow" && target?.shielded && !move.merchantBuy && !move.colossusAttack && !move.shotgunBlast && !move.shotgunSnipe) {
      delete target.shielded;
      piece.moved = true;
      clearPieceExtraMoveFlags();
      noteWorkerUltimatumMovement(boardState, piece);
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score: score + (color === aiColor ? pieceValue(target) * 0.28 : -pieceValue(target) * 0.28) };
    }
    const areaShieldTargets = /* @__PURE__ */ new Set();
    const knownDirectCaptures = [
      portalEntryTarget,
      target,
      Number.isInteger(move.capturedRow) && Number.isInteger(move.capturedCol) ? get(boardState, move.capturedRow, move.capturedCol) : null,
      move.jumpCapture ? get(boardState, move.jumpCapture.row, move.jumpCapture.col) : null
    ].filter((candidate, index, entries) => candidate && !areaShieldTargets.has(candidate) && entries.indexOf(candidate) === index && (candidate.color === color ? madHorsePortalEntryCapture && candidate === portalEntryTarget || madHorsePortalExitCapture && candidate === target : canWorkerCaptureTarget(color, candidate, piece, boardState, basicTrainingCaptureOptions)));
    if (knownDirectCaptures.length > workerSaturationCapturesRemaining(boardState, piece)) return { ok: false, score: 0 };
    let evasionTriggered = false;
    let resolvedPortalEntryTarget = portalEntryTarget;
    if (resolvedPortalEntryTarget && workerTryEvadeCapture(boardState, resolvedPortalEntryTarget, portalEntry.row, portalEntry.col, color, {
      attacker: piece,
      attackerLanding: { row: move.row, col: move.col }
    })) {
      evasionTriggered = true;
      resolvedPortalEntryTarget = null;
    } else if (resolvedPortalEntryTarget) {
      set(boardState, portalEntry.row, portalEntry.col, null);
    }
    if (target && !areaShieldTargets.has(target) && !isHpPiece(target) && workerTryEvadeCapture(boardState, target, move.row, move.col, color, {
      attacker: piece,
      attackerLanding: { row: move.row, col: move.col }
    })) {
      evasionTriggered = true;
      target = null;
    }
    const captured = [];
    const capturedEntries = [];
    const saturationAllowance = workerSaturationCapturesRemaining(boardState, piece);
    const rememberCaptured = (item, row, col) => {
      if (!item || captured.length >= saturationAllowance) return false;
      captured.push(item);
      capturedEntries.push({ item, row, col });
      return true;
    };
    if (resolvedPortalEntryTarget) rememberCaptured(resolvedPortalEntryTarget, portalEntry.row, portalEntry.col);
    if (target && !areaShieldTargets.has(target)) rememberCaptured(target, move.row, move.col);
    let checkerJumpedAttack = false;
    if (Number.isInteger(move.capturedRow) && Number.isInteger(move.capturedCol)) {
      const extra = get(boardState, move.capturedRow, move.capturedCol);
      if (canWorkerCaptureTarget(color, extra, piece, boardState)) {
        if (!workerTryEvadeCapture(boardState, extra, move.capturedRow, move.capturedCol, color, {
          attacker: piece,
          attackerLanding: { row: move.row, col: move.col }
        })) {
          if (rememberCaptured(extra, move.capturedRow, move.capturedCol)) {
            set(boardState, move.capturedRow, move.capturedCol, null);
          }
        } else {
          evasionTriggered = true;
        }
      }
    }
    if (move.jumpCapture) {
      const extra = get(boardState, move.jumpCapture.row, move.jumpCapture.col);
      const canJumpCapture = move.checkerCapture ? canWorkerCaptureTarget(color, extra, piece, boardState) : canWorkerRadicalChargeCaptureTarget(boardState, color, extra, piece);
      if (canJumpCapture) {
        if (extra.type !== "scarecrow" && extra.shielded) {
          delete extra.shielded;
          if (move.checkerCapture) checkerJumpedAttack = true;
          score += color === aiColor ? pieceValue(extra) * 0.2 : -pieceValue(extra) * 0.2;
        } else if (isHpPiece(extra)) {
          const jumpHitCount = move.checkerCapture ? 1 : Math.max(1, Math.min(2, Number(move.jumpCaptureHits) || 1));
          const currentHp = extra.hp ?? extra.maxHp ?? 1;
          const appliedDamage = Math.min(currentHp, jumpHitCount);
          if (currentHp <= appliedDamage && workerTryEvadeCapture(boardState, extra, move.jumpCapture.row, move.jumpCapture.col, color, {
            attacker: piece,
            attackerLanding: { row: move.row, col: move.col }
          })) {
            evasionTriggered = true;
          } else {
            extra.hp = Math.max(0, currentHp - appliedDamage);
            const immediatelyScoredHits = extra.hp <= 0 ? Math.max(0, appliedDamage - 1) : appliedDamage;
            score += (color === aiColor ? 1 : -1) * pieceValue(extra) * 0.55 * immediatelyScoredHits;
            if (extra.hp <= 0 && rememberCaptured(extra, move.jumpCapture.row, move.jumpCapture.col)) {
              if (move.checkerCapture) checkerJumpedAttack = true;
              clearPieceCells(boardState, extra);
            }
          }
        } else {
          if (!workerTryEvadeCapture(boardState, extra, move.jumpCapture.row, move.jumpCapture.col, color, {
            attacker: piece,
            attackerLanding: { row: move.row, col: move.col }
          })) {
            if (rememberCaptured(extra, move.jumpCapture.row, move.jumpCapture.col)) {
              if (move.checkerCapture) checkerJumpedAttack = true;
              set(boardState, move.jumpCapture.row, move.jumpCapture.col, null);
            }
          } else {
            evasionTriggered = true;
          }
        }
      }
    }
    if (Array.isArray(move.sectorCells)) {
      move.sectorCells.forEach((cell) => {
        const extra = get(boardState, cell.row, cell.col);
        if (move.colossusAttack && !workerColossusSectorTargetAllowed(boardState, extra, piece)) return;
        if (extra !== target && !areaShieldTargets.has(extra) && !isIndirectAttackImmunePiece(extra) && canWorkerCaptureTarget(color, extra, piece, boardState)) {
          if (!workerTryEvadeCapture(boardState, extra, cell.row, cell.col, color, { attacker: piece })) {
            if (rememberCaptured(extra, cell.row, cell.col)) {
              set(boardState, cell.row, cell.col, null);
            }
          } else {
            evasionTriggered = true;
          }
        }
      });
    }
    const preparedParries = prepareWorkerParryRetaliations(boardState, piece, capturedEntries, { skip: Boolean(move.merchantBuy) });
    capturedEntries.splice(0, capturedEntries.length, ...preparedParries.ordinaryEntries);
    captured.splice(0, captured.length, ...capturedEntries.map(({ item }) => item));
    const retaliationEntries = preparedParries.retaliationEntries;
    if (captured.some((item) => item?.poisonedPawn && item.color !== color)) {
      piece.poisonStunTurns = Math.max(Number(piece.poisonStunTurns) || 0, 3);
      piece.poisonStunColor = color;
    }
    areaShieldTargets.forEach((shielded) => {
      delete shielded.shielded;
      score += color === aiColor ? pieceValue(shielded) * 0.28 : -pieceValue(shielded) * 0.28;
    });
    if ((move.colossusAttack || move.shotgunBlast) && captured.length === 0 && areaShieldTargets.size === 0 && preparedParries.parryCount === 0 && !evasionTriggered) return { ok: false, score: 0 };
    if (move.merchantBuy && target && target.color !== color) {
      if (isFrozenPiece(target)) return { ok: false, score: 0 };
      const cost = merchantCost(target);
      if (!Number.isFinite(cost) || (piece.gold ?? 0) < cost) return { ok: false, score: 0 };
      piece.gold -= cost;
      const purchasedColor = target.color;
      if (boardState.democracy?.[purchasedColor] && isRoyalPiece(target)) {
        if (!boardState.kingDead) boardState.kingDead = {};
        boardState.kingDead[purchasedColor] = true;
      }
      target.color = color;
      target.moved = true;
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 1.25;
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score };
    }
    if (move.colossusAttack || move.shotgunBlast || move.shotgunSnipe) {
      if (captured.length === 0 && areaShieldTargets.size === 0 && !evasionTriggered) return { ok: false, score: 0 };
      const attackCells = move.sectorCells || [{ row: move.row, col: move.col }];
      const permittedAttackTargets = new Set(captured);
      const explosiveAttackEntries = [];
      attackCells.forEach((cell) => {
        const item = get(boardState, cell.row, cell.col);
        if (!permittedAttackTargets.has(item) || isIndirectAttackImmunePiece(item) || !canWorkerCaptureTarget(color, item, piece, boardState)) return;
        score += (color === aiColor ? 1 : -1) * pieceValue(item) * (isHpPiece(item) ? 0.55 : 1.5);
        if (isHpPiece(item)) {
          const currentHp = item.hp ?? item.maxHp ?? 1;
          if (currentHp <= 1 && workerTryEvadeCapture(boardState, item, cell.row, cell.col, color, { attacker: piece })) {
            score -= (color === aiColor ? 1 : -1) * pieceValue(item) * 0.55;
            return;
          }
          item.hp = Math.max(0, currentHp - 1);
          if (item.hp <= 0) {
            if (item.explosive) explosiveAttackEntries.push({ item, row: cell.row, col: cell.col });
            cancelWorkerPropheciesByCapture(boardState);
            recordWorkerCapturedPieces(boardState, color, [item], piece);
            clearPieceCells(boardState, item);
          }
        } else {
          if (item.explosive) explosiveAttackEntries.push({ item, row: cell.row, col: cell.col });
          cancelWorkerPropheciesByCapture(boardState);
          recordWorkerCapturedPieces(boardState, color, [item], piece);
          set(boardState, cell.row, cell.col, null);
        }
        if (isWorkerDecisiveCaptureTarget(boardState, item) && (!isHpPiece(item) || item.hp <= 0)) {
          score += workerCriticalCaptureScore(boardState, item, color, aiColor);
        }
      });
      const retaliationSurvivorIds = new Set(retaliationEntries.filter(({ item, parryTriggered }) => parryTriggered || Boolean(septemberCounterLimit(pieceAbilityType(item))) && (Number(item.bearRetaliationsRemaining) || 0) > 0).map(({ item }) => item?.id).filter(Boolean));
      const defeatedAttackEntries = capturedEntries.filter(({ item }) => !retaliationSurvivorIds.has(item?.id) && !workerPieceRemainsOnBoard(boardState, item));
      score += resolveWorkerReaperNearbyDeaths(
        boardState,
        defeatedAttackEntries,
        aiColor,
        { activePiece: piece, capturedBy: piece.color }
      );
      const activeRoyalReaperExecution = consumeWorkerActiveRoyalReaperExecution(boardState, piece);
      if (activeRoyalReaperExecution) {
        pushRecentMove(boardState, {
          from: { row: from.row, col: from.col },
          to: { row: activeRoyalReaperExecution.row, col: activeRoyalReaperExecution.col },
          pieceId: piece.id || "",
          pieceType: piece.type || "",
          color
        });
        return { ok: true, score };
      }
      recordWorkerDirectCaptures(boardState, piece, captured.filter((item) => !workerPieceRemainsOnBoard(boardState, item)));
      if (explosiveAttackEntries.length) {
        score += applyWorkerExplosionsForEntries(boardState, explosiveAttackEntries, aiColor, new Set(explosiveAttackEntries.map(({ item }) => item)));
      }
      const bearResult2 = resolveWorkerBearRetaliation(boardState, piece, retaliationEntries, aiColor, from);
      score += bearResult2.score;
      const trojanResult2 = bearResult2.attackerRemoved ? { score: 0 } : resolveWorkerTrojanHorseRetaliation(boardState, piece, capturedEntries, aiColor);
      score += trojanResult2.score;
      if (piece.type === "shotgunKing") piece.ammo = Math.max(0, (piece.ammo ?? 0) - (move.shotgunSnipe ? SHOTGUN_SNIPE_AMMO_COST : SHOTGUN_BLAST_AMMO_COST));
      piece.moved = true;
      noteWorkerUltimatumMovement(boardState, piece);
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
      return { ok: true, score };
    }
    if (target && isHpPiece(target)) {
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 0.55;
      const currentHp = target.hp ?? target.maxHp ?? 1;
      let reaperExecution2 = null;
      if (currentHp <= 1 && workerTryEvadeCapture(boardState, target, move.row, move.col, color, { attacker: piece })) {
        piece.moved = true;
        clearPieceExtraMoveFlags();
        finishWorkerMove(boardState, color);
        pushRecentMove(boardState, { from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col }, pieceId: piece.id || "", pieceType: piece.type || "", color });
        const avoidedCaptureValue = pieceValue(target) * 0.55 + 80;
        return { ok: true, score: score + (color === aiColor ? -avoidedCaptureValue : avoidedCaptureValue) };
      }
      target.hp = Math.max(0, currentHp - 1);
      if (target.hp <= 0) {
        cancelWorkerPropheciesByCapture(boardState);
        workerLearnImperialStudyMovement(boardState, piece, [target]);
        recordWorkerCapturedPieces(boardState, color, [target], piece);
        recordWorkerDirectCaptures(boardState, piece, [target]);
        clearPieceCells(boardState, target);
        if (isWorkerDecisiveCaptureTarget(boardState, target)) {
          score += workerCriticalCaptureScore(boardState, target, color, aiColor);
        }
        score += resolveWorkerReaperNearbyDeaths(
          boardState,
          [{ item: target, row: move.row, col: move.col }],
          aiColor,
          { activePiece: piece, activePieceLanding: { row: move.row, col: move.col }, capturedBy: piece.color }
        );
        const activeRoyalReaperExecution = consumeWorkerActiveRoyalReaperExecution(boardState, piece);
        if (activeRoyalReaperExecution) {
          pushRecentMove(boardState, {
            from: { row: from.row, col: from.col },
            to: { row: activeRoyalReaperExecution.row, col: activeRoyalReaperExecution.col },
            pieceId: piece.id || "",
            pieceType: piece.type || "",
            color
          });
          return { ok: true, score };
        }
        reaperExecution2 = consumeWorkerReaperExecution(boardState, piece);
        if (boardState.mode !== "gameover") {
          const previousValue = pieceValue(piece);
          if (applyWorkerTranscendence(boardState, piece, movedPieceType, from.row, from.col, `${boardState.moveCount || 0}:hp`)) {
            const gain = Math.max(0, pieceValue(piece) - previousValue) * 0.8;
            score += color === aiColor ? gain : -gain;
          }
        }
      }
      piece.moved = true;
      clearPieceExtraMoveFlags();
      if (workerCanStartFrenzyExtraMove(boardState, piece, from.row, from.col, target.hp <= 0)) retainWorkerTurn(boardState, color);
      else finishWorkerMove(boardState, color);
      pushRecentMove(boardState, {
        from: { row: from.row, col: from.col },
        to: reaperExecution2 || { row: move.row, col: move.col },
        pieceId: piece.id || "",
        pieceType: piece.type || "",
        color
      });
      return { ok: true, score };
    }
    captured.forEach((item) => {
      const hpDamage = isHpPiece(item) ? 0.55 : 1.8;
      const captureValue = pieceValue(item) * hpDamage;
      const favorable = item.color === opponent(color);
      score += (color === aiColor ? 1 : -1) * (favorable ? captureValue : -captureValue);
      if (isWorkerDecisiveCaptureTarget(boardState, item)) {
        if (!isHpPiece(item) || (Number(item.hp) || 1) <= 1) {
          score += workerCriticalCaptureScore(boardState, item, color, aiColor);
        }
      }
    });
    if (captured.length) cancelWorkerPropheciesByCapture(boardState);
    workerLearnImperialStudyMovement(boardState, piece, captured);
    recordWorkerCapturedPieces(boardState, color, captured, piece, { attackerLanding: { row: move.row, col: move.col } });
    recordWorkerDirectCaptures(boardState, piece, captured);
    score += resolveWorkerReaperNearbyDeaths(
      boardState,
      capturedEntries,
      aiColor,
      { activePiece: piece, activePieceLanding: { row: move.row, col: move.col }, capturedBy: piece.color }
    );
    if (piece.pendingReaperDefeat) {
      capturedEntries.forEach(({ item }) => clearPieceCells(boardState, item));
      const activeRoyalReaperExecution = consumeWorkerActiveRoyalReaperExecution(boardState, piece);
      if (activeRoyalReaperExecution) {
        pushRecentMove(boardState, {
          from: { row: from.row, col: from.col },
          to: { row: activeRoyalReaperExecution.row, col: activeRoyalReaperExecution.col },
          pieceId: piece.id || "",
          pieceType: piece.type || "",
          color
        });
        return { ok: true, score };
      }
    }
    if (piece.reaperExecutionTarget) {
      capturedEntries.forEach(({ item }) => clearPieceCells(boardState, item));
    }
    const reaperExecution = consumeWorkerReaperExecution(boardState, piece);
    if (reaperExecution) {
      move.row = reaperExecution.row;
      move.col = reaperExecution.col;
    }
    const ironMonarchDirectCapture = [target, portalEntryTarget].find((candidate) => candidate && captured.includes(candidate)) || null;
    const couldKeepTurnByIronMonarch = isIronMonarchExtraMoveCapture(boardState.ironMonarch, piece, ironMonarchDirectCapture);
    if (piece.type === "vampireLord" && captured.some((item) => item.color === opponent(color)) && grantWorkerBloodCard(boardState, color)) {
      score += color === aiColor ? 260 : -260;
    }
    const explosiveCaptureEntries = capturedEntries.filter(({ item }) => item?.explosive);
    const keepsTurnByBackwardKnight = queuedBackwardKnightTurnBeforeMove || shouldRetainBackwardKnightTurn(
      boardState.backwardKnight,
      piece.color,
      movedAsType,
      from.row,
      move.row,
      captured.length > 0
    );
    const couldKeepTurnByFileSurge = !wasFileSurgeSecondMove && movedAsType === "knight" && boardState.fileSurge?.[piece.color] && workerIsFileSurgeFile(boardState, move.col);
    const rookLiftTurns = wasRookLiftSecondMove && rookLiftChainBeforeMove ? Math.max(0, Number(rookLiftChainBeforeMove.turns) || 0) : 0;
    const couldKeepTurnByRookLift = movedAsType === "rook" && boardState.rookLift?.[piece.color] && captured.length === 0 && workerIsCornerSquare(boardState, move.row, move.col) && rookLiftTurns < 3;
    set(boardState, from.row, from.col, null);
    piece.moved = true;
    if (isWorkerZugzwangTargetKing(boardState, piece) && boardState.zugzwang?.[piece.color]) boardState.zugzwang[piece.color] = false;
    delete piece.londonSystemPawn;
    trackWorkerMovingProgress(boardState, piece);
    clearPieceExtraMoveFlags();
    noteWorkerUltimatumMovement(boardState, piece);
    if (usesInternalSixFixes(boardState) && wasFileSurgeSecondMove && piece.twinBondId) piece.twinSwapPending = 1;
    delete piece.quantum;
    const squireCapturePromotion = captured.length > 0 && movedAbilityType === "squire" && piece.type !== "trickster" && isPromotionRow(piece, move.row, boardState);
    if (captured.length > 0 && movedAbilityType === "squire" && !squireCapturePromotion) {
      piece.type = monochromePieceType("knight", boardState.monochromeChess);
      delete piece.tricksterMoveType;
      delete piece.tricksterPreviousAbilityForTurn;
    }
    if (usesInternalSixFixes(boardState)) delete piece.promotionRushUntil;
    const chameleonVictim = usesInternalSixFixes(boardState) ? captured.find((victim) => !isWorkerRoyalIdentityPiece(boardState, victim) && !["wall", "colossus", "bigRook", "bigBishop"].includes(victim.type)) : target && captured.includes(target) ? target : null;
    const chameleonTransformed = Boolean(piece.chameleon && chameleonVictim && !isWorkerRoyalIdentityPiece(boardState, chameleonVictim) && !["wall", "colossus", "bigRook", "bigBishop"].includes(chameleonVictim.type));
    if (chameleonTransformed) {
      if (isWorkerNativeKing(piece) || piece.editorRoyal) piece.crownRoyal = true;
      piece.type = monochromePieceType(chameleonVictim.type, boardState.monochromeChess);
    }
    if (wasDesperadoMove && ["pawn", "squire", "standardBearer"].includes(piece.type) && isPromotionRow(piece, move.row, boardState)) {
      piece.noPromotion = true;
    }
    let promotedByField = false;
    if ((boardState.mode !== "gameover" || legacyAttackRulesStates.has(boardState)) && movedAsType === "pawn" && piece.type === "pawn" && boardState.fieldPromotion?.[piece.color] && !piece.specialPromotionUsed && Math.max(0, Number(piece.totalCaptures) || 0) >= 2) {
      if (!boardState.recycling) {
        const promotionType = SPECIAL_PROMOTION_TYPES.map((type) => monochromePieceType(type, boardState.monochromeChess)).sort((a, b) => pieceValue({ type: b }) - pieceValue({ type: a }))[0] || "rook";
        piece.type = promotionType;
        piece.specialPromotionUsed = true;
        piece.promotedFromPawn = true;
        delete piece.noPromotion;
        delete piece.holdoutPromotion;
        clearWorkerPromotionTraits(piece, boardState);
        promotedByField = true;
        score += color === aiColor ? 520 : -520;
      }
    }
    let promotedByMove = promotedByField;
    if ((boardState.mode !== "gameover" || legacyAttackRulesStates.has(boardState)) && ["pawn", "squire", "standardBearer"].includes(piece.type) && isPromotionRow(piece, move.row, boardState)) {
      if (canMutationPromote(boardState, piece, move.row, boardRowCount(boardState))) {
        piece.type = "monster";
        promotedByMove = true;
      } else if (boardState.recycling) {
        const recycledType = workerBestRecyclingPromotionType(boardState, piece.color);
        if (recycledType && consumeWorkerRecycledPromotionPiece(boardState, piece.color, recycledType)) {
          piece.type = recycledType;
          if (recycledType === "pawn") piece.noPromotion = true;
          else delete piece.noPromotion;
          promotedByMove = true;
        } else {
          piece.noPromotion = true;
        }
      } else {
        piece.type = workerAutomaticPromotionType(boardState, piece, move.row);
        promotedByMove = true;
      }
      if (promotedByMove) {
        delete piece.holdoutPromotion;
        clearWorkerPromotionTraits(piece, boardState);
        if (piece.type === "monster") Object.assign(piece, { alliedMonster: true, blackMagicMonster: true, blackMagicOwner: piece.color });
        if (piece.type !== "pawn") piece.promotedFromPawn = true;
      }
      if (promotedByMove && boardState.coronation?.[piece.color]) {
        const previousProtected = Boolean(piece.protected);
        piece.protected = true;
        piece.coronationProtection = { remaining: 1, previousProtected };
      }
      if (promotedByMove) resolveWorkerSpyPromotion(piece);
      score += color === aiColor ? promotedByMove ? 650 : -180 : promotedByMove ? -650 : 180;
    }
    set(boardState, move.row, move.col, piece);
    if (piece.locustOrigin) piece.locustUsed = true;
    noteThiefMove(piece, from, move, boardState, [portalEntry, portalExit]);
    disassembleMovedQueen(boardState, piece, from, move, movedAsType, internalWorkerCallbacks(boardState));
    septemberConsumeResolveMove(boardState, color, movedAsType);
    if (movedAbilityType === "slime" && move.slimeMove && !get(boardState, from.row, from.col)) {
      const spawned = workerCreatePiece(color, "slime", boardState, from.row, from.col);
      spawned.id = `${color}-slime-spawn-${Number(boardState.moveCount) || 0}-${from.row}-${from.col}`;
      set(boardState, from.row, from.col, spawned);
    }
    if (!campaignAuthorityV1States.has(boardState)) workerCrownCheckerIfNeeded(boardState, piece, move.row);
    if (!legacyWindmillMovementStates.has(boardState) && movedAbilityType === "windmill") {
      piece.windmillMode = piece.windmillMode === "rook" ? "bishop" : "rook";
    }
    if (movedPieceType === "trickster" && piece.type === "trickster") {
      piece.tricksterPreviousAbilityForTurn = movedAbilityType;
      rerollWorkerTricksterAbility(boardState, piece, `${piece.id || "trickster"}:${Number(boardState.moveCount) || 0}:${move.row}:${move.col}`);
    }
    if (boardState.crownRule) workerReconcileCrownRule(boardState);
    const bearResult = resolveWorkerBearRetaliation(boardState, piece, retaliationEntries, aiColor, from);
    score += bearResult.score;
    const trojanResult = bearResult.attackerRemoved ? { attackerRemoved: false, score: 0 } : resolveWorkerTrojanHorseRetaliation(boardState, piece, capturedEntries, aiColor);
    score += trojanResult.score;
    let attackerRemovedByRetaliation = bearResult.attackerRemoved;
    if (trojanResult.attackerRemoved) attackerRemovedByRetaliation = true;
    if (attackerRemovedByRetaliation) {
      if (keepsTurnByBackwardKnight && boardState.mode !== "gameover") {
        retainWorkerTurn(boardState, color);
        score += color === aiColor ? 420 : -420;
      } else {
        finishWorkerMove(boardState, color);
      }
      pushRecentMove(boardState, {
        from: { row: from.row, col: from.col },
        to: { row: move.row, col: move.col },
        pieceId: piece.id || "",
        pieceType: piece.type || "",
        color
      });
      return { ok: true, score };
    }
    if (explosiveCaptureEntries.length) {
      score += applyWorkerExplosionsForEntries(boardState, explosiveCaptureEntries, aiColor, new Set(captured));
      if (get(boardState, move.row, move.col) !== piece) {
        if (keepsTurnByBackwardKnight && boardState.mode !== "gameover") {
          retainWorkerTurn(boardState, color);
          score += color === aiColor ? 420 : -420;
        } else {
          finishWorkerMove(boardState, color);
        }
        pushRecentMove(boardState, {
          from: { row: from.row, col: from.col },
          to: { row: move.row, col: move.col },
          pieceId: piece.id || "",
          pieceType: piece.type || "",
          color
        });
        return { ok: true, score };
      }
    }
    if (wasChargeRushPawnMove && piece.type === "pawn" && !promotedByMove) {
      set(boardState, move.row, move.col, null);
      recordWorkerCapturedPieces(boardState, opponent(color), [piece]);
      cancelWorkerPropheciesByCapture(boardState);
      score += color === aiColor ? -pieceValue(piece) * 0.55 : pieceValue(piece) * 0.55;
      finishWorkerMove(boardState, color);
      pushRecentMove(boardState, {
        from: { row: from.row, col: from.col },
        to: { row: move.row, col: move.col },
        pieceId: piece.id || "",
        pieceType: piece.type || "",
        color
      });
      return { ok: true, score };
    }
    if (boardState.afterimageQueen?.[color] && movedAsType === "queen" && isWorkerQueenIdentity(piece, color) && isWorkerQueenIdentity(target, opponent(color))) {
      const afterimage = workerCreatePiece(color, "queen", boardState, from.row, from.col);
      afterimage.moved = true;
      set(boardState, from.row, from.col, afterimage);
    }
    if (campaignAuthorityV1States.has(boardState)) workerCrownCheckerIfNeeded(boardState, piece, move.row);
    if (movedAbilityType === "recruiter") {
      set(boardState, from.row, from.col, workerCreatePiece(color, "pawn", boardState, from.row, from.col));
    }
    let transcendedByCapture = "";
    if (captured.length && !promotedByMove && get(boardState, move.row, move.col) === piece) {
      const previousValue = pieceValue(piece);
      transcendedByCapture = applyWorkerTranscendence(
        boardState,
        piece,
        movedPieceType,
        move.row,
        move.col,
        `${boardState.moveCount || 0}:${from.row}:${from.col}:${move.row}:${move.col}`
      );
      if (transcendedByCapture) {
        const gain = Math.max(0, pieceValue(piece) - previousValue) * 0.8;
        score += color === aiColor ? gain : -gain;
      }
    }
    if (piece.chimera && !promotedByMove && !transcendedByCapture) workerTransformChimeraAfterMove(boardState, piece, move.row, move.col);
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
    if (boardState.mode !== "gameover" && workerRacingKingWins(boardState, piece, move.row)) {
      resolveWorkerRacingKingVictory(boardState, color);
      if (boardState.winner) score += boardState.winner === aiColor ? 1e5 : -1e5;
    }
    updateWorkerEnPassantAfterMove(boardState, piece, from, move, movedAsType);
    if (applyWorkerQuantumAfterMove(boardState, piece, move.row, move.col, quantumCandidates)) {
      score += color === aiColor ? 120 : -120;
    }
    let keepsTurnByReposition = false;
    if (wasRepositionFirstMove && boardState.mode !== "gameover") {
      clearWorkerRepositionMarks(boardState, color);
      piece.repositionSecondMove = { used: true };
      keepsTurnByReposition = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      if (!keepsTurnByReposition) delete piece.repositionSecondMove;
    }
    let keepsTurnByFileSurge = false;
    let keepsTurnByMadHorse = false;
    const knightExtraMoveReasons = [];
    const addKnightExtraMoveReason = (reason) => {
      if (["mad-horse", "file-surge"].includes(reason) && !knightExtraMoveReasons.includes(reason)) {
        knightExtraMoveReasons.push(reason);
      }
    };
    if (madHorseFriendlyCapture && captured.some((item) => item.color === color) && movedAsType === "knight") addKnightExtraMoveReason("mad-horse");
    for (const reason of queuedKnightExtraMovesBeforeMove) addKnightExtraMoveReason(reason);
    if (couldKeepTurnByFileSurge) addKnightExtraMoveReason("file-surge");
    delete piece.queuedKnightExtraMoveReasons;
    while (knightExtraMoveReasons.length && boardState.mode !== "gameover") {
      const reason = knightExtraMoveReasons.shift();
      if (reason === "mad-horse") piece.madHorseSecondMove = true;
      if (reason === "file-surge") piece.fileSurgeSecondMove = true;
      const hasMove = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      if (hasMove) {
        keepsTurnByMadHorse = reason === "mad-horse";
        keepsTurnByFileSurge = reason === "file-surge";
        if (knightExtraMoveReasons.length) piece.queuedKnightExtraMoveReasons = knightExtraMoveReasons;
        break;
      }
      delete piece.madHorseSecondMove;
      delete piece.fileSurgeSecondMove;
    }
    let keepsTurnByRookLift = false;
    if (couldKeepTurnByRookLift && boardState.mode !== "gameover") {
      piece.rookLiftChain = {
        blockedCornerRow: wasRookLiftSecondMove && workerIsCornerSquare(boardState, from.row, from.col) ? from.row : null,
        blockedCornerCol: wasRookLiftSecondMove && workerIsCornerSquare(boardState, from.row, from.col) ? from.col : null,
        turns: rookLiftTurns + 1
      };
      piece.rookLiftSecondMove = true;
      keepsTurnByRookLift = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      if (!keepsTurnByRookLift) {
        delete piece.rookLiftSecondMove;
        delete piece.rookLiftChain;
      }
    } else if (wasRookLiftSecondMove) {
      delete piece.rookLiftChain;
    }
    let keepsTurnByIronMonarch = false;
    if (couldKeepTurnByIronMonarch && boardState.mode !== "gameover" && get(boardState, move.row, move.col) === piece) {
      piece.ironMonarchExtraMove = true;
      keepsTurnByIronMonarch = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      if (!keepsTurnByIronMonarch) delete piece.ironMonarchExtraMove;
    }
    let keepsTurnByChecker = false;
    if (!(usesInternalSixFixes(boardState) && chameleonTransformed) && checkerJumpedAttack && isWorkerCheckerType(movedAsType) && move.checkerCapture && boardState.mode !== "gameover") {
      piece.checkerChainCapture = true;
      keepsTurnByChecker = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      if (!keepsTurnByChecker) delete piece.checkerChainCapture;
    }
    let keepsTurnByFrenzy = false;
    if (captured.length > 0 && movedAsType === "pawn" && piece.type === "pawn" && piece.frenzy && boardState.mode !== "gameover" && get(boardState, move.row, move.col) === piece) {
      piece.frenzyExtraMove = true;
      keepsTurnByFrenzy = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      if (!keepsTurnByFrenzy) delete piece.frenzyExtraMove;
    }
    const keepsTurnByPlatform = workerTryStartPlatformExtraMove(boardState, piece);
    let keepsTurnByDesperado = false;
    if (wasDesperadoMove && boardState.mode !== "gameover" && get(boardState, move.row, move.col) === piece) {
      piece.desperado.remaining = Math.max(0, Number(piece.desperado.remaining) - 1);
      if (piece.desperado.remaining > 0) {
        keepsTurnByDesperado = generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
      }
      if (!keepsTurnByDesperado) {
        delete piece.desperado;
        delete piece.quantum;
        set(boardState, move.row, move.col, null);
        recordWorkerCapturedPieces(boardState, opponent(color), [piece]);
        score += color === aiColor ? -pieceValue(piece) * 0.45 : pieceValue(piece) * 0.45;
      }
    }
    if (internalThiefJump && captured.length === 0) resolveWorkerSubmergedPieces(boardState);
    const keepsTurnByThief = internalThiefJump && captured.length === 0 && get(boardState, move.row, move.col) === piece && generateMovesForPiece(boardState, piece, move.row, move.col).some((candidate) => isWorkerMoveAllowed(boardState, piece, move.row, move.col, candidate));
    if (keepsTurnByThief) piece.thiefSecondMove = true;
    const constrainedExtraMoveActive = keepsTurnByThief || keepsTurnByFileSurge || keepsTurnByRookLift || keepsTurnByIronMonarch || keepsTurnByChecker || keepsTurnByMadHorse || keepsTurnByReposition || keepsTurnByFrenzy || keepsTurnByPlatform || keepsTurnByDesperado;
    if (keepsTurnByBackwardKnight && constrainedExtraMoveActive) {
      piece.queuedBackwardKnightTurn = true;
    } else {
      delete piece.queuedBackwardKnightTurn;
    }
    if ((keepsTurnByThief || keepsTurnByBackwardKnight || keepsTurnByFileSurge || keepsTurnByRookLift || keepsTurnByIronMonarch || keepsTurnByChecker || keepsTurnByMadHorse || keepsTurnByReposition || keepsTurnByFrenzy || keepsTurnByPlatform || keepsTurnByDesperado) && boardState.mode !== "gameover") {
      retainWorkerTurn(boardState, color);
      if (!keepsTurnByMadHorse) score += color === aiColor ? 420 : -420;
    } else {
      finishWorkerMove(boardState, color);
    }
    pushRecentMove(boardState, {
      from: { row: from.row, col: from.col },
      to: { row: move.row, col: move.col },
      pieceId: piece.id || "",
      pieceType: piece.type || "",
      color
    });
    return { ok: true, score };
  }
  function workerQuantumCandidateMovesForMove(boardState, piece, from, move) {
    if (!boardState.quantumPending?.[piece?.color]) return [];
    return generateMovesForPiece(boardState, piece, from.row, from.col).filter((candidate) => {
      if (!Number.isInteger(candidate.row) || !Number.isInteger(candidate.col)) return false;
      const anchorRow = Number.isInteger(candidate.anchorRow) ? candidate.anchorRow : candidate.row;
      const anchorCol = Number.isInteger(candidate.anchorCol) ? candidate.anchorCol : candidate.col;
      const moveRow = Number.isInteger(move.anchorRow) ? move.anchorRow : move.row;
      const moveCol = Number.isInteger(move.anchorCol) ? move.anchorCol : move.col;
      if (anchorRow === moveRow && anchorCol === moveCol) return false;
      if (["colossus", "bigRook", "bigBishop"].includes(piece.type)) {
        if (!candidate.colossusMove && !candidate.bigRookMove) return false;
        return workerIsQuantumDestinationAvailable(boardState, piece, anchorRow, anchorCol);
      }
      if (candidate.castle || candidate.colossusAttack || candidate.colossusMove || candidate.bigRookMove || candidate.shotgunBlast || candidate.shotgunSnipe || candidate.merchantBuy || candidate.setLogDirection || candidate.dragonSwap || candidate.quantumFrom) {
        return false;
      }
      if (!isWorkerMoveAllowed(boardState, piece, from.row, from.col, candidate)) return false;
      return workerIsQuantumDestinationAvailable(boardState, piece, candidate.row, candidate.col);
    });
  }
  function applyWorkerQuantumAfterMove(boardState, piece, row, col, candidates = []) {
    if (!boardState.quantumPending?.[piece?.color]) return null;
    boardState.quantumPending[piece.color] = false;
    piece.quantumNoCaptureUntil = (boardState.turnsTaken?.[piece.color] || 0) + 1;
    piece.quantumFirstObservationFails = true;
    const valid = candidates.map((candidate) => ({
      row: Number.isInteger(candidate.anchorRow) ? candidate.anchorRow : candidate.row,
      col: Number.isInteger(candidate.anchorCol) ? candidate.anchorCol : candidate.col
    })).filter((candidate) => Number.isInteger(candidate.row) && Number.isInteger(candidate.col) && !(candidate.row === row && candidate.col === col) && workerIsQuantumDestinationAvailable(boardState, piece, candidate.row, candidate.col));
    if (!valid.length) return null;
    const centerRow = (boardRowCount(boardState) - 1) / 2;
    const centerCol = (boardColCount(boardState) - 1) / 2;
    const chosen = valid.map((candidate) => ({
      candidate,
      score: Math.abs(candidate.row - centerRow) + Math.abs(candidate.col - centerCol)
    })).sort((a, b) => a.score - b.score || a.candidate.row - b.candidate.row || a.candidate.col - b.candidate.col)[0].candidate;
    piece.quantum = { row: chosen.row, col: chosen.col };
    return { row: chosen.row, col: chosen.col };
  }
  function workerQuantumCellsForItemAt(boardState, piece, row, col) {
    if (!Number.isInteger(row) || !Number.isInteger(col)) return [];
    if (["colossus", "bigRook", "bigBishop"].includes(piece?.type)) {
      const cells = colossusCells(row, col);
      return cells.length === 4 ? cells : [];
    }
    return inBounds(row, col, boardState) ? [{ row, col }] : [];
  }
  function workerIsQuantumDestinationAvailable(boardState, piece, row, col) {
    const cells = workerQuantumCellsForItemAt(boardState, piece, row, col);
    if (!cells.length) return false;
    return cells.every((cell) => {
      const occupant = get(boardState, cell.row, cell.col);
      if (occupant) return false;
      const quantum = workerFindQuantumAt(boardState, cell.row, cell.col);
      return !quantum || quantum === piece;
    });
  }
  function workerIsFileSurgeFile(boardState, col) {
    return col === 0 || col === boardColCount(boardState) - 1;
  }
  function workerIsCornerSquare(boardState, row, col) {
    return (row === 0 || row === boardRowCount(boardState) - 1) && (col === 0 || col === boardColCount(boardState) - 1);
  }
  function workerTransformChimeraAfterMove(boardState, piece, row, col) {
    if (!piece?.chimera || get(boardState, row, col) !== piece) return false;
    const nextType = workerChimeraNextType(boardState, piece, row, col);
    delete piece.windmillMode;
    delete piece.logDir;
    delete piece.logRollAfterTurn;
    delete piece.mana;
    delete piece.maxMana;
    delete piece.ammo;
    delete piece.maxAmmo;
    delete piece.facing;
    piece.type = nextType;
    piece.moved = true;
    delete piece.shielded;
    if (boardState.monochromeChess) piece.monoShade = squareShade(row, col);
    return true;
  }
  function workerChimeraNextType(boardState, piece, row, col) {
    const seed = `${piece.id || ""}:${Number(boardState.moveCount) || 0}:${row}:${col}:${piece.type || ""}`;
    let hash = 0;
    for (let index = 0; index < seed.length; index += 1) {
      hash = (hash << 5) - hash + seed.charCodeAt(index) | 0;
    }
    const resolvedCurrentType = monochromePieceType(piece?.type, boardState.monochromeChess);
    const sourceTypes = piece?.type === "queen" ? CHIMERA_BASE_TYPES : CHIMERA_TYPES;
    const choices = sourceTypes.map((type) => monochromePieceType(type, boardState.monochromeChess)).filter((type) => type !== resolvedCurrentType).map((type) => ({ type, weight: piece?.type === "queen" ? 25 : type === "queen" ? 10 : 30 }));
    const total = choices.reduce((sum, choice) => sum + choice.weight, 0);
    let roll = (hash >>> 0) % Math.max(1, total);
    for (const choice of choices) {
      if (roll < choice.weight) return choice.type;
      roll -= choice.weight;
    }
    return monochromePieceType("knight", boardState.monochromeChess);
  }
  function workerFindQuantumAt(boardState, row, col) {
    let found = null;
    forEachPiece(boardState, (piece) => {
      if (!found && piece?.quantum?.row === row && piece.quantum.col === col) found = piece;
    });
    return found;
  }
  function applyColossusMoveAction(boardState, action, aiColor, piece, color) {
    const from = action.from || {};
    const move = action.move || {};
    if (piece.type !== "colossus") return { ok: false, score: 0 };
    const anchorRow = Number.isInteger(move.anchorRow) ? move.anchorRow : move.row;
    const anchorCol = Number.isInteger(move.anchorCol) ? move.anchorCol : move.col;
    const cells = colossusCells(anchorRow, anchorCol);
    if (cells.length !== 4) return { ok: false, score: 0 };
    const landingCaptures = workerColossusLandingCaptures(boardState, cells, piece, color);
    if (!landingCaptures) return { ok: false, score: 0 };
    const quantumCandidates = workerQuantumCandidateMovesForMove(boardState, piece, from, { ...move, row: anchorRow, col: anchorCol, anchorRow, anchorCol });
    let score = 0;
    const repeatPenalty = oscillationPenalty(boardState, action, color);
    score += color === aiColor ? -repeatPenalty : repeatPenalty;
    const captured = [];
    landingCaptures.forEach((cell) => {
      const target = get(boardState, cell.row, cell.col);
      if (!target || target === piece || captured.includes(target)) return;
      if (workerTryEvadeCapture(boardState, target, cell.row, cell.col, color, {
        attacker: piece,
        attackerLandingCells: cells
      })) return;
      captured.push(target);
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 1.6;
      if (isWorkerDecisiveCaptureTarget(boardState, target)) score += workerCriticalCaptureScore(boardState, target, color, aiColor);
    });
    recordWorkerCapturedPieces(boardState, color, captured, piece, { attackerLandingCells: cells });
    recordWorkerDirectCaptures(boardState, piece, captured);
    captured.forEach((target) => clearPieceCells(boardState, target));
    clearPieceCells(boardState, piece);
    piece.anchorRow = anchorRow;
    piece.anchorCol = anchorCol;
    piece.moved = true;
    trackWorkerMovingProgress(boardState, piece);
    delete piece.quantum;
    noteWorkerUltimatumMovement(boardState, piece);
    cells.forEach((cell) => set(boardState, cell.row, cell.col, piece));
    if (applyWorkerQuantumAfterMove(boardState, piece, anchorRow, anchorCol, quantumCandidates)) {
      score += color === aiColor ? 120 : -120;
    }
    if (workerTryStartRepositionSecondMove(boardState, piece, anchorRow, anchorCol) || workerTryStartPlatformExtraMove(boardState, piece)) retainWorkerTurn(boardState, color);
    else finishWorkerMove(boardState, color);
    pushRecentMove(boardState, {
      from: { row: from.row, col: from.col },
      to: { row: anchorRow, col: anchorCol },
      pieceId: piece.id || "",
      pieceType: piece.type || "",
      color
    });
    return { ok: true, score };
  }
  function applyBigRookMoveAction(boardState, action, aiColor, piece, color) {
    const from = action.from || {};
    const move = action.move || {};
    if (!["bigRook", "bigBishop"].includes(piece.type)) return { ok: false, score: 0 };
    const anchorRow = Number.isInteger(move.anchorRow) ? move.anchorRow : move.row;
    const anchorCol = Number.isInteger(move.anchorCol) ? move.anchorCol : move.col;
    const cells = colossusCells(anchorRow, anchorCol);
    if (cells.length !== 4) return { ok: false, score: 0 };
    const landingCaptures = workerBigRookLandingCaptures(boardState, cells, piece, color);
    if (!landingCaptures) return { ok: false, score: 0 };
    const quantumCandidates = workerQuantumCandidateMovesForMove(boardState, piece, from, { ...move, row: anchorRow, col: anchorCol, anchorRow, anchorCol });
    let score = 0;
    const repeatPenalty = oscillationPenalty(boardState, action, color);
    score += color === aiColor ? -repeatPenalty : repeatPenalty;
    const captured = [];
    landingCaptures.forEach((cell) => {
      const target = get(boardState, cell.row, cell.col);
      if (!target || target === piece || captured.includes(target)) return;
      if (workerTryEvadeCapture(boardState, target, cell.row, cell.col, color, {
        attacker: piece,
        attackerLandingCells: cells
      })) return;
      captured.push(target);
      const friendly = target.color === color;
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * (friendly ? -1.25 : 1.45);
      if (friendly && isCritical(target)) {
        boardState.mode = "gameover";
        boardState.winner = opponent(color);
        score += color === aiColor ? -12e4 : 12e4;
      } else if (isWorkerDecisiveCaptureTarget(boardState, target)) {
        score += workerCriticalCaptureScore(boardState, target, color, aiColor);
      }
    });
    recordWorkerCapturedPieces(boardState, color, captured, piece, { attackerLandingCells: cells });
    recordWorkerDirectCaptures(boardState, piece, captured);
    captured.forEach((target) => clearPieceCells(boardState, target));
    clearPieceCells(boardState, piece);
    piece.anchorRow = anchorRow;
    piece.anchorCol = anchorCol;
    piece.moved = true;
    trackWorkerMovingProgress(boardState, piece);
    delete piece.quantum;
    noteWorkerUltimatumMovement(boardState, piece);
    cells.forEach((cell) => set(boardState, cell.row, cell.col, piece));
    if (applyWorkerQuantumAfterMove(boardState, piece, anchorRow, anchorCol, quantumCandidates)) {
      score += color === aiColor ? 120 : -120;
    }
    if (workerTryStartRepositionSecondMove(boardState, piece, anchorRow, anchorCol) || workerTryStartPlatformExtraMove(boardState, piece)) retainWorkerTurn(boardState, color);
    else finishWorkerMove(boardState, color);
    pushRecentMove(boardState, {
      from: { row: from.row, col: from.col },
      to: { row: anchorRow, col: anchorCol },
      pieceId: piece.id || "",
      pieceType: piece.type || "",
      color
    });
    return { ok: true, score };
  }
  function applyShotgunReloadAction(boardState, action, aiColor) {
    const piece = get(boardState, action.from?.row, action.from?.col);
    if (!piece || piece.type !== "shotgunKing" || piece.color !== action.color) return { ok: false, score: 0 };
    if (isFrozenPiece(piece) || isPoisonStunned(piece) || isExhaustionMoveBlocked(boardState.exhaustion, piece)) return { ok: false, score: 0 };
    const maxAmmo = piece.maxAmmo ?? 3;
    if ((piece.ammo ?? 0) >= maxAmmo) return { ok: false, score: 0 };
    piece.ammo = Math.min(maxAmmo, (piece.ammo ?? 0) + 1);
    piece.moved = true;
    noteWorkerUltimatumMovement(boardState, piece);
    finishWorkerMove(boardState, action.color);
    return { ok: true, score: action.color === aiColor ? 180 : -180 };
  }
  function applyWizardSpellAction(boardState, action, aiColor) {
    const wizard = get(boardState, action.from?.row, action.from?.col);
    if (!wizard || !pieceHasAbility(wizard, "wizard") || wizard.color !== action.color) return { ok: false, score: 0 };
    if (boardState.zugzwang?.[action.color]) return { ok: false, score: 0 };
    if (isFrozenPiece(wizard)) return { ok: false, score: 0 };
    const cost = wizardSpellCost(action.spellId);
    if ((wizard.mana ?? 0) < cost) return { ok: false, score: 0 };
    let score = 0;
    if (action.spellId === "shield") {
      const target = get(boardState, action.target?.row, action.target?.col);
      if (!target || target.color !== action.color || ["wall", "scarecrow"].includes(target.type)) return { ok: false, score: 0 };
      target.shielded = true;
      score = pieceValue(target) * 0.35 + (isCritical(target) ? 900 : 0);
    } else if (action.spellId === "lightning") {
      const target = get(boardState, action.target.row, action.target.col);
      if (target && (target.color === action.color || target.type === "wall" || target.type === "football" || isFrozenPiece(target))) return { ok: false, score: 0 };
      boardState.delayedHazards.push({ type: "lightning", cells: [{ row: action.target.row, col: action.target.col }], owner: action.color, triggerAfter: opponent(action.color), casterId: wizard.id || "" });
      score = target?.color === opponent(action.color) ? pieceValue(target) * 0.75 + (isCritical(target) ? 1200 : 0) : -180;
    } else if (action.spellId === "meteor") {
      const cells = meteorCells(boardState, action.target.row, action.target.col);
      boardState.delayedHazards.push({ type: "meteor", cells, owner: action.color, triggerAfter: opponent(action.color), casterId: wizard.id || "" });
      score = cells.reduce((sum, cell) => {
        const item = get(boardState, cell.row, cell.col);
        if (!item || isFrozenPiece(item)) return sum;
        if (item.color !== action.color && !canWorkerCaptureTarget(action.color, item, wizard, boardState)) return sum;
        return sum + (item.color === opponent(action.color) ? pieceValue(item) * 0.85 : -pieceValue(item) * 0.65);
      }, 0);
      if (score <= 0 && !action.forcedRoyalSpell) return { ok: false, score: 0 };
    } else if (action.spellId === "timeStop") {
      if (!boardState.skipTurn) boardState.skipTurn = { white: false, black: false };
      boardState.skipTurn[opponent(action.color)] = true;
      score = 1250 + royalPressureScore(boardState, action.color) * 0.2;
    } else {
      return { ok: false, score: 0 };
    }
    if (action.forcedRoyalSpell) score += 8e4;
    wizard.mana -= cost;
    if (action.spellId !== "timeStop") finishWorkerMove(boardState, action.color);
    return { ok: true, score: action.color === aiColor ? score : -score };
  }
  function hasWorkerCaptureReadySameRankStandardBearer(boardState, row, color) {
    const list = standardBearerRanksByColor(boardState)[color]?.get(row);
    return Boolean(list && list.some((piece) => !isWorkerFreshNoCaptureActive(boardState, piece)));
  }
  function workerQueuedKnightExtraMoveReasons(piece) {
    if (!Array.isArray(piece?.queuedKnightExtraMoveReasons)) return [];
    return [...new Set(piece.queuedKnightExtraMoveReasons.filter((reason) => ["mad-horse", "file-surge"].includes(reason)))];
  }
  function hasWorkerAdjacentKnightmaster(boardState, row, col, color) {
    return queenDirections().some(([dr, dc]) => {
      const piece = get(boardState, row + dr, col + dc);
      return piece?.color === color && pieceHasAbility(piece, "knightmaster");
    });
  }
  function hasWorkerCaptureReadyAdjacentKnightmaster(boardState, row, col, color) {
    return queenDirections().some(([dr, dc]) => {
      const piece = get(boardState, row + dr, col + dc);
      return piece?.color === color && pieceHasAbility(piece, "knightmaster") && !isWorkerFreshNoCaptureActive(boardState, piece);
    });
  }
  function workerVortexCandidates(boardState, color) {
    const enemy = opponent(color);
    return piecesMatching(boardState, (piece) => piece.color === enemy && !isWorkerRoyalIdentityPiece(boardState, piece) && !["pawn", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(piece.type));
  }
  function workerVortexHasUnsafeShuffle(boardState, color) {
    const entries = workerVortexCandidates(boardState, color);
    const critical = criticalPieces(boardState, color);
    if (entries.length < 2 || !critical.length) return false;
    const enemy = opponent(color);
    for (let firstIndex = 0; firstIndex < entries.length; firstIndex += 1) {
      for (let secondIndex = firstIndex; secondIndex < entries.length; secondIndex += 1) {
        const first = entries[firstIndex];
        const second = entries[secondIndex];
        if (firstIndex !== secondIndex) {
          set(boardState, first.row, first.col, second.piece);
          set(boardState, second.row, second.col, first.piece);
        }
        try {
          if (critical.some(({ row, col }) => isSquareAttacked(boardState, row, col, enemy))) return true;
        } finally {
          if (firstIndex !== secondIndex) {
            set(boardState, first.row, first.col, first.piece);
            set(boardState, second.row, second.col, second.piece);
          }
        }
      }
    }
    return false;
  }
  function workerMiracleTargets(boardState, color) {
    const targets = /* @__PURE__ */ new Map();
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || piece.type !== "bishop" || isFrozenPiece(piece) || isWorkerStakedPiece(piece) || isDiceLockedPiece(boardState, piece)) return;
      for (const move of generateMovesForPiece(boardState, piece, row, col)) {
        if (!isWorkerMoveAllowed(boardState, piece, row, col, move)) continue;
        const target = get(boardState, move.row, move.col);
        if (target?.color === opponent(color) && canWorkerCaptureTarget(color, target, piece, boardState)) targets.set(target.id || target, { piece: target, row: move.row, col: move.col });
      }
    });
    return [...targets.values()];
  }
  function workerIsOthelloTarget(boardState, piece, row, col, color) {
    if (!piece || piece.color !== opponent(color) || ["wall", "football", "blackHole"].includes(piece.type) || workerIsLargePiece(piece)) return false;
    return queenDirections().some(([dr, dc]) => {
      const first = get(boardState, row + dr, col + dc);
      const second = get(boardState, row - dr, col - dc);
      return first?.color === color && second?.color === color;
    });
  }
  function markWorkerFreshNoCapture(boardState, piece) {
    if (!piece?.color || piece.color === "neutral" || piece.type === "wall") return piece;
    piece.freshNoCaptureUntil = (Number(boardState?.turnsTaken?.[piece.color]) || 0) + 1;
    if (pieceHasAbility(piece, "herald")) setWorkerHeraldJumpLock(boardState, piece, piece.color);
    return piece;
  }
  function workerCardTransformSnapshot(boardState) {
    const byId = /* @__PURE__ */ new Map();
    const byReference = /* @__PURE__ */ new Map();
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!piece || seen.has(piece)) return;
      seen.add(piece);
      const previous = { color: piece.color, type: piece.type };
      byReference.set(piece, previous);
      if (piece.id && !byId.has(piece.id)) byId.set(piece.id, previous);
    });
    return { byId, byReference };
  }
  function reconcileWorkerFreshCardTransforms(boardState, snapshot, sourceCard = null) {
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!piece || seen.has(piece)) return;
      seen.add(piece);
      if (piece.type === "merchant") {
        delete piece.freshNoCaptureUntil;
        return;
      }
      const previous = piece.id && snapshot?.byId?.get(piece.id) || snapshot?.byReference?.get(piece) || null;
      const transformed = previous ? previous.color === piece.color && previous.type !== piece.type : true;
      if (transformed && sourceCard !== "hypocrisy" && !openingPassiveSkipsFreshCapture(sourceCard)) markWorkerFreshNoCapture(boardState, piece);
    });
  }
  function isWorkerFreshNoCaptureActive(boardState, piece) {
    if (!piece?.freshNoCaptureUntil) return false;
    return (Number(boardState?.turnsTaken?.[piece.color]) || 0) < piece.freshNoCaptureUntil;
  }
  function resolveWorkerRecyclingHoldoutAfterQueenLoss(boardState, captured) {
    if (!boardState.recycling || !isWorkerQueenIdentity(captured, captured?.color) || !COLORS.includes(captured.color)) return 0;
    return resolveWorkerHoldoutPromotions(boardState, captured.color);
  }
  function isWorkerTurnExclusiveCardBlocked(boardState, card, color = boardState?.turn) {
    const usedCount = normalizeCardsUsedThisTurn(boardState?.cardsUsedThisTurn)[color] || 0;
    return isTurnExclusiveCardUseBlocked(card?.effect || card?.id, usedCount);
  }
  function applyCardAction(boardState, action, aiColor) {
    septemberBeginBoardAction(boardState);
    const color = action.color || boardState.turn;
    const card = findCard(boardState, action);
    if (!card || card.used || card.recovering || isWorkerCardPendingNextTurn(card)) return { ok: false, score: 0 };
    if (isWorkerTurnExclusiveCardBlocked(boardState, card, color)) return { ok: false, score: 0 };
    if (card.effect === "vortex" && workerVortexHasUnsafeShuffle(boardState, color)) return { ok: false, score: 0 };
    const freshTransformSnapshot = workerCardTransformSnapshot(boardState);
    boardState.cardsUsedThisTurn = normalizeCardsUsedThisTurn(boardState.cardsUsedThisTurn);
    const previousCount = boardState.cardsUsedThisTurn[color];
    boardState.cardsUsedThisTurn[color] = previousCount + 1;
    const result = applyCardActionUnchecked(boardState, action, aiColor);
    if (!result?.ok) boardState.cardsUsedThisTurn[color] = previousCount;
    else {
      if (boardState.crownRule) workerReconcileCrownRule(boardState);
      reconcileWorkerFreshCardTransforms(boardState, freshTransformSnapshot, card.effect);
      updateWorkerCaptureFlags(boardState, color);
    }
    return result;
  }
  function applyWorkerPromotionAction(boardState, action, aiColor) {
    const color = action.color || boardState.turn;
    const piece = get(boardState, action.from?.row, action.from?.col);
    if (!piece || piece.color !== color || !boardState.recycling) return { ok: false, score: 0 };
    const fieldPromotion = isFieldPromotionRecyclingReady(piece, boardState.fieldPromotion, boardState.recycling);
    if (!isPromotionRow(piece, action.from.row, boardState) && !fieldPromotion) return { ok: false, score: 0 };
    const previousValue = pieceValue(piece);
    const promotionType = workerBestRecyclingPromotionType(boardState, color);
    if (!promotionType || !consumeWorkerRecycledPromotionPiece(boardState, color, promotionType)) return { ok: false, score: 0 };
    piece.type = promotionType;
    if (fieldPromotion) piece.specialPromotionUsed = true;
    if (promotionType === "pawn") piece.noPromotion = true;
    else delete piece.noPromotion;
    clearWorkerPromotionTraits(piece, boardState);
    if (promotionType !== "pawn") piece.promotedFromPawn = true;
    if (boardState.coronation?.[piece.color] && piece.type === "queen") {
      const previousProtected = Boolean(piece.protected);
      piece.protected = true;
      piece.coronationProtection = { remaining: 1, previousProtected };
    }
    resolveWorkerSpyPromotion(piece);
    piece.moved = true;
    finishWorkerMove(boardState, color);
    const gain = Math.max(0, pieceValue(piece) - previousValue);
    return { ok: true, score: (color === aiColor ? 1 : -1) * (300 + gain) };
  }
  function applyCardActionUnchecked(boardState, action, aiColor) {
    const color = action.color || boardState.turn;
    const card = findCard(boardState, action);
    if (!card || card.used || card.recovering || isWorkerCardPendingNextTurn(card)) return { ok: false, score: 0 };
    const freezeAvailability = card.effect === "freeze" ? workerFreezeCardAvailability(boardState, color) : null;
    if (freezeAvailability && (freezeAvailability.blockedByLastWarmth || !freezeAvailability.candidates.length)) return { ok: false, score: 0 };
    let score = 0;
    if (card.effect === "summonColossus") {
      const result = applySummonColossusCard(boardState, card, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (["bigRook", "bigBishop"].includes(card.effect)) {
      const result = applyWorkerBigRookOpening(boardState, card, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (card.effect === "wizard") {
      const result = applyWizardCard(boardState, card, color, aiColor, score, action.target);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (card.effect === "merchantGuild") {
      const result = applyWorkerMerchantGuildCard(boardState, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (card.effect === "shotgunKing") {
      const result = applyShotgunKingCard(boardState, card, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (card.effect === "mongolianGambit") {
      const result = applyWorkerMongolianGambit(boardState, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (card.effect === "bloodCard") {
      const result = applyBloodCardAction(boardState, card, action, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (!legacyBloodMoonLifecycleStates.has(boardState)) {
        const slots = deck(boardState, color);
        const slot = slots.indexOf(card);
        if (slot >= 0) slots[slot] = null;
      }
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    if (String(card.effect || "").startsWith("time")) {
      const result = applyTimeTravelerCardAction(boardState, card, color, aiColor, score);
      if (!result.ok) return result;
      card.used = true;
      if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
      return result;
    }
    const target = action.target ? get(boardState, action.target.row, action.target.col) : null;
    if (card.effect === "captureTheFlag" && boardState.captureTheFlag) return { ok: false, score: 0 };
    if (card.effect === "miracle" && !workerMiracleTargets(boardState, color).length) return { ok: false, score: 0 };
    if (card.effect === "reversal" && usesSeptember18Balance(boardState) && (!target || target.color !== color || !["knight", "bishop", "camel"].includes(target.type) || isWorkerKingRole(boardState, target))) return { ok: false, score: 0 };
    if (card.effect === "reversal" && boardState.reversal?.[color]) return { ok: false, score: 0 };
    if (card.effect === "recurrence" && !workerRecurrenceTarget(boardState, target, color)) return { ok: false, score: 0 };
    if (card.effect === "nullification" && !workerNullificationTarget(target, color)) return { ok: false, score: 0 };
    if (card.effect === "outpost" && !workerOutpostTarget(boardState, target, action.target?.row, action.target?.col, color)) return { ok: false, score: 0 };
    if (card.effect === "grasshopper" && (!target || target.color !== color || !(legacyGrasshopperTargetStates.has(boardState) ? ["rook"] : ["knight", "bishop", "camel"]).includes(target.type))) {
      return { ok: false, score: 0 };
    }
    const rookTransformEffects = ["siegeRam", "magicGirl", "berserker", "slime", "trickster", "campfire", "princess"];
    const queenTransformEffects = ["siren", "undead", "hedgehog"];
    if (rookTransformEffects.includes(card.effect) && (!target || target.color !== color || !(card.effect === "campfire" && usesSeptember18Balance(boardState) ? ["knight", "bishop", "camel"] : ["rook"]).includes(target.type) || isWorkerRoyalIdentityPiece(boardState, target))) {
      return { ok: false, score: 0 };
    }
    if (queenTransformEffects.includes(card.effect) && !isWorkerQueenIdentity(target, color)) {
      return { ok: false, score: 0 };
    }
    if (card.effect === "hedgehog" && isWorkerRoyalIdentityPiece(boardState, target)) return { ok: false, score: 0 };
    if (target && ["constitutionalMonarchy", "jester", "amazon", "idol", "reaper", "localConscription", "hook"].includes(card.effect) && !isWorkerQueenIdentity(target, color)) return { ok: false, score: 0 };
    if (card.effect === "queensGambit" && !isWorkerQueensGambitQueen(boardState, target, color)) {
      return { ok: false, score: 0 };
    }
    const crownPlacementCells = ["portalGun", "hypocrisy"].includes(card.effect) ? action.target?.selections || [] : card.effect === "lobster" ? [action.target] : [];
    if (crownPlacementCells.some((cell) => cell && workerSeptember12PlacementCrownBlocked(boardState, cell.row, cell.col))) {
      return { ok: false, score: 0 };
    }
    card.used = true;
    if (card.effect === "outpost") {
      target.outpostProtected = true;
      score += (color === aiColor ? 1 : -1) * 200;
    } else if (card.effect === "bribe") {
      // Grafted from engine.optimized.js (our-only addition, genuinely
      // absent from aiWorker-raw.js -- confirmed by grepping the whole
      // real file for "bribe"/"Bribe", found nowhere but the star-cost
      // table). "나이트 하나를 3턴 동안 아마존으로 변경합니다. 시간이
      // 지나면 다시 나이트가 됩니다." Ticked in finishWorkerMove via
      // tickWorkerBribedPieces, mirroring the existing iceSheet/witchTrial
      // tick-down pattern. This branch's own eligibility (target must be
      // color's own knight) is already enforced by the "bribe" targeting
      // branch grafted alongside this into generateWorkerCardTargetsV2.
      if (!target || target.color !== color || target.type !== "knight") return { ok: false, score: 0 };
      target.type = "amazon";
      target.bribed = { remaining: 3 };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (card.effect === "collapse") {
      // Grafted from engine.optimized.js (our-only addition, genuinely
      // absent from aiWorker-raw.js -- see the workerCanResolveUntargetedCard
      // "collapse" branch above). "다음 턴에 양 끝쪽 파일과 랭크가
      // 붕괴합니다." One-shot version of the existing RULE card
      // periodicCollapse -- reuses its own one-ring shrink helper
      // (workerCollapseOneRing, already in this file) and
      // collapseDepth/collapsed fields directly. Untargeted (no `target`
      // used). Fires on the very next finishWorkerMove call after the card
      // is used via resolveWorkerOneShotCollapse, grafted into
      // finishWorkerMove's tick point below.
      boardState.pendingOneShotCollapse = (Number(boardState.pendingOneShotCollapse) || 0) + 1;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "captureTheFlag") {
      boardState.captureTheFlag = { flags: { white: { row: boardRowCount(boardState) - 1, col: workerStableIndex("flag-white:" + (boardState.moveCount || 0), boardColCount(boardState)) }, black: { row: 0, col: workerStableIndex("flag-black:" + (boardState.moveCount || 0), boardColCount(boardState)) } }, occupations: { white: null, black: null } };
    } else if (card.effect === "miracle") {
      const targets = workerMiracleTargets(boardState, color);
      const former = targets.map(({ piece }) => ({ ...clonePlain(piece), recurrence: false }));
      for (const { piece } of targets) {
        piece.color = color;
        piece.moved = true;
        piece.defected = true;
        resetConvertedLargePieceHealth(piece);
      }
      for (const item of former) score += workerCriticalCaptureScore(boardState, item, color, aiColor) + (color === aiColor ? 1 : -1) * pieceValue(item) * 2;
    } else if (card.effect === "reversal") {
      if (usesSeptember18Balance(boardState)) {
        clearPieceCells(boardState, target);
        recordWorkerCapturedPieces(boardState, opponent(color), [target]);
      }
      if (!boardState.reversal) boardState.reversal = { white: false, black: false };
      boardState.reversal[color] = true;
    } else if (card.effect === "recurrence") {
      target.recurrence = true;
    } else if (card.effect === "nullification") {
      target.nullification = true;
      score += (color === aiColor ? 1 : -1) * 200;
    } else if (card.effect === "replayMove") {
      const replay = boardState.moveReplay?.[color];
      if (!canWorkerReplayLastMove(boardState, color)) return { ok: false, score: 0 };
      restoreWorkerReplayBoardDelta(boardState, replay.delta);
      const currentCaptures = boardState.captures?.[color] || [];
      boardState.captures[color] = [
        ...clonePlain(replay.capturesBefore || []),
        ...currentCaptures.slice((replay.capturesAfter || []).length)
      ];
      if (workerReplayResurrectionPrefixMatches(boardState.undeadResurrections || [], replay.undeadResurrectionsAfter || [])) {
        boardState.undeadResurrections = [
          ...clonePlain(replay.undeadResurrectionsBefore || []),
          ...(boardState.undeadResurrections || []).slice((replay.undeadResurrectionsAfter || []).length)
        ];
      }
      const replayedMoveIsStillLatest = Object.prototype.hasOwnProperty.call(replay, "lastMoveAfter") && JSON.stringify(boardState.lastMove || null) === JSON.stringify(replay.lastMoveAfter || null);
      if (replayedMoveIsStillLatest) {
        boardState.enPassant = clonePlain(replay.enPassantBefore || null);
        boardState.lastMove = clonePlain(replay.previousLastMove || null);
        boardState.accelerationTrail = clonePlain(replay.accelerationTrailBefore || null);
      }
      [
        ["castled", "castledBefore", "castledAfter"],
        ["zugzwang", "zugzwangBefore", "zugzwangAfter"],
        ["quantumPending", "quantumPendingBefore", "quantumPendingAfter"],
        ["switcheroo", "switcherooBefore", "switcherooAfter"],
        ["moving", "movingBefore", "movingAfter"],
        ["exhaustion", "exhaustionBefore", "exhaustionAfter"]
      ].forEach(([stateKey, beforeKey, afterKey]) => {
        if (!Object.prototype.hasOwnProperty.call(replay, beforeKey) || !Object.prototype.hasOwnProperty.call(replay, afterKey)) return;
        const before = replay[beforeKey]?.[color];
        const after = replay[afterKey]?.[color];
        if (JSON.stringify(before ?? null) === JSON.stringify(after ?? null)) return;
        if (!boardState[stateKey] || typeof boardState[stateKey] !== "object") boardState[stateKey] = {};
        boardState[stateKey][color] = clonePlain(before);
      });
      if (Object.prototype.hasOwnProperty.call(replay, "crownRuleBefore") && Object.prototype.hasOwnProperty.call(replay, "crownRuleAfter") && JSON.stringify(replay.crownRuleBefore || null) !== JSON.stringify(replay.crownRuleAfter || null)) {
        boardState.crownRule = clonePlain(replay.crownRuleBefore || null);
      }
      if (boardState.ultimatum && replay.ultimatumBefore && replay.ultimatumAfter) {
        const beforeIds = new Set((replay.ultimatumBefore.movedIds || []).map(String));
        const addedByReplayedMove = new Set((replay.ultimatumAfter.movedIds || []).map(String).filter((id) => !beforeIds.has(id)));
        if (addedByReplayedMove.size) {
          boardState.ultimatum.movedIds = (boardState.ultimatum.movedIds || []).map(String).filter((id) => !addedByReplayedMove.has(id));
        }
      }
      boardState.moveReplay[color] = null;
      score += (color === aiColor ? 1 : -1) * 680;
    } else if (rookTransformEffects.includes(card.effect) || queenTransformEffects.includes(card.effect)) {
      if (card.effect === "undead") {
        delete target.vipInvitation;
        delete target.holdoutPromotion;
      } else {
        clearWorkerPromotionTraits(target, boardState);
      }
      target.type = card.effect;
      target.moved = true;
      if (card.effect === "trickster") {
        rerollWorkerTricksterAbility(boardState, target, `${target.id || "trickster"}:${boardState.moveCount || 0}`);
      }
      score += (color === aiColor ? 1 : -1) * Math.max(420, pieceValue(target) * 0.55);
    } else if (card.effect === "suspiciousPotion") {
      if (!target || !COLORS.includes(target.color) || workerIsLargePiece(target) || ["wall", "football", "blackHole", "monster", "coffin"].includes(target.type)) return { ok: false, score: 0 };
      const effects = suspiciousPotionEffectsForPiece(target, {
        isKing: isWorkerRoyalIdentityPiece(boardState, target),
        isRanged: workerIsIceSheetRangedPiece(boardState, target)
      });
      const effect = effects[workerStableIndex(`${card.instanceId || card.id}:${target.id || ""}:${boardState.moveCount || 0}`, effects.length)];
      if (!effect) return { ok: false, score: 0 };
      const hostileColor = opponent(target.color);
      if (effect.id === "sacrificeProtection") {
        const previousProtected = target.sacrificeProtection ? Boolean(target.sacrificeProtection.previousProtected) : Boolean(target.protected);
        target.sacrificeProtection = { by: target.color, remaining: 3, previousProtected };
        target.protected = true;
      } else if (effect.id === "lastResistance") {
        const previousProtected = target.lastResistance ? Boolean(target.lastResistance.previousProtected) : Boolean(target.protected);
        target.lastResistance = { by: target.color, remaining: 3, previousProtected };
        target.protected = true;
      } else if (effect.id === "coronationProtection") {
        const previousProtected = target.coronationProtection ? Boolean(target.coronationProtection.previousProtected) : Boolean(target.protected);
        target.coronationProtection = { color: target.color, startTurn: Number(boardState.turnsTaken?.[target.color]) || 0, previousProtected };
        target.protected = true;
      } else if (effect.id === "shield") target.shielded = true;
      else if (effect.id === "evasion") target.evasion = true;
      else if (effect.id === "ghost") target.ghost = true;
      else if (effect.id === "chameleon") target.chameleon = true;
      else if (effect.id === "basicTraining") {
        target.basicTraining = true;
        target.potionBasicTraining = true;
      } else if (effect.id === "stealth") target.hiddenFrom = hostileColor;
      else if (effect.id === "loyalist") target.loyalist = true;
      else if (effect.id === "explosive") target.explosive = true;
      else if (effect.id === "parry") target.parry = { chance: PARRY_CHANCE };
      else if (effect.id === "submerge") target.submerged = true;
      else if (effect.id === "chimera") target.chimera = true;
      else if (effect.id === "witchTrial") target.witchTrial = { by: hostileColor, remaining: 3 };
      else if (effect.id === "stake") target.staked = { by: target.color, remaining: 4 };
      else if (effect.id === "callingCard") target.callingCard = { by: hostileColor };
      else if (effect.id === "emptyLunchbox") target.emptyLunchbox = { by: hostileColor, deadlineTurn: (Number(boardState.turnsTaken?.[target.color]) || 0) + 3 };
      else if (effect.id === "poisonStun") {
        target.poisonStunTurns = Math.max(3, Number(target.poisonStunTurns) || 0);
        target.poisonStunColor = target.color;
      } else if (effect.id === "freeze") {
        target.frozen = true;
        target.frozenByCard = { remaining: 3, source: hostileColor };
      } else if (effect.id === "disarm") target.disarmed = { by: hostileColor, remaining: 1 };
      else if (effect.id === "mannerNoCapture") {
        target.potionManner = true;
        target.coolGuyCapturedLast = true;
      } else if (effect.id === "saturationNoCapture") {
        target.potionSaturation = true;
        target.capturesMade = SATURATION_CAPTURE_LIMIT;
      } else if (effect.id === "poisonedPawn") target.poisonedPawn = true;
      else if (effect.id === "queensGambitProtection") {
        target.protected = true;
        target.queensGambitProtection = true;
      } else if (effect.id === "trojanHorse") target.trojanHorse = true;
      else if (effect.id === "severance") target.severed = { by: hostileColor, remaining: 2 };
      else if (effect.id === "inertia") target.inertia = true;
      else if (effect.id === "outpostProtection") target.outpostProtected = true;
      else if (effect.id === "nullification") target.nullification = true;
      else if (effect.id === "recurrence" && !isWorkerRoyalIdentityPiece(boardState, target)) target.recurrence = true;
      card.suspiciousPotionResultId = effect.id;
      const negativeEffects = /* @__PURE__ */ new Set(["freeze", "witchTrial", "callingCard", "emptyLunchbox", "poisonStun", "disarm", "mannerNoCapture", "saturationNoCapture", "severance", "inertia"]);
      const favorable = target.color === color ? !negativeEffects.has(effect.id) : negativeEffects.has(effect.id);
      score += (color === aiColor ? 1 : -1) * (favorable ? 360 : -240);
    } else if (card.effect === "kingOfTheHill") {
      if (!boardState.hillKing) boardState.hillKing = { white: false, black: false };
      if (boardState.hillKing[color]) return { ok: false, score: 0 };
      boardState.hillKing[color] = true;
      score += (color === aiColor ? 1 : -1) * 430;
    } else if (card.effect === "genevaConvention") {
      if (!boardState.genevaConvention) boardState.genevaConvention = { white: false, black: false };
      if (boardState.genevaConvention[color]) return { ok: false, score: 0 };
      boardState.genevaConvention[color] = true;
      score += (color === aiColor ? 1 : -1) * 480;
    } else if (card.effect === "loyalist") {
      if (!target || target.color !== color || target.loyalist || isSlimeSpecialMovementLocked(target) || isWorkerRoyalIdentityPiece(boardState, target) || workerIsLargePiece(target) || ["merchant", "wall", "football", "blackHole", "monster", "coffin"].includes(target.type)) return { ok: false, score: 0 };
      target.loyalist = true;
      score += (color === aiColor ? 1 : -1) * (260 + pieceValue(target) * 0.2);
    } else if (card.effect === "parry") {
      if (!target || target.color !== color || target.parry || ["wall", "football", "blackHole", "monster", "scarecrow"].includes(target.type)) return { ok: false, score: 0 };
      target.parry = { chance: PARRY_CHANCE };
      score += (color === aiColor ? 1 : -1) * (280 + pieceValue(target) * 0.35);
    } else if (card.effect === "blackMagic") {
      const allied = piecesMatching(boardState, (piece) => piece.color === color);
      const royal = allied.find(({ piece }) => isWorkerKingRole(boardState, piece));
      if (!royal) return { ok: false, score: 0 };
      cancelAllQuantumPhenomena(boardState);
      const transformedIds = new Set(allied.map(({ piece }) => piece.id).filter(Boolean));
      const crownState = workerNormalizeCrownRule(boardState.crownRule, boardState);
      if (crownState) {
        const crownEntries = workerCrownRuleEntries(crownState);
        crownEntries.forEach((entry) => {
          if (entry.holderId && transformedIds.has(entry.holderId)) workerResetRemovedCrownRule(boardState, entry);
        });
        workerStoreCrownRuleEntries(boardState, crownEntries);
        workerSyncCrownFlags(boardState, boardState.crownRule);
      }
      royal.piece.type = "darkWizard";
      royal.piece.moved = true;
      delete royal.piece.darkMagicCircle;
      delete royal.piece.crownBearer;
      delete royal.piece.crownRoyal;
      const removedPieceIds = /* @__PURE__ */ new Set();
      allied.forEach(({ piece, row, col }) => {
        if (piece === royal.piece) return;
        clearPieceCells(boardState, piece);
        if (piece.id) removedPieceIds.add(piece.id);
      });
      if (Array.isArray(boardState.necromancy) && removedPieceIds.size) {
        boardState.necromancy = boardState.necromancy.filter((entry) => !removedPieceIds.has(entry?.id));
      }
      const summonCount = Math.ceil((allied.length - 1) / 2);
      const homeRow = color === "white" ? boardRowCount(boardState) - 1 : 0;
      const rowStep = color === "white" ? -1 : 1;
      const candidates = [];
      for (let offset = 0; offset < boardRowCount(boardState); offset += 1) {
        const row = homeRow + rowStep * offset;
        for (let col = 0; col < boardColCount(boardState); col += 1) {
          if (workerOpenPlacementSquare(boardState, row, col) && !workerSeptember12PlacementCrownBlocked(boardState, row, col) && !isBlackHoleCell(boardState, row, col)) candidates.push({ row, col });
        }
      }
      candidates.slice(0, summonCount).forEach(({ row, col }) => {
        const monster = workerCreatePiece("neutral", "monster", boardState, row, col);
        monster.blackMagicOwner = color;
        set(boardState, row, col, monster);
      });
      score += (color === aiColor ? 1 : -1) * 720;
    } else if (card.effect === "fleetingDream") {
      let changed = 0;
      piecesMatching(boardState, (piece) => piece.color === opponent(color) && piece.promotedFromPawn).forEach(({ piece }) => {
        piece.type = "pawn";
        piece.moved = true;
        delete piece.promotedFromPawn;
        delete piece.noPromotion;
        clearWorkerPromotionTraits(piece, boardState);
        changed += 1;
      });
      if (!changed) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * changed * 300;
    } else if (card.effect === "emptyLunchbox") {
      if (!target || target.color !== opponent(color) || isWorkerRoyalIdentityPiece(boardState, target) || target.emptyLunchbox || ["merchant", "wall", "football", "blackHole", "monster"].includes(target.type)) return { ok: false, score: 0 };
      target.emptyLunchbox = { by: color, deadlineTurn: (Number(boardState.turnsTaken?.[target.color]) || 0) + 3 };
      score += (color === aiColor ? 1 : -1) * (300 + pieceValue(target) * 0.3);
    } else if (card.effect === "platformRule") {
      if (boardState.platformRule?.enabled) return { ok: false, score: 0 };
      const currentTurn = platformTurnCount(boardState.turnsTaken, { countUnit: "ply" });
      boardState.platformRule = normalizePlatformRule({ enabled: true, countUnit: "ply", cadence: "full-turn", nextAt: currentTurn + PLATFORM_INTERVAL_TURNS * 2 }, currentTurn, boardRowCount(boardState), boardColCount(boardState));
      spawnWorkerPlatformAtTurn(boardState, currentTurn);
      score += (color === aiColor ? 1 : -1) * 340;
    } else if (card.effect === "qxe1") {
      const plan = workerQxe1UsurpationPlan(boardState, color);
      if (!plan) return { ok: false, score: 0 };
      clearPieceCells(boardState, plan.queen.piece);
      clearPieceCells(boardState, plan.king);
      forEachPiece(boardState, (piece) => {
        if (piece?.color === color) delete piece.regencyHeir;
      });
      if (plan.king.undergroundBunker) {
        plan.queen.piece.undergroundBunker = true;
        plan.queen.piece.hp = Math.max(1, Number(plan.king.hp) || 5);
        plan.queen.piece.maxHp = Math.max(plan.queen.piece.hp, Number(plan.king.maxHp) || 5);
      }
      if (plan.king.lastResistance) {
        plan.queen.piece.lastResistance = clonePlain(plan.king.lastResistance);
        plan.queen.piece.protected = Boolean(plan.king.protected);
      }
      if (Array.isArray(plan.king.imperialMoves)) {
        plan.queen.piece.imperialMoves = [...new Set(plan.king.imperialMoves.map(String))];
      }
      plan.queen.piece.regencyHeir = true;
      plan.queen.piece.moved = true;
      set(boardState, plan.throne.row, plan.throne.col, plan.queen.piece);
      if (!boardState.regency) boardState.regency = {};
      if (!boardState.kingDead) boardState.kingDead = {};
      boardState.regency[color] = true;
      boardState.kingDead[color] = true;
      score += (color === aiColor ? 1 : -1) * 760;
    } else if (card.effect === "imperialStudies") {
      if (boardState.imperialStudies?.[color] || !findWorkerKingRole(boardState, color)) return { ok: false, score: 0 };
      if (!boardState.imperialStudies) boardState.imperialStudies = { white: false, black: false };
      boardState.imperialStudies[color] = true;
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "religiousVictory") {
      if (boardState.religiousVictory?.[color]) return { ok: false, score: 0 };
      if (!boardState.religiousVictory) boardState.religiousVictory = { white: false, black: false };
      boardState.religiousVictory[color] = true;
      const winner = resolveWorkerReligiousVictory(boardState);
      score += winner ? winner === aiColor ? 1e5 : -1e5 : (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "racingKing") {
      const king = findWorkerRacingKingRole(boardState, color);
      if (!king) return { ok: false, score: 0 };
      if (!boardState.racingKing) boardState.racingKing = { white: false, black: false };
      boardState.racingKing[color] = true;
      if (resolveWorkerRacingKingVictory(boardState, color)) {
        if (boardState.winner) score += boardState.winner === aiColor ? 1e5 : -1e5;
      } else {
        score += (color === aiColor ? 1 : -1) * 460;
      }
    } else if (card.effect === "armistice") {
      boardState.armistice = { remaining: 2, by: color, actedColors: [] };
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "poisonedPawn") {
      if (!target || target.color !== color || target.type !== "pawn" || target.poisonedPawn) return { ok: false, score: 0 };
      target.poisonedPawn = true;
      score += (color === aiColor ? 1 : -1) * (220 + pieceValue(target) * 0.4);
    } else if (card.effect === "portalGun") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections.slice(0, 2) : [];
      const unique = [...new Map(selections.map((cell) => [`${cell?.row},${cell?.col}`, { row: Number(cell?.row), col: Number(cell?.col) }])).values()];
      if (unique.length !== 2 || unique.some((cell) => !inBounds(cell.row, cell.col, boardState) || get(boardState, cell.row, cell.col) || workerSeptember12PlacementCrownBlocked(boardState, cell.row, cell.col) || isWorkerCollapsedSquare(boardState, cell.row, cell.col) || isBlackHoleCell(boardState, cell.row, cell.col) || isWorkerPendingPortalReservedSquare(boardState, cell.row, cell.col))) {
        return { ok: false, score: 0 };
      }
      if (!Array.isArray(boardState.pendingPortals)) boardState.pendingPortals = [];
      boardState.pendingPortals.push({
        id: `worker-portal-${card.instanceId || card.id || boardState.moveCount || 0}`,
        color,
        cells: unique,
        triggerTurn: (Number(boardState.turnsTaken?.[color]) || 0) + 1
      });
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "exhaustion") {
      boardState.exhaustion = normalizeExhaustionState(boardState.exhaustion);
      boardState.exhaustion[opponent(color)] = { enabled: true, pieceId: "", count: 0 };
      score += (color === aiColor ? 1 : -1) * 500;
    } else if (card.effect === "missionary") {
      if (!target || target.color !== color || target.type !== "bishop") return { ok: false, score: 0 };
      target.type = "missionary";
      target.moved = true;
      markWorkerFreshNoCapture(boardState, target);
      score += (color === aiColor ? 1 : -1) * 440;
    } else if (card.effect === "mistakeCard") {
      boardState.mistakeCard = normalizeDemocracyState(boardState.mistakeCard);
      const enemy = opponent(color);
      if (boardState.mistakeCard[enemy]) return { ok: false, score: 0 };
      boardState.mistakeCard[enemy] = true;
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "frontlineResponse") {
      if (boardState.frontlineResponse?.[color]) return { ok: false, score: 0 };
      boardState.frontlineResponse = normalizeDemocracyState(boardState.frontlineResponse);
      boardState.frontlineResponse[color] = true;
      score += (color === aiColor ? 1 : -1) * 440;
    } else if (card.effect === "relay") {
      boardState.relay = normalizeDemocracyState(boardState.relay);
      boardState.relay[color] = true;
      score += (color === aiColor ? 1 : -1) * 360;
    } else if (card.effect === "fieldPromotion") {
      boardState.fieldPromotion = normalizeDemocracyState(boardState.fieldPromotion);
      boardState.fieldPromotion[color] = true;
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "gomoku") {
      boardState.gomoku = normalizeDemocracyState(boardState.gomoku);
      boardState.gomoku[color] = true;
      if (resolveWorkerGomokuVictory(boardState)) score += color === aiColor ? 1e5 : -1e5;
      else score += (color === aiColor ? 1 : -1) * 820;
    } else if (card.effect === "hypocrisy") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections : [];
      const unique = [...new Map(selections.map((cell) => [`${Number(cell?.row)},${Number(cell?.col)}`, { row: Number(cell?.row), col: Number(cell?.col) }])).values()];
      if (unique.length !== 4 || unique.some((cell) => !workerOpenPlacementSquare(boardState, cell.row, cell.col) || workerSeptember12PlacementCrownBlocked(boardState, cell.row, cell.col) || isWorkerCollapsedSquare(boardState, cell.row, cell.col) || isBlackHoleCell(boardState, cell.row, cell.col))) {
        return { ok: false, score: 0 };
      }
      unique.forEach(({ row, col }, index) => {
        const pawn = workerCreatePiece(opponent(color), "pawn", boardState, row, col);
        pawn.id = `hypocrisy-ai-${card.instanceId || card.id || boardState.moveCount || 0}-${index}`;
        pawn.origin = workerSquareName(boardState, row, col);
        set(boardState, row, col, pawn);
      });
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "cleanupPieces") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections : [];
      const chosen = [];
      const seen = /* @__PURE__ */ new Set();
      selections.forEach(({ row, col }) => {
        const piece = get(boardState, Number(row), Number(col));
        if (!piece || seen.has(piece) || piece.color !== color || isWorkerRoyalIdentityPiece(boardState, piece) || workerIsLargePiece(piece) || ["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)) return;
        seen.add(piece);
        chosen.push({ piece, row: Number(row), col: Number(col) });
      });
      if (!chosen.length || chosen.length !== selections.length) return { ok: false, score: 0 };
      chosen.forEach(({ piece }) => {
        clearPieceCells(boardState, piece);
        score += color === aiColor ? -pieceValue(piece) : pieceValue(piece);
      });
    } else if (card.effect === "democracy") {
      boardState.democracy = normalizeDemocracyState(boardState.democracy);
      if (boardState.democracy[color] || workerCountPiecesOf(boardState, color, "pawn") <= 0) return { ok: false, score: 0 };
      boardState.democracy[color] = true;
      score += (color === aiColor ? 1 : -1) * 620;
    } else if (card.effect === "callingCard") {
      const candidates = piecesMatching(boardState, (piece) => piece.color === opponent(color) && piece.type !== "pawn" && !["guard", "jester"].some((type) => pieceHasAbility(piece, type)) && !isWorkerNativeKing(piece) && !isWorkerRegencyRoyalHeir(boardState, piece));
      if (!candidates.length) return { ok: false, score: 0 };
      const chosen = candidates[workerStableIndex(`${card.instanceId || card.id}:${boardState.moveCount || 0}`, candidates.length)];
      chosen.piece.callingCard = { by: color };
      score += (color === aiColor ? 1 : -1) * 280;
    } else if (card.effect === "frenzy") {
      if (!target || target.color !== color || target.type !== "pawn") return { ok: false, score: 0 };
      target.frenzy = { by: color };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (card.effect === "zugzwang") {
      const enemy = opponent(color);
      if (!workerZugzwangKingMoves(boardState, enemy).length) return { ok: false, score: 0 };
      if (!boardState.zugzwang) boardState.zugzwang = { white: false, black: false };
      boardState.zugzwang[enemy] = true;
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "vip") {
      if (!target || !["white", "black"].includes(target.color) || target.type !== "pawn") return { ok: false, score: 0 };
      target.vipInvitation = {
        by: color,
        triggerTurn: (Number(boardState.turnsTaken?.[target.color]) || 0) + 3
      };
      score += (color === aiColor ? 1 : -1) * (target.color === color ? -420 : 480);
    } else if (card.effect === "babyBear") {
      if (!isWorkerNonRoyalQueen(boardState, target, color)) return { ok: false, score: 0 };
      const row = Number(action.target?.row);
      const col = Number(action.target?.col);
      clearPieceCells(boardState, target);
      recordWorkerCapturedPieces(boardState, opponent(color), [target]);
      const summoned = workerCreatePiece(color, "babyBear", boardState, row, col);
      summoned.origin = workerSquareName(boardState, row, col);
      summoned.babyBearGrowAtTurn = sharedTurnCount(boardState) + 7;
      set(boardState, row, col, summoned);
      score += (color === aiColor ? 1 : -1) * 360;
    } else if (card.effect === "submerge") {
      const targetRow2 = Number(action.target?.row);
      const targetCol2 = Number(action.target?.col);
      if (!target || target.color !== color || target.submerged || workerIsLargePiece(target) || ["wall", "football", "blackHole", "coffin"].includes(target.type) || hasAdjacentEnemyPiece(boardState.board, targetRow2, targetCol2, color)) {
        return { ok: false, score: 0 };
      }
      target.submerged = true;
      resolveWorkerSubmergedPieces(boardState);
      score += (color === aiColor ? 1 : -1) * 240;
    } else if (card.effect === "scarecrow" && usesSeptember18Balance(boardState)) {
      if (!target || target.color !== color) return { ok: false, score: 0 };
      boardState.pendingScarecrows ||= [];
      boardState.pendingScarecrows.push({ id: `worker-scarecrow-${target.id}`, reserved: true, color, by: color, row: action.target.row, col: action.target.col, remainingOwnTurns: 3 });
      const decisive = isWorkerDecisiveCaptureTarget(boardState, target);
      clearPieceCells(boardState, target);
      recordWorkerCapturedPieces(boardState, opponent(color), [target]);
      if (decisive) {
        boardState.mode = "gameover";
        boardState.winner = opponent(color);
      }
    } else if (card.effect === "gale") {
      if (!Array.isArray(boardState.pendingGales)) boardState.pendingGales = [];
      boardState.pendingGales.push({
        id: `worker-gale-${card.instanceId || card.id || boardState.moveCount || 0}`,
        color,
        ...usesSeptember18Balance(boardState) ? { remainingOwnTurns: 3 } : {},
        triggerTurn: (Number(boardState.turnsTaken?.[color]) || 0) + 3
      });
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "idol") {
      const queen = workerSelectedQueenSquare(boardState, color, action.target);
      if (!queen) return { ok: false, score: 0 };
      clearWorkerPromotionTraits(queen.piece, boardState);
      queen.piece.type = "idol";
      queen.piece.moved = true;
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (card.effect === "homecoming") {
      if (!target || target.color !== color || isWorkerRoyalIdentityPiece(boardState, target) || workerIsLargePiece(target) || ["wall", "football", "blackHole", "coffin"].includes(target.type)) {
        return { ok: false, score: 0 };
      }
      const origin = parseWorkerSquare(boardState, target.origin);
      if (!workerOpenPlacementSquare(boardState, origin.row, origin.col) || origin.row === action.target.row && origin.col === action.target.col) {
        return { ok: false, score: 0 };
      }
      set(boardState, action.target.row, action.target.col, null);
      set(boardState, origin.row, origin.col, target);
      delete target.freshNoCaptureUntil;
      delete target.cardNoCaptureUntil;
      delete target.quantumNoCaptureUntil;
      target.coolGuyCapturedLast = false;
      target.moved = true;
      score += (color === aiColor ? 1 : -1) * 180;
    } else if (card.effect === "otherworld") {
      const pawns = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn");
      if (!pawns.length) return { ok: false, score: 0 };
      const chosen = pawns[workerStableIndex(`${card.instanceId || card.id}:${boardState.moveCount || 0}`, pawns.length)];
      if (!Array.isArray(boardState.pendingOtherworld)) boardState.pendingOtherworld = [];
      boardState.pendingOtherworld.push({
        id: `worker-otherworld-${card.instanceId || card.id || boardState.moveCount || 0}`,
        color,
        pieceId: chosen.piece.id,
        row: chosen.row,
        col: chosen.col,
        origin: chosen.piece.origin || workerSquareName(boardState, chosen.row, chosen.col),
        dueMoveCount: (Number(boardState.moveCount) || 0) + OTHERWORLD_RETURN_HALF_TURNS,
        remainingHalfTurns: OTHERWORLD_RETURN_HALF_TURNS
      });
      clearPieceCells(boardState, chosen.piece);
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "twins") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections.slice(0, 2) : [];
      const selected = [];
      const seen = /* @__PURE__ */ new Set();
      selections.forEach(({ row, col }) => {
        const piece = get(boardState, row, col);
        if (!piece || piece.color !== color || isSlimeSpecialMovementLocked(piece) || piece.twinBondId || workerIsLargePiece(piece) || ["wall", "football", "blackHole", "coffin"].includes(piece.type) || seen.has(piece)) return;
        seen.add(piece);
        selected.push(piece);
      });
      if (selected.length !== 2) return { ok: false, score: 0 };
      const bondId = `worker-twins-${card.instanceId || card.id || boardState.moveCount || 0}`;
      selected[0].twinBondId = bondId;
      selected[0].twinPartnerId = selected[1].id;
      selected[1].twinBondId = bondId;
      selected[1].twinPartnerId = selected[0].id;
      score += (color === aiColor ? 1 : -1) * 320;
    } else if (card.effect === "judgment") {
      const candidates = workerJudgmentEntries(boardState);
      const chosen = candidates.find((entry) => entry.piece === target);
      if (!chosen) return { ok: false, score: 0 };
      const value = pieceValue(target);
      const returnPhase = workerNextJudgmentDraftPhase(boardState);
      if (returnPhase) {
        if (!Array.isArray(boardState.judgmentExiles)) boardState.judgmentExiles = [];
        clearWorkerObsoleteRestoration(boardState, target);
        clearPieceCells(boardState, target);
        boardState.judgmentExiles.push({
          id: `worker-judgment-${card.instanceId || card.id || boardState.moveCount || 0}-${target.id || chosen.row + "-" + chosen.col}`,
          piece: target,
          returnPhase,
          exiledBy: color,
          from: { row: chosen.row, col: chosen.col }
        });
      } else {
        const captureOwner = target.color === color ? opponent(color) : color;
        score += forceWorkerRemovePieceAt(boardState, chosen.row, chosen.col, captureOwner, aiColor);
      }
      score += (color === aiColor ? 1 : -1) * (target.color === color ? -value : value);
    } else if (card.effect === "lobster") {
      const row = Number(action.target?.row);
      const col = Number(action.target?.col);
      if (!workerOpenPlacementSquare(boardState, row, col) || workerSeptember12PlacementCrownBlocked(boardState, row, col)) return { ok: false, score: 0 };
      if (!Array.isArray(boardState.pendingLobsters)) boardState.pendingLobsters = [];
      boardState.pendingLobsters.push({
        id: `worker-lobster-${card.instanceId || card.id || boardState.moveCount || 0}-${row}-${col}`,
        color,
        by: color,
        row,
        col,
        dueMoveCount: (Number(boardState.moveCount) || 0) + LOBSTER_SUMMON_HALF_TURNS,
        remainingHalfTurns: LOBSTER_SUMMON_HALF_TURNS
      });
      score += (color === aiColor ? 1 : -1) * 220;
    } else if (card.effect === "lastResistance") {
      const king = findWorkerKingRole(boardState, color);
      if (!king) return { ok: false, score: 0 };
      const previousProtected = Boolean(king.piece.protected);
      king.piece.protected = true;
      king.piece.lastResistance = { remaining: 3, previousProtected };
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "evasion") {
      const candidates = workerEvasionEligiblePieces(boardState, color);
      if (!candidates.length) return { ok: false, score: 0 };
      const chosen = candidates[workerStableIndex(`${card.instanceId || card.id}:${boardState.moveCount || 0}`, candidates.length)];
      chosen.piece.evasion = true;
      score += (color === aiColor ? 1 : -1) * (180 + pieceValue(chosen.piece) * 0.16);
    } else if (card.effect === "randomRoulette") {
      const result = applyWorkerRandomRoulette(
        boardState,
        card,
        target,
        action.target?.row,
        action.target?.col,
        color,
        aiColor
      );
      if (!result.ok) return result;
      score += result.score;
    } else if (card.effect === "freeMove") {
      const requested = Array.isArray(action.target?.selections) ? action.target.selections.slice(0, 3) : [];
      const seen = /* @__PURE__ */ new Set();
      const moves = [];
      requested.forEach((selection) => {
        const from = selection?.from || selection;
        const to = selection?.to || {};
        const piece = get(boardState, from?.row, from?.col);
        if (!piece || piece.color !== color || !piece.id || seen.has(piece.id)) return;
        const legal = generateMovesForPiece(boardState, piece, from.row, from.col).find((move) => isWorkerMoveAllowed(boardState, piece, from.row, from.col, move) && move.row === to.row && move.col === to.col);
        if (!legal) return;
        seen.add(piece.id);
        moves.push({
          pieceId: piece.id,
          pieceType: piece.type,
          from: { row: from.row, col: from.col },
          to: { row: legal.row, col: legal.col },
          fromCells: [{ row: from.row, col: from.col }],
          toCells: Array.isArray(legal.highlightCells) && legal.highlightCells.length ? legal.highlightCells.slice(0, 4).map(({ row, col }) => ({ row, col })) : [{ row: legal.row, col: legal.col }]
        });
      });
      if (!moves.length) return { ok: false, score: 0 };
      if (!Array.isArray(boardState.pendingFreeMoves)) boardState.pendingFreeMoves = [];
      const triggerColor = opponent(color);
      boardState.pendingFreeMoves.push({
        id: `worker-premove-${card.instanceId || card.id || boardState.moveCount || 0}`,
        color,
        triggerColor,
        triggerTurn: (Number(boardState.turnsTaken?.[triggerColor]) || 0) + 1,
        moves
      });
      score += (color === aiColor ? 1 : -1) * (220 + moves.length * 90);
    } else if (card.effect === "icbm") {
      const sourceQueen = piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color))[0];
      const targetQueen = piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, opponent(color)))[0];
      if (!sourceQueen || !targetQueen) return { ok: false, score: 0 };
      if (!Array.isArray(boardState.pendingIcbm)) boardState.pendingIcbm = [];
      boardState.pendingIcbm.push({
        id: `worker-icbm-${card.instanceId || card.id || boardState.moveCount || 0}`,
        color,
        triggerTurn: (Number(boardState.turnsTaken?.[color]) || 0) + 1,
        sourceQueenId: sourceQueen.piece.id,
        targetQueenId: targetQueen.piece.id
      });
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "guard") {
      const pawnSquare = workerGuardPawnSquare(boardState, color);
      if (!pawnSquare) return { ok: false, score: 0 };
      clearWorkerPromotionTraits(pawnSquare.piece, boardState);
      pawnSquare.piece.type = "guard";
      pawnSquare.piece.moved = true;
      score += (color === aiColor ? 1 : -1) * 190;
    } else if (card.effect === "reaper") {
      const queenSquare = workerSelectedQueenSquare(boardState, color, action.target);
      if (!queenSquare) return { ok: false, score: 0 };
      clearWorkerPromotionTraits(queenSquare.piece, boardState);
      queenSquare.piece.type = "reaper";
      queenSquare.piece.reaperCaptures = 0;
      queenSquare.piece.moved = true;
      score += (color === aiColor ? 1 : -1) * 360;
    } else if (card.effect === "trojanHorse") {
      if (!target || target.color !== color || target.type !== "knight" || target.trojanHorse) return { ok: false, score: 0 };
      target.trojanHorse = true;
      score += (color === aiColor ? 1 : -1) * 280;
    } else if (card.effect === "madHorse") {
      if (!boardState.madHorse) boardState.madHorse = { white: false, black: false };
      boardState.madHorse[color] = true;
      score += (color === aiColor ? 1 : -1) * 250;
    } else if (card.effect === "freeze") {
      const candidates = freezeAvailability.candidates;
      const start = workerStableIndex(`${card.instanceId || card.id}:${boardState.moveCount || 0}`, candidates.length);
      const chosen = Array.from({ length: Math.min(3, candidates.length) }, (_, offset) => candidates[(start + offset) % candidates.length]);
      chosen.forEach(({ piece }) => {
        piece.frozen = true;
        piece.frozenByCard = !usesSeptember18Balance(boardState) ? { remaining: 3, source: color } : { remaining: 3, source: color, countBy: color };
      });
      score += (color === aiColor ? 1 : -1) * chosen.reduce((sum, entry) => sum + 120 + pieceValue(entry.piece) * 0.22, 0);
    } else if (card.effect === "sacrifice") {
      if (!target || !workerIsSacrificeTarget(boardState, target, color)) return { ok: false, score: 0 };
      const candidates = workerSacrificeProtectionCandidates(boardState, target, color);
      if (!candidates.length) return { ok: false, score: 0 };
      const protectedEntry = candidates[workerStableIndex(`${card.instanceId || card.id}:${boardState.moveCount || 0}`, candidates.length)];
      const sacrificedValue = pieceValue(target);
      clearPieceCells(boardState, target);
      recordWorkerCapturedPieces(boardState, opponent(color), [target]);
      protectedEntry.piece.sacrificeProtection = {
        remaining: 3,
        previousProtected: Boolean(protectedEntry.piece.protected),
        by: color
      };
      protectedEntry.piece.protected = true;
      const utility = 260 + pieceValue(protectedEntry.piece) * 0.7 - sacrificedValue * 0.35;
      score += (color === aiColor ? 1 : -1) * utility;
    } else if (card.effect === "clonePassive") {
      if (!boardState.clonePassive) boardState.clonePassive = { white: false, black: false };
      boardState.clonePassive[color] = true;
      score += (color === aiColor ? 1 : -1) * 620;
    } else if (card.effect === "joker") {
      const reusable = workerReusableActiveCards(boardState, color, card);
      const selected = reusable.find((usedCard) => usedCard.instanceId === action.target?.cardInstanceId);
      if (!selected) return { ok: false, score: 0 };
      selected.used = false;
      delete selected.usedAt;
      delete selected.recovering;
      delete selected.passiveApplied;
      score += (color === aiColor ? 1 : -1) * Math.max(240, cardValue(selected) * 0.8);
    } else if (card.effect === "vanish") {
      if (!boardState.vanishing) boardState.vanishing = { white: false, black: false };
      boardState.vanishing[color] = true;
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "moving") {
      boardState.moving = normalizeMovingState(boardState.moving);
      if (boardState.moving[color].enabled || !workerEvasionEligiblePieces(boardState, color, { requireUnmarked: false }).length) {
        return { ok: false, score: 0 };
      }
      boardState.moving[color] = { enabled: true, pieceId: "", count: 0 };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (card.effect === "promotionRush") {
      if (!isPromotionRushEligiblePiece(target, color)) return { ok: false, score: 0 };
      target.promotionRushUntil = (Number(boardState.turnsTaken?.[color]) || 0) + 1;
      if (!usesInternalSixFixes(boardState)) markWorkerCardNoCaptureThisTurn(boardState, target);
      score += (color === aiColor ? 1 : -1) * (210 + pieceValue(target) * 0.12);
    } else if (card.effect === "substitution") {
      if (!workerHasSubstitutionCandidate(boardState, color)) return { ok: false, score: 0 };
      if (!boardState.substitution) boardState.substitution = { white: false, black: false };
      boardState.substitution[color] = true;
      score += (color === aiColor ? 1 : -1) * 280;
    } else if (card.effect === "chain") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections : [];
      const unique = [];
      const seen = /* @__PURE__ */ new Set();
      selections.forEach(({ row, col }) => {
        const piece = get(boardState, row, col);
        if (!piece || piece.color !== opponent(color) || workerIsLargePiece(piece) || ["wall", "football", "blackHole"].includes(piece.type)) return;
        const key = piece.id || `${row}:${col}`;
        if (seen.has(key)) return;
        seen.add(key);
        unique.push({ piece, row, col });
      });
      if (unique.length !== 2 || !isWithinChainRange(unique[0], unique[1])) return { ok: false, score: 0 };
      const firstId = workerEnsureChainPieceId(boardState, unique[0].piece, unique[0].row, unique[0].col);
      const secondId = workerEnsureChainPieceId(boardState, unique[1].piece, unique[1].row, unique[1].col);
      const pairKey = [firstId, secondId].sort().join("\0");
      if (normalizeChainBonds(boardState.chainBonds).some((bond) => [bond.aId, bond.bId].sort().join("\0") === pairKey)) {
        return { ok: false, score: 0 };
      }
      boardState.chainBonds = normalizeChainBonds([
        ...boardState.chainBonds || [],
        {
          id: `chain-ai-${Number(boardState.moveCount) || 0}-${firstId}-${secondId}`,
          aId: firstId,
          bId: secondId,
          by: color
        }
      ]);
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "ghost") {
      const pawns = piecesMatching(boardState, (piece) => piece?.color === color && piece.type === "pawn" && !piece.ghost);
      if (!pawns.length) return { ok: false, score: 0 };
      pawns.forEach(({ piece }) => {
        piece.ghost = true;
      });
      score += (color === aiColor ? 1 : -1) * pawns.length * 190;
    } else if (card.effect === "cornerKick") {
      if (!workerCountPiecesOf(boardState, color, "knight") || boardState.cornerKick?.[color]) return { ok: false, score: 0 };
      if (!boardState.cornerKick) boardState.cornerKick = { white: false, black: false };
      boardState.cornerKick[color] = true;
      score += (color === aiColor ? 1 : -1) * 360;
    } else if (card.effect === "conversion") {
      const knights = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight");
      if (!knights.length) return { ok: false, score: 0 };
      knights.forEach(({ piece }) => {
        piece.type = "bishop";
        piece.moved = true;
      });
      score += (color === aiColor ? 1 : -1) * knights.length * 70;
    } else if (card.effect === "dutch") {
      let changed = 0;
      forEachPiece(boardState, (piece) => {
        if (!isDutchTransformPiece(piece, color)) return;
        piece.type = "windmill";
        piece.windmillMode = "bishop";
        piece.moved = true;
        changed += 1;
      });
      score += (color === aiColor ? 1 : -1) * changed * 120;
    } else if (card.effect === "binaMate") {
      if (!boardState.binaMate) boardState.binaMate = { white: false, black: false };
      boardState.binaMate[color] = true;
      score += (color === aiColor ? 1 : -1) * 620;
    } else if (card.effect === "overwhelm") {
      if (!boardState.overwhelm) boardState.overwhelm = { white: false, black: false };
      boardState.overwhelm[color] = true;
      score += (color === aiColor ? 1 : -1) * 580;
    } else if (card.effect === "quantumMechanics") {
      if (!boardState.quantumPending) boardState.quantumPending = { white: false, black: false };
      boardState.quantumPending[color] = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "coronation") {
      if (!boardState.coronation) boardState.coronation = { white: false, black: false };
      boardState.coronation[color] = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (INTERNAL_THREE_IDS.includes(card.id)) {
      const result = applyThreeLocalCard(boardState, card, action.target, color, { isRoyal: (item) => isWorkerKingRole(boardState, item), isRanged: (item) => workerIsIceSheetRangedPiece(boardState, item) });
      if (!result.ok) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (INTERNAL_FIVE_IDS.includes(card.id)) {
      const result = applyFiveLocalCard(boardState, card, action.target, color, { isRoyal: (item) => isWorkerKingRole(boardState, item) });
      if (!result.ok) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (INTERNAL_EIGHT_EFFECTS.includes(card.effect)) {
      const result = applyWorkerInternalEightCard(boardState, card, action.target, color);
      if (!result.ok) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (SEPTEMBER_PASSIVE_EFFECTS.includes(card.effect) && card.effect !== "bigBishop") {
      boardState[card.effect] = { ...boardState[card.effect] || {}, [color]: true };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (card.effect === "fianchetto") {
      if (!workerCountPiecesOf(boardState, color, "bishop") || boardState.fianchetto?.[color]) return { ok: false, score: 0 };
      if (!boardState.fianchetto) boardState.fianchetto = { white: false, black: false };
      boardState.fianchetto[color] = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "pawnConversion") {
      if (!workerCountPiecesOf(boardState, color, "pawn") || boardState.pawnConversion?.[color]) return { ok: false, score: 0 };
      if (!boardState.pawnConversion) boardState.pawnConversion = { white: false, black: false };
      boardState.pawnConversion[color] = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "ruleTicket") {
      const ruleId = action.target?.ruleId;
      const rule = workerRuleTicketCardPool(boardState).find((entry) => entry.id === ruleId);
      if (!rule) return { ok: false, score: 0 };
      if (!Array.isArray(boardState.pendingRuleTickets)) boardState.pendingRuleTickets = [];
      boardState.pendingRuleTickets.push({
        id: `worker-rule-ticket-${card.instanceId || card.id || "card"}`,
        ruleId: rule.id,
        color,
        startTurnCount: Math.max(0, Number(boardState.turnsTaken?.[color]) || 0)
      });
      score += (color === aiColor ? 1 : -1) * Math.max(180, workerRuleTicketScore(rule, color) * 0.45);
    } else if (card.effect === "insight") {
      const removed = applyWorkerInsight(boardState, color);
      if (removed) score += (color === aiColor ? 1 : -1) * (180 + removed * 70);
    } else if (card.effect === "prophecy") {
      const royal = workerFindProphecyRoyal(boardState, color);
      if (!royal || boardState.prophecy?.[color]) return { ok: false, score: 0 };
      if (!boardState.prophecy) boardState.prophecy = { white: null, black: null };
      boardState.prophecy[color] = {
        by: color,
        remainingHalfTurns: 6
      };
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "holdout") {
      if (!target || target.color !== color || target.type !== "pawn" || target.holdoutPromotion) return { ok: false, score: 0 };
      target.holdoutPromotion = { by: color, readyTurn: sharedTurnCount(boardState) + 14 };
      score += (color === aiColor ? 1 : -1) * 300;
    } else if (card.effect === "chimera") {
      if (!target || target.color !== color || target.chimera || !["knight", "bishop"].includes(target.type)) return { ok: false, score: 0 };
      target.chimera = true;
      score += (color === aiColor ? 1 : -1) * (240 + pieceValue(target) * 0.18);
    } else if (card.effect === "fileSurge") {
      if (!boardState.fileSurge) boardState.fileSurge = { white: false, black: false };
      boardState.fileSurge[color] = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "rookLift") {
      if (!boardState.rookLift) boardState.rookLift = { white: false, black: false };
      boardState.rookLift[color] = true;
      score += (color === aiColor ? 1 : -1) * 280;
    } else if (card.effect === "underpromotion") {
      if (!boardState.underpromotion) boardState.underpromotion = { white: false, black: false };
      boardState.underpromotion[color] = true;
      score += (color === aiColor ? 1 : -1) * 240;
    } else if (card.effect === "recycling") {
      boardState.recycling = true;
      score += (color === aiColor ? 1 : -1) * 220;
    } else if (card.effect === "finalWeapon") {
      if (!boardState.finalWeapon) boardState.finalWeapon = { white: false, black: false };
      boardState.finalWeapon[color] = true;
      score += (color === aiColor ? 1 : -1) * 340;
    } else if (card.effect === "stake") {
      if (!target || target.color !== color || target.staked || ["wall", "football", "colossus"].includes(target.type)) return { ok: false, score: 0 };
      target.staked = { by: color, remaining: 4 };
      score += (color === aiColor ? 1 : -1) * (180 + pieceValue(target) * 0.12);
    } else if (card.effect === "localConscription") {
      if (!isWorkerQueenIdentity(target, color)) return { ok: false, score: 0 };
      target.type = "recruiter";
      target.moved = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "canceling") {
      const enemy = opponent(color);
      if (!applyWorkerCanceling(boardState, enemy, color, aiColor)) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "breakthroughOrder") {
      if (!boardState.breakthroughPawns) boardState.breakthroughPawns = { white: false, black: false };
      boardState.breakthroughPawns[color] = true;
      score += (color === aiColor ? 1 : -1) * 220;
    } else if (card.effect === "charge") {
      if (!target || !workerCanChargePawn(boardState, target, action.target?.row, action.target?.col, color)) return { ok: false, score: 0 };
      clearWorkerChargeRush(boardState, color);
      target.chargeRush = true;
      score += (color === aiColor ? 1 : -1) * (240 + Math.max(0, pieceValue(target) * 0.1));
    } else if (card.effect === "enPassantBang") {
      if (workerCountPiecesOf(boardState, color, "pawn") <= 0) return { ok: false, score: 0 };
      if (!boardState.enPassantFrenzy) boardState.enPassantFrenzy = { white: false, black: false };
      boardState.enPassantFrenzy[color] = true;
      score += (color === aiColor ? 1 : -1) * 220;
    } else if (card.effect === "taunt") {
      const enemy = opponent(color);
      if (!workerHasTauntCandidate(boardState, color)) return { ok: false, score: 0 };
      if (!boardState.taunt) boardState.taunt = { white: 0, black: 0 };
      boardState.taunt[enemy] = Math.max(Number(boardState.taunt[enemy]) || 0, 1);
      score += (color === aiColor ? 1 : -1) * 160;
    } else if (card.effect === "basicTraining") {
      if (!isWorkerBasicTrainingEligible(target, color)) return { ok: false, score: 0 };
      target.basicTraining = true;
      score += (color === aiColor ? 1 : -1) * (180 + pieceValue(target) * 0.08);
    } else if (card.effect === "hallucination") {
      const enemy = opponent(color);
      if (!boardState.hallucination) boardState.hallucination = { white: null, black: null };
      boardState.hallucination[enemy] = { color, remaining: 5 };
      score += (color === aiColor ? 1 : -1) * 80;
    } else if (card.effect === "blueJeans") {
      const result = applyWorkerBlueJeans(boardState, color, aiColor, score);
      if (!result.ok) return result;
      score = result.score;
    } else if (card.effect === "severance") {
      if (!target || target.color !== opponent(color) || isWorkerRoyalIdentityPiece(boardState, target) || !workerIsIceSheetRangedPiece(boardState, target)) return { ok: false, score: 0 };
      target.severed = { remaining: 2, expiresFullMove: (Number(boardState.fullMove) || 1) + 2 };
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 0.24;
    } else if (card.effect === "inertia") {
      if (!target || target.color !== opponent(color) || isWorkerRoyalIdentityPiece(boardState, target) || !workerIsIceSheetRangedPiece(boardState, target)) return { ok: false, score: 0 };
      target.inertia = true;
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 0.3;
    } else if (card.effect === "iceSheet") {
      const targets = piecesMatching(boardState, (piece) => piece.color === opponent(color) && workerIsIceSheetEffectTarget(boardState, piece));
      if (!targets.length) return { ok: false, score: 0 };
      targets.forEach(({ piece }) => {
        piece.iceSheet = { by: color, remaining: 3 };
      });
      const value = targets.reduce((sum, entry) => sum + pieceValue(entry.piece), 0);
      score += (color === aiColor ? 1 : -1) * Math.min(720, value * 0.22);
    } else if (card.effect === "emergencyEvacuation") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections.slice(0, 3) : [];
      const seen = /* @__PURE__ */ new Set();
      const pieces = [];
      selections.forEach(({ row, col }) => {
        const key = `${row}:${col}`;
        if (seen.has(key)) return;
        seen.add(key);
        const item = get(boardState, row, col);
        const nextRow = row - pawnDir(boardState, color);
        if (!item || item.color !== color || ["wall", "football", "colossus"].includes(item.type) || isWorkerUndergroundBunkerKing(item)) return;
        if (!inBounds(nextRow, col, boardState) || get(boardState, nextRow, col)) return;
        if (isWorkerFianchettoBlockedMove(boardState, item, row, col, { row: nextRow, col })) return;
        pieces.push({ piece: item, row, col, nextRow });
      });
      if (!pieces.length) return { ok: false, score: 0 };
      pieces.forEach(({ piece, row, col, nextRow }) => {
        set(boardState, row, col, null);
        set(boardState, nextRow, col, piece);
        piece.moved = true;
        markWorkerCardNoCaptureThisTurn(boardState, piece);
        noteWorkerUltimatumMovement(boardState, piece);
      });
      score += (color === aiColor ? 1 : -1) * (120 + pieces.length * 55);
    } else if (card.effect === "panic") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections.slice(0, 2) : [];
      const pieces = [];
      selections.forEach(({ row, col }) => {
        const item = get(boardState, row, col);
        if (workerIsPanicTargetCandidate(boardState, item, color)) pieces.push({ id: item.id || "", row, col });
      });
      if (pieces.length !== 2) return { ok: false, score: 0 };
      if (!Array.isArray(boardState.pendingPanic)) boardState.pendingPanic = [];
      boardState.pendingPanic.push({ color: opponent(color), by: color, pieces });
      score += (color === aiColor ? 1 : -1) * pieces.reduce((sum, ref) => {
        const item = get(boardState, ref.row, ref.col);
        return sum + pieceValue(item) * 0.18;
      }, 220);
    } else if (card.effect === "spy") {
      const selectedTargets = Array.isArray(action.target?.selections) ? action.target.selections : Number.isInteger(action.target?.row) && Number.isInteger(action.target?.col) ? [{ row: action.target.row, col: action.target.col }] : [];
      const enemyPawns = piecesMatching(boardState, (piece) => piece.color === opponent(color) && piece.type === "pawn").length;
      const required = Math.min(2, enemyPawns);
      if (required <= 0) return { ok: false, score: 0 };
      const seen = /* @__PURE__ */ new Set();
      const pawns = [];
      selectedTargets.forEach(({ row, col }) => {
        const key = `${row}:${col}`;
        if (seen.has(key)) return;
        seen.add(key);
        const item = get(boardState, row, col);
        if (item && item.color === opponent(color) && item.type === "pawn") pawns.push(item);
      });
      if (pawns.length !== required) return { ok: false, score: 0 };
      pawns.forEach((pawn) => {
        pawn.spyOwner = color;
      });
      score += (color === aiColor ? 1 : -1) * (160 + pawns.length * 90);
    } else if (card.effect === "trolley") {
      const enemy = opponent(color);
      const pair = workerFindTrolleyBundlePair(boardState, enemy);
      if (!pair) return { ok: false, score: 0 };
      if (!Array.isArray(boardState.pendingTrolley)) boardState.pendingTrolley = [];
      boardState.pendingTrolley.push({ color: enemy, by: color });
      const lowerValue = Math.min(pair[0].value, pair[1].value);
      score += (color === aiColor ? 1 : -1) * (320 + lowerValue * 90);
    } else if (card.effect === "pawnStorm") {
      const selections = Array.isArray(action.target?.selections) ? action.target.selections : [];
      const seen = /* @__PURE__ */ new Set();
      const pieces = [];
      selections.forEach(({ row, col }) => {
        const key = `${row}:${col}`;
        if (seen.has(key)) return;
        seen.add(key);
        const item = get(boardState, row, col);
        if (workerCanPawnStormAdvance(boardState, item, row, col, color)) pieces.push({ id: item.id || "", row, col });
      });
      if (!pieces.length) return { ok: false, score: 0 };
      const dir = workerPawnStormDirection(boardState, color);
      const ordered = pieces.map((pieceRef) => findWorkerPieceByRef(boardState, pieceRef)).filter((square) => square && workerCanPawnStormAdvance(boardState, square.piece, square.row, square.col, color)).sort((a, b) => (dir < 0 ? a.row - b.row : b.row - a.row) || a.col - b.col);
      let moved = 0;
      ordered.forEach(({ piece, row, col }) => {
        if (!workerCanPawnStormAdvance(boardState, piece, row, col, color)) return;
        const nextRow = row + dir;
        set(boardState, row, col, null);
        set(boardState, nextRow, col, piece);
        piece.moved = true;
        markWorkerCardNoCaptureThisTurn(boardState, piece);
        noteWorkerUltimatumMovement(boardState, piece);
        if (isPromotionRow(piece, nextRow, boardState)) workerAutoPromotePawnStormPiece(boardState, piece, nextRow);
        moved += 1;
      });
      if (!moved) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * (180 + moved * 70);
    } else if (card.effect === "queenAfterimage") {
      if (!piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color)).length) return { ok: false, score: 0 };
      if (!boardState.afterimageQueen) boardState.afterimageQueen = { white: false, black: false };
      boardState.afterimageQueen[color] = true;
      score += (color === aiColor ? 1 : -1) * 260;
    } else if (card.effect === "queensGambit") {
      const queenCol = Number(action.target?.col);
      if (!isWorkerQueensGambitQueen(boardState, target, color) || !Number.isInteger(queenCol)) return { ok: false, score: 0 };
      const pawnFiles = [];
      forEachPiece(boardState, (piece, row, col) => {
        if (piece?.color === color && piece.type === "pawn") pawnFiles.push(col);
      });
      const randomFiles = queensGambitRandomFileCandidates(boardColCount(boardState), queenCol, pawnFiles);
      const randomCol = randomFiles.length ? randomFiles[Math.abs((Number(boardState.moveCount) || 0) + queenCol) % randomFiles.length] : null;
      if (!Number.isInteger(randomCol)) return { ok: false, score: 0 };
      const pawns = workerQueensGambitPawns(boardState, color, queenCol, randomCol);
      clearPieceCells(boardState, target);
      recordWorkerCapturedPieces(boardState, opponent(color), [target]);
      if (!boardState.queensGambitFiles) boardState.queensGambitFiles = { white: null, black: null };
      boardState.queensGambitFiles[color] = { queenCol, randomCol };
      pawns.forEach(({ piece }) => {
        piece.queensGambitPreviousProtected = Boolean(piece.protected);
        piece.protected = true;
        piece.queensGambitProtection = true;
      });
      score += (color === aiColor ? 1 : -1) * (360 + pawns.length * 150);
    } else if (card.effect === "martyrdom") {
      const bishops = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "bishop");
      const pawns = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn");
      if (!bishops.length || !pawns.length) return { ok: false, score: 0 };
      bishops.forEach(({ row, col }) => set(boardState, row, col, null));
      pawns.forEach(({ piece }) => {
        piece.shielded = true;
      });
      score += (color === aiColor ? 1 : -1) * Math.max(80, pawns.length * 70 - bishops.length * 160);
    } else if (card.effect === "whiteBox" || card.effect === "blackBox") {
      card.boxRevealedCardId = card.effect === "whiteBox" ? "random-passive" : "random-active";
      score += (color === aiColor ? 1 : -1) * 360;
    } else if (card.effect === "deathSquad") {
      if (!action.target || !Number.isInteger(action.target.col)) return { ok: false, score: 0 };
      let changed = 0;
      for (let row = 0; row < boardRowCount(boardState); row += 1) {
        const item = get(boardState, row, action.target.col);
        if (!item || item.color !== color || item.type !== "pawn") continue;
        item.type = "fanatic";
        item.moved = true;
        changed += 1;
      }
      if (!changed) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * changed * 90;
    } else if (card.effect === "chameleonMutation") {
      const requested = Array.isArray(action.target?.selections) ? action.target.selections.slice(0, 3) : Number.isInteger(action.target?.row) && Number.isInteger(action.target?.col) ? [action.target] : [];
      const seen = /* @__PURE__ */ new Set();
      const selected = [];
      requested.forEach(({ row, col }) => {
        const item = get(boardState, row, col);
        if (!item || item.color !== color || isWorkerRoyalIdentityPiece(boardState, item) || ["merchant", "wall", "colossus", "bigRook", "bigBishop"].includes(item.type) || seen.has(item.id)) return;
        seen.add(item.id);
        selected.push(item);
      });
      if (!selected.length) return { ok: false, score: 0 };
      selected.forEach((item) => {
        item.chameleon = true;
      });
      score += (color === aiColor ? 1 : -1) * selected.reduce((sum, item) => sum + 90 + pieceValue(item) * 0.08, 0);
    } else if (card.effect === "desperado") {
      if (!target || target.color !== color || isCritical(target) || isWorkerRegencyRoyalHeir(boardState, target) || ["merchant", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(target.type) || isFrozenPiece(target) || isWorkerStakedPiece(target)) {
        return { ok: false, score: 0 };
      }
      target.desperado = { remaining: 2 };
      score += (color === aiColor ? 1 : -1) * Math.max(220, pieceValue(target) * 0.18);
    } else if (card.effect === "royalCommand") {
      if (!boardState.royalCommand) boardState.royalCommand = { white: null, black: null };
      boardState.royalCommand[color] = createRoyalCommandWindow(boardState.turnsTaken?.[color]);
      score += color === aiColor ? 520 : -520;
    } else if (card.effect === "reposition") {
      clearWorkerRepositionMarks(boardState, color);
      forEachPiece(boardState, (piece) => {
        if (piece.color === color && !["wall", "football"].includes(piece.type)) piece.repositionSecondMove = { used: false };
      });
      score += color === aiColor ? 160 : -160;
    } else if (card.effect === "apprenticeKnights") {
      const changed = applyWorkerApprenticeKnights(boardState, color);
      if (!changed) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * 180;
    } else if (card.effect === "checker") {
      const changed = applyWorkerChecker(boardState, color);
      if (!changed) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * changed * 120;
    } else if (card.effect === "ultimatum") {
      boardState.ultimatum = {
        by: color,
        remaining: 4,
        remainingHalfTurns: ULTIMATUM_DURATION_HALF_TURNS,
        expiresFullMove: null,
        movedIds: []
      };
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "royalShield") {
      const targetEntry = action.target ? { piece: target } : strongestFriendly(boardState, color);
      const shieldTarget = targetEntry?.piece || target;
      if (!shieldTarget || shieldTarget.color !== color || shieldTarget.type === "scarecrow") return { ok: false, score: 0 };
      shieldTarget.shielded = true;
      score += (color === aiColor ? 1 : -1) * (220 + pieceValue(shieldTarget) * 0.18);
    } else if (card.effect === "undergroundBunker") {
      const king = piecesMatching(boardState, (piece) => piece.color === color && isWorkerKingRole(boardState, piece) && !piece.undergroundBunker)[0];
      if (!king) return { ok: false, score: 0 };
      king.piece.undergroundBunker = true;
      king.piece.hp = 5;
      king.piece.maxHp = 5;
      score += (color === aiColor ? 1 : -1) * 520;
    } else if (card.effect === "encouragement") {
      const protectTarget = target || strongestFriendly(boardState, color)?.piece;
      if (!protectTarget || protectTarget.color !== color) return { ok: false, score: 0 };
      protectTarget.protected = true;
      score += (color === aiColor ? 1 : -1) * (180 + pieceValue(protectTarget) * 0.16);
    } else if (card.effect === "freeCastling") {
      const plan = workerImmediateCastlingPlans(boardState, color).find(({ rook }) => rook.row === action.target?.row && rook.col === action.target?.col);
      if (!plan) return { ok: false, score: 0 };
      set(boardState, plan.king.row, plan.king.col, null);
      set(boardState, plan.rook.row, plan.rook.col, null);
      [
        { ...plan.kingTo, attacker: plan.king.piece },
        { ...plan.rookTo, attacker: plan.rook.piece }
      ].forEach((destination) => {
        const crushed = get(boardState, destination.row, destination.col);
        if (!crushed) return;
        const captureOwner = crushed.color === color ? opponent(color) : color;
        clearPieceCells(boardState, crushed);
        score += workerCriticalCaptureScore(boardState, crushed, captureOwner, aiColor);
        recordWorkerCapturedPieces(boardState, captureOwner, [crushed], destination.attacker, {
          attackerLanding: { row: destination.row, col: destination.col }
        });
        workerLearnImperialStudyMovement(boardState, plan.king.piece, [crushed]);
        const materialDirection = captureOwner === aiColor ? 1 : -1;
        score += materialDirection * pieceValue(crushed) * 0.8;
      });
      set(boardState, plan.kingTo.row, plan.kingTo.col, plan.king.piece);
      set(boardState, plan.rookTo.row, plan.rookTo.col, plan.rook.piece);
      plan.king.piece.moved = true;
      plan.rook.piece.moved = true;
      trackWorkerMovingProgress(boardState, plan.king.piece);
      if (!boardState.castled) boardState.castled = { white: false, black: false };
      boardState.castled[color] = true;
      if (boardState.freeCastling) boardState.freeCastling[color] = false;
      if (boardState.zugzwang) boardState.zugzwang[color] = false;
      boardState.enPassant = null;
      noteWorkerUltimatumMovement(boardState, plan.king.piece);
      noteWorkerUltimatumMovement(boardState, plan.rook.piece);
      pushRecentMove(boardState, {
        from: { row: plan.king.row, col: plan.king.col },
        to: { row: plan.kingTo.row, col: plan.kingTo.col },
        pieceId: plan.king.piece.id || "",
        pieceType: plan.king.piece.type || "",
        color
      });
      score += (color === aiColor ? 1 : -1) * 330;
    } else if (card.effect === "injury") {
      if (!boardState.knightInjury) boardState.knightInjury = {};
      boardState.knightInjury[opponent(color)] = true;
      score += (color === aiColor ? 1 : -1) * 360;
    } else if (card.effect === "switcheroo") {
      if (!workerHasSwitcherooCandidate(boardState, color)) return { ok: false, score: 0 };
      if (!boardState.switcheroo) boardState.switcheroo = { white: false, black: false };
      boardState.switcheroo[color] = true;
      score += (color === aiColor ? 1 : -1) * 240;
    } else if (card.effect === "cleanupSacrifice") {
      if (!target || !workerIsCleanupSacrificeTarget(boardState, target, color)) return { ok: false, score: 0 };
      const victim = workerCleanupSacrificeEnemyCandidates(boardState, color, target.type).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece) || a.row - b.row || a.col - b.col)[0];
      if (!victim) return { ok: false, score: 0 };
      clearPieceCells(boardState, target);
      recordWorkerCapturedPieces(boardState, opponent(color), [target]);
      clearPieceCells(boardState, victim.piece);
      recordWorkerCapturedPieces(boardState, color, [victim.piece]);
      score += workerCriticalCaptureScore(boardState, victim.piece, color, aiColor);
      if (boardState.mode !== "gameover") {
        score += workerCriticalCaptureScore(boardState, target, opponent(color), aiColor);
      }
      score += (color === aiColor ? 1 : -1) * (pieceValue(victim.piece) * 1.1 - pieceValue(target) * 0.75);
    } else if (card.effect === "bishopSnipe") {
      if (!boardState.bishopSnipe) boardState.bishopSnipe = {};
      boardState.bishopSnipe[color] = true;
      score += (color === aiColor ? 1 : -1) * 240;
    } else if (card.effect === "retreat") {
      if (!boardState.retreat) boardState.retreat = {};
      boardState.retreat[color] = true;
      score += (color === aiColor ? 1 : -1) * 180;
    } else if (card.effect === "socialism") {
      if (!boardState.socialism) boardState.socialism = {};
      boardState.socialism[opponent(color)] = Math.max(Number(boardState.socialism[opponent(color)]) || 0, 1);
      score += (color === aiColor ? 1 : -1) * 420;
    } else if (card.effect === "dice") {
      const lock = bestWorkerDiceLock(boardState, opponent(color));
      if (!lock) return { ok: false, score: 0 };
      if (!boardState.diceLocks) boardState.diceLocks = {};
      boardState.diceLocks[opponent(color)] = { type: lock.type, remaining: 2 };
      score += (color === aiColor ? 1 : -1) * (260 + lock.value * 0.28);
    } else if (card.effect === "disarm") {
      if (!target || target.color !== opponent(color) || isWorkerRoyalIdentityPiece(boardState, target)) return { ok: false, score: 0 };
      target.disarmed = { by: color, remaining: 1 };
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 0.34;
    } else if (card.effect === "witchTrial") {
      if (!target || target.color !== opponent(color) || target.witchTrial || isWorkerRoyalIdentityPiece(boardState, target) || ["vip", "merchant", "wall", "football", "colossus", "bigRook", "bigBishop"].includes(target.type)) {
        return { ok: false, score: 0 };
      }
      const trialScore = workerWitchTrialTargetScore(boardState, target, action.target?.row, action.target?.col, color);
      target.witchTrial = !usesSeptember18Balance(boardState) ? { by: color, remaining: 3 } : { by: color, countBy: color, remaining: 3 };
      score += (color === aiColor ? 1 : -1) * trialScore;
    } else if (card.effect === "suicideBomber") {
      if (!target || target.color !== color || !["pawn", "fanatic"].includes(target.type) || target.explosive || target.feudalContractId) {
        return { ok: false, score: 0 };
      }
      const bomberScore = workerSuicideBomberTargetScore(boardState, target, action.target?.row, action.target?.col, color);
      target.explosive = true;
      score += (color === aiColor ? 1 : -1) * bomberScore;
    } else if (card.effect === "exile") {
      if (!target || target.color !== opponent(color) || isWorkerRoyalIdentityPiece(boardState, target) || ["wall", "football", "colossus"].includes(target.type)) {
        return { ok: false, score: 0 };
      }
      const origin = parseWorkerSquare(boardState, target.origin || workerSquareName(boardState, action.target.row, action.target.col));
      if (!inBounds(origin.row, origin.col, boardState) || get(boardState, origin.row, origin.col)) return { ok: false, score: 0 };
      set(boardState, action.target.row, action.target.col, null);
      set(boardState, origin.row, origin.col, target);
      target.moved = true;
      if (isWorkerCollapsedSquare(boardState, origin.row, origin.col)) {
        forceWorkerRemovePieceAt(boardState, origin.row, origin.col, color, aiColor);
        score += (color === aiColor ? 1 : -1) * pieceValue(target) * 1.2;
      } else {
        score += (color === aiColor ? 1 : -1) * 160;
      }
    } else if (["londonSystem", "horde"].includes(card.effect)) {
      const homeRow = color === "white" ? boardRowCount(boardState) - 1 : 0;
      const homeCol = Math.min(boardColCount(boardState) - 1, Math.floor(boardColCount(boardState) / 2));
      if (isWorkerCollapsedSquare(boardState, homeRow, homeCol)) {
        boardState.mode = "gameover";
        boardState.winner = opponent(color);
        score += color === aiColor ? -1e5 : 1e5;
      } else {
        score += (color === aiColor ? 1 : -1) * (card.effect === "horde" ? 420 : 260);
      }
    } else if (card.effect === "traitor") {
      const victim = weakestEnemyPiece(boardState, color);
      if (!victim) return { ok: false, score: 0 };
      const converted = { ...victim.piece };
      victim.piece.color = color;
      victim.piece.moved = true;
      score += workerCriticalCaptureScore(boardState, converted, color, aiColor);
      score += (color === aiColor ? 1 : -1) * pieceValue(victim.piece) * 1.5;
    } else if (card.effect === "othello" && usesSeptember18Balance(boardState)) {
      boardState.othelloPending ||= { white: false, black: false };
      boardState.othelloPending[color] = true;
      resolveWorkerOthelloAll(boardState, color);
    } else if (card.effect === "othello") {
      if (!target || !workerIsOthelloTarget(boardState, target, action.target.row, action.target.col, color)) return { ok: false, score: 0 };
      const converted = { ...target };
      target.color = color;
      target.moved = true;
      score += (color === aiColor ? 1 : -1) * (pieceValue(converted) * 1.2 + 180);
      if (isWorkerDecisiveCaptureTarget(boardState, converted)) {
        score += workerCriticalCaptureScore(boardState, converted, color, aiColor);
      }
    } else if (card.effect === "horseRiding") {
      const kingRole = findWorkerKingRole(boardState, color);
      const knights = piecesMatching(
        boardState,
        (piece) => piece.color === color && piece.type === "knight"
      );
      knights.forEach(({ piece }) => clearPieceCells(boardState, piece));
      if (kingRole?.piece?.type === "royalKnight") {
        if (!boardState.royalKnightKing) boardState.royalKnightKing = {};
        boardState.royalKnightKing[color] = true;
      } else {
        if (!boardState.kingKnight) boardState.kingKnight = {};
        boardState.kingKnight[color] = true;
      }
      score += (color === aiColor ? 1 : -1) * (260 - knights.length * 300);
    } else if (card.effect === "eagle") {
      const knights = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "knight");
      if (!knights.length) return { ok: false, score: 0 };
      knights.forEach(({ piece, row, col }) => {
        piece.type = "eagle";
        piece.moved = true;
        piece.origin = workerSquareName(boardState, row, col);
        markWorkerFreshNoCapture(boardState, piece);
        if (boardState.monochromeChess && !piece.monoShade) piece.monoShade = squareShade(row, col);
      });
      let added = 0;
      for (const origin of knights) {
        const targetSquare = workerAlibabaPlacementCandidates(boardState, origin, color)[0];
        if (!targetSquare) break;
        const summoned = workerCreatePiece(color, "eagle", boardState, targetSquare.row, targetSquare.col);
        summoned.origin = workerSquareName(boardState, targetSquare.row, targetSquare.col);
        markWorkerFreshNoCapture(boardState, summoned);
        set(boardState, targetSquare.row, targetSquare.col, summoned);
        added += 1;
      }
      score += (color === aiColor ? 1 : -1) * (knights.length * 35 + added * 300);
    } else if (card.effect === "knightmate") {
      let changed = 0;
      forEachPiece(boardState, (piece) => {
        if (piece.color !== color) return;
        if (isWorkerKingRole(boardState, piece)) {
          piece.type = "royalKnight";
          changed += 1;
        } else if (piece.type === "knight") {
          piece.type = "man";
          changed += 1;
        }
      });
      if (!changed) return { ok: false, score: 0 };
      if (boardState.kingKnight?.[color]) {
        if (!boardState.royalKnightKing) boardState.royalKnightKing = {};
        boardState.royalKnightKing[color] = true;
      }
      score += (color === aiColor ? 1 : -1) * 280;
    } else if (card.effect === "reformation") {
      let changed = 0;
      forEachPiece(boardState, (piece) => {
        if (piece.color === color && piece.type === "bishop") {
          piece.type = "protestant";
          changed += 1;
        }
      });
      if (!changed) return { ok: false, score: 0 };
      score += (color === aiColor ? 1 : -1) * changed * 90;
    } else if (card.effect === "fanaticalRitual") {
      const candidates = piecesMatching(boardState, (piece) => piece.color === opponent(color) && !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "colossus"].includes(piece.type)).sort((a, b) => pieceValue(a.piece) - pieceValue(b.piece));
      const chosen = candidates[0];
      if (!chosen) return { ok: false, score: 0 };
      const previousValue = pieceValue(chosen.piece);
      chosen.piece.type = "fanatic";
      chosen.piece.moved = true;
      score += (color === aiColor ? 1 : -1) * Math.max(180, previousValue * 0.35);
    } else if (card.effect === "vortex") {
      score += (color === aiColor ? 1 : -1) * Math.min(500, workerVortexCandidateCount(boardState, color) * 55);
    }
    if (target?.color === opponent(color) && ["snipe", "assassinate", "banish"].some((key) => String(card.effect || card.id).includes(key))) {
      set(boardState, action.target.row, action.target.col, null);
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 1.4;
    } else if (target?.color === color && !["cleanupSacrifice", "randomRoulette", "queensGambit", "submerge", "homecoming", "idol", "twins", "judgment", "suicideBomber", "othello"].includes(card.effect)) {
      if (card.effect === "amazon") {
        const knightRow = Number(action.target?.knight?.row);
        const knightCol = Number(action.target?.knight?.col);
        const sacrifice = Number.isInteger(knightRow) && Number.isInteger(knightCol) ? get(boardState, knightRow, knightCol) : null;
        if (!sacrifice || sacrifice.color !== color || sacrifice.type !== "knight") return { ok: false, score: 0 };
        clearPieceCells(boardState, sacrifice);
        cancelWorkerPropheciesByCapture(boardState);
        recordWorkerCapturedPieces(boardState, opponent(color), [sacrifice]);
        score += (color === aiColor ? -1 : 1) * pieceValue(sacrifice);
      }
      if (card.effect === "ordination") {
        const sacrificeSquare = piecesMatching(boardState, (piece) => piece !== target && piece.color === color && piece.type === "bishop").sort((a, b) => pieceValue(a.piece) - pieceValue(b.piece))[0];
        if (!sacrificeSquare) return { ok: false, score: 0 };
        clearPieceCells(boardState, sacrificeSquare.piece);
        cancelWorkerPropheciesByCapture(boardState);
        recordWorkerCapturedPieces(boardState, opponent(color), [sacrificeSquare.piece]);
        score += resolveWorkerReaperNearbyDeaths(
          boardState,
          [{ item: sacrificeSquare.piece, row: sacrificeSquare.row, col: sacrificeSquare.col }],
          aiColor
        );
        score += (color === aiColor ? -1 : 1) * pieceValue(sacrificeSquare.piece);
      }
      score += (color === aiColor ? 1 : -1) * pieceValue(target) * 0.18;
      applyCardTransform(boardState, target, card, action.target.row, action.target.col);
    }
    if (boardState.mode !== "gameover") resolveWorkerDemocracyDefeat(boardState);
    resolveWorkerSubmergedPieces(boardState);
    const doubleCheckWinner = boardState.mode === "gameover" ? null : resolveWorkerDoubleCheckThreats(boardState, color);
    if (doubleCheckWinner) score += doubleCheckWinner === aiColor ? 1e5 : -1e5;
    const objectiveWinner = boardState.mode === "gameover" ? null : resolveWorkerSpecialObjectiveVictories(boardState, color);
    if (["white", "black"].includes(objectiveWinner) && objectiveWinner !== doubleCheckWinner) score += objectiveWinner === aiColor ? 1e5 : -1e5;
    if (!boardState.aiSearchBonus) boardState.aiSearchBonus = { white: 0, black: 0 };
    const colorBenefit = color === aiColor ? score : -score;
    if (colorBenefit > 0) {
      boardState.aiSearchBonus[color] = (boardState.aiSearchBonus[color] || 0) + colorBenefit * 0.35;
    }
    if (["trolley", "freeMove", "miracle"].includes(card.effect) && boardState.mode !== "gameover") {
      forceWorkerTurnEnd(boardState, color);
    } else {
      boardState.turn = color;
    }
    return { ok: true, score };
  }
  function applyWorkerMongolianGambit(boardState, color, aiColor, baseScore) {
    let changed = 0;
    const gambitType = monochromePieceType("knight", boardState.monochromeChess);
    const targets = piecesMatching(boardState, (piece) => piece.color === color && !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "football"].includes(piece.type));
    targets.forEach(({ piece, row, col }) => {
      const nextType = gambitType;
      if (["bigRook", "bigBishop"].includes(piece.type)) {
        const anchorRow = Number.isInteger(piece.anchorRow) ? piece.anchorRow : row;
        const anchorCol = Number.isInteger(piece.anchorCol) ? piece.anchorCol : col;
        const footprint = colossusCells(anchorRow, anchorCol);
        if (footprint.length === 4) {
          clearPieceCells(boardState, piece);
          footprint.forEach((cell) => {
            set(boardState, cell.row, cell.col, workerCreatePiece(color, nextType, boardState, cell.row, cell.col));
          });
          changed += footprint.length;
          return;
        }
      }
      if (piece.type === "colossus" || Number.isInteger(piece.anchorRow) || Number.isInteger(piece.anchorCol)) {
        const id = piece.id;
        for (let r = 0; r < boardRowCount(boardState); r += 1) {
          for (let c = 0; c < boardColCount(boardState); c += 1) {
            const item = get(boardState, r, c);
            if (item === piece || id && item?.id === id) set(boardState, r, c, null);
          }
        }
        set(boardState, row, col, {
          ...piece,
          type: nextType,
          moved: true,
          anchorRow: void 0,
          anchorCol: void 0,
          hp: void 0,
          maxHp: void 0,
          regencyHeir: piece.regencyHeir
        });
      } else {
        piece.type = nextType;
        piece.moved = true;
      }
      changed += 1;
    });
    if (!changed) return { ok: false, score: 0 };
    const direction = color === aiColor ? 1 : -1;
    if (!boardState.aiSearchBonus) boardState.aiSearchBonus = { white: 0, black: 0 };
    boardState.aiSearchBonus[color] = (boardState.aiSearchBonus[color] || 0) + changed * 35;
    boardState.turn = color;
    return { ok: true, score: baseScore + direction * Math.min(380, changed * 28) };
  }
  function applyWorkerBlueJeans(boardState, color, aiColor, baseScore) {
    const targets = workerMajorPieces(boardState);
    if (!targets.length) return { ok: false, score: 0 };
    let swing = 0;
    targets.forEach(({ piece }) => {
      const value = pieceValue(piece);
      swing += piece.color === aiColor ? -value : value;
      clearPieceCells(boardState, piece);
    });
    if (!sideHasSurvivalPiece(boardState, "white") && !sideHasSurvivalPiece(boardState, "black")) {
      boardState.mode = "gameover";
      boardState.winner = "draw";
    } else if (!sideHasSurvivalPiece(boardState, "white")) {
      boardState.mode = "gameover";
      boardState.winner = "black";
    } else if (!sideHasSurvivalPiece(boardState, "black")) {
      boardState.mode = "gameover";
      boardState.winner = "white";
    }
    boardState.turn = color;
    return { ok: true, score: baseScore + swing * 0.72 };
  }
  function applySummonColossusCard(boardState, card, color, aiColor, baseScore) {
    const anchor = initialColossusAnchor(boardState, color);
    const cells = colossusCells(anchor.row, anchor.col);
    const pawns = workerSummonColossusSacrificePawns(boardState, color);
    if (pawns.length !== 6) return { ok: false, score: 0 };
    if (cells.length !== 4) return { ok: false, score: 0 };
    const removedVictims = collectColossusSummonVictims(
      boardState.board,
      cells,
      pawns.map(({ row, col, piece }) => ({ row, col, item: piece }))
    );
    const crushedRoyalColors = [...new Set(removedVictims.filter(({ item }) => isWorkerRoyalIdentityPiece(boardState, item)).map(({ item }) => item.color).filter((victimColor) => COLORS.includes(victimColor)))];
    removedVictims.forEach(({ item }) => clearPieceCells(boardState, item));
    recordWorkerCapturedPieces(
      boardState,
      color,
      removedVictims.filter(({ item }) => item.color !== color).map(({ item }) => item)
    );
    recordWorkerCapturedPieces(
      boardState,
      opponent(color),
      removedVictims.filter(({ item }) => item.color === color).map(({ item }) => item)
    );
    const colossus = {
      id: `${card.instanceId || card.id || "ai"}-colossus`,
      color,
      type: "colossus",
      hp: 3,
      maxHp: 3,
      moved: true,
      anchorRow: anchor.row,
      anchorCol: anchor.col
    };
    cells.forEach((cell) => set(boardState, cell.row, cell.col, colossus));
    boardState.aiSearchBonus[color] = (boardState.aiSearchBonus[color] || 0) + 450;
    if (crushedRoyalColors.length > 0) {
      boardState.mode = "gameover";
      boardState.winner = crushedRoyalColors.length > 1 ? "draw" : opponent(crushedRoyalColors[0]);
    } else {
      forceWorkerTurnEnd(boardState, color);
    }
    const terminalScore = scoreTerminalOutcome({
      mode: boardState.mode,
      winner: boardState.winner,
      perspectiveColor: aiColor
    }) ?? 0;
    return {
      ok: true,
      score: baseScore + (color === aiColor ? 1100 : -1100) + terminalScore
    };
  }
  function applyWorkerBigRookOpening(boardState, card, color, aiColor, baseScore) {
    const bigBishop = card.effect === "bigBishop";
    if (boardRowCount(boardState) !== 8 || boardColCount(boardState) !== 8) return { ok: false, score: 0 };
    const hordeFormation = workerCountPiecesOf(boardState, color, "pawn") > 8;
    const minorType = hordeFormation ? "pawn" : monochromePieceType("knight", boardState.monochromeChess);
    const config = bigBishop ? septemberBigBishopOpening(color, minorType) : color === "white" ? {
      anchor: "g2",
      placements: [
        ["f3", minorType, true],
        ["g3", "pawn", true],
        ["h3", "pawn", true]
      ]
    } : {
      anchor: "g8",
      placements: [
        ["f6", minorType, true],
        ["g6", "pawn", true],
        ["h6", "pawn", true]
      ]
    };
    const anchor = parseWorkerSquare(boardState, config.anchor);
    const bigRookCells = colossusCells(anchor.row, anchor.col);
    if (bigRookCells.length !== 4) return { ok: false, score: 0 };
    const layout = config.placements.map(([square, type, moved]) => ({ square, type, moved, ...parseWorkerSquare(boardState, square) }));
    const targetKeys = /* @__PURE__ */ new Set([
      ...layout.map(({ row, col }) => `${row}:${col}`),
      ...bigRookCells.map(({ row, col }) => `${row}:${col}`)
    ]);
    const captured = [];
    const removed = [];
    const capturedIds = /* @__PURE__ */ new Set();
    for (let row = 0; row < boardRowCount(boardState); row += 1) {
      for (let col = 0; col < boardColCount(boardState); col += 1) {
        const piece = get(boardState, row, col);
        if (!piece || !targetKeys.has(`${row}:${col}`)) continue;
        const id = piece.id || `${row}:${col}`;
        if (capturedIds.has(id)) continue;
        capturedIds.add(id);
        removed.push(piece);
        if (piece.color !== color) captured.push(piece);
        clearPieceCells(boardState, piece);
        if (isWorkerDecisiveCaptureTarget(boardState, piece)) {
          workerCriticalCaptureScore(boardState, piece, piece.color === color ? opponent(color) : color, aiColor);
        }
      }
    }
    layout.forEach(({ row, col, type, moved }) => {
      const next = workerCreatePiece(color, type, boardState, row, col);
      next.moved = moved;
      next.origin = workerSquareName(boardState, row, col);
      set(boardState, row, col, next);
    });
    const rook = workerCreatePiece(color, bigBishop ? "bigBishop" : "bigRook", boardState, anchor.row, anchor.col);
    rook.hp = 2;
    rook.maxHp = 2;
    rook.moved = false;
    rook.origin = config.anchor;
    rook.anchorRow = anchor.row;
    rook.anchorCol = anchor.col;
    bigRookCells.forEach((cell) => set(boardState, cell.row, cell.col, rook));
    resolveWorkerBatchRoyalDefeats(boardState, removed);
    recordWorkerCapturedPieces(boardState, color, captured);
    boardState.turn = color;
    if (boardState.mode === "gameover") {
      const terminalScore = scoreTerminalOutcome({ mode: boardState.mode, winner: boardState.winner, perspectiveColor: aiColor }) ?? 0;
      return { ok: true, score: baseScore + terminalScore + captured.length * 160 };
    }
    const royalScore = boardState.mode === "gameover" ? boardState.winner === aiColor ? 12e4 : -12e4 : 0;
    return { ok: true, score: baseScore + (color === aiColor ? 760 : -760) + captured.length * 160 + royalScore };
  }
  function initialColossusAnchor(boardState, color) {
    return colossusOpeningAnchor(
      boardRowCount(boardState),
      boardColCount(boardState),
      color,
      workerIsDiagonalChessActive(boardState)
    );
  }
  function workerSummonColossusPawnIsPreserved(boardState, color, row, col) {
    return isColossusPawnPreserved({
      row,
      col,
      rookSquares: piecesMatching(boardState, (piece) => piece.color === color && piece.type === "rook"),
      pawnDirection: pawnDir(boardState, color),
      colCount: boardColCount(boardState),
      diagonalChessActive: workerIsDiagonalChessActive(boardState)
    });
  }
  function workerSummonColossusSacrificePawns(boardState, color) {
    const pawns = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "pawn");
    if (pawns.length > 8) {
      return colossusHordeSacrificeCells(
        boardRowCount(boardState),
        boardColCount(boardState),
        color,
        workerIsDiagonalChessActive(boardState)
      ).map(({ row, col }) => ({ row, col, piece: get(boardState, row, col) })).filter(({ piece }) => piece?.color === color && piece.type === "pawn");
    }
    return selectColossusSacrificePawns(
      pawns,
      ({ row, col }) => workerSummonColossusPawnIsPreserved(boardState, color, row, col)
    );
  }
  function workerSelectedQueenSquare(boardState, color, selectedTarget) {
    if (selectedTarget !== void 0 && selectedTarget !== null) {
      if (!Number.isInteger(selectedTarget.row) || !Number.isInteger(selectedTarget.col)) return null;
      const piece = get(boardState, selectedTarget.row, selectedTarget.col);
      if (!isWorkerQueenIdentity(piece, color)) return null;
      return { piece, row: selectedTarget.row, col: selectedTarget.col };
    }
    return piecesMatching(boardState, (piece) => isWorkerQueenIdentity(piece, color))[0] || null;
  }
  function applyWizardCard(boardState, card, color, aiColor, baseScore, selectedTarget) {
    const queenSquare = workerSelectedQueenSquare(boardState, color, selectedTarget);
    if (!queenSquare) return { ok: false, score: 0 };
    queenSquare.piece.type = "wizard";
    queenSquare.piece.mana = 0;
    queenSquare.piece.maxMana = 5;
    queenSquare.piece.moved = true;
    boardState.aiSearchBonus[color] = (boardState.aiSearchBonus[color] || 0) + 320;
    boardState.turn = color;
    return { ok: true, score: baseScore + (color === aiColor ? 520 : -520) };
  }
  function applyWorkerMerchantGuildCard(boardState, color, aiColor, baseScore) {
    const kingSquare = criticalPieces(boardState, color).find(({ piece }) => piece.type === "king" && !isWorkerUndergroundBunkerKing(piece));
    if (!kingSquare) return { ok: false, score: 0 };
    const target = {
      row: kingSquare.row + pawnDir(boardState, color) * 2,
      col: kingSquare.col
    };
    if (!inBounds(target.row, target.col, boardState)) return { ok: false, score: 0 };
    const occupant = get(boardState, target.row, target.col);
    if (workerIsLargePiece(occupant)) return { ok: false, score: 0 };
    if (occupant && occupant.color !== color) return { ok: false, score: 0 };
    if (occupant && occupant !== kingSquare.piece) {
      recordWorkerCapturedPieces(boardState, opponent(color), [occupant]);
    }
    set(boardState, kingSquare.row, kingSquare.col, null);
    const merchant = kingSquare.piece;
    merchant.type = "merchant";
    merchant.gold = 0;
    merchant.moved = true;
    delete merchant.freshNoCaptureUntil;
    delete merchant.undergroundBunker;
    delete merchant.hp;
    delete merchant.maxHp;
    set(boardState, target.row, target.col, merchant);
    boardState.turn = color;
    return { ok: true, score: baseScore + (color === aiColor ? 900 : -900) };
  }
  function applyShotgunKingCard(boardState, card, color, aiColor, baseScore) {
    const king = findWorkerKingRole(boardState, color);
    if (!king) return { ok: false, score: 0 };
    king.piece.type = "shotgunKing";
    king.piece.hp = king.piece.hp || 5;
    king.piece.maxHp = king.piece.maxHp || 5;
    king.piece.ammo = king.piece.ammo ?? 0;
    king.piece.maxAmmo = king.piece.maxAmmo || 3;
    king.piece.moved = true;
    forceWorkerTurnEnd(boardState, color);
    return { ok: true, score: baseScore + (color === aiColor ? 850 : -850) };
  }
  function applyTimeTravelerCardAction(boardState, card, color, aiColor, baseScore) {
    const traveler = findPieceByType(boardState, color, "timeTraveler");
    const data = timeTravelerState(boardState);
    if (!traveler || !data) return { ok: false, score: 0 };
    const effect = card.effect || "";
    let score = baseScore;
    if (effect === "timePhaseShift") {
      const current = normalizeTimePhase(traveler.piece.timePhase || data.phase);
      const next = current === "future" ? "past" : "future";
      traveler.piece.timePhase = next;
      data.phase = next;
      score += color === aiColor ? 180 : -180;
    } else if (effect === "timeClumsyAttack") {
      data.attackEnabledFor = color;
      score += color === aiColor ? 260 : -260;
    } else if (effect === "timeIsMine") {
      const ready = (Number(boardState.fullMove) || 1) >= 20;
      if (ready) {
        boardState.mode = "gameover";
        boardState.winner = color;
        score += color === aiColor ? 1e5 : -1e5;
      } else {
        score += color === aiColor ? -120 : 120;
      }
    } else {
      return { ok: false, score: 0 };
    }
    boardState.turn = color;
    return { ok: true, score };
  }
  function applyBloodCardAction(boardState, card, action, color, aiColor, baseScore) {
    if (!hasVampireLord(boardState, color)) return { ok: false, score: 0 };
    const data = ensureBloodMoonState(boardState);
    if (!data) return { ok: false, score: 0 };
    const effectId = action.target?.bloodEffectId || action.bloodEffectId || card.bloodEffectId || bestBloodEffectId(boardState, color);
    card.bloodEffectId = effectId;
    card.bloodRevealed = true;
    let effectScore = bloodEffectScore(boardState, effectId, color);
    if (effectId === "summon") {
      const count = summonWorkerBats(boardState, color);
      if (!count && !campaignAuthorityV1States.has(boardState)) return { ok: false, score: 0 };
      effectScore += count * pieceValue({ type: "bat" }) * 0.9;
    } else if (effectId === "veil") {
      data.veilUntil = bloodMoonHalfTurns(boardState) + 3;
    } else if (effectId === "sunlight") {
      data.sunlightOverride = color;
    } else if (effectId === "curse") {
      const target = bestBloodCurseTarget(boardState, color);
      if (!target && !campaignAuthorityV1States.has(boardState)) return { ok: false, score: 0 };
      if (target) {
        target.piece.bloodCurse = { by: color };
        effectScore += pieceValue(target.piece) * 0.4;
      }
    } else if (effectId === "coffin") {
      if (!installWorkerCoffin(boardState, color) && !campaignAuthorityV1States.has(boardState)) return { ok: false, score: 0 };
    }
    if (!boardState.aiSearchBonus) boardState.aiSearchBonus = { white: 0, black: 0 };
    boardState.aiSearchBonus[color] = (boardState.aiSearchBonus[color] || 0) + Math.max(120, effectScore * 0.35);
    boardState.turn = color;
    return { ok: true, score: baseScore + (color === aiColor ? effectScore : -effectScore) };
  }
  function bestBloodEffectId(boardState, color) {
    return BLOOD_EFFECT_IDS.map((effectId) => ({ effectId, score: bloodEffectScore(boardState, effectId, color) })).sort((a, b) => b.score - a.score)[0]?.effectId || "summon";
  }
  function bloodEffectScore(boardState, effectId, color) {
    const lord = findPieceByType(boardState, color, "vampireLord");
    if (!lord || !bloodMoonState(boardState)) return -500;
    const enemy = opponent(color);
    const lordInDanger = isSquareAttacked(boardState, lord.row, lord.col, enemy);
    const longRangeDanger = vampireLongRangeDanger(boardState, color);
    if (effectId === "summon") {
      const emptyCells = workerBloodHomeCells(boardState, color, false).length;
      const ownBats = friendlyPieces(boardState, color).filter(({ piece }) => piece.type === "bat").length;
      return 430 + Math.min(2, emptyCells) * 175 - ownBats * 45;
    }
    if (effectId === "veil") {
      return 300 + (longRangeDanger ? 540 : 0) + (lordInDanger ? 260 : 0) + (!usesBloodMoonNightMovement(boardState, color) ? 120 : 0);
    }
    if (effectId === "sunlight") {
      return 260 + (!usesBloodMoonNightMovement(boardState, color) ? 260 : 20) + bloodNightAttackScore(boardState, color) * 140;
    }
    if (effectId === "curse") {
      const target = bestBloodCurseTarget(boardState, color);
      return target ? 280 + pieceValue(target.piece) * 0.55 : -120;
    }
    if (effectId === "coffin") {
      const hasCoffin = friendlyPieces(boardState, color).some(({ piece }) => piece.type === "coffin");
      return hasCoffin ? 80 : 370 + (!usesBloodMoonNightMovement(boardState, color) ? 180 : 0) + (lordInDanger ? 220 : 0);
    }
    return 120;
  }
  function summonWorkerBats(boardState, color) {
    const cells = workerBloodHomeCells(boardState, color, false).sort((a, b) => Math.abs(a.col - 3.5) - Math.abs(b.col - 3.5) || a.row - b.row).slice(0, 2);
    cells.forEach(({ row, col }, index) => {
      set(boardState, row, col, {
        id: `ai-bat-${color}-${boardState.moveCount || 0}-${row}-${col}-${index}`,
        color,
        type: "bat",
        moved: true
      });
    });
    return cells.length;
  }
  function installWorkerCoffin(boardState, color) {
    if (friendlyPieces(boardState, color).some(({ piece }) => piece.type === "coffin")) return false;
    const cells = workerBloodHomeCells(boardState, color, true);
    const preferred = cells.sort((a, b) => Math.abs(a.col - 3.5) - Math.abs(b.col - 3.5))[0];
    if (!preferred) return false;
    set(boardState, preferred.row, preferred.col, {
      id: `ai-coffin-${color}-${boardState.moveCount || 0}-${preferred.row}-${preferred.col}`,
      color,
      type: "coffin",
      moved: true
    });
    return true;
  }
  function workerBloodHomeCells(boardState, color, homeRankOnly) {
    const homeRow = color === "white" ? boardRowCount(boardState) - 1 : 0;
    const pawnRow = color === "white" ? Math.max(0, boardRowCount(boardState) - 2) : Math.min(boardRowCount(boardState) - 1, 1);
    const rows = homeRankOnly ? [homeRow] : [pawnRow, homeRow];
    const cells = [];
    rows.forEach((row) => {
      for (let col = 0; col < boardColCount(boardState); col += 1) {
        if (!get(boardState, row, col)) cells.push({ row, col });
      }
    });
    return cells;
  }
  function bestBloodCurseTarget(boardState, color) {
    return enemyPieces(boardState, color).filter(({ piece }) => !["king", "royalKnight", "shotgunKing", "merchant", "vampireLord", "knight", "pawn", "wall"].includes(piece.type)).sort((a, b) => pieceValue(b.piece) - pieceValue(a.piece))[0] || null;
  }
  function bloodNightAttackScore(boardState, color) {
    let score = 0;
    friendlyPieces(boardState, color).filter(({ piece }) => piece.type === "vampireLord" || piece.type === "bat").forEach(({ piece, row, col }) => {
      enemyPieces(boardState, color).forEach(({ piece: target, row: targetRow2, col: targetCol2 }) => {
        const dr = targetRow2 - row;
        const dc = targetCol2 - col;
        const vampireCanReach = piece.type === "vampireLord" && (Math.abs(dr) === 2 && Math.abs(dc) === 1 || Math.abs(dr) === 1 && Math.abs(dc) === 2 || (dr === 0 || dc === 0 || Math.abs(dr) === Math.abs(dc)) && clearRay(boardState, row, col, targetRow2, targetCol2));
        const batCanReach = piece.type === "bat" && Math.max(Math.abs(dr), Math.abs(dc)) <= 2;
        if (vampireCanReach || batCanReach) score += isCritical(target) ? 8 : Math.max(1, pieceValue(target) / 150);
      });
    });
    return score;
  }
  function vampireLongRangeDanger(boardState, color) {
    const lord = findPieceByType(boardState, color, "vampireLord");
    if (!lord) return false;
    return enemyPieces(boardState, color).some(({ piece, row, col }) => {
      const type = renderType(piece);
      const distance = Math.max(Math.abs(row - lord.row), Math.abs(col - lord.col));
      return distance >= 2 && ["bishop", "rook", "queen", "wizard", "amazon", "dragon", "hook"].includes(type) && attacksSquare(boardState, piece, row, col, lord.row, lord.col);
    });
  }
  function grantWorkerBloodCard(boardState, color) {
    if (!bloodMoonState(boardState) || !hasVampireLord(boardState, color)) return false;
    const slots = deck(boardState, color);
    if (!legacyBloodMoonLifecycleStates.has(boardState)) {
      slots.forEach((card, slot2) => {
        if (card?.effect === "bloodCard" && card.used) slots[slot2] = null;
      });
    }
    const activeBlood = slots.filter((card) => card?.effect === "bloodCard" && !card.used).length;
    if (activeBlood >= 3) return false;
    const slot = slots.findIndex((card) => !card);
    if (slot < 0) return false;
    slots[slot] = {
      id: "blood",
      instanceId: `ai-blood-${color}-${boardState.moveCount || 0}-${slot}`,
      name: "blood",
      phase: "BLOOD",
      stars: 0,
      effect: "bloodCard",
      campaignCard: true,
      deckCard: true,
      bloodEffectId: null,
      bloodRevealed: false
    };
    return true;
  }
  function colossusCells(anchorRow, anchorCol) {
    return [
      { row: anchorRow, col: anchorCol },
      { row: anchorRow, col: anchorCol + 1 },
      { row: anchorRow + 1, col: anchorCol },
      { row: anchorRow + 1, col: anchorCol + 1 }
    ].filter(({ row, col }) => inBounds(row, col));
  }
  function workerIsLargePiece(piece) {
    return Boolean(piece && ["colossus", "bigRook", "bigBishop"].includes(piece.type));
  }
  // Grafted from engine.optimized.js (real bug fix, not a behavior change):
  // the "submerge" card's targeting (this file's own effect === "submerge"
  // branch above) and its apply-time re-validation both call
  // hasAdjacentEnemyPiece(board, row, col, color), but this function is
  // referenced without ever being defined anywhere in aiWorker-raw.js
  // either (confirmed: grepped the whole file) -- a genuine latent
  // ReferenceError crash in the real site itself any time "submerge" is
  // played, not something our engine invented differently. Implemented
  // directly on the raw board array, matching how both real call sites
  // already invoke it.
  function hasAdjacentEnemyPiece(board, row, col, color) {
    for (let dr = -1; dr <= 1; dr += 1) {
      for (let dc = -1; dc <= 1; dc += 1) {
        if (!dr && !dc) continue;
        const piece = board[row + dr]?.[col + dc];
        if (piece && piece.color && piece.color !== color) return true;
      }
    }
    return false;
  }
  function clearPieceCells(boardState, piece) {
    if (!piece) return;
    for (let row = 0; row < boardRowCount(boardState); row += 1) {
      for (let col = 0; col < boardColCount(boardState); col += 1) {
        const item = get(boardState, row, col);
        if (item === piece || piece.id && item?.id === piece.id) set(boardState, row, col, null);
      }
    }
  }
  function trackWorkerMovingProgress(boardState, piece) {
    if (!piece?.id || !piece.color || ["wall", "football", "blackHole"].includes(piece.type)) return false;
    boardState.exhaustion = advanceExhaustionStreak(boardState.exhaustion, piece.color, piece.id);
    boardState.moving = normalizeMovingState(boardState.moving);
    const result = advanceMovingStreak(boardState.moving[piece.color], piece.id);
    boardState.moving[piece.color] = result.entry;
    if (!result.triggered) return false;
    piece.evasion = true;
    return true;
  }
  function resetWorkerMovingProgressAfterEvasionLoss(boardState, piece) {
    if (!piece?.id || !COLORS.includes(piece.color)) return;
    boardState.moving = normalizeMovingState(boardState.moving);
    boardState.moving[piece.color] = resetMovingStreakForPiece(boardState.moving[piece.color], piece.id);
  }
  function workerEvasionDestinationCandidates(boardState, piece, row, col, options = {}) {
    const origin = workerIsLargePiece(piece) ? { row: piece.anchorRow, col: piece.anchorCol } : { row, col };
    const blockedLandings = Array.isArray(options.attackerLandingCells) ? options.attackerLandingCells : options.attackerLanding ? [options.attackerLanding] : [];
    return adjacentEscapeSquares(origin.row, origin.col, boardRowCount(boardState), boardColCount(boardState)).filter((candidate) => {
      if (!isWorkerChainDestinationAllowed(boardState, piece, candidate)) return false;
      const cells = workerIsLargePiece(piece) ? colossusCells(candidate.row, candidate.col) : [candidate];
      if (workerIsLargePiece(piece) && cells.length !== 4) return false;
      if (cells.some((cell) => isWorkerCollapsedSquare(boardState, cell.row, cell.col))) return false;
      if (cells.some((cell) => {
        const occupant = get(boardState, cell.row, cell.col);
        return occupant && occupant !== piece && (!piece.id || occupant.id !== piece.id);
      })) return false;
      if (cells.some((cell) => isBlackHoleCell(boardState, cell.row, cell.col))) return false;
      if (boardState.monochromeChess && piece.monoShade && squareShade(candidate.row, candidate.col) !== piece.monoShade) return false;
      return !cells.some((cell) => blockedLandings.some((landing) => cell.row === landing.row && cell.col === landing.col));
    });
  }
  function workerTryEvadeCapture(boardState, piece, row, col, capturerColor, options = {}) {
    if (piece?.type === "scarecrow") return false;
    if (!options.attacker || !piece?.evasion || !piece.color || piece.color === capturerColor) return false;
    const origin = workerIsLargePiece(piece) ? { row: piece.anchorRow, col: piece.anchorCol } : findWorkerPieceByRef(boardState, { id: piece.id, row, col }) || { row, col };
    const candidates = workerEvasionDestinationCandidates(boardState, piece, origin.row, origin.col, options);
    if (!candidates.length) return false;
    const destination = candidates[workerStableIndex(`${piece.id}:${row}:${col}:${boardState.moveCount || 0}`, candidates.length)];
    delete piece.evasion;
    resetWorkerMovingProgressAfterEvasionLoss(boardState, piece);
    clearPieceCells(boardState, piece);
    if (workerIsLargePiece(piece)) {
      piece.anchorRow = destination.row;
      piece.anchorCol = destination.col;
      colossusCells(destination.row, destination.col).forEach((cell) => set(boardState, cell.row, cell.col, piece));
    } else {
      set(boardState, destination.row, destination.col, piece);
    }
    piece.moved = true;
    noteWorkerUltimatumMovement(boardState, piece);
    return true;
  }
  function oscillationPenalty(boardState, action, color) {
    if (action?.type !== "move") return 0;
    const piece = get(boardState, action.from?.row, action.from?.col);
    const move = action.move || {};
    if (!piece || !Number.isInteger(move.row) || !Number.isInteger(move.col)) return 0;
    const destination = workerPortalMoveDestination(move) || move;
    const target = get(boardState, destination.row, destination.col);
    const captureSwing = workerActionCaptureSwing(boardState, action, color);
    if (captureSwing.decisive) return 0;
    const recent = (boardState.recentMoves || []).slice(-8).reverse();
    let penalty = 0;
    let ownMoveIndex = 0;
    recent.forEach((trace) => {
      if (!trace || trace.color !== color) return;
      const recency = ownMoveIndex;
      ownMoveIndex += 1;
      const samePiece = piece?.id && trace.pieceId && piece.id === trace.pieceId;
      const reverseById = samePiece && sameSquare(trace.to, action.from) && sameSquare(trace.from, destination);
      const reverseBySquare = sameSquare(trace.to, action.from) && sameSquare(trace.from, destination);
      const returningToRecentFrom = samePiece && sameSquare(trace.from, destination);
      const returningToRecentTo = samePiece && sameSquare(trace.to, destination);
      const immediateOwnMove = recency === 0;
      if (reverseById) penalty += immediateOwnMove ? 3600 : 1900;
      else if (reverseBySquare) penalty += immediateOwnMove ? 2400 : 1250;
      else if (returningToRecentFrom) penalty += immediateOwnMove ? 1600 : 820;
      else if (returningToRecentTo) penalty += immediateOwnMove ? 520 : 280;
      else if (samePiece && sameSquare(trace.to, action.from)) penalty += immediateOwnMove ? 320 : 160;
    });
    if (!penalty) return 0;
    if (!target && !captureSwing.value) penalty *= 1.55;
    else if (captureSwing.value) {
      const movingValue = workerStrategicPieceValue(boardState, piece);
      penalty *= captureSwing.value >= movingValue ? 0.22 : 0.55;
    }
    penalty *= oscillationSafetyMultiplier(boardState, action, piece);
    return penalty;
  }
  function pushRecentMove(boardState, trace) {
    if (!Array.isArray(boardState.recentMoves)) boardState.recentMoves = [];
    boardState.recentMoves.push(trace);
    if (boardState.recentMoves.length > 10) boardState.recentMoves = boardState.recentMoves.slice(-10);
  }
  function oscillationSafetyMultiplier(boardState, action, piece) {
    const from = action.from || {};
    if (workerUltimatumMoveReliefScore(boardState, piece) > 0) return 0.4;
    if (piece?.thiefSecondMove || piece?.fileSurgeSecondMove || piece?.rookLiftSecondMove || piece?.ironMonarchExtraMove || piece?.underpromotionSecondMove || piece?.checkerChainCapture || piece?.repositionSecondMove || piece?.desperado) return 0.45;
    if (!Number.isInteger(from.row) || !Number.isInteger(from.col)) return 1;
    const beforeThreat = workerBestCaptureThreat(boardState, piece, from.row, from.col);
    if (!beforeThreat) return 1;
    const afterThreat = workerBestCaptureThreatAfterMove(boardState, action, piece);
    if (!afterThreat) return 0.35;
    const beforeAttackerValue = pieceValue(beforeThreat.piece);
    const afterAttackerValue = pieceValue(afterThreat.piece);
    return afterAttackerValue > beforeAttackerValue ? 0.62 : 1;
  }
  function sameSquare(a, b) {
    return a?.row === b?.row && a?.col === b?.col;
  }
  function applyCardTransform(boardState, piece, card, row, col) {
    const effect = card.effect || "";
    const transforms = {
      hedgehog: "hedgehog",
      campfire: "campfire",
      princess: "princess",
      wizard: "wizard",
      jester: "jester",
      amazon: "amazon",
      knightmaster: "knightmaster",
      standardBearer: "standardBearer",
      assassin: "assassin",
      dragon: "dragon",
      grasshopper: "grasshopper",
      ordination: "cardinal",
      reformation: "protestant",
      herald: "herald",
      cannon: "cannon",
      log: "log",
      pegasus: "unicorn",
      unicorn: "unicorn",
      constitutionalMonarchy: "primeMinister",
      idol: "idol",
      siegeRam: "siegeRam",
      magicGirl: "magicGirl",
      berserker: "berserker",
      slime: "slime",
      siren: "siren",
      trickster: "trickster",
      undead: "undead"
    };
    if (transforms[effect]) {
      piece.type = transforms[effect];
      if (effect === "hedgehog") piece.bearRetaliationsRemaining = 3;
      piece.moved = true;
      if (effect === "trickster") {
        setWorkerTricksterAbilityType(
          boardState,
          piece,
          TRICKSTER_MOVEMENT_TYPES[workerStableIndex(`${piece.id || "trickster"}:${boardState.moveCount || 0}:${row}:${col}`, TRICKSTER_MOVEMENT_TYPES.length)] || "queen"
        );
      }
      markWorkerFreshNoCapture(boardState, piece);
    }
    if (effect === "suicideBomber") piece.explosive = true;
    if (boardState?.monochromeChess && !piece.monoShade && inBounds(row, col)) piece.monoShade = squareShade(row, col);
  }
  function applyWorkerApprenticeKnights(boardState, color) {
    const origins = workerApprenticePawnOrigins(boardState, color);
    let changed = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || piece.type !== "pawn") return;
      if (!origins.has(piece.origin || workerSquareName(boardState, row, col))) return;
      piece.type = "squire";
      piece.moved = true;
      changed += 1;
    });
    return changed;
  }
  function workerSecondRankLabel(boardState, color) {
    return color === "white" ? 2 : Math.max(1, boardRowCount(boardState) - 1);
  }
  function workerSquareName(boardState, row, col) {
    return `${workerFileLabels(boardColCount(boardState))[col] || "?"}${boardRowCount(boardState) - row}`;
  }
  function workerApprenticePawnOrigins(boardState, color) {
    if (workerIsDiagonalChessActive(boardState)) {
      return new Set(DIAGONAL_CHESS_CARD_SQUARES.apprenticeKnights[color] || []);
    }
    const files = workerFileLabels(boardColCount(boardState));
    const rank = workerSecondRankLabel(boardState, color);
    const targetFiles = [files[1], files[files.length - 2]].filter(Boolean);
    return new Set(targetFiles.map((file) => file + rank));
  }
  function workerEdgePawnOrigins(boardState, color) {
    if (workerIsDiagonalChessActive(boardState)) {
      return new Set(DIAGONAL_CHESS_CARD_SQUARES.checker[color] || []);
    }
    const files = workerFileLabels(boardColCount(boardState));
    const rank = workerSecondRankLabel(boardState, color);
    return new Set([files[0], files[files.length - 1]].filter(Boolean).map((file) => `${file}${rank}`));
  }
  function workerHasCheckerCandidate(boardState, color) {
    const origins = workerEdgePawnOrigins(boardState, color);
    return piecesMatching(boardState, (piece, row, col) => piece.color === color && piece.type === "pawn" && origins.has(piece.origin || workerSquareName(boardState, row, col))).length > 0;
  }
  function applyWorkerChecker(boardState, color) {
    const origins = workerEdgePawnOrigins(boardState, color);
    let changed = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || piece.type !== "pawn") return;
      if (!origins.has(piece.origin || workerSquareName(boardState, row, col))) return;
      piece.type = "checker";
      piece.moved = true;
      if (boardState.monochromeChess) piece.monoShade = squareShade(row, col);
      changed += 1;
    });
    return changed;
  }
  // Grafted from engine.optimized.js (our-only addition, behavior-preserving
  // refactor): evaluateState's weighted-sum formula below split out into
  // named sub-scores via evaluateStateComponents, so external tooling
  // (NNUE tuning, debugging) can inspect each term individually. Every
  // sub-score computation and every weight is copied verbatim from
  // aiWorker-raw.js's own evaluateState above -- this changes nothing about
  // what evaluateState returns, only how the computation is organized.
  function evaluateStateComponents(boardState, aiColor) {
    if (ATTACK_MEMO !== null) return evaluateStateComponentsRaw(boardState, aiColor);
    ATTACK_MEMO = { board: boardState, map: /* @__PURE__ */ new Map(), pieces: null, cols: -1, encouraged: /* @__PURE__ */ new Map(), ranged: /* @__PURE__ */ new Map(), attackers: /* @__PURE__ */ new Map() };
    try {
      return evaluateStateComponentsRaw(boardState, aiColor);
    } finally {
      ATTACK_MEMO = null;
    }
  }
  function evaluateStateComponentsRaw(boardState, aiColor) {
    const terminal = scoreTerminalOutcome({
      mode: boardState?.mode,
      winner: boardState?.winner,
      perspectiveColor: aiColor
    });
    if (terminal !== null) return { terminal };
    const enemy = opponent(aiColor);
    if (!sideHasSurvivalPiece(boardState, aiColor)) return { terminal: -1e5 };
    if (!sideHasSurvivalPiece(boardState, enemy)) return { terminal: 1e5 };
    const material = materialBalance(boardState, aiColor);
    const exchange = scoreExchangeSimplification({
      materialLead: material,
      remainingPieces: workerTradablePieceCount(boardState),
      unitValue: PIECE_VALUES.pawn
    }) * PIECE_VALUES.pawn;
    return {
      terminal: null,
      material,
      exchange,
      positionSelf: positionalScore(boardState, aiColor),
      positionEnemy: positionalScore(boardState, enemy),
      kingSafetySelf: kingSafetyScore(boardState, aiColor),
      kingSafetyEnemy: kingSafetyScore(boardState, enemy),
      pressureSelf: royalPressureScore(boardState, aiColor),
      pressureEnemy: royalPressureScore(boardState, enemy),
      specialSelf: specialThreatScore(boardState, aiColor),
      specialEnemy: specialThreatScore(boardState, enemy),
      bloodMoonSelf: bloodMoonStrategicScore(boardState, aiColor),
      bloodMoonEnemy: bloodMoonStrategicScore(boardState, enemy),
      cardsSelf: cardThreatScore(boardState, aiColor),
      cardsEnemy: cardThreatScore(boardState, enemy),
      campaignSelf: campaignStrategicScore(boardState, aiColor),
      campaignEnemy: campaignStrategicScore(boardState, enemy),
      ultimatum: ultimatumStrategicScore(boardState, aiColor),
      persistentObjectivesSelf: workerPersistentObjectiveScore(boardState, aiColor),
      persistentObjectivesEnemy: workerPersistentObjectiveScore(boardState, enemy),
      bonus: (boardState.aiSearchBonus?.[aiColor] || 0) - (boardState.aiSearchBonus?.[enemy] || 0),
      tacticalSafety: tacticalSafetyScore(boardState, aiColor)
    };
  }
  function evaluateState(boardState, aiColor) {
    const c = evaluateStateComponents(boardState, aiColor);
    if (c.terminal !== null) return c.terminal;
    return c.material * 1.35 + c.exchange
      + (c.positionSelf - c.positionEnemy * 0.88)
      + (c.kingSafetySelf - c.kingSafetyEnemy * 0.72)
      + (c.pressureSelf - c.pressureEnemy * 0.65)
      + (c.specialSelf - c.specialEnemy * 0.82)
      + (c.bloodMoonSelf - c.bloodMoonEnemy * 0.82)
      + (c.cardsSelf - c.cardsEnemy * 0.92)
      + (c.campaignSelf - c.campaignEnemy * 0.9)
      + c.ultimatum
      + (c.persistentObjectivesSelf - c.persistentObjectivesEnemy)
      + c.bonus + c.tacticalSafety;
  }
  function tacticalSafetyScore(boardState, aiColor) {
    const enemy = opponent(aiColor);
    return hangingMaterialRisk(boardState, enemy) * 0.72 - hangingMaterialRisk(boardState, aiColor);
  }
  function tacticalSafetyAdjustment(beforeState, afterState, action, aiColor) {
    if (!beforeState || !afterState || !action) return 0;
    if (afterState.mode === "gameover") return afterState.winner === aiColor ? 0 : -3e3;
    const enemy = opponent(aiColor);
    const ownBefore = memoHangingMaterialRisk(beforeState, aiColor);
    const ownAfter = hangingMaterialRisk(afterState, aiColor);
    const enemyBefore = memoHangingMaterialRisk(beforeState, enemy);
    const enemyAfter = hangingMaterialRisk(afterState, enemy);
    let score = (ownBefore - ownAfter) * 1.08 + (enemyAfter - enemyBefore) * 0.48;
    if (action.type === "move" && (action.color || beforeState.turn) === aiColor) {
      score += movedPieceSafetyAdjustment(beforeState, afterState, action, aiColor);
    }
    return Math.max(-3600, Math.min(3600, Math.round(score)));
  }
  function movedPieceSafetyAdjustment(beforeState, afterState, action, aiColor) {
    const from = action.from || {};
    const moving = get(beforeState, from.row, from.col);
    if (!moving || moving.color !== aiColor || isWorkerDecisiveCaptureTarget(beforeState, moving)) return 0;
    const beforePenalty = workerHangingPenalty(beforeState, moving, from.row, from.col);
    const destination = workerActionDestination(action) || action.move || {};
    const afterPiece = findWorkerPieceByRef(afterState, { id: moving.id, row: destination.row, col: destination.col });
    const afterPenalty = afterPiece ? workerHangingPenalty(afterState, afterPiece.piece, afterPiece.row, afterPiece.col) : 0;
    const swing = workerActionCaptureSwing(beforeState, action, aiColor);
    const movingValue = workerStrategicPieceValue(beforeState, moving);
    const strongCompensation = swing.decisive || swing.value >= movingValue;
    let score = 0;
    if (beforePenalty > 0 && afterPenalty <= beforePenalty * 0.35) {
      score += beforePenalty * 0.28;
    }
    if (afterPenalty > 0 && afterPenalty >= beforePenalty * 0.55) {
      score -= afterPenalty * (strongCompensation ? 0.18 : 0.82);
    } else if (beforePenalty <= 0 && afterPenalty > 0 && !strongCompensation) {
      score -= afterPenalty * 0.68;
    }
    score -= workerBadLowValueCapturePenalty(beforeState, afterState, action, aiColor);
    return score;
  }
  function hangingMaterialRisk(boardState, color) {
    // Same per-call attack memo evaluateStateComponents uses: the board is not
    // modified while this runs, so many pieces share one attacker list / attack cache.
    if (ATTACK_MEMO !== null) return hangingMaterialRiskRaw(boardState, color);
    ATTACK_MEMO = { board: boardState, map: /* @__PURE__ */ new Map(), pieces: null, cols: -1, encouraged: /* @__PURE__ */ new Map(), ranged: /* @__PURE__ */ new Map(), attackers: /* @__PURE__ */ new Map() };
    try {
      return hangingMaterialRiskRaw(boardState, color);
    } finally {
      ATTACK_MEMO = null;
    }
  }
  function hangingMaterialRiskRaw(boardState, color) {
    let risk = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color) return;
      risk += workerHangingPenalty(boardState, piece, row, col);
    });
    return risk;
  }
  function workerHangingPenalty(boardState, piece, row, col) {
    if (!piece || !piece.color || isWorkerDecisiveCaptureTarget(boardState, piece) || ["wall", "football", "blackHole"].includes(piece.type)) return 0;
    if (!Number.isInteger(row) || !Number.isInteger(col)) return 0;
    const threat = workerBestCaptureThreat(boardState, piece, row, col);
    if (!threat) return 0;
    const value = workerStrategicPieceValue(boardState, piece);
    const attackerValue = Math.max(40, pieceValue(threat.piece));
    let risk = value * 0.42;
    if (attackerValue < value) {
      risk += (value - attackerValue) * 1.32 + value * 0.42;
    } else if (attackerValue === value) {
      risk += value * 0.18;
    } else {
      risk = Math.max(24, value * 0.2 - (attackerValue - value) * 0.025);
    }
    if (piece.shielded) risk *= 0.34;
    if (isHpPiece(piece)) {
      const hp = Number(piece.hp ?? piece.maxHp ?? 1);
      risk *= hp > 1 ? 0.46 : 0.72;
    }
    if (piece.type === "coffin") risk *= 0.5;
    return Math.min(2100, risk);
  }
  function workerBestCaptureThreat(boardState, targetPiece, targetRow2, targetCol2) {
    const byColor = opponent(targetPiece?.color);
    const cells = workerThreatTargetCells(boardState, targetPiece, targetRow2, targetCol2);
    const memo = ATTACK_MEMO;
    if (memo !== null && memo.board === boardState) {
      // Same result as the scan below: the first (in scan order) attacker with the minimal pieceValue.
      let sorted = memo.attackers.get(byColor);
      if (sorted === void 0) {
        sorted = [];
        forEachPiece(boardState, (piece, row, col) => {
          if (piece.color !== byColor || piece.type === "wall" || isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return;
          sorted.push({ piece, row, col, value: pieceValue(piece), order: sorted.length });
        });
        if (sorted.some((e) => e.value !== e.value)) sorted = null;
        else sorted.sort((a, b) => a.value - b.value || a.order - b.order);
        memo.attackers.set(byColor, sorted);
      }
      if (sorted !== null) {
        for (let i = 0; i < sorted.length; i += 1) {
          const e = sorted[i];
          if (cells.some((cell) => attacksSquare(boardState, e.piece, e.row, e.col, cell.row, cell.col))) return { piece: e.piece, row: e.row, col: e.col };
        }
        return null;
      }
    }
    let best = null;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== byColor || piece.type === "wall" || isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return;
      if (!cells.some((cell) => attacksSquare(boardState, piece, row, col, cell.row, cell.col))) return;
      if (!best || pieceValue(piece) < pieceValue(best.piece)) best = { piece, row, col };
    });
    return best;
  }
  function workerThreatTargetCells(boardState, piece, row, col) {
    if (["colossus", "bigRook", "bigBishop"].includes(piece?.type)) {
      const anchorRow = Number.isInteger(piece.anchorRow) ? piece.anchorRow : row;
      const anchorCol = Number.isInteger(piece.anchorCol) ? piece.anchorCol : col;
      const cells = colossusCells(anchorRow, anchorCol).filter((cell) => get(boardState, cell.row, cell.col) === piece);
      return cells.length ? cells : [{ row, col }];
    }
    return [{ row, col }];
  }
  function workerActionCaptureSwing(boardState, action, color) {
    const pieces = workerActionCaptureTargets(boardState, action, color);
    return {
      value: pieces.reduce((sum, piece) => sum + workerStrategicPieceValue(boardState, piece), 0),
      count: pieces.length,
      decisive: pieces.some((piece) => isWorkerDecisiveCaptureTarget(boardState, piece))
    };
  }
  function workerActionCaptureEntries(boardState, action, color) {
    if (!boardState || action?.type !== "move") return [];
    const moving = get(boardState, action.from?.row, action.from?.col);
    if (!moving) return [];
    const move = action.move || {};
    if (move.substitutionSwap) return [];
    const entries = [];
    const seen = /* @__PURE__ */ new Set();
    const add = (row, col) => {
      if (!Number.isInteger(row) || !Number.isInteger(col)) return;
      const target = get(boardState, row, col);
      if (!canWorkerCaptureTarget(color, target, moving, boardState, {
        allowBasicTrainingCapture: Boolean(move.basicTrainingCapture)
      })) return;
      const key = target.id || `${row}:${col}`;
      if (seen.has(key)) return;
      seen.add(key);
      entries.push({ piece: target, row, col });
    };
    workerPortalCaptureCells(move).forEach((cell) => add(cell.row, cell.col));
    add(move.capturedRow, move.capturedCol);
    add(move.jumpCapture?.row, move.jumpCapture?.col);
    if (Array.isArray(move.sectorCells)) move.sectorCells.forEach((cell) => add(cell.row, cell.col));
    if (Array.isArray(move.colossusLandingCaptures)) move.colossusLandingCaptures.forEach((cell) => add(cell.row, cell.col));
    if (Array.isArray(move.bigRookLandingCaptures)) move.bigRookLandingCaptures.forEach((cell) => add(cell.row, cell.col));
    return entries;
  }
  function workerActionCaptureTargets(boardState, action, color) {
    return workerActionCaptureEntries(boardState, action, color).map((entry) => entry.piece);
  }
  function workerWorthwhileCapturePreference(boardState, action, color) {
    if (!boardState || action?.type !== "move") return 0;
    const moving = get(boardState, action.from?.row, action.from?.col);
    if (!moving || moving.color !== color || isWorkerDecisiveCaptureTarget(boardState, moving)) return 0;
    const swing = workerActionCaptureSwing(boardState, action, color);
    if (!swing.value || swing.decisive) return 0;
    const movingValue = workerStrategicPieceValue(boardState, moving);
    const threat = workerBestCaptureThreatAfterMove(boardState, action, moving);
    return Math.round(scoreExchangePreference({
      movingValue,
      capturedValues: workerActionCaptureTargets(boardState, action, color).map((piece) => workerStrategicPieceValue(boardState, piece)),
      canBeRecaptured: Boolean(threat),
      unitValue: PIECE_VALUES.pawn
    }) * 180);
  }
  function workerLowValueCaptureOrderingPenalty(boardState, action, color) {
    if (!boardState || action?.type !== "move") return 0;
    const moving = get(boardState, action.from?.row, action.from?.col);
    if (!moving || moving.color !== color || isWorkerDecisiveCaptureTarget(boardState, moving)) return 0;
    const swing = workerActionCaptureSwing(boardState, action, color);
    if (!swing.value || swing.decisive) return 0;
    const movingValue = workerStrategicPieceValue(boardState, moving);
    if (swing.value + Math.max(55, movingValue * 0.12) >= movingValue) return 0;
    const threat = workerBestCaptureThreatAfterMove(boardState, action, moving);
    if (!threat) return 0;
    const threatValue = pieceValue(threat.piece);
    const tradeLoss = movingValue - swing.value;
    let penalty = 280 + tradeLoss * 1.6;
    if (threatValue < movingValue) penalty += (movingValue - threatValue) * 0.85;
    if (threatValue <= swing.value + 35) penalty += 180;
    return Math.min(2200, penalty);
  }
  function workerBadLowValueCapturePenalty(beforeState, afterState, action, color) {
    if (!beforeState || !afterState || action?.type !== "move") return 0;
    const moving = get(beforeState, action.from?.row, action.from?.col);
    if (!moving || moving.color !== color || isWorkerDecisiveCaptureTarget(beforeState, moving)) return 0;
    const swing = workerActionCaptureSwing(beforeState, action, color);
    if (!swing.value || swing.decisive) return 0;
    const movingValue = workerStrategicPieceValue(beforeState, moving);
    if (swing.value + Math.max(55, movingValue * 0.12) >= movingValue) return 0;
    const destination = workerActionDestination(action) || action.move || {};
    const afterPiece = findWorkerPieceByRef(afterState, { id: moving.id, row: destination.row, col: destination.col });
    if (!afterPiece) return 0;
    const threat = workerBestCaptureThreat(afterState, afterPiece.piece, afterPiece.row, afterPiece.col);
    if (!threat) return 0;
    const threatValue = pieceValue(threat.piece);
    const tradeLoss = movingValue - swing.value;
    let penalty = 320 + tradeLoss * 1.9;
    if (threatValue < movingValue) penalty += (movingValue - threatValue) * 1.15;
    if (threatValue <= swing.value + 35) penalty += 220;
    return Math.min(3e3, penalty);
  }
  function workerBestCaptureThreatAfterMove(boardState, action, moving) {
    if (!boardState || action?.type !== "move" || !moving) return null;
    const move = action.move || {};
    if (!Number.isInteger(move.row) || !Number.isInteger(move.col)) return null;
    if (move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.setLogDirection) return null;
    const next = cloneState(boardState);
    const nextMoving = get(next, action.from?.row, action.from?.col);
    if (!nextMoving) return null;
    const capturedEntries = workerActionCaptureEntries(boardState, action, moving.color);
    clearPieceCells(next, nextMoving);
    capturedEntries.forEach(({ piece }) => clearPieceCells(next, piece));
    set(next, move.row, move.col, nextMoving);
    return workerBestCaptureThreat(next, nextMoving, move.row, move.col);
  }
  function workerExplosionPieceOrderingScore(boardState, piece, perspectiveColor, moving) {
    const value = pieceValue(piece);
    if (piece.color === perspectiveColor) {
      if (isWorkerDecisiveCaptureTarget(boardState, piece)) return -12e4;
      if (piece === moving) {
        if (value <= 130) return -90;
        if (value < 500) return -(600 + value * 0.9);
        return -(2200 + value * 1.2);
      }
      if (value <= 130) return -55;
      if (value < 500) return -(220 + value * 0.55);
      return -(900 + value * 0.9);
    }
    if (isWorkerDecisiveCaptureTarget(boardState, piece)) return 1e5;
    if (value <= 130) return 80;
    if (value < 500) return 260 + value * 0.7;
    return 850 + value * 1.15;
  }
  function workerExplosiveCaptureOrderingScore(boardState, action, color, perspectiveColor) {
    if (action?.type !== "move") return 0;
    const moving = get(boardState, action.from?.row, action.from?.col);
    if (!moving) return 0;
    const entries = workerActionCaptureEntries(boardState, action, color);
    const explosiveEntries = entries.filter(({ piece }) => piece?.explosive);
    if (!explosiveEntries.length) return 0;
    const move = action.move || {};
    const counted = new Set(entries.map(({ piece }) => piece));
    let score = 0;
    explosiveEntries.forEach(({ row, col }) => {
      for (let dr = -1; dr <= 1; dr += 1) {
        for (let dc = -1; dc <= 1; dc += 1) {
          const targetRow2 = row + dr;
          const targetCol2 = col + dc;
          if (!inBounds(targetRow2, targetCol2, boardState)) continue;
          let item = get(boardState, targetRow2, targetCol2);
          if (targetRow2 === action.from?.row && targetCol2 === action.from?.col) item = null;
          if (targetRow2 === move.row && targetCol2 === move.col) item = moving;
          if (!item || isFrozenPiece(item) || isIndirectAttackImmunePiece(item) || isBombImmuneNeutralPiece(item) || counted.has(item)) continue;
          const key = item.id || `${targetRow2}:${targetCol2}`;
          if (counted.has(key)) continue;
          counted.add(key);
          score += workerExplosionPieceOrderingScore(boardState, item, perspectiveColor, moving);
        }
      }
    });
    return score;
  }
  // Grafted from engine.optimized.js (our-only addition): the same per-card
  // threat-value logic cardThreatScore below sums over a whole hand,
  // factored out into a single-card function so other grafted callers
  // (workerBestEnemyCardThreat, rootCardComboFollowupBonus,
  // isTacticallyRelevantCardAction) can reuse it without duplicating the
  // scoring rules. cardThreatScore itself is untouched real-file logic.
  function singleCardThreatValue(boardState, card, color) {
    if (!card || card.emptySlot || card.used || card.recovering || isWorkerCardPendingNextTurn(card)) return 0;
    if (isWorkerTurnExclusiveCardBlocked(boardState, card, color)) return 0;
    const value = cardValue(card);
    const targets = generateCardTargets(boardState, card, color);
    if (!targets.length) return -Math.min(260, value * 0.22);
    let score = value * 0.34;
    if (card.effect === "bloodCard") score += bloodEffectScore(boardState, card.bloodEffectId || "summon", color) * 0.4;
    if (["summonColossus", "bigRook", "shotgunKing", "wizard", "merchantGuild", "collapse", "ultimatum", "holdout", "chimera", "finalWeapon", "localConscription", "recycling", "fanaticalRitual", "undergroundBunker", "whiteBox", "blackBox", "trolley", "blueJeans", "randomRoulette"].includes(card.effect)) score += 260;
    if (["disarm", "witchTrial", "severance", "inertia", "traitor", "vortex", "socialism", "dice", "panic", "deathSquad", "desperado"].includes(card.effect)) score += Math.min(320, targets.length * 55);
    if (["royalShield", "encouragement", "queensGambit", "lastResistance", "coronation", "alekhineMachineGun", "fileSurge", "rookLift", "underpromotion", "canceling", "stake", "enPassantBang", "pawnStorm", "queenAfterimage"].includes(card.effect)) score += 160;
    return score;
  }
  function cardThreatScore(boardState, color) {
    let score = 0;
    deck(boardState, color).forEach((card) => {
      if (!card || card.emptySlot || card.used || card.recovering || isWorkerCardPendingNextTurn(card)) return;
      if (isWorkerTurnExclusiveCardBlocked(boardState, card, color)) return;
      const value = cardValue(card);
      const targets = generateCardTargets(boardState, card, color);
      if (!targets.length) {
        score -= Math.min(260, value * 0.22);
        return;
      }
      score += value * 0.34;
      if (card.effect === "bloodCard") score += bloodEffectScore(boardState, card.bloodEffectId || "summon", color) * 0.4;
      if (["summonColossus", "bigRook", "shotgunKing", "wizard", "merchantGuild", "collapse", "ultimatum", "holdout", "chimera", "finalWeapon", "localConscription", "recycling", "fanaticalRitual", "undergroundBunker", "whiteBox", "blackBox", "trolley", "blueJeans", "randomRoulette"].includes(card.effect)) score += 260;
      if (["disarm", "witchTrial", "severance", "inertia", "traitor", "vortex", "socialism", "dice", "panic", "deathSquad", "desperado"].includes(card.effect)) score += Math.min(320, targets.length * 55);
      if (["royalShield", "encouragement", "queensGambit", "lastResistance", "coronation", "alekhineMachineGun", "fileSurge", "rookLift", "underpromotion", "canceling", "stake", "enPassantBang", "pawnStorm", "queenAfterimage"].includes(card.effect)) score += 160;
    });
    return score;
  }
  function campaignStrategicScore(boardState, color) {
    const setup = boardState?.campaign?.setup;
    if (!setup) return 0;
    const enemy = opponent(color);
    let score = 0;
    if (setup === "machineRebellion") {
      const ownColossi = piecesMatching(boardState, (piece) => piece.color === color && piece.type === "colossus");
      const enemyColossi = piecesMatching(boardState, (piece) => piece.color === enemy && piece.type === "colossus");
      score += ownColossi.reduce((sum, { piece, row, col }) => sum + 900 + (piece.hp || 0) * 260 + royalPressureFromSquare(boardState, row, col, color) * 0.5, 0);
      score += enemyColossi.reduce((sum, { piece, row, col }) => sum + (isSquareAttacked(boardState, row, col, color) ? 560 : 0) + Math.max(0, (piece.maxHp || 3) - (piece.hp || piece.maxHp || 3)) * 280, 0);
    } else if (setup === "maharajaSepoy") {
      piecesMatching(boardState, (piece) => piece.color === color && piece.type === "amazon").forEach(({ row, col }) => {
        score += 900 + royalPressureFromSquare(boardState, row, col, color) * 0.7;
      });
    } else if (setup === "magicParty") {
      const wizards = piecesMatching(boardState, (piece) => piece.color === color && pieceHasAbility(piece, "wizard"));
      score += wizards.length * 420 + wizards.reduce((sum, { piece }) => sum + (piece.mana || 0) * 120, 0);
    } else if (setup === "timeTraveler") {
      const traveler = findPieceByType(boardState, color, "timeTraveler");
      if (traveler) {
        score += 1300;
        score += (Number(boardState.fullMove) || 1) >= 20 ? 4e3 : (Number(boardState.fullMove) || 1) * 90;
        if (timeTravelerState(boardState)?.attackEnabledFor === color) score += 360;
      }
    } else if (setup === "bloodMoon") {
      score += bloodMoonStrategicScore(boardState, color);
    } else if (setup === "shotgunKing") {
      const shotgun = findPieceByType(boardState, color, "shotgunKing");
      if (shotgun) score += 900 + (shotgun.piece.ammo || 0) * 190 + (shotgun.piece.hp || 0) * 220;
    } else if (setup === "wideBoard") {
      score += centerOccupationScore(boardState, color) * 26;
    } else if (setup === "auctionChess") {
      score += materialBalance(boardState, color) * 0.08;
    }
    return score;
  }
  function royalPressureFromSquare(boardState, row, col, color) {
    return criticalPieces(boardState, opponent(color)).reduce((sum, target) => {
      const distance = Math.max(Math.abs(target.row - row), Math.abs(target.col - col));
      return sum + Math.max(0, 10 - distance) * 70;
    }, 0);
  }
  function centerOccupationScore(boardState, color) {
    let score = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color === color && !isCritical(piece) && piece.type !== "wall") score += centerScore(row, col, boardState);
    });
    return score;
  }
  function materialBalance(boardState, aiColor) {
    let score = 0;
    forEachPiece(boardState, (piece) => {
      if (piece.type === "wall" || isWorkerDecisiveCaptureTarget(boardState, piece)) return;
      score += (piece.color === aiColor ? 1 : -1) * workerStrategicPieceValue(boardState, piece);
    });
    return score;
  }
  function workerTradablePieceCount(boardState) {
    let count = 0;
    forEachPiece(boardState, (piece) => {
      if (["wall", "football", "blackHole"].includes(piece.type) || isWorkerDecisiveCaptureTarget(boardState, piece)) return;
      count += 1;
    });
    return count;
  }
  function positionalScore(boardState, color) {
    const endgame = isEndgame(boardState);
    let score = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== color || piece.type === "wall") return;
      score += pieceSquareBonus(boardState, piece, row, col, endgame);
      if (["knight", "bishop", "protestant", "cardinal", "pawn", "fanatic", "checker", "checkerKing"].includes(renderType(piece))) {
        score += centerControlBonus(boardState, piece, row, col);
      }
    });
    return score;
  }
  function pieceSquareBonus(boardState, piece, row, col, endgame) {
    const type = renderType(piece);
    const center = centerScore(row, col, boardState);
    const pawnHomeRow = piece.color === "white" ? boardRowCount(boardState) - 2 : 1;
    const advance = piece.color === "white" ? pawnHomeRow - row : row - pawnHomeRow;
    if (isWorkerKingRole(boardState, piece) && workerHasRacingKingObjective(boardState, piece.color)) {
      return kingPositionBonus(boardState, piece, row, col, endgame);
    }
    if (type === "pawn" || type === "fanatic") return center * 18 + Math.max(0, advance) * 18;
    if (type === "checker") return center * 28 + Math.max(0, advance) * 12;
    if (type === "checkerKing") return center * 40 + 60;
    if (type === "knight") return center * 36 - edgePenalty(row, col, boardState) * 35;
    if (["bishop", "protestant", "cardinal"].includes(type)) return center * 34 + diagonalMobilityHint(row, col, boardState) * 8;
    if (["rook", "queen", "amazon", "wizard"].includes(type)) return center * 12;
    if (isCritical(piece)) return kingPositionBonus(boardState, piece, row, col, endgame);
    return center * 16;
  }
  function kingPositionBonus(boardState, piece, row, col, endgame) {
    const enemy = opponent(piece.color);
    const inCheck = isSquareAttacked(boardState, row, col, enemy);
    if (isWorkerKingRole(boardState, piece) && workerHasRacingKingObjective(boardState, piece.color)) {
      const goalRow = workerRacingKingGoalRow(boardState, piece.color);
      const distance = Math.abs(row - goalRow);
      const progress = Math.max(0, boardRowCount(boardState) - 1 - distance);
      return progress * 620 + (distance <= 2 ? (3 - distance) * 1500 : 0) - (inCheck ? 2400 : 0);
    }
    if (endgame) return centerScore(row, col, boardState) * 32;
    let score = 0;
    const homeDistance = piece.color === "white" ? boardRowCount(boardState) - 1 - row : row;
    const centerCol = (boardColCount(boardState) - 1) / 2;
    const fileCenter = centerCol - Math.abs(col - centerCol);
    if (!inCheck) {
      score -= homeDistance * 260;
      score -= Math.max(0, fileCenter) * 70;
      if (homeDistance >= 2) score -= 520;
      if (centerSquares(boardState).some(([r, c]) => Math.abs(row - r) <= 1 && Math.abs(col - c) <= 1)) score -= 720;
    }
    queenDirections().forEach(([dr, dc]) => {
      const r = row + dr;
      const c = col + dc;
      const neighbor = get(boardState, r, c);
      if (neighbor?.color === piece.color && !isCritical(neighbor)) score += 42;
    });
    return score;
  }
  function centerControlBonus(boardState, piece, row, col) {
    return centerSquares(boardState).reduce((sum, [targetRow2, targetCol2]) => {
      if (!attacksSquare(boardState, piece, row, col, targetRow2, targetCol2)) return sum;
      const type = renderType(piece);
      if (type === "pawn" || type === "fanatic") return sum + 72;
      if (type === "knight") return sum + 55;
      return sum + 58;
    }, 0);
  }
  function centerSquares(boardState) {
    const rowMid = (boardRowCount(boardState) - 1) / 2;
    const colMid = (boardColCount(boardState) - 1) / 2;
    const rows = [.../* @__PURE__ */ new Set([Math.floor(rowMid), Math.ceil(rowMid)])];
    const cols = [.../* @__PURE__ */ new Set([Math.floor(colMid), Math.ceil(colMid)])];
    return rows.flatMap((row) => cols.map((col) => [row, col]));
  }
  function centerScore(row, col, boardState = null) {
    const rowMid = ((boardState ? boardRowCount(boardState) : workerBoardRows) - 1) / 2;
    const colMid = ((boardState ? boardColCount(boardState) : workerBoardCols) - 1) / 2;
    return Math.max(0, rowMid - Math.abs(row - rowMid)) + Math.max(0, colMid - Math.abs(col - colMid));
  }
  function edgePenalty(row, col, boardState = null) {
    const rows = boardState ? boardRowCount(boardState) : workerBoardRows;
    const cols = boardState ? boardColCount(boardState) : workerBoardCols;
    return (row === 0 || row === rows - 1 ? 1 : 0) + (col === 0 || col === cols - 1 ? 1 : 0);
  }
  function diagonalMobilityHint(row, col, boardState = null) {
    const rows = boardState ? boardRowCount(boardState) : workerBoardRows;
    const cols = boardState ? boardColCount(boardState) : workerBoardCols;
    return Math.min(row, col, rows - 1 - row, cols - 1 - col);
  }
  function isEndgame(boardState) {
    let nonPawnMaterial = 0;
    let majorPieces = 0;
    forEachPiece(boardState, (piece) => {
      if (piece.type === "wall" || isCritical(piece) || ["pawn", "fanatic"].includes(piece.type)) return;
      nonPawnMaterial += pieceValue(piece);
      if (pieceValue(piece) >= 500) majorPieces += 1;
    });
    return nonPawnMaterial < 2600 || majorPieces <= 2 || (Number(boardState.moveCount) || 0) >= 32;
  }
  // Grafted from engine.optimized.js (our-only addition): kingZoneThreatApprox
  // replaces kingSafetyScore/royalPressureScore's per-neighbor-square
  // isSquareAttacked calls (each an O(pieces) scan, so 8 exact queries per
  // critical piece) with a single O(pieces) pass over the enemy pieces,
  // scoring by distance-to-king instead of exact reachability. A deliberate
  // search-speed/eval-strength tradeoff (isSquareAttacked was measured as
  // evaluateState's dominant cost), not a bug fix -- the single exact "is
  // the king actually in check" isSquareAttacked call at the critical
  // square itself is kept as-is in both functions below.
  function kingZoneThreatApprox(boardState, kingRow, kingCol, byColor) {
    let score = 0;
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color !== byColor || piece.type === "wall") return;
      if (isFrozenPiece(piece) || isWorkerStakedPiece(piece)) return;
      const dist = Math.max(Math.abs(row - kingRow), Math.abs(col - kingCol));
      if (dist === 0 || dist > 3) return;
      const reach = pieceValue(piece) >= 300 ? 3 : 1;
      if (dist <= reach) score += Math.max(24, 96 - dist * 24);
    });
    return score;
  }
  function kingSafetyScore(boardState, color) {
    const enemy = opponent(color);
    const critical = criticalPieces(boardState, color);
    if (!critical.length) return campaignSurvivalSafetyScore(boardState, color);
    let score = 0;
    critical.forEach(({ row, col }) => {
      if (isSquareAttacked(boardState, row, col, enemy)) score -= 2800;
      queenDirections().forEach(([dr, dc]) => {
        const r = row + dr;
        const c = col + dc;
        if (!inBounds(r, c)) {
          score -= 18;
          return;
        }
        const neighbor = get(boardState, r, c);
        if (neighbor?.color === color) score += 35;
      });
      score -= kingZoneThreatApprox(boardState, row, col, enemy);
    });
    return score;
  }
  function royalPressureScore(boardState, color) {
    const enemy = opponent(color);
    let score = 0;
    const targets = criticalPieces(boardState, enemy);
    if (!targets.length) targets.push(...campaignSurvivalPieces(boardState, enemy));
    targets.forEach(({ row, col }) => {
      if (isSquareAttacked(boardState, row, col, color)) score += 1800;
      score += kingZoneThreatApprox(boardState, row, col, color);
    });
    return score;
  }
  function sideHasSurvivalPiece(boardState, color) {
    if (boardState?.democracy?.[color]) return workerHasDemocracyPawn(boardState, color);
    return criticalPieces(boardState, color).length > 0 || boardState.regency?.[color] && sideHasQueen(boardState, color) || campaignSurvivalPieces(boardState, color).length > 0;
  }
  function resolveWorkerBatchRoyalDefeats(boardState, removed) {
    const affectedColors = new Set((removed || []).map((piece) => piece.color).filter((color) => COLORS.includes(color)));
    const defeatedColors = [...affectedColors].filter((color) => !sideHasSurvivalPiece(boardState, color));
    if (!defeatedColors.length) return false;
    boardState.mode = "gameover";
    boardState.winner = defeatedColors.length > 1 ? "draw" : opponent(defeatedColors[0]);
    return true;
  }
  function sideHasQueen(boardState, color) {
    let found = false;
    forEachPiece(boardState, (piece) => {
      if (!found && piece?.color === color && piece.type === "queen") found = true;
    });
    return found;
  }
  function workerFindRegencyHeir(boardState, color) {
    let marked = null;
    let firstQueen = null;
    forEachPiece(boardState, (piece, row, col) => {
      if (!piece || piece.color !== color) return;
      if (!marked && piece.regencyHeir) marked = { piece, row, col };
      if (!firstQueen && piece.type === "queen") firstQueen = { piece, row, col };
    });
    return marked || firstQueen;
  }
  function workerEnsureRegencyHeir(boardState, color, excludedPiece = null) {
    if (!boardState?.regency?.[color] || !boardState?.kingDead?.[color]) return null;
    const heir = excludedPiece ? (() => {
      let marked = null;
      let firstQueen = null;
      forEachPiece(boardState, (piece, row, col) => {
        if (!piece || piece === excludedPiece || piece.color !== color) return;
        if (!marked && piece.regencyHeir) marked = { piece, row, col };
        if (!firstQueen && piece.type === "queen") firstQueen = { piece, row, col };
      });
      return marked || firstQueen;
    })() : workerFindRegencyHeir(boardState, color);
    forEachPiece(boardState, (piece) => {
      if (piece?.color === color && piece !== heir?.piece) delete piece.regencyHeir;
    });
    if (heir?.piece) heir.piece.regencyHeir = true;
    return heir;
  }
  function workerCriticalCaptureScore(boardState, captured, attackerColor, aiColor) {
    if (!captured) return 0;
    if (!(captured.editorRoyal || isWorkerKingRole(boardState, captured)) && septemberQueueRecurrence(boardState, captured, attackerColor)) return 0;
    const defeatedColor = captured.color;
    const direction = attackerColor === aiColor ? 1 : -1;
    if (isDemocracyProtectedMerchant(boardState.democracy)) ;
    if (isDemocracyProtectedRoyal(boardState.democracy, captured)) {
      if (!boardState.kingDead) boardState.kingDead = {};
      boardState.kingDead[defeatedColor] = true;
      if (boardState.zugzwang && !findWorkerKingRole(boardState, defeatedColor)) boardState.zugzwang[defeatedColor] = false;
      return direction * 900;
    }
    if (captured.type === "queen" && boardState.kingDead?.[defeatedColor] && boardState.regency?.[defeatedColor] && (captured.regencyHeir || !workerEnsureRegencyHeir(boardState, defeatedColor, captured))) {
      boardState.mode = "gameover";
      boardState.winner = attackerColor;
      return direction * 1e5;
    }
    if (captured.regencyHeir && boardState.kingDead?.[defeatedColor] && boardState.regency?.[defeatedColor]) {
      boardState.mode = "gameover";
      boardState.winner = attackerColor;
      return direction * 1e5;
    }
    if (isCritical(captured)) {
      if (boardState.regency?.[defeatedColor] && sideHasQueen(boardState, defeatedColor)) {
        if (!boardState.kingDead) boardState.kingDead = {};
        boardState.kingDead[defeatedColor] = true;
        workerEnsureRegencyHeir(boardState, defeatedColor);
        return direction * 1600;
      }
      boardState.mode = "gameover";
      boardState.winner = attackerColor;
      return direction * 1e5;
    }
    return 0;
  }
  function campaignSurvivalSafetyScore(boardState, color) {
    const pieces = campaignSurvivalPieces(boardState, color);
    if (!pieces.length) return -5e4;
    const enemy = opponent(color);
    return pieces.reduce((score, { piece, row, col }) => {
      const attacked = isSquareAttacked(boardState, row, col, enemy);
      return score + (attacked ? -900 : 80) + Math.min(260, pieceValue(piece) * 0.08);
    }, 0);
  }
  function campaignSurvivalPieces(boardState, color) {
    const setup = boardState?.campaign?.setup;
    const survival = [];
    const addType = (type) => {
      piecesMatching(boardState, (piece) => piece.color === color && piece.type === type).forEach((entry) => survival.push(entry));
    };
    if (setup === "machineRebellion") addType("colossus");
    else if (setup === "maharajaSepoy") addType("amazon");
    else if (setup === "magicParty") addType("wizard");
    else if (setup === "knightJourney" || setup === "crazyKnightJourney" || setup === "knightGame") addType("knight");
    else if (setup === "timeTraveler") addType("timeTraveler");
    else if (setup === "bloodMoon") addType("vampireLord");
    else if (setup === "shotgunKing") addType("shotgunKing");
    return survival;
  }
  function specialThreatScore(boardState, color) {
    const enemy = opponent(color);
    let score = 0;
    const enemyMerchant = criticalPieces(boardState, enemy).some(({ piece }) => piece.type === "merchant");
    if (enemyMerchant) {
      score += royalPressureScore(boardState, color) * 2.4;
      score += materialBalance(boardState, color) * -0.04;
    }
    const enemyHpPieces = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color === enemy && isHpPiece(piece)) enemyHpPieces.push({ piece, row, col });
    });
    enemyHpPieces.forEach(({ piece, row, col }) => {
      if (isSquareAttacked(boardState, row, col, color)) score += 520 + Math.max(0, (piece.maxHp || 3) - (piece.hp || piece.maxHp || 3)) * 180;
      if (specialAttackZone(boardState, piece, row, col).some((cell) => criticalPieces(boardState, color).some((own) => own.row === cell.row && own.col === cell.col))) {
        score -= 900;
      }
    });
    const hazards = boardState.delayedHazards || [];
    hazards.forEach((hazard) => {
      const owner = hazard.owner || enemy;
      hazard.cells?.forEach((cell) => {
        const item = get(boardState, cell.row, cell.col);
        if (!item || isFrozenPiece(item)) return;
        const delta = isCritical(item) ? 1200 : pieceValue(item) * 0.5;
        score += item.color === color && owner !== color ? -delta : item.color === enemy && owner === color ? delta : 0;
      });
    });
    const enemyWizard = enemyPieces(boardState, color).find(({ piece }) => pieceHasAbility(piece, "wizard") && (piece.mana ?? 0) > 0);
    if (enemyWizard) {
      criticalPieces(boardState, color).forEach(({ row, col }) => {
        const escapeSquares = queenDirections().filter(([dr, dc]) => {
          const r = row + dr;
          const c = col + dc;
          return inBounds(r, c) && !get(boardState, r, c) && !isSquareAttacked(boardState, r, c, enemy);
        }).length;
        if (escapeSquares <= 1) score -= 850;
      });
    }
    return score;
  }
  function bloodMoonStrategicScore(boardState, color) {
    if (!bloodMoonState(boardState)) return 0;
    const vampireColor = bloodMoonVampireColor(boardState);
    if (color !== vampireColor) return antiVampireStrategicScore(boardState, color, vampireColor);
    const lord = findPieceByType(boardState, color, "vampireLord");
    if (!lord) return -6e4;
    const enemy = opponent(color);
    let score = 0;
    const bloodCards = deck(boardState, color).filter((card) => card?.effect === "bloodCard" && !card.used).length;
    score += bloodCards * 190;
    if (usesBloodMoonNightMovement(boardState, color)) score += 260 + bloodNightAttackScore(boardState, color) * 90;
    if (isSquareAttacked(boardState, lord.row, lord.col, enemy)) score -= 1800;
    if (vampireLongRangeDanger(boardState, color)) score -= 520;
    friendlyPieces(boardState, color).forEach(({ piece }) => {
      if (piece.type === "coffin") score += 360;
    });
    enemyPieces(boardState, color).forEach(({ piece }) => {
      if (piece.bloodCurse) score += Math.min(420, pieceValue(piece) * 0.35);
    });
    const data = bloodMoonState(boardState);
    if (data?.veilUntil && data.veilUntil > bloodMoonHalfTurns(boardState)) score += 260;
    return score;
  }
  function antiVampireStrategicScore(boardState, color, vampireColor) {
    const lord = findPieceByType(boardState, vampireColor, "vampireLord");
    if (!lord) return 6e4;
    let score = 0;
    if (isSquareAttacked(boardState, lord.row, lord.col, color)) score += 1700;
    if (vampireLongRangeDanger(boardState, vampireColor)) score += 420;
    score -= deck(boardState, vampireColor).filter((card) => card?.effect === "bloodCard" && !card.used).length * 210;
    score -= friendlyPieces(boardState, vampireColor).filter(({ piece }) => piece.type === "coffin").length * 340;
    if (usesBloodMoonNightMovement(boardState, vampireColor)) score -= 260 + bloodNightAttackScore(boardState, vampireColor) * 80;
    friendlyPieces(boardState, color).forEach(({ piece, row, col }) => {
      if (pieceValue(piece) >= 500 && isSquareAttacked(boardState, row, col, vampireColor)) score -= pieceValue(piece) * 0.25;
    });
    return score;
  }
  function bloodMoonVampireColor(boardState) {
    const whiteLord = findPieceByType(boardState, "white", "vampireLord");
    if (whiteLord) return "white";
    const blackLord = findPieceByType(boardState, "black", "vampireLord");
    if (blackLord) return "black";
    return boardState?.campaign?.playerColor === "black" ? "black" : "white";
  }
  function specialAttackZone(boardState, piece, row, col) {
    if (piece.type === "shotgunKing") {
      return queenDirections().flatMap(([dr, dc]) => shotgunBlastCells(row, col, [dr, dc]));
    }
    if (piece.type === "colossus") return colossusAttackSectors(boardState, row, col, piece.color).flat();
    if (["bigRook", "bigBishop"].includes(piece.type)) return bigRookMoves(boardState, row, col, piece.color).flatMap((move) => move.highlightCells || []);
    return [];
  }
  function workerHasDoubleCheckVictory(boardState, color) {
    if (!boardState.binaMate?.[color] || boardState.mode === "gameover") return false;
    const royals = piecesMatching(boardState, (piece) => piece.color === opponent(color) && (isRoyalPiece(piece) || isWorkerRegencyRoyalHeir(boardState, piece)));
    return royals.some((royal) => {
      const checkedAttackers = /* @__PURE__ */ new Set();
      forEachPiece(boardState, (piece, row, col) => {
        if (piece?.color !== color) return;
        if (workerPieceCanAttackRoyalForDoubleCheck(boardState, piece, row, col, royal)) {
          checkedAttackers.add(piece.id || piece);
        }
      });
      return hasDoubleCheckAttackers(checkedAttackers);
    });
  }
  function workerPieceCanAttackRoyalForDoubleCheck(boardState, piece, row, col, royal) {
    if (!piece || !royal || piece.color === royal.color || !(isRoyalPiece(royal) || isWorkerRegencyRoyalHeir(boardState, royal))) return false;
    if (renderType(piece) === "guard" || isFrozenPiece(piece) || isDiceLockedPiece(boardState, piece)) return false;
    if (isWorkerUndergroundBunkerKing(piece) || isWorkerStakedPiece(piece)) return false;
    if ((Number(piece.disarmed?.remaining) || 0) > 0) return false;
    if (isWorkerMannerCaptureLocked(boardState, piece) || isWorkerInitiativeCaptureLocked(boardState, piece.color)) return false;
    if (!canWorkerCaptureTarget(piece.color, royal, piece, boardState)) return false;
    return generateMovesForPiece(boardState, piece, row, col, { ignoreGlobalCaptureForce: true }).filter((move) => isWorkerBaseMoveAllowed(boardState, piece, row, col, move)).some((move) => moveThreatensSquare(move, royal.row, royal.col));
  }
  function resolveWorkerDoubleCheckThreats(boardState, actorColor) {
    if (boardState.mode === "gameover") return "";
    const orderedColors = [actorColor, opponent(actorColor)].filter((color, index, colors) => COLORS.includes(color) && colors.indexOf(color) === index);
    const winner = orderedColors.find((color) => workerHasDoubleCheckVictory(boardState, color)) || "";
    if (!winner) return "";
    boardState.mode = "gameover";
    boardState.winner = winner;
    return winner;
  }
  function isSquareAttacked(boardState, row, col, byColor) {
    let attacked = false;
    forEachPiece(boardState, (piece, pieceRow, pieceCol) => {
      if (attacked || piece.color !== byColor || piece.type === "wall") return;
      if (isFrozenPiece(piece)) return;
      if (isWorkerStakedPiece(piece)) return;
      if (attacksSquare(boardState, piece, pieceRow, pieceCol, row, col)) attacked = true;
    });
    return attacked;
  }
  function workerShotgunKingAttacksSquare(boardState, piece, row, col, targetRow2, targetCol2) {
    const target = get(boardState, targetRow2, targetCol2);
    if (isIndirectAttackImmunePiece(target)) return false;
    const ammo = Math.max(0, Number(piece?.ammo) || 0);
    if (ammo >= SHOTGUN_BLAST_AMMO_COST && queenDirections().some((direction) => shotgunBlastCells(row, col, direction).some((cell) => cell.row === targetRow2 && cell.col === targetCol2))) return true;
    return ammo >= SHOTGUN_SNIPE_AMMO_COST && (targetRow2 === row || targetCol2 === col || Math.abs(targetRow2 - row) === Math.abs(targetCol2 - col)) && clearRay(boardState, row, col, targetRow2, targetCol2);
  }
  // Perf: per-evaluateStateComponents-call memo of attacksSquare (pure w.r.t. an unmutated board).
  function attacksSquare(boardState, piece, row, col, targetRow2, targetCol2) {
    const memo = ATTACK_MEMO;
    if (memo === null || memo.board !== boardState || !(row >= 0 && row < 64 && col >= 0 && col < 64 && targetRow2 >= 0 && targetRow2 < 64 && targetCol2 >= 0 && targetCol2 < 64) || (row | 0) !== row || (col | 0) !== col || (targetRow2 | 0) !== targetRow2 || (targetCol2 | 0) !== targetCol2) return attacksSquareRaw(boardState, piece, row, col, targetRow2, targetCol2);
    let m = memo.map.get(piece);
    if (m === void 0) { m = /* @__PURE__ */ new Map(); memo.map.set(piece, m); }
    const key = ((row * 64 + col) * 64 + targetRow2) * 64 + targetCol2;
    let v = m.get(key);
    if (v === void 0) { v = attacksSquareRaw(boardState, piece, row, col, targetRow2, targetCol2); m.set(key, v); }
    return v;
  }
  function attacksSquareRaw(boardState, piece, row, col, targetRow2, targetCol2) {
    if (piece.type === "hedgehog" && Number(boardState.turnsTaken?.[piece.color]) < Number(piece.bearMoveLockedUntilTurn)) return false;
    if (pieceHasAbility(piece, "campfire")) return false;
    const basicTrainingCapture = isWorkerBasicTrainingPawnCapture(boardState, piece, row, col, targetRow2, targetCol2);
    const royalCommandCapture = hasWorkerRoyalCommandCaptureAccess(boardState, piece);
    if (pieceHasAbility(piece, "guard") && !basicTrainingCapture && !royalCommandCapture) return false;
    if (isFrozenPiece(piece) || isPoisonStunned(piece) || isDiceLockedPiece(boardState, piece)) return false;
    if (isWorkerUndergroundBunkerKing(piece)) return false;
    if (isWorkerStakedPiece(piece)) return false;
    if (isWorkerMannerCaptureLocked(boardState, piece)) return false;
    const type = renderType(piece);
    const target = get(boardState, targetRow2, targetCol2);
    if (type === "siegeRam") {
      return workerSiegeRamMoves(boardState, piece, row, col).some((move) => move.highlightCells?.some((cell) => cell.row === targetRow2 && cell.col === targetCol2));
    }
    if (target && isIndirectAttackImmunePiece(target) && ["colossus", "shotgunKing"].includes(type)) return false;
    if (isFrozenPiece(target)) return false;
    if (!pieceHasAbility(piece, "missionary") && target && !canWorkerCaptureTarget(piece.color, target, piece, boardState, {
      allowBasicTrainingCapture: basicTrainingCapture
    })) return false;
    if (isWorkerHighGroundCaptureBlocked(boardState, piece, row, col, { row: targetRow2, col: targetCol2, basicTrainingCapture })) return false;
    if (isWorkerKingRole(boardState, piece) && workerImperialStudyMoves(boardState, row, col, piece).some((move) => move.row === targetRow2 && move.col === targetCol2)) return true;
    if (workerKillerKingMoves(boardState, row, col, piece).some((move) => move.row === targetRow2 && move.col === targetCol2)) return true;
    const dr = targetRow2 - row;
    const dc = targetCol2 - col;
    if (type === "missionary") return Math.abs(dr) === 1 && Math.abs(dc) === 1 && Boolean(target && target.color !== piece.color && COLORS.includes(target.color));
    if (isWorkerInitiativeCaptureLocked(boardState, piece?.color)) return false;
    if (isWorkerVanguardPawn(boardState, piece, row) && dr === pawnDir(boardState, piece.color) && Math.abs(dc) <= 1) return true;
    if (["pawn", "squire", "standardBearer"].includes(type)) {
      if (type === "pawn" && hasWorkerAdjacentKnightmaster(boardState, row, col, piece.color)) {
        return hasWorkerCaptureReadyAdjacentKnightmaster(boardState, row, col, piece.color) && workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c);
      }
      const dir = pawnDir(boardState, piece.color);
      if (type === "pawn" && boardState.pawnConversion?.[piece.color]) {
        if ((dr === dir || boardState.retreat?.[piece.color] && dr === -dir) && dc === 0) return true;
      } else if ((dr === dir || boardState.retreat?.[piece.color] && dr === -dir) && Math.abs(dc) === 1) return true;
      if ((type === "standardBearer" || type === "pawn" && hasSameRankStandardBearer(boardState, row, piece.color)) && hasWorkerCaptureReadySameRankStandardBearer(boardState, row, piece.color) && dr === 0 && Math.abs(dc) === 1) return true;
      return false;
    }
    if (type === "fanatic") {
      return applyWorkerPortalMoves(boardState, piece, fanaticMoves(boardState, row, col, piece.color)).some((move) => moveThreatensSquare(move, targetRow2, targetCol2));
    }
    if (basicTrainingCapture) return true;
    if (isWorkerCheckerType(type)) {
      if (!workerCheckerDirections(boardState, piece.color, type).some(([r, c]) => dr === r && dc === c)) return false;
      const landingRow = targetRow2 + dr;
      const landingCol = targetCol2 + dc;
      if (!inBounds(landingRow, landingCol, boardState) || get(boardState, landingRow, landingCol)) return false;
      return target ? canWorkerCaptureTarget(piece.color, target, piece, boardState) : true;
    }
    if (["knight", "pegasus", "unicorn"].includes(type)) {
      return workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || type === "knight" && boardState.cornerKick?.[piece.color] && isBoardCorner(row, col, boardRowCount(boardState), boardColCount(boardState)) && Math.abs(dr) === Math.abs(dc) && clearRay(boardState, row, col, targetRow2, targetCol2);
    }
    if (type === "assassin") {
      return workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || target && (isRoyalPiece(target) || isWorkerRegencyRoyalHeir(boardState, target)) && (dr === 0 || dc === 0 || Math.abs(dr) === Math.abs(dc)) && clearRay(boardState, row, col, targetRow2, targetCol2);
    }
    if (type === "royalKnight") return workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || boardState.royalKnightKing?.[piece.color] && Math.max(Math.abs(dr), Math.abs(dc)) === 1 || boardState.hillKing?.[piece.color] && isCenterTwoByTwoCell(row, col, boardRowCount(boardState), boardColCount(boardState)) && queenDirections().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
    if (type === "camel") return Math.abs(dr) === 3 && Math.abs(dc) === 1 || Math.abs(dr) === 1 && Math.abs(dc) === 3;
    if (type === "eagle") return eagleDeltas().some(([r, c]) => dr === r && dc === c);
    if (type === "knightmaster") return Math.abs(dr) === 1 && Math.abs(dc) === 1;
    if (type === "alibaba") return Math.abs(dr) === 2 && dc === 0 || Math.abs(dc) === 2 && dr === 0 || Math.abs(dr) === 2 && Math.abs(dc) === 2;
    if (type === "alfil") return Math.abs(dr) === 2 && Math.abs(dc) === 2;
    if (type === "ferz") return Math.abs(dr) === 1 && Math.abs(dc) === 1;
    if (type === "vampireLord") return usesBloodMoonNightMovement(boardState, piece.color) ? workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || queenDirections().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8)) : Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (type === "bat") return batMoves(boardState, row, col, piece.color).some((move) => move.row === targetRow2 && move.col === targetCol2);
    if (type === "coffin") return false;
    if (type === "timeTraveler") return timeTravelerState(boardState)?.attackEnabledFor === piece.color && Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (type === "darkWizard") return piece.darkMagicCircle ? workerDarkMagicCircleContains(boardState, piece, row, col, targetRow2, targetCol2) && Math.abs(dr) + Math.abs(dc) === 1 : Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (type === "recruiter") return royalCommandCapture && Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (["bigRook", "bigBishop"].includes(type)) return workerBigRookAttacksSquare(boardState, piece, row, col, targetRow2, targetCol2);
    if (type === "colossus") {
      return colossusAttackSectors(boardState, row, col, piece.color).some((sector) => sector.some((cell) => cell.row === targetRow2 && cell.col === targetCol2));
    }
    if (canUseWorkerHighwayMove(boardState, piece, row, col) && isHighwayCaptureAllowed(piece, target) && workerHighwayRayReaches(boardState, row, col, targetRow2, targetCol2)) return true;
    if (["vip", "crown"].includes(type)) return Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (type === "queen" && isWorkerRegencyRoyalHeir(boardState, piece) && boardState.kingKnight?.[piece.color]) {
      return Math.max(Math.abs(dr), Math.abs(dc)) === 1 || workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || boardState.hillKing?.[piece.color] && isCenterTwoByTwoCell(row, col, boardRowCount(boardState), boardColCount(boardState)) && queenDirections().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
    }
    if (type === "king") return Math.max(Math.abs(dr), Math.abs(dc)) === 1 || boardState.kingKnight?.[piece.color] && workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c);
    if (["man", "guard", "reaper"].includes(type)) return Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (type === "shotgunKing") return workerShotgunKingAttacksSquare(boardState, piece, row, col, targetRow2, targetCol2);
    if (type === "amazon" && workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c)) return true;
    if (type === "cardinal") return cardinalMoves(boardState, row, col, piece.color).some((move) => move.row === targetRow2 && move.col === targetCol2);
    if (type === "protestant") {
      return applyWorkerPortalMoves(boardState, piece, protestantMoves(boardState, row, col, piece.color)).some((move) => moveThreatensSquare(move, targetRow2, targetCol2));
    }
    if (type === "bishop" && boardState.bishopSnipe?.[piece.color] && bishopSnipeAttacksSquare(boardState, row, col, targetRow2, targetCol2)) return true;
    if (type === "bishop") return (boardState.reversal?.[piece.color] ? orthogonals() : diagonals()).some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
    if (type === "herald") return false;
    if (type === "cannon") {
      return applyWorkerPortalMoves(boardState, piece, cannonMoves(boardState, row, col, piece.color)).some((move) => moveThreatensSquare(move, targetRow2, targetCol2));
    }
    if (type === "rook") return (boardState.reversal?.[piece.color] ? diagonals() : orthogonals()).some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
    if (type === "dragon") return workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c);
    if (type === "magicGirl") {
      return boardState.magicGirlSurge?.[piece.color] ? workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || queenDirections().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8)) : Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    }
    if (type === "berserker") {
      const tier = berserkerMovementTier(workerUniqueAlliedPieceCount(boardState, piece.color));
      if (tier === "amazon") {
        return workerKnightDeltasForMove(boardState, row, col, piece.color).some(([r, c]) => dr === r && dc === c) || queenDirections().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
      }
      return Math.max(Math.abs(dr), Math.abs(dc)) === 1 || tier === "rook-king-step" && orthogonals().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
    }
    if (type === "slime") return (dr === 0 || dc === 0) && Math.abs(dr) + Math.abs(dc) === 3;
    if (type === "princess") return septemberPrincessHasQueenMovementCached(boardState.board, piece.color) ? queenDirections().some(([r, c]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, r, c, 8)) : Math.abs(dr) === 1 && Math.abs(dc) === 1;
    if (["siren", "undead", "hedgehog"].includes(type)) return Math.max(Math.abs(dr), Math.abs(dc)) === 1;
    if (type === "trickster") {
      const movementType = TRICKSTER_MOVEMENT_TYPES.includes(piece.tricksterMoveType) ? piece.tricksterMoveType : TRICKSTER_MOVEMENT_TYPES[0] || "queen";
      const moves = generateMovesForPiece(boardState, { ...piece, type: movementType }, row, col, { tricksterProjection: true });
      if (movementType === "siegeRam") {
        return moves.some((move) => move.highlightCells?.some((cell) => cell.row === targetRow2 && cell.col === targetCol2));
      }
      return moves.some((move) => moveThreatensSquare(move, targetRow2, targetCol2));
    }
    if (type === "wizard" || type === "idol") return false;
    if (type === "lobster") return dr === pawnDir(boardState, piece.color) && Math.abs(dc) <= 1;
    if (type === "primeMinister") return primeMinisterMoves(boardState, row, col, piece.color).some((move) => move.row === targetRow2 && move.col === targetCol2);
    if (type === "grasshopper") return grasshopperMoves(boardState, row, col, piece.color).some((move) => move.row === targetRow2 && move.col === targetCol2);
    if (type === "jester") return jesterMoves(boardState, row, col, piece.color).some((move) => move.row === targetRow2 && move.col === targetCol2);
    if (type === "hook") return applyWorkerPortalMoves(boardState, piece, hookMoves(boardState, row, col, piece.color)).some((move) => moveThreatensSquare(move, targetRow2, targetCol2));
    if (["queen", "bear", "amazon"].includes(type)) return queenDirections().some(([stepR, stepC]) => workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, 8));
    return false;
  }
  function workerRayReaches(boardState, row, col, targetRow2, targetCol2, stepR, stepC, limit = 8) {
    const maxSteps = limit === 8 ? boardRayLimit(boardState) : limit;
    const attacker = get(boardState, row, col) || null;
    const portalRule = workerPortalRule(boardState);
    const isTransparent = (target) => Boolean(
      target && (septemberOvertakeCanCross(boardState.overtake?.[attacker.color], attacker, target) || isGhostTransparentFor(attacker, target, renderType(attacker), boardState))
    );
    let nextRow = row + stepR;
    let nextCol = col + stepC;
    let step = 1;
    let portalTransit = false;
    while (step <= maxSteps && inBounds(nextRow, nextCol, boardState)) {
      const portalExit = portalRule ? workerPortalExitAt(boardState, nextRow, nextCol) : null;
      if (portalExit && (portalTransit || isWorkerCollapsedSquare(boardState, nextRow, nextCol) || isWorkerCollapsedSquare(boardState, portalExit.row, portalExit.col))) return false;
      if (nextRow === targetRow2 && nextCol === targetCol2) return true;
      const blocker = get(boardState, nextRow, nextCol);
      if (blocker && !isTransparent(blocker)) return false;
      if (portalExit) {
        if (portalExit.row === targetRow2 && portalExit.col === targetCol2) return true;
        const exitBlocker = get(boardState, portalExit.row, portalExit.col);
        if (exitBlocker && !isTransparent(exitBlocker)) return false;
        portalTransit = true;
        nextRow = portalExit.row + stepR;
        nextCol = portalExit.col + stepC;
        step += 1;
        continue;
      }
      nextRow += stepR;
      nextCol += stepC;
      step += 1;
    }
    return false;
  }
  function clearRay(boardState, row, col, targetRow2, targetCol2, limit) {
    const dr = Math.sign(targetRow2 - row);
    const dc = Math.sign(targetCol2 - col);
    const distance = Math.max(Math.abs(targetRow2 - row), Math.abs(targetCol2 - col));
    const maxLimit = boardRayLimit(boardState);
    const attacker = get(boardState, row, col);
    if (distance > maxLimit) return false;
    for (let step = 1; step < distance; step += 1) {
      const blocker = get(boardState, row + dr * step, col + dc * step);
      if (blocker && !isGhostTransparentFor(attacker, blocker, renderType(attacker), boardState)) return false;
    }
    return true;
  }
  function criticalPieces(boardState, color) {
    const pieces = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (piece.color === color && isWorkerDecisiveCaptureTarget(boardState, piece)) pieces.push({ piece, row, col });
    });
    const citizens = boardState?.democracy?.[color] ? workerDemocracyCitizenEntries(boardState, color) : [];
    if (citizens.length === 1 && !pieces.some(({ piece }) => piece === citizens[0].piece)) pieces.push(citizens[0]);
    return pieces;
  }
  function isCritical(piece) {
    return Boolean(piece?.crownRoyal || piece?.editorRoyal) || piece?.type === "king" || piece?.type === "royalKnight" || piece?.type === "shotgunKing" || piece?.type === "darkWizard" || piece?.type === "vip" || piece?.type === "merchant" || piece?.type === "timeTraveler" || piece?.type === "vampireLord";
  }
  function isWorkerRegencyCaptureTarget(boardState, piece) {
    return isWorkerRegencyRoyalHeir(boardState, piece);
  }
  function isWorkerRegencyRoyalHeir(boardState, piece) {
    return Boolean(piece?.color && piece.regencyHeir && boardState?.kingDead?.[piece.color] && boardState?.regency?.[piece.color]);
  }
  function isWorkerRoyalIdentityPiece(boardState, piece) {
    return Boolean(piece && (piece.type === "merchant" && usesSeptember18Balance(boardState) || piece.editorRoyal || isRoyalPiece(piece) || isWorkerRegencyRoyalHeir(boardState, piece)));
  }
  function isWorkerKingRole(boardState, piece) {
    return Boolean(piece && (piece.type === "merchant" && usesSeptember18Balance(boardState) || piece.crownRoyal || isWorkerNativeKing(piece) || isWorkerRegencyRoyalHeir(boardState, piece)));
  }
  function isWorkerNonRoyalQueen(boardState, piece, color = piece?.color) {
    return isWorkerQueenIdentity(piece, color) && !isWorkerRoyalIdentityPiece(boardState, piece);
  }
  function isWorkerQueenIdentity(piece, color = piece?.color) {
    return Boolean(piece && piece.color === color && piece.type === "queen" && piece.regencyHeir !== true);
  }
  function isWorkerRuleRoyalKing(boardState, piece) {
    if (isDemocracyProtectedRoyal(boardState.democracy, piece)) return false;
    if (isDemocracyProtectedMerchant(boardState.democracy)) ;
    return isWorkerRoyalIdentityPiece(boardState, piece);
  }
  function isWorkerDecisiveCaptureTarget(boardState, piece) {
    if (isDemocracyProtectedMerchant(boardState?.democracy)) ;
    if (isDemocracyProtectedRoyal(boardState?.democracy, piece)) return false;
    return isCritical(piece) || isWorkerRegencyCaptureTarget(boardState, piece);
  }
  function isRoyalPiece(piece) {
    return Boolean(
      piece?.crownRoyal || piece?.type === "king" || piece?.type === "royalKnight" || piece?.type === "shotgunKing" || piece?.type === "darkWizard" || piece?.type === "vip"
    );
  }
  function workerRacingKingWins(boardState, piece, row) {
    if (!isWorkerKingRole(boardState, piece) || !workerHasRacingKingObjective(boardState, piece.color)) return false;
    const goalRow = workerRacingKingGoalRow(boardState, piece.color);
    return row === goalRow;
  }
  function workerHasRacingKingObjective(boardState, color) {
    return Boolean(boardState?.racingKing?.[color] || boardState?.machoChess);
  }
  function workerRacingKingGoalRow(boardState, color) {
    if (boardState?.collapsed) {
      const depth = normalizeCollapseDepth(
        boardState.collapseDepth,
        boardRowCount(boardState),
        boardColCount(boardState),
        true
      );
      return color === "white" ? depth : Math.max(0, boardRowCount(boardState) - 1 - depth);
    }
    return color === "white" ? 0 : boardRowCount(boardState) - 1;
  }
  function isHpPiece(piece) {
    return Boolean(piece && (piece.type === "colossus" || piece.type === "shotgunKing" || Number.isFinite(Number(piece.hp))));
  }
  function isWorkerUndergroundBunkerKing(piece) {
    return Boolean(piece && (isWorkerNativeKing(piece) || piece.regencyHeir) && piece.undergroundBunker && Number.isFinite(Number(piece.hp)));
  }
  function pieceValue(piece) {
    if (!piece) return 0;
    return PIECE_VALUES[renderType(piece)] || PIECE_VALUES[piece.type] || 300;
  }
  function workerRecyclingCapturePoolColor(color) {
    return opponent(color);
  }
  function workerRecyclingPromotionEntries(boardState, color) {
    return (boardState.captures?.[workerRecyclingCapturePoolColor(color)] || []).map((item, index) => ({ item, index, type: monochromePieceType(item?.type, boardState.monochromeChess) })).filter(({ type }) => type && !RECYCLING_PROMOTION_EXCLUDED_TYPES.has(type));
  }
  function workerBestRecyclingPromotionType(boardState, color) {
    return [...new Set(workerRecyclingPromotionEntries(boardState, color).map(({ type }) => type))].sort((a, b) => pieceValue({ type: b }) - pieceValue({ type: a }))[0] || "";
  }
  function consumeWorkerRecycledPromotionPiece(boardState, color, type) {
    const captures = boardState.captures?.[workerRecyclingCapturePoolColor(color)];
    if (!Array.isArray(captures)) return false;
    const entry = workerRecyclingPromotionEntries(boardState, color).find((candidate) => candidate.type === type);
    if (!entry) return false;
    captures.splice(entry.index, 1);
    return true;
  }
  function workerCallingCardFollowupCandidates(boardState, captured, attacker = null) {
    const remaining = piecesMatching(boardState, (piece) => piece !== captured && piece.color === captured?.color);
    const targetableNonRoyal = remaining.filter(({ piece }) => piece !== attacker && !isWorkerNativeKing(piece) && !isWorkerRegencyRoyalHeir(boardState, piece) && !["wall", "football", "blackHole"].includes(piece.type) && !["guard", "jester"].some((type) => pieceHasAbility(piece, type)));
    if (targetableNonRoyal.length) return targetableNonRoyal;
    if (!remaining.length || remaining.some(({ piece }) => !isWorkerNativeKing(piece) && !isWorkerRegencyRoyalHeir(boardState, piece))) return [];
    return remaining.filter(({ piece }) => piece !== attacker);
  }
  function resolveWorkerCallingCardCapture(boardState, captured, attacker = null) {
    const notice = captured?.callingCard;
    if (!notice || boardState.mode === "gameover") return;
    const candidates = workerCallingCardFollowupCandidates(boardState, captured, attacker);
    if (!candidates.length) return;
    const chosen = candidates[workerStableIndex(`${captured.id || captured.type}:${boardState.moveCount || 0}`, candidates.length)];
    const removed = chosen.piece;
    const captureOwner = notice.by;
    clearPieceCells(boardState, removed);
    if (!boardState.captures) boardState.captures = { white: [], black: [] };
    if (!Array.isArray(boardState.captures[captureOwner])) boardState.captures[captureOwner] = [];
    boardState.captures[captureOwner].push(clonePiece(removed));
    resolveWorkerRecyclingHoldoutAfterQueenLoss(boardState, removed);
    if (isWorkerDecisiveCaptureTarget(boardState, removed)) {
      workerCriticalCaptureScore(boardState, removed, captureOwner, captureOwner);
    }
  }
  function recordWorkerNewCardCaptureReactions(boardState, color, item) {
    if (!item) return;
    if (!boardState.magicGirlSurge) boardState.magicGirlSurge = { white: false, black: false };
    if (COLORS.includes(item.color)) {
      boardState.magicGirlSurge[item.color] = true;
      if (!legacyCompletedTurnEffectsStates.has(boardState) && item.color === boardState.turn && boardState.board.some((row) => row.some((piece) => piece?.color === item.color && pieceHasAbility(piece, "magicGirl")))) {
        boardState.magicGirlSurgeRefreshPending = normalizeDemocracyState(boardState.magicGirlSurgeRefreshPending);
        boardState.magicGirlSurgeRefreshPending[item.color] = true;
      }
    }
    septemberNotePawnCapture(boardState, item);
    if (!(item.editorRoyal || isWorkerKingRole(boardState, item)) && septemberQueueRecurrence(boardState, item, color)) return;
    if (pieceHasAbility(item, "undead") && COLORS.includes(item.color)) {
      if (!Array.isArray(boardState.undeadResurrections)) boardState.undeadResurrections = [];
      boardState.undeadResurrections.push({
        id: `undead-ai-${item.id || boardState.moveCount || 0}-${boardState.undeadResurrections.length}`,
        color: item.color,
        capturedBy: color,
        dueMoveCount: undeadResurrectionDueMoveCount(boardState.moveCount),
        remainingHalfTurns: UNDEAD_RESURRECTION_HALF_MOVES + 1,
        piece: clonePiece(item)
      });
    }
  }
  function recordWorkerCapturedPieces(boardState, color, items, attacker = null, options = {}) {
    if (!boardState.captures) boardState.captures = { white: [], black: [] };
    if (!Array.isArray(boardState.captures[color])) boardState.captures[color] = [];
    let recorded = false;
    (items || []).forEach((item) => {
      if (!item || ["wall", "football"].includes(item.type)) return;
      const deferBearReaction = Boolean(
        options.deferRetaliatingBearReactions && Boolean(septemberCounterLimit(pieceAbilityType(item))) && (Number(item.bearRetaliationsRemaining) || 0) > 0
      );
      if (!deferBearReaction) recordWorkerNewCardCaptureReactions(boardState, color, item);
      workerTransferCrownAfterCapture(boardState, item, attacker, options);
      if (item.color === color) return;
      boardState.captures[color].push(clonePiece(item));
      resolveWorkerCallingCardCapture(boardState, item, attacker);
      recorded = true;
    });
    if (recorded) cancelWorkerPropheciesByCapture(boardState);
  }
  function resolveWorkerReaperNearbyDeaths(boardState, deaths, aiColor, options = {}) {
    if (boardState.mode === "gameover") return 0;
    const removed = (Array.isArray(deaths) ? deaths : [deaths]).filter((entry) => entry?.item && Number.isInteger(entry.row) && Number.isInteger(entry.col));
    if (!removed.length) return 0;
    const deadIds = new Set(removed.map(({ item }) => item?.id).filter(Boolean));
    let score = 0;
    removed.forEach((death) => {
      if (boardState.mode === "gameover") return;
      const witnesses = piecesMatching(boardState, (piece, row, col) => piece?.type === "reaper" && !deadIds.has(piece.id) && reaperSoulCountsCapture(piece.color) && isAdjacentCell({ row, col }, death));
      witnesses.forEach(({ piece: reaper }) => {
        if (boardState.mode === "gameover") return;
        const progress = advanceReaperCaptureCount(reaper.reaperCaptures);
        reaper.reaperCaptures = progress.count;
        if (!progress.triggered) return;
        const royal = piecesMatching(boardState, (piece) => piece.color === opponent(reaper.color) && (isWorkerNativeKing(piece) || isWorkerRegencyRoyalHeir(boardState, piece)))[0] || piecesMatching(boardState, (piece) => piece.color === opponent(reaper.color) && ["merchant", "vip", "timeTraveler", "vampireLord"].includes(piece.type))[0] || null;
        if (!royal) {
          boardState.mode = "gameover";
          boardState.winner = reaper.color;
          score += reaper.color === aiColor ? 1e5 : -1e5;
          return;
        }
        const activeRoyalExecution = Boolean(
          options.activePiece && royal.piece && (options.activePiece === royal.piece || options.activePiece.id && royal.piece.id && options.activePiece.id === royal.piece.id)
        );
        const activePieceLanding = options.activePieceLanding;
        const executionTarget = activeRoyalExecution && inBounds(activePieceLanding?.row, activePieceLanding?.col, boardState) ? { row: activePieceLanding.row, col: activePieceLanding.col } : { row: royal.row, col: royal.col };
        if (options.activePiece?.id === reaper.id) reaper.reaperExecutionTarget = { row: royal.row, col: royal.col };
        score += workerCriticalCaptureScore(boardState, royal.piece, reaper.color, aiColor);
        clearPieceCells(boardState, royal.piece);
        recordWorkerCapturedPieces(boardState, reaper.color, [royal.piece]);
        if (activeRoyalExecution) {
          options.activePiece.pendingReaperDefeat = {
            reaperId: reaper.id,
            row: executionTarget.row,
            col: executionTarget.col
          };
        } else {
          clearPieceCells(boardState, reaper);
          set(boardState, executionTarget.row, executionTarget.col, reaper);
        }
        if (boardState.mode !== "gameover") {
          boardState.mode = "gameover";
          boardState.winner = reaper.color;
          score += reaper.color === aiColor ? 1e5 : -1e5;
        }
      });
    });
    return score;
  }
  function consumeWorkerReaperExecution(boardState, reaper) {
    const target = reaper?.reaperExecutionTarget;
    if (reaper) delete reaper.reaperExecutionTarget;
    if (!target || !inBounds(target.row, target.col, boardState)) return null;
    if (get(boardState, target.row, target.col) === reaper) return { row: target.row, col: target.col };
    if (get(boardState, target.row, target.col)) return null;
    clearPieceCells(boardState, reaper);
    set(boardState, target.row, target.col, reaper);
    return { row: target.row, col: target.col };
  }
  function cardValue(card) {
    if (!card) return 0;
    return (CARD_EFFECT_VALUES[card.effect] || 0) + (Number(card.stars) || 0) * 140;
  }
  function findCard(boardState, action) {
    return deck(boardState, action.color).find((card) => card?.instanceId === action.cardInstanceId) || deck(boardState, action.color).find((card) => card?.id === action.cardId && !card.used && !card.recovering && !isWorkerCardPendingNextTurn(card));
  }
  function isWorkerCardPendingNextTurn(card) {
    return Boolean(card?.nextTurnPending && !card.devCard && ["MIDDLE", "END"].includes(card?.phase));
  }
  function deck(boardState, color) {
    const slots = boardState.deckSlots || {};
    return slots[color] || [];
  }
  function stripRemovedPieceTypesFromWorkerBoard(boardState) {
    if (!Array.isArray(boardState?.board)) return;
    boardState.board.forEach((row) => {
      if (!Array.isArray(row)) return;
      row.forEach((piece, col) => {
        if (piece?.type === "timeAfterimage") row[col] = null;
      });
    });
  }
  function normalizeLegacyHookNames(value, seen = /* @__PURE__ */ new WeakSet()) {
    if (!value || typeof value !== "object") return value;
    if (seen.has(value)) return value;
    seen.add(value);
    if (value instanceof Set) {
      if (value.delete("ninefold")) value.add("hook");
      return value;
    }
    if (value instanceof Map) {
      value.forEach((entryValue, entryKey) => {
        if (entryValue && typeof entryValue === "object") normalizeLegacyHookNames(entryValue, seen);
        if (entryKey === "ninefold") {
          value.delete(entryKey);
          value.set("hook", entryValue === "ninefold" ? "hook" : entryValue);
        }
      });
      return value;
    }
    if (Array.isArray(value)) {
      value.forEach((entry, index) => {
        if (entry === "ninefold") value[index] = "hook";
        else normalizeLegacyHookNames(entry, seen);
      });
      return value;
    }
    Object.keys(value).forEach((key) => {
      const entry = value[key];
      if (entry === "ninefold") value[key] = "hook";
      else normalizeLegacyHookNames(entry, seen);
      if (key === "ninefold") {
        value.hook = value.hook ?? value[key];
        delete value[key];
      }
    });
    return value;
  }
  function normalizeIdolEncoreUsage(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const normalized = {};
    Object.entries(value).slice(0, 256).forEach(([pieceId, usedTurn]) => {
      if (!pieceId || pieceId.length > 96 || !Number.isFinite(Number(usedTurn))) return;
      normalized[pieceId] = Math.max(-1, Math.floor(Number(usedTurn)));
    });
    return normalized;
  }
  function normalizeState(raw) {
    const state = normalizeLegacyHookNames(cloneState(raw || {}));
    if (!Array.isArray(state.board)) state.board = emptyBoard();
    stripRemovedPieceTypesFromWorkerBoard(state);
    if (!state.deckSlots) state.deckSlots = { white: [], black: [] };
    if (!state.turn) state.turn = "black";
    if (!state.mode) state.mode = "play";
    state.acceleration = Boolean(state.acceleration);
    state.actionsRemaining = Math.max(1, Number(state.actionsRemaining) || 1);
    state.idolEncoreUsedByPiece = normalizeIdolEncoreUsage(state.idolEncoreUsedByPiece);
    if (!state.turnsTaken) state.turnsTaken = { white: 0, black: 0 };
    state.cardsUsedThisTurn = normalizeCardsUsedThisTurn(state.cardsUsedThisTurn);
    if (!state.royalCommand) state.royalCommand = { white: null, black: null };
    if (!state.aiSearchBonus) state.aiSearchBonus = { white: 0, black: 0 };
    if (!Array.isArray(state.recentMoves)) state.recentMoves = [];
    if (!Array.isArray(state.delayedHazards)) state.delayedHazards = [];
    if (!state.skipTurn) state.skipTurn = { white: false, black: false };
    if (!state.castlingCanceled) state.castlingCanceled = { white: false, black: false };
    if (!state.castled) state.castled = { white: false, black: false };
    if (!state.fileSurge) state.fileSurge = { white: false, black: false };
    if (!state.rookLift) state.rookLift = { white: false, black: false };
    if (!state.trojanHorse) state.trojanHorse = { white: false, black: false };
    if (!state.madHorse) state.madHorse = { white: false, black: false };
    if (!state.clonePassive) state.clonePassive = { white: false, black: false };
    if (!state.frontlineResponse) state.frontlineResponse = { white: false, black: false };
    if (!state.relay) state.relay = { white: false, black: false };
    if (!state.fieldPromotion) state.fieldPromotion = { white: false, black: false };
    if (!state.gomoku) state.gomoku = { white: false, black: false };
    if (!state.magicGirlSurge) state.magicGirlSurge = { white: false, black: false };
    state.magicGirlSurge.white = Boolean(state.magicGirlSurge.white);
    state.magicGirlSurge.black = Boolean(state.magicGirlSurge.black);
    if (!state.moveReplay) state.moveReplay = { white: null, black: null };
    if (!state.sirenExposure || typeof state.sirenExposure !== "object") state.sirenExposure = {};
    if (!Array.isArray(state.undeadResurrections)) state.undeadResurrections = [];
    if (!state.clonedPassiveCards) state.clonedPassiveCards = { white: [], black: [] };
    state.clonedPassiveCards.white = Array.isArray(state.clonedPassiveCards.white) ? state.clonedPassiveCards.white.filter((key) => typeof key === "string").slice(-64) : [];
    state.clonedPassiveCards.black = Array.isArray(state.clonedPassiveCards.black) ? state.clonedPassiveCards.black.filter((key) => typeof key === "string").slice(-64) : [];
    if (!state.vanishing) state.vanishing = { white: false, black: false };
    state.collapseDepth = normalizeCollapseDepth(
      state.collapseDepth,
      boardRowCount(state),
      boardColCount(state),
      Boolean(state.collapsed)
    );
    state.collapsedCells = normalizeCollapsedCells(
      state.collapsedCells,
      boardRowCount(state),
      boardColCount(state)
    );
    state.collapsed = state.collapseDepth > 0;
    state.periodicCollapse = state.periodicCollapse?.enabled ? {
      enabled: true,
      interval: PERIODIC_COLLAPSE_INTERVAL,
      nextAt: Math.max(1, Math.floor(Number(state.periodicCollapse.nextAt) || nextPeriodicCollapseTurn(sharedTurnCount(state))))
    } : null;
    state.ruleBombs = normalizeRuleBombs(state.ruleBombs, boardRowCount(state), boardColCount(state), workerRuleBombNormalizationLimit(state.ruleBombs));
    state.saturationRule = Boolean(state.saturationRule);
    delete state.ruleCakes;
    state.portalRule = normalizePortalRule(state.portalRule, boardRowCount(state), boardColCount(state));
    state.crownRule = workerNormalizeCrownRule(state.crownRule, state);
    state.conveyorRule = Boolean(state.conveyorRule);
    state.armistice = normalizeArmistice(state.armistice);
    state.pendingPortals = normalizePendingPortals(state.pendingPortals, boardRowCount(state), boardColCount(state));
    state.exhaustion = normalizeExhaustionState(state.exhaustion);
    state.mistakeCard = normalizeDemocracyState(state.mistakeCard);
    state.democracy = normalizeDemocracyState(state.democracy);
    state.hillKing = normalizeDemocracyState(state.hillKing);
    state.genevaConvention = normalizeDemocracyState(state.genevaConvention);
    state.platformRule = normalizePlatformRule(state.platformRule, sharedTurnCount(state), boardRowCount(state), boardColCount(state));
    state.mistakeRule = Boolean(state.mistakeRule);
    state.transcendenceRule = Boolean(state.transcendenceRule);
    state.camouflageRule = Boolean(state.camouflageRule);
    if (!state.imperialStudies) state.imperialStudies = { white: false, black: false };
    state.imperialStudies.white = Boolean(state.imperialStudies.white);
    state.imperialStudies.black = Boolean(state.imperialStudies.black);
    if (!state.religiousVictory) state.religiousVictory = { white: false, black: false };
    state.religiousVictory.white = Boolean(state.religiousVictory.white);
    state.religiousVictory.black = Boolean(state.religiousVictory.black);
    if (!state.racingKing) state.racingKing = { white: false, black: false };
    state.racingKing.white = Boolean(state.racingKing.white);
    state.racingKing.black = Boolean(state.racingKing.black);
    if (!state.binaMate) state.binaMate = { white: false, black: false };
    state.binaMate.white = Boolean(state.binaMate.white);
    state.binaMate.black = Boolean(state.binaMate.black);
    if (!state.overwhelm) state.overwhelm = { white: false, black: false };
    state.overwhelm.white = Boolean(state.overwhelm.white);
    state.overwhelm.black = Boolean(state.overwhelm.black);
    if (!state.substitution) state.substitution = { white: false, black: false };
    if (!state.cornerKick) state.cornerKick = { white: false, black: false };
    state.chainBonds = normalizeChainBonds(state.chainBonds);
    state.moving = normalizeMovingState(state.moving);
    if (!state.underpromotion) state.underpromotion = { white: false, black: false };
    if (!state.enPassantFrenzy) state.enPassantFrenzy = { white: false, black: false };
    if (!state.zugzwang) state.zugzwang = { white: false, black: false };
    state.zugzwang.white = Boolean(state.zugzwang.white);
    state.zugzwang.black = Boolean(state.zugzwang.black);
    if (!state.fianchetto) state.fianchetto = { white: false, black: false };
    if (!state.pawnSprint) state.pawnSprint = { white: false, black: false };
    if (!state.pawnConversion) state.pawnConversion = { white: false, black: false };
    if (!state.pawnLeap) state.pawnLeap = { white: false, black: false };
    if (!state.prophecy) state.prophecy = { white: null, black: null };
    if (!Array.isArray(state.pendingPanic)) state.pendingPanic = [];
    if (!Array.isArray(state.pendingFreeMoves)) state.pendingFreeMoves = [];
    if (!state.freeMoveCaptureLock) state.freeMoveCaptureLock = { white: false, black: false };
    state.freeMoveCaptureLock.white = Boolean(state.freeMoveCaptureLock.white);
    state.freeMoveCaptureLock.black = Boolean(state.freeMoveCaptureLock.black);
    state.chaosNoCaptureUntilHalfTurn = Math.max(0, Math.floor(Number(state.chaosNoCaptureUntilHalfTurn) || 0));
    if (!Array.isArray(state.pendingTrojanHorse)) state.pendingTrojanHorse = [];
    if (!Array.isArray(state.pendingTrolley)) state.pendingTrolley = [];
    if (!Array.isArray(state.pendingScarecrows)) state.pendingScarecrows = [];
    if (!Array.isArray(state.pendingLobsters)) state.pendingLobsters = [];
    if (!Array.isArray(state.pendingRuleTickets)) state.pendingRuleTickets = [];
    if (!Array.isArray(state.additionalRuleCards)) state.additionalRuleCards = [];
    if (!state.activeTrolley) state.activeTrolley = null;
    if (!Array.isArray(state.pendingPawnStorm)) state.pendingPawnStorm = [];
    if (!state.afterimageQueen) state.afterimageQueen = { white: false, black: false };
    if (!state.finalWeapon) state.finalWeapon = { white: false, black: false };
    if (!state.taunt) state.taunt = { white: 0, black: 0 };
    state.taunt.white = Math.max(0, Number(state.taunt.white) || 0);
    state.taunt.black = Math.max(0, Number(state.taunt.black) || 0);
    if (!state.hallucination) state.hallucination = { white: null, black: null };
    if (!state.captures) state.captures = { white: [], black: [] };
    state.highway = Boolean(state.highway);
    state.recycling = Boolean(state.recycling);
    state.coolGuy = Boolean(state.coolGuy);
    if (!state.manner || typeof state.manner !== "object") state.manner = { white: false, black: false };
    state.manner.white = Boolean(state.manner.white);
    state.manner.black = Boolean(state.manner.black);
    state.blackHole = normalizeCells(state.blackHole);
    state.winterKingdom = normalizeWinterKingdom(state.winterKingdom);
    state.ultimatum = normalizeUltimatum(state.ultimatum);
    return state;
  }
  function cloneState(source) {
    const largePieceClones = /* @__PURE__ */ new Map();
    const board = Array.isArray(source.board) ? source.board.map((row) => row.map((piece) => {
      if (!piece) return null;
      if (!workerIsLargePiece(piece) || !piece.id) return clonePiece(piece);
      if (!largePieceClones.has(piece.id)) largePieceClones.set(piece.id, clonePiece(piece));
      return largePieceClones.get(piece.id);
    })) : emptyBoard();
    const cloned = {
      // Campaign states cross JSON boundaries, so a 2x2 piece arrives as four
      // equal objects with one id. Rejoin those cells before applying any rule:
      // HP damage must update the entire entity, not only the struck body cell.
      board,
      deckSlots: {
        white: (source.deckSlots?.white || []).map(cloneCard),
        black: (source.deckSlots?.black || []).map(cloneCard)
      },
      captures: {
        white: (source.captures?.white || []).map(clonePiece),
        black: (source.captures?.black || []).map(clonePiece)
      },
      ...source.temporaryQueens ? { temporaryQueens: clonePlain(source.temporaryQueens) } : {},
      ...source.necromancy ? { necromancy: clonePlain(source.necromancy) } : {},
      turn: source.turn,
      mode: source.mode,
      aiSearchNoCards: Boolean(source.aiSearchNoCards),
      skipRandomVanishing: source.skipRandomVanishing !== false,
      winner: source.winner || "",
      campaign: clonePlain(source.campaign || null),
      acceleration: Boolean(source.acceleration),
      actionsRemaining: Math.max(1, Number(source.actionsRemaining) || 1),
      idolEncoreUsedByPiece: { ...source.idolEncoreUsedByPiece || {} },
      draftDelete: Boolean(source.draftDelete),
      collapsed: Boolean(source.collapsed),
      collapseDepth: normalizeCollapseDepth(source.collapseDepth, boardRowCount(source), boardColCount(source), Boolean(source.collapsed)),
      collapsedCells: normalizeCollapsedCells(source.collapsedCells, boardRowCount(source), boardColCount(source)),
      periodicCollapse: clonePlain(source.periodicCollapse || null),
      ruleBombs: clonePlain(source.ruleBombs || []),
      saturationRule: Boolean(source.saturationRule),
      portalRule: clonePlain(source.portalRule || null),
      crownRule: clonePlain(source.crownRule || null),
      conveyorRule: Boolean(source.conveyorRule),
      armistice: clonePlain(source.armistice || null),
      pendingPortals: clonePlain(source.pendingPortals || []),
      exhaustion: normalizeExhaustionState(source.exhaustion),
      mistakeCard: normalizeDemocracyState(source.mistakeCard),
      democracy: normalizeDemocracyState(source.democracy),
      hillKing: normalizeDemocracyState(source.hillKing),
      genevaConvention: normalizeDemocracyState(source.genevaConvention),
      platformRule: clonePlain(source.platformRule || null),
      mistakeRule: Boolean(source.mistakeRule),
      transcendenceRule: Boolean(source.transcendenceRule),
      camouflageRule: Boolean(source.camouflageRule),
      binaMate: { ...source.binaMate || { white: false, black: false } },
      overwhelm: { ...source.overwhelm || { white: false, black: false } },
      effects: clonePlain(source.effects || {}),
      regency: { ...source.regency || {} },
      retreat: { ...source.retreat || {} },
      racingKing: { ...source.racingKing || {} },
      radicalCharge: { ...source.radicalCharge || {} },
      encouragement: { ...source.encouragement || {} },
      ironMonarch: { ...source.ironMonarch || {} },
      imperialStudies: { ...source.imperialStudies || {} },
      religiousVictory: { ...source.religiousVictory || {} },
      backwardKnight: { ...source.backwardKnight || {} },
      trojanHorse: { ...source.trojanHorse || {} },
      madHorse: { ...source.madHorse || {} },
      clonePassive: { ...source.clonePassive || {} },
      frontlineResponse: normalizeDemocracyState(source.frontlineResponse),
      relay: normalizeDemocracyState(source.relay),
      fieldPromotion: normalizeDemocracyState(source.fieldPromotion),
      gomoku: normalizeDemocracyState(source.gomoku),
      magicGirlSurge: normalizeDemocracyState(source.magicGirlSurge),
      ...Object.values(source.magicGirlSurgeRefreshPending || {}).some(Boolean) ? { magicGirlSurgeRefreshPending: normalizeDemocracyState(source.magicGirlSurgeRefreshPending) } : {},
      moveReplay: clonePlain(source.moveReplay || { white: null, black: null }),
      sirenExposure: clonePlain(source.sirenExposure || {}),
      clonedPassiveCards: clonePlain(source.clonedPassiveCards || { white: [], black: [] }),
      vanishing: { ...source.vanishing || {} },
      knightInjury: { ...source.knightInjury || {} },
      fianchetto: { ...source.fianchetto || {} },
      ...septemberPassiveState(source),
      pawnSprint: { ...source.pawnSprint || {} },
      pawnConversion: { ...source.pawnConversion || {} },
      pawnLeap: { ...source.pawnLeap || {} },
      fileSurge: { ...source.fileSurge || {} },
      rookLift: { ...source.rookLift || {} },
      underpromotion: { ...source.underpromotion || {} },
      finalWeapon: { ...source.finalWeapon || {} },
      highway: Boolean(source.highway),
      recycling: Boolean(source.recycling),
      coolGuy: Boolean(source.coolGuy),
      manner: {
        white: Boolean(source.manner?.white),
        black: Boolean(source.manner?.black)
      },
      freeCastling: { ...source.freeCastling || {} },
      switcheroo: { ...source.switcheroo || {} },
      substitution: { ...source.substitution || {} },
      cornerKick: { ...source.cornerKick || {} },
      chainBonds: normalizeChainBonds(source.chainBonds),
      moving: normalizeMovingState(source.moving),
      castlingCanceled: { ...source.castlingCanceled || {} },
      castled: { ...source.castled || {} },
      bishopSnipe: { ...source.bishopSnipe || {} },
      breakthroughPawns: { ...source.breakthroughPawns || {} },
      highGround: normalizeCells(source.highGround),
      appliedRuleCard: clonePlain(source.appliedRuleCard || null),
      additionalRuleCards: clonePlain(source.additionalRuleCards || []),
      pendingRuleTickets: clonePlain(source.pendingRuleTickets || []),
      initiative: clonePlain(source.initiative || {}),
      machoChess: Boolean(source.machoChess),
      palaces: clonePlain(source.palaces || []),
      kingKnight: { ...source.kingKnight || {} },
      royalKnightKing: { ...source.royalKnightKing || {} },
      socialism: { ...source.socialism || {} },
      taunt: { ...source.taunt || {} },
      hallucination: clonePlain(source.hallucination || { white: null, black: null }),
      prophecy: clonePlain(source.prophecy || { white: null, black: null }),
      diceLocks: clonePlain(source.diceLocks || {}),
      enPassant: clonePlain(source.enPassant || null),
      lastMove: clonePlain(source.lastMove || null),
      accelerationTrail: clonePlain(source.accelerationTrail || null),
      ...source.captureTheFlag ? { captureTheFlag: clonePlain(source.captureTheFlag) } : {},
      ...source.reversal ? { reversal: clonePlain(source.reversal) } : {},
      ...source.resolveReady ? { resolveReady: clonePlain(source.resolveReady) } : {},
      ...source.resolveSpentTurn ? { resolveSpentTurn: clonePlain(source.resolveSpentTurn) } : {},
      ...source.resolveMoveCredit ? { resolveMoveCredit: clonePlain(source.resolveMoveCredit) } : {},
      ...source.pendingRecurrences?.length ? { pendingRecurrences: clonePlain(source.pendingRecurrences) } : {},
      undeadResurrections: clonePlain(source.undeadResurrections || []),
      enPassantFrenzy: { ...source.enPassantFrenzy || {} },
      zugzwang: { ...source.zugzwang || {} },
      pendingPanic: clonePlain(source.pendingPanic || []),
      pendingFreeMoves: clonePlain(source.pendingFreeMoves || []),
      freeMoveCaptureLock: { ...source.freeMoveCaptureLock || {} },
      chaosNoCaptureUntilHalfTurn: Math.max(0, Math.floor(Number(source.chaosNoCaptureUntilHalfTurn) || 0)),
      pendingIcbm: normalizePendingIcbm(source.pendingIcbm),
      pendingTrojanHorse: clonePlain(source.pendingTrojanHorse || []),
      pendingGales: clonePlain(source.pendingGales || []),
      pendingOtherworld: clonePlain(source.pendingOtherworld || []),
      judgmentExiles: clonePlain(source.judgmentExiles || []),
      pendingTrolley: clonePlain(source.pendingTrolley || []),
      pendingScarecrows: clonePlain(source.pendingScarecrows || []),
      othelloPending: clonePlain(source.othelloPending || { white: false, black: false }),
      pendingLobsters: clonePlain(source.pendingLobsters || []),
      activeTrolley: clonePlain(source.activeTrolley || null),
      pendingPawnStorm: clonePlain(source.pendingPawnStorm || []),
      afterimageQueen: { ...source.afterimageQueen || {} },
      kingDead: { ...source.kingDead || {} },
      quantumPending: { ...source.quantumPending || {} },
      coronation: { ...source.coronation || {} },
      earlyPromotion: { ...source.earlyPromotion || {} },
      fastGrowth: { ...source.fastGrowth || {} },
      turnsTaken: { ...source.turnsTaken || {} },
      cardsUsedThisTurn: normalizeCardsUsedThisTurn(source.cardsUsedThisTurn),
      royalCommand: clonePlain(source.royalCommand || { white: null, black: null }),
      moveCount: Number(source.moveCount) || 0,
      fullMove: Number(source.fullMove) || 1,
      aiSearchBonus: { ...source.aiSearchBonus || {} },
      blackHole: normalizeCells(source.blackHole),
      winterKingdom: normalizeWinterKingdom(source.winterKingdom),
      ultimatum: normalizeUltimatum(source.ultimatum),
      delayedHazards: (source.delayedHazards || []).map((hazard) => ({
        ...hazard,
        cells: (hazard.cells || []).map((cell) => ({ row: cell.row, col: cell.col }))
      })),
      skipTurn: { ...source.skipTurn || {} },
      recentMoves: (source.recentMoves || []).map((move) => ({
        from: move.from ? { ...move.from } : null,
        to: move.to ? { ...move.to } : null,
        pieceId: move.pieceId || "",
        pieceType: move.pieceType || "",
        color: move.color || ""
      })),
      monochromeChess: Boolean(source.monochromeChess)
    };
    if (campaignAuthorityV1States.has(source)) campaignAuthorityV1States.add(cloned);
    if (legacyCompletedTurnEffectsStates.has(source)) legacyCompletedTurnEffectsStates.add(cloned);
    if (legacyReusablePlatformStates.has(source)) legacyReusablePlatformStates.add(cloned);
    if (legacyWindmillMovementStates.has(source)) legacyWindmillMovementStates.add(cloned);
    if (legacyBloodMoonLifecycleStates.has(source)) legacyBloodMoonLifecycleStates.add(cloned);
    if (legacyGrasshopperTargetStates.has(source)) legacyGrasshopperTargetStates.add(cloned);
    return cloned;
  }
  function normalizeCells(value) {
    if (!Array.isArray(value)) return [];
    return value.map((cell) => ({ row: Number(cell?.row), col: Number(cell?.col) })).filter((cell) => inBounds(cell.row, cell.col));
  }
  function normalizeWinterKingdom(value) {
    return {
      enabled: Boolean(value?.enabled),
      lastCycle: Number(value?.lastCycle) || 0,
      frozenIds: Array.isArray(value?.frozenIds) ? value.frozenIds.filter(Boolean).map(String).slice(0, 12) : []
    };
  }
  // Grafted from engine.optimized.js (real bug fix, not a behavior change):
  // aiWorker-raw.js's own normalizeWinterKingdom above already tracks
  // `lastCycle` and `frozenIds`, but nothing anywhere in the real file ever
  // WRITES to them -- confirmed by grepping the whole file for "lastCycle"
  // outside this normalizer -- so the "winterKingdom" RULE card's core
  // periodic freeze mechanic was entirely inert in the real engine. Bundle
  // picks 3 random eligible pieces per color (shuffle().slice(0,3)); this
  // engine avoids true Math.random() in state-mutation paths elsewhere (see
  // royalShield's deterministic "strongest friendly" substitute for its own
  // random-target card), so board-iteration order is used as the
  // deterministic stand-in here too. Wired into finishWorkerMove's tick
  // point above (unconditional call, mirroring engine.optimized.js exactly
  // -- the function itself early-returns when winterKingdom isn't enabled).
  function applyWorkerWinterFreezeCycle(boardState, force = false) {
    const winter = normalizeWinterKingdom(boardState.winterKingdom);
    boardState.winterKingdom = winter;
    if (!winter.enabled || boardState.mode === "gameover") return false;
    function clearWorkerFrozenWinterPieces() {
      const ids = new Set(winter.frozenIds || []);
      forEachPiece(boardState, (piece) => {
        if (piece?.id && ids.has(piece.id) && !piece.frozenByCard) delete piece.frozen;
      });
    }
    if (winter.disabledByLastWarmth) {
      clearWorkerFrozenWinterPieces();
      winter.frozenIds = [];
      return false;
    }
    const eligibleByColor = Object.fromEntries(COLORS.map((color) => [color, workerWinterEligiblePieces(boardState, color)]));
    if (COLORS.some((color) => isLastWarmthActive(eligibleByColor[color].length))) {
      clearWorkerFrozenWinterPieces();
      winter.frozenIds = [];
      winter.disabledByLastWarmth = true;
      return false;
    }
    const cycle = Math.min(Number(boardState.turnsTaken?.white) || 0, Number(boardState.turnsTaken?.black) || 0);
    if (cycle <= 0 || cycle % 3 !== 0) return false;
    if (!force && winter.lastCycle === cycle) return false;
    clearWorkerFrozenWinterPieces();
    const frozenIds = [];
    COLORS.forEach((color) => {
      eligibleByColor[color].slice(0, 3).forEach(({ piece }) => {
        piece.frozen = true;
        frozenIds.push(piece.id);
      });
    });
    winter.lastCycle = cycle;
    winter.frozenIds = frozenIds;
    return frozenIds.length > 0;
  }
  function normalizeUltimatum(value) {
    if (!value || typeof value !== "object") return null;
    const expiresFullMove = Number.isFinite(Number(value.expiresFullMove)) ? Math.max(1, Number(value.expiresFullMove)) : null;
    const remaining = Math.max(0, Math.min(4, Number(value.remaining) || 0));
    const remainingHalfTurns = Number.isSafeInteger(Number(value.remainingHalfTurns)) ? Math.max(0, Math.min(ULTIMATUM_DURATION_HALF_TURNS, Number(value.remainingHalfTurns))) : null;
    if (!remaining && !expiresFullMove && remainingHalfTurns === null) return null;
    return {
      by: value.by === "black" ? "black" : "white",
      remaining: remaining || (remainingHalfTurns === null ? 4 : Math.ceil(remainingHalfTurns / 2)),
      remainingHalfTurns,
      expiresFullMove,
      movedIds: Array.isArray(value.movedIds) ? value.movedIds.filter(Boolean).map(String).slice(-96) : []
    };
  }
  function isBlackHoleCell(boardState, row, col) {
    return normalizeCells(boardState?.blackHole).some((cell) => cell.row === row && cell.col === col);
  }
  function isFrozenPiece(piece) {
    return Boolean(piece?.type !== "scarecrow" && piece?.frozen);
  }
  function isWorkerStakedPiece(piece) {
    return Boolean(piece?.staked && Number(piece.staked.remaining) > 0);
  }
  function isDiceLockedPiece(boardState, piece) {
    const lock = boardState?.diceLocks?.[piece?.color];
    if (!lock || Number(lock.remaining) <= 0) return false;
    if (lock.type === "king") return isWorkerKingRole(boardState, piece);
    return piece?.type === lock.type;
  }
  function isWorkerSaturationCaptureLocked(boardState, piece) {
    return isSaturationCaptureLocked(Boolean(boardState?.saturationRule || piece?.potionSaturation), piece, SATURATION_CAPTURE_LIMIT);
  }
  function workerSaturationCapturesRemaining(boardState, piece) {
    if (!boardState?.saturationRule && !piece?.potionSaturation) return Number.POSITIVE_INFINITY;
    if (legacyAttackRulesStates.has(boardState)) return Math.max(0, SATURATION_CAPTURE_LIMIT - saturationCaptureCount(piece));
    return isWorkerSaturationCaptureLocked(boardState, piece) ? 0 : Number.POSITIVE_INFINITY;
  }
  function recordWorkerSaturationCaptureAttempts(boardState, attacker, items) {
    if (!attacker) return 0;
    const attempts = (items || []).filter((item) => item && item !== attacker).length;
    if (!attempts) return 0;
    const progress = advanceSaturationCaptureCount(attacker.capturesMade, attempts, SATURATION_CAPTURE_LIMIT);
    attacker.capturesMade = progress.count;
    if (legacyAttackRulesStates.has(boardState)) attacker.capturesMade = Math.min(SATURATION_CAPTURE_LIMIT, progress.count);
    return attempts;
  }
  function recordWorkerDirectCaptures(boardState, attacker, items) {
    if (!attacker) return 0;
    const targets = (items || []).filter((item) => item && item !== attacker);
    recordWorkerSaturationCaptureAttempts(boardState, attacker, targets);
    const captures = targets.filter((item) => !(Boolean(septemberCounterLimit(pieceAbilityType(item))) && (Number(item.bearRetaliationsRemaining) || 0) > 0)).length;
    if (!captures) return 0;
    attacker.totalCaptures = Math.max(0, Number(attacker.totalCaptures) || 0) + captures;
    if (!legacyWitchTrialCaptureStates.has(boardState)) resolveWitchTrialCapture(attacker, captures > 0);
    return captures;
  }
  function workerPieceRemainsOnBoard(boardState, target) {
    let found = false;
    forEachPiece(boardState, (piece) => {
      if (piece === target) found = true;
    });
    return found;
  }
  function workerPieceIdRemainsOnBoard(boardState, target) {
    if (!target?.id) return workerPieceRemainsOnBoard(boardState, target);
    let found = false;
    forEachPiece(boardState, (piece) => {
      if (piece?.id === target.id) found = true;
    });
    return found;
  }
  function canWorkerCaptureTarget(color, target, attacker = null, boardState = null, options = {}) {
    if (septemberNullificationBlocks(target, attacker)) return false;
    const attackerType = attacker ? pieceAbilityType(attacker) || renderType(attacker) || attacker.type : "";
    if (["campfire", "paladin"].includes(attackerType)) return false;
    if (target?.metalized && target.type !== "scarecrow" && !options.forceCapture) return false;
    const actualAttacker = attacker || { color, type: attackerType };
    if (actualAttacker.color === color && isChaosChessCaptureBlocked(boardState?.chaosNoCaptureUntilHalfTurn, boardState?.turnsTaken)) return false;
    if (attackerType === "monster" && (target?.type === "darkWizard" || !legacySeptember12RulesStates.has(boardState) && target?.type === "scarecrow")) return false;
    if (isWorkerQueenIdentity(actualAttacker, actualAttacker.color) && target?.type === "pawn" && boardState?.genevaConvention?.[target.color]) return false;
    if (attacker && boardState?.frontlineResponse?.[target?.color]) {
      let attackerSquare = null;
      let targetSquare = null;
      forEachPiece(boardState, (piece, row, col) => {
        if (!attackerSquare && (piece === attacker || attacker.id && piece?.id === attacker.id)) attackerSquare = { row, col };
        if (!targetSquare && (piece === target || target?.id && piece?.id === target.id)) targetSquare = { row, col };
      });
      if (frontlineResponseBlocksCapture(true, target, attackerSquare, targetSquare, attackerType) && !(pieceHasAbility(actualAttacker, "primeMinister") && primeMinisterHasDiagonalCapturePath(attackerSquare, targetSquare, (row, col) => inBounds(row, col, boardState) && !get(boardState, row, col) && !isWorkerCollapsedSquare(boardState, row, col) && !workerPortalExitAt(boardState, row, col)))) return false;
    }
    return Boolean(target && (options.ignoreFreeMoveLock || !isFreeMoveCaptureBlocked(boardState?.freeMoveCaptureLock, actualAttacker, target)) && !isArmisticeCaptureBlocked(boardState?.armistice, actualAttacker, target) && !(attacker && !options.ignoreSaturation && isWorkerSaturationCaptureLocked(boardState, attacker)) && (!["recruiter", "guard"].includes(attackerType) || options.allowBasicTrainingCapture && attacker?.basicTraining || hasWorkerRoyalCommandCaptureAccess(boardState, attacker)) && !isOverwhelmCaptureBlocked(boardState?.overwhelm, actualAttacker, target) && !(attacker && isWorkerCardNoCaptureActive(boardState, attacker)) && !(attacker && isWorkerQuantumCaptureLocked(boardState, attacker)) && (attackerType !== "idol" || attacker?.crownBearer) && !target.submerged && (target.color !== color || options.allowFriendly) && target.type !== "wall" && target.type !== "football" && (target.type !== "monster" || attackerType === "darkWizard") && !pieceHasAbility(target, "guard") && target.captureRestriction !== "immune" && !(attacker?.frenzy && isRoyalPiece(target)) && !isDesperadoRoyalCaptureBlocked(attacker, target) && (target.type === "scarecrow" || !target.protected) && !isWorkerEncouragedTarget(boardState, target) && !isFrozenPiece(target) && (!attacker || canTimePhaseInteract(boardState, attacker, target)) && !(attacker?.type === "timeTraveler" && timeTravelerState(boardState)?.attackEnabledFor !== attacker.color) && (!pieceHasAbility(target, "jester") && target.captureRestriction !== "royal-only" || attackerType !== "jester" && (attacker?.crownBearer || isWorkerNativeKing(attacker) || isWorkerRegencyRoyalHeir(boardState, attacker))));
  }
  function canWorkerRadicalChargeCaptureTarget(boardState, color, target, attacker = null) {
    return Boolean(
      !isWorkerMannerCaptureLocked(boardState, attacker) && canWorkerCaptureTarget(color, target, attacker, boardState) && !pieceHasAbility(target, "jester")
    );
  }
  function isWorkerEncouragedTarget(boardState, target) {
    const memo = ATTACK_MEMO;
    if (memo !== null && memo.board === boardState && target && typeof target === "object") {
      let v = memo.encouraged.get(target);
      if (v === void 0) { v = isWorkerEncouragedTargetRaw(boardState, target); memo.encouraged.set(target, v); }
      return v;
    }
    return isWorkerEncouragedTargetRaw(boardState, target);
  }
  function isWorkerEncouragedTargetRaw(boardState, target) {
    if (target?.type === "scarecrow") return false;
    if (target?.outpostProtected) return true;
    if (boardState && !target?.editorRoyal && !isWorkerKingRole(boardState, target) && septemberBoardCampfireProtects(boardState.board, target)) return true;
    if (!boardState || !target || !boardState.encouragement?.[target.color]) return false;
    let targetSquare = null;
    let kingSquare = null;
    forEachPiece(boardState, (piece, row, col) => {
      if (!targetSquare && piece === target) targetSquare = { row, col };
      if (!kingSquare && piece?.color === target.color && isWorkerKingRole(boardState, piece)) {
        kingSquare = { row, col };
      }
    });
    if (!targetSquare || !kingSquare) return false;
    return Math.abs(kingSquare.row - targetSquare.row) + Math.abs(kingSquare.col - targetSquare.col) === 1;
  }
  function timeTravelerState(boardState) {
    if (boardState?.campaign?.setup !== "timeTraveler") return null;
    if (!boardState.campaign.timeTraveler) {
      boardState.campaign.timeTraveler = { visited: [], phase: "future", attackEnabledFor: null };
    }
    delete boardState.campaign.timeTraveler.afterimageArmed;
    boardState.campaign.timeTraveler.phase = normalizeTimePhase(boardState.campaign.timeTraveler.phase);
    return boardState.campaign.timeTraveler;
  }
  function normalizeTimePhase(phase) {
    return phase === "past" || phase === "future" ? phase : "future";
  }
  function timePhaseOf(boardState, piece) {
    if (boardState?.campaign?.setup !== "timeTraveler" || !piece || piece.type === "wall" || piece.type === "football") return null;
    piece.timePhase = normalizeTimePhase(piece.timePhase);
    return piece.timePhase;
  }
  function canTimePhaseInteract(boardState, attacker, target) {
    if (boardState?.campaign?.setup !== "timeTraveler" || !attacker || !target || target.type === "wall" || target.type === "football") return true;
    return timePhaseOf(boardState, attacker) === timePhaseOf(boardState, target);
  }
  function isWorkerHighGroundCell(boardState, row, col) {
    return normalizeCells(boardState?.highGround).some((cell) => cell.row === row && cell.col === col);
  }
  function ignoresWorkerHighGround(boardState, piece) {
    return piece?.type === "merchant";
  }
  function isWorkerFreeMoveCaptureLocked(boardState, color) {
    return Boolean(COLORS.includes(color) && boardState?.freeMoveCaptureLock?.[color]);
  }
  function isWorkerCaptureMove(boardState, piece, move) {
    if (!piece || !move) return false;
    if (move.substitutionSwap) return false;
    if (move.missionaryConvert) return true;
    if (move.madHorseCapture || move.madHorsePortalEntryCapture || move.madHorsePortalExitCapture) return true;
    if (move.siegeRamMove) {
      const entries = (Array.isArray(move.highlightCells) ? move.highlightCells : []).filter((cell) => inBounds(cell?.row, cell?.col, boardState)).map((cell) => ({ ...cell, item: get(boardState, cell.row, cell.col) })).filter((entry) => entry.item && entry.item !== piece);
      return workerSiegeRamPotentialCaptureCount(entries, boardState) > 0;
    }
    if (move.enPassant || move.jumpCapture || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy) return true;
    if (Array.isArray(move.colossusLandingCaptures) && move.colossusLandingCaptures.length > 0) return true;
    if (Array.isArray(move.bigRookLandingCaptures) && move.bigRookLandingCaptures.length > 0) return true;
    if (!Number.isInteger(move.row) || !Number.isInteger(move.col)) return false;
    return workerPortalCaptureCells(move).some(({ row, col }) => canWorkerCaptureTarget(piece.color, get(boardState, row, col), piece, boardState, {
      ignoreSaturation: true,
      ignoreFreeMoveLock: true,
      allowBasicTrainingCapture: Boolean(move.basicTrainingCapture)
    }));
  }
  function workerMoveCaptureCells(move) {
    const cells = [];
    if (move?.enPassant) cells.push({ row: move.capturedRow, col: move.capturedCol });
    else if (Number.isInteger(move?.row) && Number.isInteger(move?.col)) cells.push(...workerPortalCaptureCells(move));
    if (move?.jumpCapture) cells.push(move.jumpCapture);
    if (Array.isArray(move?.sectorCells)) cells.push(...move.sectorCells);
    if (Array.isArray(move?.colossusLandingCaptures)) cells.push(...move.colossusLandingCaptures);
    if (Array.isArray(move?.bigRookLandingCaptures)) cells.push(...move.bigRookLandingCaptures);
    return uniqueCells(cells.filter((cell) => Number.isInteger(cell?.row) && Number.isInteger(cell?.col)));
  }
  function workerMoveCapturesDesperadoRoyal(boardState, piece, move) {
    if (!piece?.desperado) return false;
    return workerMoveCaptureCells(move).some(({ row, col }) => isDesperadoRoyalCaptureBlocked(piece, get(boardState, row, col)));
  }
  function isWorkerRepositionCaptureMove(boardState, piece, move) {
    if (!piece || !move) return false;
    if (move.madHorseCapture || move.madHorsePortalEntryCapture || move.madHorsePortalExitCapture) return true;
    if (move.enPassant || move.jumpCapture || move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy) return true;
    if (Array.isArray(move.colossusLandingCaptures) && move.colossusLandingCaptures.length > 0) return true;
    if (Array.isArray(move.bigRookLandingCaptures) && move.bigRookLandingCaptures.length > 0) return true;
    if (Array.isArray(move.sectorCells) && move.sectorCells.some((cell) => {
      const target = get(boardState, cell.row, cell.col);
      return target && target.color !== piece.color;
    })) return true;
    if (!Number.isInteger(move.row) || !Number.isInteger(move.col)) return false;
    return workerPortalCaptureCells(move).some(({ row, col }) => {
      const target = get(boardState, row, col);
      return Boolean(target && target.color !== piece.color);
    });
  }
  function isWorkerHighGroundCaptureBlocked(boardState, attacker, fromRow, fromCol, move) {
    if (!attacker || !normalizeCells(boardState?.highGround).length) return false;
    if (move.substitutionSwap || move.relaySwap || move.missionaryConvert || move.merchantBuy) return false;
    const largeLanding = Boolean(move.bigRookMove || move.colossusMove);
    if (ignoresWorkerHighGround(boardState, attacker) || !largeLanding && isWorkerHighGroundCell(boardState, fromRow, fromCol)) return false;
    if (move.colossusMove && !move.colossusLandingCaptures?.length || move.bigRookMove && !move.bigRookLandingCaptures?.length || move.setLogDirection || move.dragonSwap) return false;
    const targets = [];
    if (move.enPassant) targets.push({ row: move.capturedRow, col: move.capturedCol });
    if (Number.isInteger(move.row) && Number.isInteger(move.col)) targets.push(...workerPortalCaptureCells(move));
    if (move.jumpCapture) targets.push(move.jumpCapture);
    if (Array.isArray(move.sectorCells)) targets.push(...move.sectorCells);
    if (Array.isArray(move.colossusLandingCaptures)) targets.push(...move.colossusLandingCaptures);
    if (Array.isArray(move.bigRookLandingCaptures)) targets.push(...move.bigRookLandingCaptures);
    return targets.some(({ row, col }) => {
      if (!highGroundBlocksCaptureCell(
        { row: fromRow, col: fromCol },
        move,
        { row, col },
        largeLanding,
        (sourceRow, sourceCol) => isWorkerHighGroundCell(boardState, sourceRow, sourceCol)
      )) return false;
      const target = get(boardState, row, col);
      return target && target.color !== attacker.color && !ignoresWorkerHighGround(boardState, target) && canWorkerCaptureTarget(attacker.color, target, attacker, boardState, {
        allowBasicTrainingCapture: Boolean(move.basicTrainingCapture)
      });
    });
  }
  function isWorkerHighwayCell(boardState, row, col) {
    if (!boardState?.highway) return false;
    const cols = boardColCount(boardState);
    return inBounds(row, col, boardState) && (col === 1 || col === cols - 2);
  }
  function canUseWorkerHighwayMove(boardState, piece, row, col) {
    return Boolean(
      boardState?.highway && piece?.color && piece.type !== "wall" && piece.type !== "football" && piece.type !== "colossus" && !["bigRook", "bigBishop"].includes(piece.type) && isWorkerHighwayCell(boardState, row, col)
    );
  }
  function workerHighwayMoves(boardState, row, col, piece) {
    if (!canUseWorkerHighwayMove(boardState, piece, row, col)) return [];
    const moves = [];
    orthogonals().forEach(([dr, dc]) => {
      let nextRow = row + dr;
      let nextCol = col + dc;
      while (inBounds(nextRow, nextCol, boardState) && isWorkerHighwayCell(boardState, nextRow, nextCol)) {
        const target = get(boardState, nextRow, nextCol);
        if (!target) {
          moves.push({ row: nextRow, col: nextCol, highwayMove: true });
        } else {
          if (isGhostTransparentFor(piece, target, "rook", boardState)) {
            nextRow += dr;
            nextCol += dc;
            continue;
          }
          if (isHighwayCaptureAllowed(piece, target) && canWorkerCaptureTarget(piece.color, target, piece, boardState)) {
            moves.push({ row: nextRow, col: nextCol, highwayMove: true });
          }
          break;
        }
        nextRow += dr;
        nextCol += dc;
      }
    });
    return uniqueMoves(moves);
  }
  function workerHighwayRayReaches(boardState, row, col, targetRow2, targetCol2) {
    const attacker = get(boardState, row, col);
    const dr = Math.sign(targetRow2 - row);
    const dc = Math.sign(targetCol2 - col);
    if (dr === 0 && dc === 0 || dr !== 0 && dc !== 0) return false;
    let nextRow = row + dr;
    let nextCol = col + dc;
    while (inBounds(nextRow, nextCol, boardState) && isWorkerHighwayCell(boardState, nextRow, nextCol)) {
      if (nextRow === targetRow2 && nextCol === targetCol2) return true;
      const blocker = get(boardState, nextRow, nextCol);
      if (blocker && !isGhostTransparentFor(attacker, blocker, "rook", boardState)) return false;
      nextRow += dr;
      nextCol += dc;
    }
    return false;
  }
  function isWorkerMachoMoveBlocked(boardState, piece, row, col, move) {
    if (!boardState?.machoChess || !piece?.color || piece.type === "football") return false;
    if (move.colossusAttack || move.shotgunBlast || move.shotgunSnipe || move.merchantBuy || move.setLogDirection) return false;
    const destination = workerPortalMoveDestination(move);
    if (!destination) return false;
    const dr = destination.row - row;
    const dc = destination.col - col;
    const dir = piece.color === "white" ? -1 : 1;
    if (dr * dir < 0) return true;
    if (dr === 0 && dc !== 0) return !isWorkerCaptureMove(boardState, piece, move);
    return false;
  }
  function isWorkerBloodCurseMoveBlocked(boardState, piece, row, col, move) {
    if (!piece?.bloodCurse || !bloodMoonState(boardState) || usesBloodMoonNightMovement(boardState, piece.color)) return false;
    const destination = workerPortalMoveDestination(move);
    if (!destination) return true;
    return Math.max(Math.abs(destination.row - row), Math.abs(destination.col - col)) > 1;
  }
  function isWorkerPalaceMoveBlocked(boardState, piece, row, col, move) {
    if (!isWorkerKingRole(boardState, piece) || move.shotgunBlast || move.shotgunSnipe) return false;
    const palace = (boardState?.palaces || []).find((entry) => entry?.color === piece.color);
    if (!palace?.cells?.length) return false;
    const insideNow = palace.cells.some((cell) => cell.row === row && cell.col === col);
    if (!insideNow) return false;
    const destination = workerPortalMoveDestination(move);
    return !destination || !palace.cells.some((cell) => cell.row === destination.row && cell.col === destination.col);
  }
  function isWorkerInitiativeCaptureLocked(boardState, color) {
    const entry = boardState?.initiative?.[color];
    if (!entry) return false;
    const elapsed = (Number(boardState?.turnsTaken?.[color]) || 0) - (Number(entry.startTurn) || 0);
    return elapsed < (Number(entry.limit) || 10);
  }
  function workerBreakInitiativeByCheck(boardState, attackerColor) {
    const targetColor = opponent(attackerColor);
    const entry = boardState?.initiative?.[targetColor];
    if (!entry || entry.by !== attackerColor) return false;
    const royal = findWorkerRoyalKing(boardState, targetColor);
    if (!royal || !isSquareAttacked(boardState, royal.row, royal.col, attackerColor)) return false;
    boardState.initiative[targetColor] = null;
    return true;
  }
  function workerFullMoveEffectRemaining(boardState, effect) {
    const expiresFullMove = Number(effect?.expiresFullMove);
    if (!Number.isFinite(expiresFullMove)) return 0;
    return Math.max(0, expiresFullMove - (Number(boardState?.fullMove) || 1));
  }
  function workerUltimatumRemaining(boardState, ultimatum = boardState?.ultimatum) {
    if (!ultimatum) return 0;
    if (Number.isSafeInteger(Number(ultimatum.remainingHalfTurns))) {
      return Math.max(0, Math.min(4, Math.ceil(Number(ultimatum.remainingHalfTurns) / 2)));
    }
    if (Number.isFinite(Number(ultimatum.expiresFullMove))) {
      return Math.max(0, Math.min(4, Number(ultimatum.expiresFullMove) - (Number(boardState?.fullMove) || 1)));
    }
    return Math.max(0, Math.min(4, Number(ultimatum.remaining) || 0));
  }
  function workerUltimatumMoveReliefScore(boardState, piece) {
    if (!piece?.id || !isWorkerUltimatumTarget(boardState, piece)) return 0;
    const ultimatum = boardState?.ultimatum;
    const remaining = workerUltimatumRemaining(boardState, ultimatum);
    if (!ultimatum || remaining <= 0) return 0;
    if ((ultimatum.movedIds || []).map(String).includes(String(piece.id))) return 0;
    const urgency = 1 + (4 - remaining) * 0.45;
    return Math.min(2200, pieceValue(piece) * urgency);
  }
  function ultimatumStrategicScore(boardState, color) {
    const ultimatum = boardState?.ultimatum;
    const remaining = workerUltimatumRemaining(boardState, ultimatum);
    if (!ultimatum || remaining <= 0) return 0;
    const moved = new Set((ultimatum.movedIds || []).map(String));
    const urgency = 0.85 + (4 - remaining) * 0.55;
    let ownRisk = 0;
    let enemyRisk = 0;
    const seen = /* @__PURE__ */ new Set();
    forEachPiece(boardState, (piece) => {
      if (!isWorkerUltimatumTarget(boardState, piece) || seen.has(piece.id)) return;
      seen.add(piece.id);
      if (moved.has(String(piece.id))) return;
      const value = pieceValue(piece);
      const risk = Math.min(2600, value * urgency);
      if (piece.color === color) ownRisk += risk;
      else if (piece.color === opponent(color)) enemyRisk += risk;
    });
    return enemyRisk * 0.82 - ownRisk * 1.18;
  }
  function resolveWorkerUltimatumIfDue(boardState) {
    const ultimatum = boardState?.ultimatum;
    if (!ultimatum) return false;
    if (Number.isSafeInteger(Number(ultimatum.remainingHalfTurns))) {
      ultimatum.remainingHalfTurns = Math.max(0, Number(ultimatum.remainingHalfTurns) - 1);
      ultimatum.remaining = workerUltimatumRemaining(boardState, ultimatum);
      if (ultimatum.remainingHalfTurns > 0) return false;
      resolveWorkerUltimatum(boardState);
      return boardState.mode === "gameover";
    }
    ultimatum.remaining = workerUltimatumRemaining(boardState, ultimatum);
    if (ultimatum.remaining > 0) return false;
    resolveWorkerUltimatum(boardState);
    return boardState.mode === "gameover";
  }
  function resolveWorkerUltimatum(boardState) {
    const ultimatum = boardState?.ultimatum;
    if (!ultimatum) return;
    const moved = new Set((ultimatum.movedIds || []).map(String));
    const seen = /* @__PURE__ */ new Set();
    const removals = [];
    forEachPiece(boardState, (piece, row, col) => {
      if (!isWorkerUltimatumTarget(boardState, piece) || seen.has(piece.id)) return;
      seen.add(piece.id);
      if (moved.has(String(piece.id))) return;
      removals.push({ piece, row, col });
    });
    removals.forEach(({ piece, row, col }) => {
      if (piece.type === "colossus") clearPieceCells(boardState, piece);
      else set(boardState, row, col, null);
    });
    boardState.ultimatum = null;
    const royalHeirDefeats = new Set(
      removals.filter(({ piece }) => isWorkerRegencyRoyalHeir(boardState, piece)).map(({ piece }) => piece.color)
    );
    if (royalHeirDefeats.has("white") && royalHeirDefeats.has("black") || !sideHasSurvivalPiece(boardState, "white") && !sideHasSurvivalPiece(boardState, "black")) {
      boardState.mode = "gameover";
      boardState.winner = "draw";
    } else if (royalHeirDefeats.has("white") || !sideHasSurvivalPiece(boardState, "white")) {
      boardState.mode = "gameover";
      boardState.winner = "black";
    } else if (royalHeirDefeats.has("black") || !sideHasSurvivalPiece(boardState, "black")) {
      boardState.mode = "gameover";
      boardState.winner = "white";
    }
  }
  function workerSeveranceRemaining(boardState, piece) {
    if (!piece?.severed) return 0;
    if (Number.isFinite(Number(piece.severed.expiresFullMove))) return workerFullMoveEffectRemaining(boardState, piece.severed);
    return Math.max(0, Number(piece.severed.remaining) || 0);
  }
  function workerIceSheetRemaining(piece) {
    if (!piece?.iceSheet) return 0;
    return Math.max(0, Number(piece.iceSheet.remaining) || 0);
  }
  function noteWorkerTwinMovement(piece) {
    if (!piece?.twinBondId) return false;
    piece.twinSwapPending = Math.max(0, Number(piece.twinSwapPending) || 0) + 1;
    return true;
  }
  function noteWorkerUltimatumMovement(boardState, piece) {
    noteWorkerTwinMovement(piece);
    const ultimatum = boardState?.ultimatum;
    if (!ultimatum || !piece?.id || !isWorkerUltimatumTarget(boardState, piece)) return;
    if (!Array.isArray(ultimatum.movedIds)) ultimatum.movedIds = [];
    const id = String(piece.id);
    if (!ultimatum.movedIds.includes(id)) ultimatum.movedIds.push(id);
    if (ultimatum.movedIds.length > 96) ultimatum.movedIds = ultimatum.movedIds.slice(-96);
  }
  function isWorkerUltimatumTarget(boardState, piece) {
    return Boolean(piece && ["white", "black"].includes(piece.color) && !isWorkerRoyalIdentityPiece(boardState, piece) && !["merchant", "wall", "football"].includes(piece.type));
  }
  function clonePiece(piece) {
    if (!piece) return null;
    const cloned = {
      ...piece,
      quantum: piece.quantum ? { ...piece.quantum } : void 0,
      iceSheet: piece.iceSheet ? { ...piece.iceSheet } : void 0,
      shielded: Boolean(piece.shielded),
      frozen: Boolean(piece.frozen),
      moved: Boolean(piece.moved)
    };
    for (const key of [
      "witchTrial",
      "disarmed",
      "severed",
      "lastResistance",
      "coronationProtection",
      "sacrificeProtection",
      "frozenByCard",
      "staked",
      "necromancy"
    ]) {
      if (piece[key] && typeof piece[key] === "object") cloned[key] = clonePlain(piece[key]);
    }
    return cloned;
  }
  function cloneCard(card) {
    return card ? { ...card } : null;
  }
  // Perf: same result as JSON.parse(JSON.stringify(value)) for plain JSON-like data; anything
  // exotic (undefined/function/symbol/non-finite/non-plain objects/__proto__/cycles) falls back to JSON.
  const CLONE_PLAIN_BAIL = { bail: true };
  function clonePlainFast(value, depth) {
    if (value === null) return null;
    const t = typeof value;
    if (t === "string" || t === "boolean") return value;
    if (t === "number") return Number.isFinite(value) ? (value === 0 ? 0 : value) : CLONE_PLAIN_BAIL;
    if (t !== "object" || depth > 40) return CLONE_PLAIN_BAIL;
    if (Array.isArray(value)) {
      if (Object.getPrototypeOf(value) !== Array.prototype) return CLONE_PLAIN_BAIL;
      const out = new Array(value.length);
      for (let i = 0; i < value.length; i += 1) {
        const c = clonePlainFast(value[i], depth + 1);
        if (c === CLONE_PLAIN_BAIL) return c;
        out[i] = c;
      }
      return out;
    }
    const proto = Object.getPrototypeOf(value);
    if (proto !== Object.prototype && proto !== null) return CLONE_PLAIN_BAIL;
    const keys = Object.keys(value);
    const out = {};
    for (let i = 0; i < keys.length; i += 1) {
      const k = keys[i];
      if (k === "__proto__") return CLONE_PLAIN_BAIL;
      const c = clonePlainFast(value[k], depth + 1);
      if (c === CLONE_PLAIN_BAIL) return c;
      out[k] = c;
    }
    return out;
  }
  function clonePlain(value) {
    if (value == null) return value;
    const fast = clonePlainFast(value, 0);
    if (fast !== CLONE_PLAIN_BAIL) return fast;
    return JSON.parse(JSON.stringify(value));
  }
  function cloneAction(action) {
    return action ? JSON.parse(JSON.stringify(action)) : action;
  }
  function sameAction(a, b) {
    if (!a || !b || a.type !== b.type) return false;
    if (a.type === "card") return a.cardInstanceId === b.cardInstanceId && JSON.stringify(a.target || null) === JSON.stringify(b.target || null);
    if (a.type === "shotgunReload") return a.from?.row === b.from?.row && a.from?.col === b.from?.col;
    if (a.type === "wizardSpell") return a.spellId === b.spellId && a.from?.row === b.from?.row && a.from?.col === b.from?.col && JSON.stringify(a.target || null) === JSON.stringify(b.target || null);
    return a.from?.row === b.from?.row && a.from?.col === b.from?.col && a.move?.row === b.move?.row && a.move?.col === b.move?.col;
  }
  function emptyBoard(rows = 8, cols = 8) {
    return Array.from({ length: rows }, () => Array.from({ length: cols }, () => null));
  }
  function forEachPiece(boardState, callback) {
    const memo = ATTACK_MEMO;
    if (memo !== null && memo.board === boardState) {
      let list = memo.pieces;
      if (list === null) {
        list = memo.pieces = [];
        forEachPieceRaw(boardState, (piece, row, col) => list.push({ piece, row, col }));
      }
      for (let i = 0; i < list.length; i += 1) callback(list[i].piece, list[i].row, list[i].col);
      return;
    }
    forEachPieceRaw(boardState, callback);
  }
  function forEachPieceRaw(boardState, callback) {
    // Pieces without an id are unique per cell, so only real ids need the dedupe set
    // (multi-cell pieces share an id).
    let seen = null;
    const rowCount = boardRowCount(boardState);
    const colCount = boardColCount(boardState);
    const boardRows = boardState.board;
    for (let row = 0; row < rowCount; row += 1) {
      for (let col = 0; col < colCount; col += 1) {
        const piece = boardRows[row]?.[col] || null;
        if (!piece) continue;
        const id = piece.id;
        if (id) {
          if (seen === null) seen = /* @__PURE__ */ new Set();
          if (seen.has(id)) continue;
          seen.add(id);
        }
        if (Number.isInteger(piece.anchorRow) && Number.isInteger(piece.anchorCol) && (piece.anchorRow !== row || piece.anchorCol !== col)) continue;
        callback(piece, row, col);
      }
    }
  }
  // Grafted from engine.optimized.js (our-only additions): a shared
  // per-boardState piece-list cache, since forEachPiece was found to be a
  // major CPU cost from many different special-piece mechanics each doing
  // their own uncached full-board scan. Safe because a boardState is never
  // mutated in place mid-search except through set() below, which now
  // unconditionally invalidates the cache entry on every square mutation.
  const ALL_PIECES_LIST_CACHE = /* @__PURE__ */ new WeakMap();
  function allPiecesList(boardState) {
    let cached = ALL_PIECES_LIST_CACHE.get(boardState);
    if (!cached) {
      cached = [];
      forEachPiece(boardState, (piece, row, col) => cached.push({ piece, row, col }));
      ALL_PIECES_LIST_CACHE.set(boardState, cached);
    }
    return cached;
  }
  function forEachPieceCached(boardState, callback) {
    const list = allPiecesList(boardState);
    for (let i = 0; i < list.length; i += 1) {
      callback(list[i].piece, list[i].row, list[i].col);
    }
  }
  function get(boardState, row, col) {
    if (!inBounds(row, col, boardState)) return null;
    return boardState.board[row]?.[col] || null;
  }
  function set(boardState, row, col, value) {
    if (!inBounds(row, col, boardState)) return;
    boardState.board[row][col] = value;
    if (ATTACK_MEMO !== null && ATTACK_MEMO.board === boardState) {
      // In-place mutation (e.g. vortex shuffle simulation) invalidates every per-call cache.
      const memo = ATTACK_MEMO;
      memo.map = /* @__PURE__ */ new Map();
      memo.pieces = null;
      memo.encouraged = /* @__PURE__ */ new Map();
      memo.ranged = /* @__PURE__ */ new Map();
      memo.attackers = /* @__PURE__ */ new Map();
    }
    if (ALL_PIECES_LIST_CACHE.has(boardState)) ALL_PIECES_LIST_CACHE.delete(boardState);
  }
  function workerCreatePiece(color, type, boardState, row, col) {
    type = monochromePieceType(type, boardState?.monochromeChess);
    const piece = {
      color,
      type,
      moved: true,
      shielded: false,
      id: `${color}-${type}-ai-${Number(boardState.moveCount) || 0}-${row}-${col}`
    };
    if (["bigRook", "bigBishop"].includes(type)) {
      piece.hp = 2;
      piece.maxHp = 2;
    }
    if (boardState?.monochromeChess && inBounds(row, col, boardState)) piece.monoShade = squareShade(row, col);
    return piece;
  }
  function isWorkerAlibabaPlacementOpen(boardState, row, col) {
    if (!inBounds(row, col, boardState) || get(boardState, row, col)) return false;
    if (workerSeptember12PlacementCrownBlocked(boardState, row, col)) return false;
    if (isWorkerCollapsedSquare(boardState, row, col)) return false;
    if (normalizeCells(boardState?.blackHole).some((cell) => cell.row === row && cell.col === col)) return false;
    return !normalizePendingPortals(boardState?.pendingPortals, boardRowCount(boardState), boardColCount(boardState)).some((entry) => entry.cells.some((cell) => cell.row === row && cell.col === col));
  }
  function isWorkerAlibabaPlacementSafe(boardState, row, col, color) {
    const previous = get(boardState, row, col);
    set(boardState, row, col, { color, type: "eagle", moved: true, shielded: false });
    try {
      return !isSquareAttacked(boardState, row, col, opponent(color));
    } finally {
      set(boardState, row, col, previous);
    }
  }
  function workerAlibabaPlacementCandidates(boardState, origin, color) {
    return nearestAlibabaPlacementCandidates(
      origin,
      boardRowCount(boardState),
      boardColCount(boardState),
      ({ row, col }) => isWorkerAlibabaPlacementOpen(boardState, row, col),
      ({ row, col }) => isWorkerAlibabaPlacementSafe(boardState, row, col, color)
    );
  }
  function setWorkerBoardDimensions(boardState) {
    workerBoardRows = boardRowCount(boardState);
    workerBoardCols = boardColCount(boardState);
  }
  function boardRowCount(boardState) {
    return Array.isArray(boardState?.board) && boardState.board.length ? boardState.board.length : workerBoardRows;
  }
  function boardColCount(boardState) {
    const memo = ATTACK_MEMO;
    if (memo !== null && memo.board === boardState) {
      if (memo.cols === -1) memo.cols = boardColCountRaw(boardState);
      return memo.cols;
    }
    return boardColCountRaw(boardState);
  }
  function boardColCountRaw(boardState) {
    if (!Array.isArray(boardState?.board) || !boardState.board.length) return workerBoardCols;
    const rows = boardState.board;
    let max = 1;
    for (let i = 0; i < rows.length; i += 1) {
      const r = rows[i];
      if (Array.isArray(r) && r.length > max) max = r.length;
    }
    return max;
  }
  function boardRayLimit(boardState) {
    return Math.max(boardRowCount(boardState), boardColCount(boardState));
  }
  function inBounds(row, col, boardState = null) {
    const rows = boardState ? boardRowCount(boardState) : workerBoardRows;
    const cols = boardState ? boardColCount(boardState) : workerBoardCols;
    return Number.isInteger(row) && Number.isInteger(col) && row >= 0 && row < rows && col >= 0 && col < cols;
  }
  function isWorkerCollapsedSquare(boardState, row, col) {
    const depth = normalizeCollapseDepth(
      boardState?.collapseDepth,
      boardRowCount(boardState),
      boardColCount(boardState),
      Boolean(boardState?.collapsed)
    );
    return isCollapsedBoardCell(
      row,
      col,
      boardRowCount(boardState),
      boardColCount(boardState),
      depth,
      boardState?.collapsedCells
    );
  }
  function squareShade(row, col) {
    return (row + col) % 2 === 0 ? "light" : "dark";
  }
  function opponent(color) {
    return color === "white" ? "black" : "white";
  }
  function renderType(piece) {
    if (!piece) return "";
    if (piece.type === "windmill") return piece.windmillMode === "rook" ? "rook" : "bishop";
    return piece.type;
  }
  function isPromotionRow(piece, row, boardState) {
    if (!piece || !["pawn", "squire", "standardBearer"].includes(piece.type) || piece.noPromotion) return false;
    const promotionRow = piece.color === "white" ? 0 : boardRowCount(boardState) - 1;
    if (boardState.finalWeapon?.[piece.color]) return !boardState.collapsed && row === promotionRow;
    const collapseDepth = normalizeCollapseDepth(
      boardState.collapseDepth,
      boardRowCount(boardState),
      boardColCount(boardState),
      Boolean(boardState.collapsed)
    );
    const collapsedPromotionRow = piece.color === "white" ? collapseDepth : Math.max(0, boardRowCount(boardState) - 1 - collapseDepth);
    if (!boardState.collapsed && row === promotionRow) return true;
    if (boardState.collapsed && row === collapsedPromotionRow) return true;
    if (!boardState.earlyPromotion?.[piece.color] && !boardState.fastGrowth?.[piece.color]) return false;
    return workerHasReachedPromotionRow(
      piece.color,
      row,
      advancedPromotionRow(
        piece.color,
        boardRowCount(boardState),
        Boolean(boardState.earlyPromotion?.[piece.color]),
        Boolean(boardState.fastGrowth?.[piece.color])
      )
    );
  }
  function workerHasReachedPromotionRow(color, row, targetRow2) {
    return color === "white" ? row <= targetRow2 : row >= targetRow2;
  }
  function isWorkerFastGrowthPromotion(boardState, piece, row) {
    if (!boardState.fastGrowth?.[piece.color]) return false;
    const promotionRow = piece.color === "white" ? 0 : boardRowCount(boardState) - 1;
    if (row === promotionRow) return false;
    const targetRow2 = advancedPromotionRow(
      piece.color,
      boardRowCount(boardState),
      Boolean(boardState.earlyPromotion?.[piece.color]),
      true
    );
    return workerHasReachedPromotionRow(piece.color, row, targetRow2);
  }
  function workerAutomaticPromotionType(boardState, piece, row) {
    if (boardState.finalWeapon?.[piece.color] && !boardState.collapsed) return "amazon";
    if (isWorkerFastGrowthPromotion(boardState, piece, row)) {
      return monochromePieceType("bishop", boardState.monochromeChess);
    }
    return "queen";
  }
  function diagonals() {
    return [[1, 1], [1, -1], [-1, 1], [-1, -1]];
  }
  function orthogonals() {
    return [[1, 0], [-1, 0], [0, 1], [0, -1]];
  }
  function queenDirections() {
    return [...diagonals(), ...orthogonals()];
  }
  function uniqueMoves(moves) {
    const seen = /* @__PURE__ */ new Set();
    return (moves || []).filter((move) => {
      const key = `${move.row},${move.col},${move.colossusMove ? "cm" : ""},${move.colossusAttack ? "ca" : ""},${move.bigRookMove ? "br" : ""}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  function bloodMoonState(boardState) {
    if (boardState?.campaign?.setup !== "bloodMoon") return null;
    return boardState.campaign.bloodMoon || {};
  }
  function ensureBloodMoonState(boardState) {
    if (boardState?.campaign?.setup !== "bloodMoon") return null;
    if (!boardState.campaign.bloodMoon) {
      boardState.campaign.bloodMoon = {
        nightBloodPeriods: { white: -1, black: -1 },
        sunlightOverride: null,
        veilUntil: null
      };
    }
    if (!boardState.campaign.bloodMoon.nightBloodPeriods) {
      boardState.campaign.bloodMoon.nightBloodPeriods = { white: -1, black: -1 };
    }
    return boardState.campaign.bloodMoon;
  }
  function bloodMoonHalfTurns(boardState) {
    return Math.max(0, (Number(boardState?.turnsTaken?.white) || 0) + (Number(boardState?.turnsTaken?.black) || 0));
  }
  function workerOutpostTarget(boardState, piece, row, col, color) {
    if (!piece || piece.color !== color || piece.editorRoyal || isWorkerKingRole(boardState, piece) || piece.outpostProtected || ["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type)) return false;
    const cells = workerIsLargePiece(piece) ? colossusCells(row, col) : [{ row, col }];
    return septemberOutpostEligible(color, cells, boardRowCount(boardState));
  }
  function workerRecurrenceTarget(boardState, piece, color) {
    return Boolean(piece && piece.color === color && !(piece.editorRoyal || isWorkerKingRole(boardState, piece)) && !piece.recurrence && !["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type));
  }
  function workerNullificationTarget(piece, color) {
    return Boolean(piece && piece.color === color && !piece.nullification && !["wall", "football", "blackHole", "monster", "coffin"].includes(piece.type));
  }
  function clearWorkerBloodMoonTurnEffects(boardState, color) {
    if (legacyBloodMoonLifecycleStates.has(boardState)) return;
    const data = bloodMoonState(boardState);
    if (data?.sunlightOverride === color) data.sunlightOverride = null;
  }
  function maybeGrantWorkerNightBloodForTurn(boardState, color) {
    if (legacyBloodMoonLifecycleStates.has(boardState) || !bloodMoonState(boardState) || !hasVampireLord(boardState, color)) return;
    const cycle = bloodMoonCycleInfo(boardState);
    if (!cycle.night) return;
    const data = ensureBloodMoonState(boardState);
    if (data.nightBloodPeriods[color] === cycle.period) return;
    data.nightBloodPeriods[color] = cycle.period;
    grantWorkerBloodCard(boardState, color);
  }
  function findPieceByType(boardState, color, type) {
    let found = null;
    forEachPiece(boardState, (piece, row, col) => {
      if (!found && piece.color === color && piece.type === type) found = { piece, row, col };
    });
    return found;
  }
  function hasVampireLord(boardState, color) {
    return Boolean(findPieceByType(boardState, color, "vampireLord"));
  }
  function sharedTurnCount(boardState) {
    return Math.min(Number(boardState?.turnsTaken?.white) || 0, Number(boardState?.turnsTaken?.black) || 0);
  }
  function setWorkerHeraldJumpLock(boardState, piece, color) {
    if (!piece) return;
    piece.heraldJumpUnlocked = false;
    piece.heraldJumpLockTurn = Number(boardState?.turnsTaken?.[color]) || 0;
  }
  function isWorkerHeraldJumpLocked(boardState, piece, color) {
    if (!piece || !pieceHasAbility(piece, "herald")) return false;
    if (Number.isFinite(Number(piece.heraldJumpLockTurn))) {
      return boardState?.turn === color && (Number(boardState?.turnsTaken?.[color]) || 0) === Number(piece.heraldJumpLockTurn);
    }
    return piece.heraldJumpUnlocked === false;
  }
  function bloodMoonCycleInfo(boardState) {
    const cycleLength = BLOOD_MOON_DAY_TURNS + BLOOD_MOON_NIGHT_TURNS;
    const offset = sharedTurnCount(boardState) % cycleLength;
    return {
      night: offset >= BLOOD_MOON_DAY_TURNS,
      period: Math.floor(sharedTurnCount(boardState) / cycleLength) * 2 + (offset >= BLOOD_MOON_DAY_TURNS ? 1 : 0),
      offset,
      length: offset >= BLOOD_MOON_DAY_TURNS ? BLOOD_MOON_NIGHT_TURNS : BLOOD_MOON_DAY_TURNS
    };
  }
  function usesBloodMoonNightMovement(boardState, color) {
    const data = bloodMoonState(boardState);
    return Boolean(data && (bloodMoonCycleInfo(boardState).night || data.sunlightOverride === color));
  }
  function consumeWorkerActiveRoyalReaperExecution(boardState, activePiece) {
    const pending = activePiece?.pendingReaperDefeat;
    if (activePiece) delete activePiece.pendingReaperDefeat;
    if (!pending || !inBounds(pending.row, pending.col, boardState)) return null;
    const located = piecesMatching(boardState, (piece) => piece.id && piece.id === pending.reaperId)[0] || null;
    const reaper = located?.piece;
    if (!reaper || reaper.type !== "reaper") return null;
    clearPieceCells(boardState, reaper);
    set(boardState, pending.row, pending.col, reaper);
    return { row: pending.row, col: pending.col };
  }
  const AUGMENT_BASE_PARITY_FILES = "abcdefgh";
  const AUGMENT_BASE_PARITY_PROMOTIONS = /* @__PURE__ */ new Set(["queen", "rook", "bishop", "knight"]);
  const AUGMENT_BASE_PARITY_TYPES = Object.freeze({ P: "pawn", N: "knight", B: "bishop", R: "rook", Q: "queen", K: "king" });
  const AUGMENT_BASE_PARITY_CODES = Object.freeze({ pawn: "P", knight: "N", bishop: "B", rook: "R", queen: "Q", king: "K" });
  function augmentBaseParityCoordinates(square) {
    if (typeof square !== "string" || !/^[a-h][1-8]$/u.test(square)) return null;
    return { row: 8 - Number(square[1]), col: AUGMENT_BASE_PARITY_FILES.indexOf(square[0]) };
  }
  function augmentBaseParitySquare(row, col) {
    return `${AUGMENT_BASE_PARITY_FILES[col]}${8 - row}`;
  }
  function augmentBaseParityPieceMoved(before, code, row, col) {
    const color = code[0] === "w" ? "white" : "black";
    const type = code[1];
    const homeRow = color === "white" ? 7 : 0;
    if (type === "P") return row !== (color === "white" ? 6 : 1);
    if (type === "K") {
      const rights = before.castlingRights?.[color];
      return !(rights?.kingSide || rights?.queenSide) || row !== homeRow || col !== 4;
    }
    if (type === "R" && row === homeRow && [0, 7].includes(col)) {
      const side = col === 7 ? "kingSide" : "queenSide";
      return before.castlingRights?.[color]?.[side] !== true;
    }
    return true;
  }
  function augmentBaseParityClientState(before) {
    const board = before.board.map((sourceRow, row) => sourceRow.map((code, col) => {
      if (code === null) return null;
      const color = code[0] === "w" ? "white" : "black";
      return {
        id: `parity-${augmentBaseParitySquare(row, col)}-${code}`,
        color,
        type: AUGMENT_BASE_PARITY_TYPES[code[1]],
        moved: augmentBaseParityPieceMoved(before, code, row, col),
        origin: augmentBaseParitySquare(row, col)
      };
    }));
    const enPassantTarget = augmentBaseParityCoordinates(before.enPassantTarget);
    const previousColor = opponent(before.turn);
    const previousDirection = previousColor === "white" ? -1 : 1;
    const completedWhiteTurns = Math.max(0, before.fullmoveNumber - (before.turn === "white" ? 1 : 0));
    const completedBlackTurns = Math.max(0, before.fullmoveNumber - 1);
    return normalizeState({
      board,
      turn: before.turn,
      mode: before.status === "active" ? "play" : "gameover",
      winner: before.winner || "",
      aiSearchNoCards: true,
      draftDelete: true,
      actionsRemaining: 1,
      turnsTaken: { white: completedWhiteTurns, black: completedBlackTurns },
      moveCount: completedWhiteTurns + completedBlackTurns,
      fullMove: before.fullmoveNumber,
      enPassant: enPassantTarget ? {
        row: enPassantTarget.row,
        col: enPassantTarget.col,
        capturedRow: enPassantTarget.row + previousDirection,
        capturedCol: enPassantTarget.col,
        color: previousColor
      } : null
    });
  }
  function augmentBaseParityCastleMove(before, clientState, actor, from, to) {
    const homeRow = actor === "white" ? 7 : 0;
    if (from.row !== homeRow || from.col !== 4 || to.row !== homeRow || ![2, 6].includes(to.col)) return null;
    if (isPortalCastlingRequestBlocked(from, to, workerPortalRule(clientState)?.cells)) return null;
    const kingSide = to.col === 6;
    const side = kingSide ? "kingSide" : "queenSide";
    if (before.castlingRights?.[actor]?.[side] !== true) return null;
    const rookCol = kingSide ? 7 : 0;
    const rookToCol = kingSide ? 5 : 3;
    const betweenCols = kingSide ? [5, 6] : [1, 2, 3];
    const transitCol = kingSide ? 5 : 3;
    const rook = get(clientState, homeRow, rookCol);
    if (rook?.type !== "rook" || rook.color !== actor || rook.moved || betweenCols.some((col) => get(clientState, homeRow, col)) || isSquareAttacked(clientState, homeRow, 4, opponent(actor)) || isSquareAttacked(clientState, homeRow, transitCol, opponent(actor)) || isSquareAttacked(clientState, homeRow, to.col, opponent(actor))) return null;
    return {
      row: to.row,
      col: to.col,
      castle: true,
      rookFrom: { row: homeRow, col: rookCol },
      rookTo: { row: homeRow, col: rookToCol }
    };
  }
  function augmentBaseParityFindMove(before, clientState, actor, payload, from, to, piece) {
    const castle = piece.type === "king" ? augmentBaseParityCastleMove(before, clientState, actor, from, to) : null;
    if (castle) return castle;
    return generateMovesForPiece(clientState, piece, from.row, from.col).filter((move) => isWorkerMoveAllowed(clientState, piece, from.row, from.col, move)).find((move) => move.row === to.row && move.col === to.col) ?? null;
  }
  function augmentBaseParityHasOrdinaryMove(clientState, actor) {
    let found = false;
    forEachPiece(clientState, (piece, row, col) => {
      if (found || piece.color !== actor) return;
      found = generateMovesForPiece(clientState, piece, row, col).some((move) => isWorkerMoveAllowed(clientState, piece, row, col, move));
    });
    return found;
  }
  function augmentBaseParityCastlingRights(clientState) {
    const rights = {};
    for (const color of ["white", "black"]) {
      const homeRow = color === "white" ? 7 : 0;
      const king = get(clientState, homeRow, 4);
      rights[color] = { kingSide: false, queenSide: false };
      if (king?.color !== color || king.type !== "king" || king.moved) continue;
      for (const [side, rookCol] of [["kingSide", 7], ["queenSide", 0]]) {
        const rook = get(clientState, homeRow, rookCol);
        rights[color][side] = Boolean(rook?.color === color && rook.type === "rook" && !rook.moved);
      }
    }
    return rights;
  }
  function augmentBaseParityProjection(clientState, termination) {
    return {
      board: clientState.board.map((row) => row.map((piece) => {
        if (!piece) return null;
        const prefix = piece.color === "white" ? "w" : "b";
        return `${prefix}${AUGMENT_BASE_PARITY_CODES[piece.type]}`;
      })),
      turn: clientState.turn,
      castlingRights: augmentBaseParityCastlingRights(clientState),
      enPassantTarget: clientState.enPassant ? augmentBaseParitySquare(clientState.enPassant.row, clientState.enPassant.col) : null,
      fullmoveNumber: Number(clientState.fullMove) || 1,
      status: clientState.mode === "gameover" ? "finished" : "active",
      winner: clientState.mode === "gameover" ? clientState.winner || null : null,
      termination: clientState.mode === "gameover" ? termination : null
    };
  }
  function applyAugmentBaseClientMoveForParity(before, actor, payload) {
    const from = augmentBaseParityCoordinates(payload?.from);
    const to = augmentBaseParityCoordinates(payload?.to);
    if (!from || !to || !["white", "black"].includes(actor)) return { ok: false, code: "INVALID_ACTION" };
    const clientState = augmentBaseParityClientState(before);
    setWorkerBoardDimensions(clientState);
    if (clientState.mode !== "play") return { ok: false, code: "GAME_OVER" };
    if (clientState.turn !== actor) return { ok: false, code: "NOT_YOUR_TURN" };
    const piece = get(clientState, from.row, from.col);
    if (!piece) return { ok: false, code: "NO_PIECE_AT_SOURCE" };
    if (piece.color !== actor) return { ok: false, code: "PIECE_NOT_OWNED" };
    const target = get(clientState, to.row, to.col);
    const reachesPromotion = piece.type === "pawn" && to.row === (actor === "white" ? 0 : 7);
    const capturesKing = target?.type === "king" && target.color === opponent(actor);
    if (reachesPromotion && capturesKing && payload.promotion !== null) return { ok: false, code: "UNEXPECTED_PROMOTION" };
    if (reachesPromotion && !capturesKing && !AUGMENT_BASE_PARITY_PROMOTIONS.has(payload.promotion)) {
      return { ok: false, code: "PROMOTION_REQUIRED" };
    }
    if (!reachesPromotion && payload.promotion !== null) return { ok: false, code: "UNEXPECTED_PROMOTION" };
    const move = augmentBaseParityFindMove(before, clientState, actor, payload, from, to, piece);
    if (!move) return { ok: false, code: "ILLEGAL_MOVE" };
    move.promotion = payload.promotion;
    const result = applyMoveAction(clientState, { type: "move", color: actor, from, move }, actor);
    if (!result.ok) return { ok: false, code: "ILLEGAL_MOVE" };
    if (reachesPromotion) {
      const landed = get(clientState, to.row, to.col);
      if (landed?.color === actor) landed.type = capturesKing ? "pawn" : payload.promotion;
    }
    let termination = capturesKing ? "king_capture" : null;
    if (clientState.mode !== "gameover" && !augmentBaseParityHasOrdinaryMove(clientState, clientState.turn)) {
      clientState.mode = "gameover";
      clientState.winner = actor;
      termination = "no_legal_move";
    }
    return { ok: true, after: augmentBaseParityProjection(clientState, termination) };
  }
  globalThis.__engineMerged = { searchBestAction, generateActions, applyAction, evaluateState, evaluateStateComponents, cloneState, setWorkerBoardDimensions };
  if (typeof module !== "undefined") module.exports = globalThis.__engineMerged;
})();
