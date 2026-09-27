"use strict";

// Enumerates UI selection spaces using the frozen client's target/rule helpers.
// It does not carry move or card-effect implementations from the site.
function installEnumerationSource() {
  return `
${completeCardTargets.toString()}
${completeWizardActions.toString()}
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
    if (card.effect === "portalGun") { combinations(getTargetSquares(card), 2, 2, selections => ({ selections })); return targets; }
    if (card.effect === "hypocrisy") { combinations(hypocrisyCandidateSquares(), 4, 4, selections => ({ selections })); return targets; }
    const ranges = { cleanupPieces: [1, INTERNAL_CLEANUP_LIMIT], chameleonMutation: [1, 3], emergencyEvacuation: [1, 3], panic: [2, 2], spy: [1, 2], pawnStorm: [1, 8] };
    if (ranges[card.effect]) { combinations(uniqueTargetSquaresForCard(card), ...ranges[card.effect], selections => ({ selections })); return targets; }
    if (card.effect === "freeMove") {
      const groups = [], seen = new Set();
      forEachSquare((piece, row, col) => {
        if (!isFreeMovePieceCandidate(piece, color) || seen.has(piece.id)) return;
        seen.add(piece.id);
        const from = normalizePieceSquare(row, col);
        const moves = freeMoveDeclarationMoves(from.row, from.col, piece).map(move => ({ from: { row: from.row, col: from.col }, to: { row: move.row, col: move.col } }));
        if (moves.length) groups.push(moves);
      });
      function visit(selected, used) {
        if (selected.length) emit({ selections: selected });
        if (selected.length >= FREE_MOVE_MAX_PLANS) return;
        // Resolution executes plans in the submitted order. Distinct orderings
        // must survive even when they contain the same chosen pieces.
        for (let index = 0; index < groups.length; index++) if (!used.has(index)) for (const move of groups[index]) visit([...selected, move], new Set([...used, index]));
      }
      visit([], new Set());
      return targets;
    }
    if (!card.target) return [undefined];
    return getTargetSquares(card);
  } finally { state.turn = originalTurn; }
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
module.exports = { installEnumerationSource };
