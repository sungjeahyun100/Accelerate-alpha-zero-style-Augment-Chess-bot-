import catalog from '../../../augment-chess/contracts/catalog/site-20260928.json' with { type: 'json' };
import definitions from '../../../augment-chess/contracts/catalog/card-definitions-20260928.json' with { type: 'json' };
import presentation from '../../../augment-chess/contracts/catalog/card-presentation-20260928.json' with { type: 'json' };
import { square } from './presentation.ts';
import type { CardView, Color, PublicIntent, Square } from './presentation.ts';

export type SelectionChoice = { value: string; label: string };
export type SelectionSlot = {
  key: string;
  label: string;
  kind: 'square' | 'squares' | 'choice';
  choices?: SelectionChoice[];
};
export type CardSelectionForm = {
  label: string;
  slots: SelectionSlot[];
  minSelections?: number;
  maxSelections?: number;
};
export type CardSelectionContext = { cards?: CardView[] };
export type PublicMoveMode = 'normal' | 'shotgun' | 'snipe' | 'log-direction';

/** An input grammar failure; position-dependent legality belongs to WASM. */
export class PublicSelectionError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = 'PublicSelectionError';
    this.code = code;
  }
}

const cards = new Map(catalog.cards.map((card) => [card.id, card]));
const sourceDefinitions = new Map(definitions.definitions.map((card) => [card.id, card]));
const names = new Map(presentation.definitions.map((card) => [card.id, card.name]));
const ruleChoices = catalog.cards.filter((card) => card.draftCategory === 'RULE')
  .map((card) => ({ value: card.id, label: names.get(card.id) ?? card.id }));

const at = (key: string, label: string): SelectionSlot => ({ key, label, kind: 'square' });

/**
 * Frozen v7 user-input grammar, not an environment action enumeration.
 * Compound fields follow v7_card_piece::actions, v7_card_board::actions and
 * v7_card_status::actions. Ordered selections follow card_effects::staged_cursor.
 * Choices contain publicly named inputs; an offered choice can still be
 * rejected by the engine because of the current position or another rule.
 */
export function getCardSelectionForm(cardId: string, context: CardSelectionContext = {}): CardSelectionForm {
  const card = cards.get(cardId);
  const definition = sourceDefinitions.get(cardId);
  // The source definitions include this hidden card, while the public draft
  // catalog excludes it. v7_card_piece::actions emits its untargeted intent.
  if (cardId === 'shotgun-king' && definition?.effect === 'shotgunKing') {
    return { label: `${names.get(cardId) ?? cardId} 선택`, slots: [] };
  }
  if (!card || !definition) {
    throw new PublicSelectionError('unknown-card-grammar', `동결 v7 카탈로그에 카드 ${cardId}의 입력 문법이 없습니다.`);
  }
  const label = `${names.get(cardId) ?? cardId} 선택`;
  switch (cardId) {
    case 'amazon': return { label, slots: [at('target', '변환할 퀸'), at('knight', '희생할 나이트')] };
    case 'grappler': return { label, slots: [at('target', '변환할 퀸'), at('minor', '희생할 소형 기물')] };
    case 'hook': return { label, slots: [at('target', '변환할 퀸'), at('rook', '희생할 룩')] };
    case 'windmill': return { label, slots: [at('bishop', '결합할 비숍'), at('target', '결합할 룩')] };
    case 'feudal-contract': return { label, slots: [at('pawn', '계약할 폰'), at('target', '보호할 기물')] };
    case 'sacrifice': return { label, slots: [at('sacrifice', '희생할 기물'), at('target', '보호할 기물')] };
    case 'barricade': return { label, slots: [
      { key: 'direction', label: '바리케이드 방향', kind: 'choice', choices: [
        { value: 'horizontal', label: '가로' }, { value: 'vertical', label: '세로' },
      ] },
      at('target', '바리케이드 중앙'),
    ] };
    case 'joker': return { label, slots: [{
      key: 'cardInstanceId', label: '재사용할 공개 카드', kind: 'choice',
      choices: (context.cards ?? []).map((owned) => ({
        value: owned.instanceId, label: `${owned.name} (${owned.instanceId})`,
      })),
    }] };
    case 'rule-ticket': return { label, slots: [{
      key: 'ruleId', label: '추가할 규칙 · 가능 여부는 엔진 확인', kind: 'choice', choices: ruleChoices,
    }] };
    case 'premove': return { label, slots: [{
      key: 'selections', label: '출발 → 도착 순으로 최대 세 이동 지정', kind: 'squares',
    }], minSelections: 2, maxSelections: 6 };
  }
  if (card.selection.maxSquares > 1) {
    return { label, slots: [{
      key: 'selections', label: orderedSelectionLabel(cardId), kind: 'squares',
    }], minSelections: card.selection.minSquares,
    // Frozen metadata records the AI's eight-pawn ceiling. The native source
    // UI does not impose that ceiling (card_effects::staged_cursor).
    maxSelections: cardId === 'pawn-storm' ? 64 : card.selection.maxSquares };
  }
  if ('target' in definition && definition.target) {
    return { label, slots: [at('target', '공개 보드에서 대상 선택')] };
  }
  if (card.selection.kind === 'none') return { label, slots: [] };
  throw new PublicSelectionError('unsupported-card-grammar', `${cardId}의 ${card.selection.kind} 입력 문법이 정의되지 않았습니다.`);
}

