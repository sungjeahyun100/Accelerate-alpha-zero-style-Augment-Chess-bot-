"use strict";

// Enumerates UI selection spaces using the frozen client's target/rule helpers.
// It does not carry move or card-effect implementations from the site.
function installEnumerationSource() {
  return `
${completeCardTargets.toString()}
${completeWizardActions.toString()}
${orderedSelections.toString()}
${orderedCardTargetPlan.toString()}
${orderedSquareTargets.toString()}
${boundedOrderedTargetCount.toString()}
${freeMoveTargetGroups.toString()}
${iterateFreeMoveTargets.toString()}
${iterateCandidatePayloads.toString()}
collectAiCardTargets = completeCardTargets;
collectAiWizardActions = completeWizardActions;
`;
}
function completeCardTargets(card, color) {
  const originalTurn = state.turn;
  state.turn = color;
  const targets = [];
  const emit = target => {
    if (targets.length >= __maxCandidates) throw new Error("Complete target enumeration exceeded its explicit candidate budget.");
    targets.push(target);
  };
  const combinations = (items, min, max, transform) => {
    function visit(start, selected) {
      if (selected.length >= min) emit(transform(selected));
      if (selected.length >= max) return;
      for (let index = start; index < items.length; index++) visit(index + 1, [...selected, items[index]]);
    }
    visit(0, []);
  };
  try {
    if (card.effect === "joker") return reusableActiveCards(color).map(card => ({ cardInstanceId: card.instanceId }));
    if (card.effect === "ruleTicket") return ruleTicketCardPool().map(card => ({ ruleId: card.id }));
    if (card.effect === "barricade") {
      for (const direction of ["horizontal", "vertical"]) for (let row = 0; row < 8; row++) for (let col = 0; col < 8; col++) if (canPlaceBarricade(row, col, direction)) emit({ row, col, direction });
      return targets;
    }
    if (card.effect === "amazon") return findPiecesByPredicate(piece => isQueenIdentityPiece(piece, color)).flatMap(queen => findPieces(color, "knight").map(knight => ({ row: queen.row, col: queen.col, knight: { row: knight.row, col: knight.col } })));
    if (card.id === "grappler" && usesSeptember26Rebalance(state)) return findPiecesByPredicate(piece => piece.color === color && piece.type === "queen" && !isRoyalIdentityPiece(piece)).flatMap(queen => findPiecesByPredicate(piece => piece.color === color && isMinorPieceType(piece.type) && !isRoyalIdentityPiece(piece)).map(minor => ({ row: queen.row, col: queen.col, minor: { row: minor.row, col: minor.col } })));
    if (card.effect === "windmill") return findPieces(color, "bishop").flatMap(bishop => findPieces(color, "rook").map(rook => ({ row: rook.row, col: rook.col, bishop: { row: bishop.row, col: bishop.col } })));
    if (card.effect === "hook") return findPiecesByPredicate(piece => isQueenIdentityPiece(piece, color)).flatMap(queen => findPieces(color, "rook").map(rook => ({ row: queen.row, col: queen.col, rook: { row: rook.row, col: rook.col } })));
    if (card.effect === "feudalContract") {
      const pawns = [], guardians = [];
      forEachSquare((piece, row, col) => {
        if (!piece || piece.color !== color) return;
        if (["pawn", "fanatic"].includes(piece.type) && !piece.explosive && !piece.feudalContractId) pawns.push({ row, col });
        else if (!["pawn", "fanatic", "wall", "colossus", "bigRook", "bigBishop"].includes(piece.type)) guardians.push({ row, col });
      });
      return pawns.flatMap(pawn => guardians.map(guardian => ({ row: guardian.row, col: guardian.col, pawn })));
    }
    if (card.id === "brainwash") return brainwashChoices(state, color, { value: piece => pieceCombatValue(piece, state), isRoyal: isRoyalIdentityPiece }).map(({ source, target }) => ({ selections: [{ row: source.row, col: source.col }, { row: target.row, col: target.col }] }));
    if (card.id === "taboo") {
      for (const { item, row, col } of expansionBoardEntries(state.board)) if (item.color === color && item.type === "queen" && !isRoyalIdentityPiece(item)) forEachSquare((_, r, c) => { if (isSquareOpenForPiecePlacement(r, c)) emit({ selections: [{ row, col }, { row: r, col: c }] }); });
      return targets;
    }
    if (card.effect === "twins" || card.effect === "chain") return (card.effect === "twins" ? twinPairCandidates(color) : chainPairCandidates(color)).map(pair => ({ selections: pair.map(({ row, col }) => ({ row, col })) }));
    const ordered = orderedCardTargetPlan(card);
    if (ordered) {
      boundedOrderedTargetCount(ordered.squares.length, ordered.length, __maxCandidates);
      for (const target of orderedSquareTargets(ordered.squares, ordered.length)) emit(target);
      return targets;
    }
    const ranges = { cleanupPieces: [1, INTERNAL_CLEANUP_LIMIT], chameleonMutation: [1, 3], emergencyEvacuation: [1, 3], spy: [1, 2], pawnStorm: [1, 8] };
    if (ranges[card.effect]) { combinations(uniqueTargetSquaresForCard(card), ...ranges[card.effect], selections => ({ selections })); return targets; }
    if (card.effect === "freeMove") {
      for (const target of iterateFreeMoveTargets(color)) emit(target);
      return targets;
    }
    if (!card.target) return [undefined];
    return getTargetSquares(card);
  } finally { state.turn = originalTurn; }
}
// At most three submitted plans, with one choice from each distinct piece.
// Only the current depth-3 prefix is retained; exhaustion covers all orderings.
function* orderedSelections(groups, maxPlans = 3) {
  if (!Number.isInteger(maxPlans) || maxPlans < 1 || maxPlans > 3) throw new TypeError("Ordered plan depth must be 1..3.");
  function* visit(selected, used) {
    if (selected.length) yield selected.slice();
    if (selected.length >= maxPlans) return;
    for (let index = 0; index < groups.length; index++) if (!used.has(index)) {
      used.add(index);
      for (const move of groups[index]) {
        selected.push(move);
        yield* visit(selected, used);
        selected.pop();
      }
      used.delete(index);
    }
  }
  yield* visit([], new Set());
}
// These three effects have source-observed order-sensitive results. Other
// multi-target families retain their existing combination enumeration.
function orderedCardTargetPlan(card) {
  const length = card.effect === "hypocrisy" ? 4 : ["portalGun", "panic"].includes(card.effect) ? 2 : 0;
  if (!length) return null;
  const raw = card.effect === "hypocrisy" ? hypocrisyCandidateSquares() :
    card.effect === "portalGun" ? getTargetSquares(card) : uniqueTargetSquaresForCard(card);
  const seen = new Set(), squares = [];
  for (const square of raw) {
    if (!Number.isInteger(square?.row) || !Number.isInteger(square?.col)) throw new Error("Invalid ordered card target square.");
    const key = `${square.row},${square.col}`;
    if (!seen.has(key)) { seen.add(key); squares.push({ row: square.row, col: square.col }); }
  }
  return { squares, length };
}
// At most four selected squares are retained, even if the full surface is large.
function* orderedSquareTargets(squares, length) {
  const selected = [], used = new Set();
  function* visit() {
    if (selected.length === length) { yield { selections: selected.slice() }; return; }
    for (let index = 0; index < squares.length; index++) if (!used.has(index)) {
      used.add(index); selected.push(squares[index]);
      yield* visit();
      selected.pop(); used.delete(index);
    }
  }
  yield* visit();
}
function boundedOrderedTargetCount(squareCount, length, limit) {
  if (squareCount < length) return 0;
  let count = 1;
  for (let selected = 0; selected < length; selected++) {
    count *= squareCount - selected;
    if (count > limit) throw new Error("Complete target enumeration exceeded its explicit candidate budget; use actionStream.");
  }
}
function freeMoveTargetGroups(color) {
  const originalTurn = state.turn, groups = [], seen = new Set();
  state.turn = color;
  try {
    forEachSquare((piece, row, col) => {
      if (!isFreeMovePieceCandidate(piece, color) || seen.has(piece.id)) return;
      seen.add(piece.id);
      const from = normalizePieceSquare(row, col);
      const moves = freeMoveDeclarationMoves(from.row, from.col, piece).map(move => ({ from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col } }));
      if (moves.length) groups.push(moves);
    });
    return groups;
  } finally { state.turn = originalTurn; }
}
function* iterateFreeMoveTargets(color) {
  for (const selections of orderedSelections(freeMoveTargetGroups(color), FREE_MOVE_MAX_PLANS)) yield { selections };
}
function* iterateCandidatePayloads(color, cardId = null) {
  let emitted = false;
  const checked = actions => {
    if (actions.length > __maxCandidates) throw new Error("Non-lazy action family exceeded its explicit candidate budget.");
    return actions;
  };
  if (cardId === null) for (const action of checked(collectValidAiActions(color, { includeCards: false, allowFriendlyCrush: false }))) {
    emitted = true; yield action;
  }
  // The source collector owns availability, exclusive-turn and forced-window
  // checks. One card family is selected by its stable execution identity.
  const cards = playerDeck(color).filter(card => card && (!cardId || card.id === cardId)).map(card => ({ instanceId: card.instanceId, effect: card.effect }));
  for (const card of cards) {
    const options = { cardsOnly: true, includeCards: true, exhaustiveCards: true, cardFilter: current => current.instanceId === card.instanceId };
    if (card.effect === "freeMove" || ["portalGun", "hypocrisy", "panic"].includes(card.effect)) {
      const originalTargets = collectAiCardTargets;
      let marker;
      try {
        collectAiCardTargets = current => current.instanceId === card.instanceId ? [undefined] : [];
        marker = collectValidAiActions(color, options)[0];
      } finally { collectAiCardTargets = originalTargets; }
      if (marker) {
        let targets;
        if (card.effect === "freeMove") targets = iterateFreeMoveTargets(color);
        else {
          const current = playerDeck(color).find(entry => entry?.instanceId === card.instanceId);
          const plan = current && orderedCardTargetPlan(current);
          if (!plan) throw new Error("Ordered card disappeared during source enumeration.");
          targets = orderedSquareTargets(plan.squares, plan.length);
        }
        for (const target of targets) {
          emitted = true; yield { ...marker, target };
        }
      }
    } else for (const action of checked(collectValidAiActions(color, options))) {
      emitted = true; yield action;
    }
  }
  // Preserve the source's friendly-crush fallback only after all ordinary
  // piece and card families really proved empty.
  if (!emitted && cardId === null) for (const action of checked(collectValidAiActions(color, { includeCards: false, allowFriendlyCrush: true }))) yield action;
}
function completeWizardActions(piece, row, col, color) {
  if (isWizardSpellBlockedByZugzwang(color)) return [];
  const actions = [];
  for (const [spellId, cost] of [["meteor", 3], ["lightning", 1], ["shield", 2], ["timeStop", 5]]) {
    if ((piece.mana || 0) < cost) continue;
    if (spellId === "timeStop") { actions.push({ type: "wizardSpell", color, from: { row, col }, spellId, target: { row, col } }); continue; }
    for (let r = 0; r < (spellId === "meteor" ? 7 : 8); r++) for (let c = 0; c < (spellId === "meteor" ? 7 : 8); c++) actions.push({ type: "wizardSpell", color, from: { row, col }, spellId, target: { row: r, col: c } });
  }
  return actions;
}
module.exports = { installEnumerationSource, orderedSelections };
