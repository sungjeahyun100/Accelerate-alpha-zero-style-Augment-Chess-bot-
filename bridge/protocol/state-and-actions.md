# GameState, Action, 카드 정의 (DRAFT bridge-draft-0)

> 초안입니다. 확정 아님. 스키마: `../schemas/game-state.schema.json`, `action.schema.json`, `card-definition.schema.json`, `common.schema.json`. "확신 정도" 열은 fixture(PR #19)에서 직접 확인했는지를 뜻합니다.

## 1. 왜 "핵심 + extra" 구조인가

fixture 349개의 상태를 모으면 사이트 worker 상태의 최상위 키가 **156개**입니다(README에는 "약 156개"). 이 중 보드, 카드 슬롯, 턴 같은 것은 명확하지만, 나머지는 규칙 플래그와 예약 효과(`pendingPortals`, `crownRule`, `freeCastling` 등 카드별 장부)입니다. 어느 것이 Rust 엔진에 꼭 필요한지 이 시점에는 판단할 수 없어서 다음과 같이 나눴습니다.

- **핵심(이름 있는 필드)**: `turn`, `mode`, `winner`, `actionsRemaining`, `moveCount`, `fullMove`, `turnsTaken`, `cardsUsedThisTurn`, `board`, `deckSlots`, `captures`, `enPassant`.
- **`extra`**: 나머지 사이트 키를 **이름 그대로, 내용 그대로** 담는 자유 객체. bridge는 이 안을 해석하지 않습니다.

이렇게 하면 정보가 사라지지 않고 되돌릴 수 있습니다: 사이트 상태 = `extra`의 키들 + 핵심 필드(단 `winner`는 `null` <-> `""`). fixture 349개 전부와 그 합법 행동 목록 전부(12,462개)를 이 변환으로 만들어 스키마 검사를 통과하는 것까지 확인했습니다(임시 검사, 저장소에는 포함하지 않음).

fixture만으로 알 수 있었던 참고 수치(349개 위치 기준, 지도용일 뿐 "필요 여부"의 증거는 아님):

- 44개 키는 fixture마다 값이 달랐고, 112개 키는 모든 fixture에서 같은 값이었습니다(fixture가 손으로 만든 보드와 무작위 대국이라 실제 대국의 분포가 아닙니다).
- 행동을 적용했을 때 상태 차이(`stateDelta`)에 등장한 최상위 키는 `board`, `turn`, `deckSlots`, `moveCount`, `turnsTaken`, `cardsUsedThisTurn`, `fullMove`, `captures`, `enPassant`, `mode`, `winner`처럼 핵심 필드 외에 `recentMoves`, `parrotMovement`, `sirenExposure`, `freeCastling`, `switcheroo`, `substitution`, `bishopSnipe`, `breakthroughPawns`, `enPassantFrenzy`, `freeMoveCaptureLock`, `zugzwang`, `magicGirlSurge`, `aiSearchBonus` 등 핵심 필드가 아닌 키만 78개입니다(적용 결과를 기록한 758개 행동 기준). 즉 `extra` 안의 값도 매 수마다 바뀌므로, 엔진은 `extra`를 받아서 갱신한 상태를 그대로 돌려줘야 합니다.
- 그 중 `aiSearchBonus`는 이름상 탐색(AI)용 값으로 보이는데 규칙 값인지 확인하지 않았습니다.

## 2. GameState

| 필드 | 형식 | 필수 | 확신 정도 |
|---|---|---|---|
| `turn` | `"white"` / `"black"` | O | 확인함 |
| `mode` | 문자열. 관측: `"play"`, `"gameover"` | O | 이 둘만 확인. 다른 모드(드래프트 등)는 fixture에 없어 enum으로 못 박지 않음 |
| `winner` | `null` / `"white"` / `"black"` / `"draw"` | O | `null`(사이트에선 `""`), white, black 확인. `"draw"`는 GAME-RULES.md 근거로만 넣음 |
| `actionsRemaining` | 정수 >= 0 | O | 모든 fixture에서 1이라 실제 의미(연속 행동 등)는 미검증 |
| `moveCount`, `fullMove` | 정수 | 선택 | 값은 확인, 정확한 정의(반수인지, 누가 올리는지)는 미확인 |
| `turnsTaken`, `cardsUsedThisTurn` | `{white, black}` 정수 | 선택 | 값 확인. 규칙 표(GAME-RULES 4절)가 이들을 별도로 보존하라고 함 |
| `board` | 8x8, `board[row][col]`, 칸은 `null` 또는 Piece | O | 8x8만 확인 |
| `deckSlots` | `{white: [CardSlot], black: [CardSlot]}` | O | 확인 |
| `captures` | `{white: [Piece], black: [Piece]}` | 선택 | 배열 내용은 확인, 키가 잡은 쪽인지 잡힌 쪽인지는 확인 안 함 |
| `enPassant` | `null` 또는 `{row, col, capturedRow, capturedCol, color}` | 선택 | 확인 |
| `extra` | 자유 객체 | 선택 | 위 1절 참고 |

### Piece

필수: `type`(문자열), `color`, `moved`(불리언). 이 세 개는 fixture의 모든 기물(3,857개)에 있었습니다. 기물 종류는 fixture에 50종이 나왔지만 열거하지 않았습니다(사이트 업데이트로 늘어날 수 있음).

선택으로 알려진 값(fixture에서 나온 것 일부): `id`, `shielded`, `frozen`, `hp`, `maxHp`, `mana`, `maxMana`, `capturesMade`, `totalCaptures`, `freshNoCaptureUntil`. 그 외 `windmillMode`, `thiefVisited`, `thiefLastDirection`, `anchorRow`, `anchorCol`, `quantum`, `callingCard` 등 훨씬 많은 플래그가 있고 일부는 객체나 배열 값입니다. **스키마는 알 수 없는 키를 허용하고(`additionalProperties: true`) 엔진은 이를 그대로 보존해야 합니다.** 전체 목록은 Twist의 `compactBoard`(불리언 41개 + 숫자 23개 + enum 6개 등)와 사이트 코드에 있으나 아직 확정하지 않았습니다.

### CardSlot (`deckSlots.white[]`, `deckSlots.black[]`)

| 필드 | 형식 | 확신 정도 |
|---|---|---|
| `id`, `effect` | 문자열 (fixture에서 항상 같은 값) | 확인 |
| `instanceId` | 문자열, 카드 한 장마다 고유 (예: `white-hallucination-2`) | 확인 |
| `stars` | 정수 | 슬롯마다 1~5. 같은 카드 id가 서로 다른 별 수로 나옴. fixture 생성기가 무작위로 붙였을 가능성이 있음(확인 안 함) |
| `used` | 불리언 | 확인 |
| `recovering` | 불리언 | 카드 1종(submerge)에서 없었음. 선택 |

## 3. Action

`type`으로 구분하는 합집합(union)입니다.

| type | 필드 | fixture에서 확인? |
|---|---|---|
| `move` | `color`, `from:{row,col}`, `move:{row,col, ...}` | 확인 (11,259개 목록 항목) |
| `card` | `color`, `cardId`, `cardInstanceId`, `target`(없거나 `null`이거나 객체) | 확인 (1,203개) |
| `promotion` | `color`, `from` | fixture에 없음. GAME-RULES.md와 `engine-merged.js`에서 옮김 |
| `shotgunReload` | `color`, `from` | 위와 같음 |
| `wizardSpell` | `color`, `from`, `spellId`, `target` | 위와 같음 |
| `fileSurgeSkip` | `color`, `from` | 위와 같음 |

- `move.move`에는 목적지 `row`, `col` 말고도 이동의 성격을 정하는 플래그가 붙습니다(관측: `bent`, `tricksterMove`, `sirenMove`, `dragonSwap`, `standardPawnDoubleStep`, `bigRookMove`, `colossusMove`, `slimeMove`, `jumpCapture`, `checkerCapture`, `highlightCells` 등). 같은 도착 칸이라도 플래그가 다르면 다른 행동이므로, 행동은 **받은 그대로** 돌려 보내야 합니다.
- 카드 `target` 모양 (관측): 없음(`null` 포함, 예: vanguard), `{row,col}`(slime), `{row,col,knight:{row,col}}`(amazon), `{selections:[{row,col}]}`(cleanupPieces), `{ruleId}`(ruleTicket). 이 밖에 카드마다 다른 모양이 있을 수 있어 스키마는 "아무 객체"로 열어 두었습니다. 예시는 `../examples/action.card-targets.json`.
- 행동 식별: 사이트 fixture는 행동에서 `id`, `instanceId`, `pieceId`를 제거한 정규화 문자열(키)을 비교용으로 썼습니다. 이 초안은 정규화 규칙을 정하지 않았습니다(open-questions).
- 정책(policy) 출력 인덱스와 행동을 연결하는 방식(행동 공간)은 Phase 6 결정 사항이라 여기서 다루지 않습니다.

## 4. 카드 정의

`new_game.cards[]`의 원소입니다.

| 필드 | 형식 | 비고 |
|---|---|---|
| `id` | 문자열 | 필수. fixture 239종 |
| `effect` | 문자열 | 필수. fixture에서 항상 `id`와 같음 |
| `stars` | 정수 | 선택. 슬롯의 별 수가 카드마다 고정값이 아닌 것으로 보여 기본값 정도로만 둠 |
| `phase` | `OPENING`/`MIDDLE`/`END`/`PIECE`/`RULE` | 선택. GAME-RULES.md 근거. fixture에는 없음 |
| `name` | 문자열 | 선택. 표시 이름. fixture에는 없음 |

**카드의 실제 정의(효과, 별 비용, phase, 설명)를 어디서 가져와 채울지는 미정입니다.** 이 초안의 예시는 fixture에서 얻은 `id`/`effect`만 채웠습니다.

## 5. 사이트 상태와의 대응

| 사이트 worker 상태 | bridge GameState |
|---|---|
| `turn`, `mode`, `actionsRemaining`, `moveCount`, `fullMove`, `turnsTaken`, `cardsUsedThisTurn`, `board`, `deckSlots`, `captures`, `enPassant` | 같은 이름의 필드 |
| `winner: ""` | `winner: null` |
| `winner: "white"` / `"black"` (또는 `"draw"`) | 같음 |
| 위에 없는 모든 키(144개) | `extra` 안에 이름과 값 그대로 |

변환은 이 표대로 하면 되돌릴 수 있습니다. 예시를 만든 변환 코드는 `../tools/make-examples-from-fixtures.js`의 `toGameState` 함수입니다.