function orderedSelectionLabel(cardId: string): string {
  switch (cardId) {
    case 'brainwash': return '희생할 내 기물 → 세뇌할 상대 기물';
    case 'taboo': return '내 퀸 → 금기 대상 칸';
    case 'portal-gun': return '첫 번째 포털 → 두 번째 포털';
    case 'hypocrisy': return '칸 네 개를 순서대로 지정';
    default: return '대상을 순서대로 지정 · 중복 칸 입력 금지';
  }
}

function coordinate(value: unknown, field: string): Square {
  const result = square(value);
  if (!result) throw new PublicSelectionError('invalid-square-input', `${field}에 8×8 보드 좌표가 필요합니다.`);
  return result;
}

function stringChoice(value: unknown, field: string): string {
  if (typeof value !== 'string' || value.length === 0 || value.length > 256) {
    throw new PublicSelectionError('invalid-choice-input', `${field}에 비어 있지 않은 공개 선택 식별자가 필요합니다.`);
  }
  return value;
}

function validateColor(color: Color): void {
  if (color !== 'white' && color !== 'black') {
    throw new PublicSelectionError('invalid-actor-input', '행동 색은 white 또는 black이어야 합니다.');
  }
}

/** Preserve order and build only the exact user-supplied public intent. */
export function buildCardIntent(card: CardView, color: Color, values: Record<string, unknown>): PublicIntent {
  validateColor(color);
  const form = getCardSelectionForm(card.id);
  const keys = new Set(form.slots.map((slot) => slot.key));
  if (Object.keys(values).some((key) => !keys.has(key))) {
    throw new PublicSelectionError('unknown-selection-field', `${card.id}의 입력에 문법에 없는 선택 필드가 있습니다.`);
  }
  const intent: PublicIntent = {
    type: 'card', color, cardId: card.id,
    cardInstanceId: stringChoice(card.instanceId, 'cardInstanceId'),
  };
  if (form.slots.length === 0) return intent;
  if (card.id === 'joker') {
    intent.target = { cardInstanceId: stringChoice(values.cardInstanceId, '재사용 카드') };
  } else if (card.id === 'rule-ticket') {
    const ruleId = stringChoice(values.ruleId, '규칙');
    if (!ruleChoices.some((choice) => choice.value === ruleId)) {
      throw new PublicSelectionError('unknown-rule-input', `${ruleId}는 동결 공개 RULE 카탈로그의 규칙이 아닙니다.`);
    }
    intent.target = { ruleId };
  } else if (form.slots.some((slot) => slot.kind === 'squares')) {
    if (!Array.isArray(values.selections)) {
      throw new PublicSelectionError('missing-ordered-selections', `${card.id}에 순서가 있는 선택 배열이 필요합니다.`);
    }
    const selected = values.selections.map((value, index) => coordinate(value, `선택 ${index + 1}`));
    if (selected.length < (form.minSelections ?? 1) || selected.length > (form.maxSelections ?? 64)) {
      throw new PublicSelectionError('selection-count-out-of-range', `${card.id}는 ${form.minSelections}~${form.maxSelections}개 좌표 입력이 필요합니다.`);
    }
    if (card.id === 'premove') {
      if (selected.length % 2 !== 0) {
        throw new PublicSelectionError('incomplete-move-pair', '미리 이동은 출발·도착 좌표가 한 쌍이어야 합니다.');
      }
      const moves = [];
      for (let index = 0; index < selected.length; index += 2) {
        moves.push({ from: selected[index], to: selected[index + 1] });
      }
      intent.target = { selections: moves };
    } else {
      if (new Set(selected.map((value) => `${value.row},${value.col}`)).size !== selected.length) {
        throw new PublicSelectionError('duplicate-square-selection', '같은 칸을 중복 지정할 수 없습니다. 선택을 해제한 뒤 다시 지정하세요.');
      }
      intent.target = { selections: selected };
    }
  } else {
    const target: Record<string, unknown> = { ...coordinate(values.target, '대상') };
    for (const slot of form.slots) {
      if (slot.key === 'target') continue;
      if (slot.kind === 'square') target[slot.key] = coordinate(values[slot.key], slot.label);
      else {
        const choice = stringChoice(values[slot.key], slot.label);
        if (!slot.choices?.some((option) => option.value === choice)) {
          throw new PublicSelectionError('unknown-choice-input', `${slot.label}의 선택값 ${choice}가 입력 문법에 없습니다.`);
        }
        target[slot.key] = choice;
      }
    }
    intent.target = target;
  }
  return intent;
}

