import cardPresentation from '../../../augment-chess/contracts/catalog/card-presentation-20260928.json' with { type: 'json' };
import cardDefinitions from '../../../augment-chess/contracts/catalog/card-definitions-20260928.json' with { type: 'json' };

export type Color = 'white' | 'black';
export type Square = { row: number; col: number };
export type PublicRecord = Record<string, unknown>;
export type PublicIntent = PublicRecord & { type: string; color: Color };
export type PublicObservation = {
  viewer: Color;
  turn: Color;
  board: (PublicRecord | null)[][];
  ownCards: PublicRecord[];
  publicState: PublicRecord;
  history: unknown[];
};
export type CardView = {
  id: string;
  instanceId: string;
  name: string;
  phase: string;
  stars: number;
  text: string;
  help: string;
  target: string | null;
  used: boolean;
  recovering: boolean;
  slot: number | null;
  revealed?: PublicRecord;
};

const descriptions = new Map(cardPresentation.definitions.map((card) => [card.id, card]));
const definitions = new Map(cardDefinitions.definitions.map((card) => [card.id, card]));

export const catalogProvenance = {
  rulesVersion: cardPresentation.rulesVersion,
  catalogVersion: cardPresentation.catalogVersion,
  sourceMainSha256: cardPresentation.sourceMainSha256,
};

export function record(value: unknown): PublicRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as PublicRecord : {};
}
export function records(value: unknown): PublicRecord[] {
  return Array.isArray(value) ? value.map(record) : [];
}
export function text(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}
export function numeric(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}
export function square(value: unknown): Square | null {
  const item = record(value);
  return Number.isInteger(item.row) && Number.isInteger(item.col)
    && numeric(item.row, -1) >= 0 && numeric(item.row, -1) < 8
    && numeric(item.col, -1) >= 0 && numeric(item.col, -1) < 8
    ? { row: item.row as number, col: item.col as number } : null;
}
export function sameSquare(a: Square | null, b: Square | null): boolean {
  return a !== null && b !== null && a.row === b.row && a.col === b.col;
}
export function squareName(at: Square): string {
  return `${String.fromCharCode(97 + at.col)}${8 - at.row}`;
}
export function colorName(color: unknown): string {
  return color === 'white' ? '백' : color === 'black' ? '흑' : '중립';
}
export function cardView(card: PublicRecord): CardView {
  const id = text(card.id);
  const description = descriptions.get(id);
  const definition = definitions.get(id);
  return {
    id,
    instanceId: text(card.instanceId),
    name: description?.name ?? id,
    phase: text(card.phase, description?.phase ?? ''),
    stars: Number.isInteger(card.ratingHalfStars) && numeric(card.ratingHalfStars, -1) >= 0
      ? numeric(card.ratingHalfStars) / 2 : numeric(card.stars, description?.stars ?? 0),
    text: description?.text ?? '',
    help: description?.help ?? '',
    target: definition && 'target' in definition ? text(definition.target) || null : null,
    used: card.used === true,
    recovering: card.recovering === true,
    slot: Number.isInteger(card.slot) ? card.slot as number : null,
    revealed: record(card.revealed),
  };
}
export function ownCards(observation: PublicObservation | null): CardView[] {
  return (observation?.ownCards ?? []).map(cardView);
}
export function opponentCards(observation: PublicObservation | null): CardView[] {
  return records(observation?.publicState.revealedOpponentCards).map(cardView);
}
export function draftCards(observation: PublicObservation | null): CardView[] {
  // The native viewer projection is authoritative. Opposing hidden offers are
  // never reconstructed from the catalog or another player's snapshot.
  return records(record(observation?.publicState.draft).choices).map(cardView);
}
export function moveHints(observation: PublicObservation | null, from: Square | null): Square[] {
  if (!observation || !from) return [];
  const hints = records(record(observation.publicState.legalHints).moves);
  const hint = hints.find((item) => sameSquare(square(item.from), from));
  return Array.isArray(hint?.destinations)
    ? hint.destinations.map(square).filter((at): at is Square => at !== null) : [];
}
export function cardTargetHints(observation: PublicObservation | null, instanceId: string): Square[] {
  const hints = records(record(observation?.publicState.legalHints).cardTargets);
  const hint = hints.find((item) => item.cardInstanceId === instanceId);
  return Array.isArray(hint?.targets)
    ? hint.targets.map(square).filter((at): at is Square => at !== null) : [];
}
export function anchorSquare(piece: PublicRecord, cell: Square): Square {
  return square({ row: piece.anchorRow, col: piece.anchorCol }) ?? cell;
}
export function pieceName(piece: PublicRecord): string {
  const kind = text(piece.type, 'unknown');
  const names: Record<string, string> = {
    pawn: '폰', knight: '나이트', bishop: '비숍', rook: '룩', queen: '퀸', king: '킹',
    bigRook: '대형 룩', bigBishop: '대형 비숍', colossus: '콜로서스',
    shotgunKing: '샷건 킹', wizard: '마법사', royalKnight: '로열 나이트',
  };
  return names[kind] ?? descriptions.get(kind)?.name ?? kind;
}
export function pieceStatus(piece: PublicRecord): string[] {
  const status = record(piece.status);
  const result = Object.entries(status)
    .filter(([, value]) => value !== false && value !== null)
    .map(([key, value]) => value === true ? key : `${key}: ${String(value)}`);
  for (const key of ['hp', 'mana', 'ammo']) {
    if (typeof piece[key] === 'number') result.push(`${key}: ${piece[key]}`);
  }
  return result;
}
export function markLabel(mark: PublicRecord): string {
  const labels: Record<string, string> = {
    fogHidden: '안개', portal: '포털', meteor: '유성', lightning: '번개', ruleBomb: '폭탄',
    blackHole: '블랙홀', taboo: '금기', captureFlag: '깃발', crownGround: '왕관',
    conveyor: '컨베이어', winterForecast: '겨울 예보', accelerationTrail: '가속 흔적',
  };
  const kind = text(mark.kind);
  return labels[kind] ?? kind;
}
export function historySummary(value: unknown): string {
  const event = record(value);
  const actor = colorName(event.actor);
  const changes = records(event.boardChanges);
  const changed = changes.map((change) => square(change.square)).filter((at): at is Square => at !== null);
  const phase = text(event.phase);
  return [actor, text(event.kind, '전이'), phase, changed.map(squareName).join(' · ')].filter(Boolean).join(' / ');
}