/** Modes name UI input semantics; they do not claim current availability. */
export function getPublicMoveModes(): { value: PublicMoveMode; label: string }[] {
  return [
    { value: 'normal', label: '일반 이동' },
    { value: 'shotgun', label: '샷건 사격' },
    { value: 'snipe', label: '샷건 저격' },
    { value: 'log-direction', label: '통나무 방향' },
  ];
}

export function buildMoveIntent(color: Color, from: Square, destination: Square, mode: PublicMoveMode = 'normal'): PublicIntent {
  validateColor(color);
  if (!getPublicMoveModes().some((option) => option.value === mode)) {
    throw new PublicSelectionError('unknown-move-mode', `공개 이동 입력 모드 ${String(mode)}는 지원하지 않습니다.`);
  }
  const intent: PublicIntent = {
    type: 'move', color, from: coordinate(from, '출발'), destination: coordinate(destination, '도착'),
  };
  if (mode !== 'normal') intent.selectionMode = mode;
  return intent;
}

export type PieceActionInput = 'promotion' | 'shotgunReload' | 'fileSurgeSkip' | 'wizardSpell';
export const wizardSpellChoices = [
  { value: 'lightning', label: '번개' }, { value: 'shield', label: '보호' },
  { value: 'meteor', label: '메테오' }, { value: 'timeStop', label: '시간 정지' },
];

/** Manual special-action input, with no TS eligibility or cost calculation. */
export function buildPieceIntent(type: PieceActionInput, color: Color, from: Square, values: Record<string, unknown> = {}): PublicIntent {
  validateColor(color);
  if (!['promotion', 'shotgunReload', 'fileSurgeSkip', 'wizardSpell'].includes(type)) {
    throw new PublicSelectionError('unknown-piece-action', `특수 입력 ${String(type)}는 지원하지 않습니다.`);
  }
  const intent: PublicIntent = { type, color, from: coordinate(from, '기물') };
  if (type === 'wizardSpell') {
    const spellId = stringChoice(values.spellId, '주문');
    if (!wizardSpellChoices.some((choice) => choice.value === spellId)) {
      throw new PublicSelectionError('unknown-spell-input', `공개 주문 ${spellId}는 지원하지 않습니다.`);
    }
    if (Object.keys(values).some((key) => key !== 'spellId' && key !== 'target')) {
      throw new PublicSelectionError('unknown-selection-field', '주문 입력에 알 수 없는 필드가 있습니다.');
    }
    intent.spellId = spellId;
    intent.target = coordinate(values.target, '주문 대상');
  } else if (Object.keys(values).length > 0) {
    throw new PublicSelectionError('unknown-selection-field', `${type}는 추가 선택 필드를 받지 않습니다.`);
  }
  return intent;
}
