# 미결정 사항과 확신 없는 필드 (DRAFT bridge-draft-0)

> 이 문서는 초안의 **미결정 사항과 이후 결정 연결**을 모은 목록입니다. 번호 `B-Q*`는
> 이 초안의 임시 번호입니다. D-004의 직접 호출 방식만 반영하며, 다른 필드·상태 방식을
> 확정하거나 구현하지 않습니다. 새 결정의 적용 시점은 `docs/DECISIONS.md`를 따릅니다.

## A. 이미 열려 있는 결정과의 관계

| 항목 | 이 초안의 처리 |
|---|---|
| **O-001 → D-007** Python-first encoding | O-001은 해결됐습니다. 초안의 선택 필드 `encode`/`encoded`와 sparse 예시는 현재 v1 실행 계약이 아닙니다. Python이 공개 Observation을 인코딩합니다. |
| **D-003/D-004** 카드 정의 1회 + 위치별 상태 | JSON 기록·검증의 논리 의미를 유지. 반복 호출은 PyO3 타입·배열, 패키징은 maturin. |
| **D-001** 책임 분리 | 영향 없음. bridge에는 규칙 로직을 넣지 않았습니다. |
| **D-019** 확률적 전이 | decision node의 플레이어 행동과 chance node의 환경 결과를 구분합니다. 엔진이 실제 확률 규칙의 출처이며 AI observation에는 알려지지 않은 미래 정보를 넣지 않습니다. 초안 스키마는 그대로이고 세부 표현은 B-Q9에서 결정합니다. |

## B. 프로토콜 설계에서 열어 둔 질문

| 번호 | 질문 | 초안의 임시 선택 | 비고 |
|---|---|---|---|
| B-Q1 | 엔진이 위치 단위 stateless(요청마다 전체 `GameState`)여도 되나, 아니면 서버가 현재 상태를 기억하나 | stateless | fixture와 모양이 같고 MCTS 되돌리기가 쉬움. 대신 요청이 커짐(상태 하나 약 5~10 KB) |
| B-Q2 | `new_game`에서 시작 상태를 누가 만드나(엔진 기본 시작 상태 vs 호출자가 `initialState`로 제공), 덱 배정과 드래프트는 누가 하나 | `initialState` 선택 필드. 없으면 엔진 기본값 | GAME-RULES.md는 엔진이 드래프트 UI 전체를 재현하지는 않는다고 함 |
| B-Q3 | 카드 정의의 전체 필드(효과 설명, 별 비용, phase)와 출처 | `id`, `effect`만 필수 | fixture에는 이 둘뿐. 사이트/JS oracle의 카드 목록에서 어떻게 뽑을지 미정 |
| B-Q4 | `extra`(사이트 내부 키 144개)를 언제까지 통째로 통과시킬지, Rust가 어떤 키를 모델링할지 | 통째로 통과 | 규칙 카드와 예약 효과가 여기 들어 있어 Phase 3에서 정리 필요 |
| B-Q5 | 행동의 정규화 키(중복 제거와 비교에 쓰는 문자열)를 bridge에서 정할지 | 정하지 않음 | fixture는 `id`/`instanceId`/`pieceId`를 뺀 JSON 문자열을 사용 |
| B-Q6 | `apply_action` 응답에 종료 정보(`terminal`, `winner`)를 별도로 넣을지, `get_result`를 따로 둘지 | 둘 다 가능(상태에 `mode`/`winner`가 있고 `get_result`도 있음) | MCTS에선 호출 횟수를 줄이려면 합치는 편이 유리할 수 있음 |
| B-Q7 | 합법 목록의 정렬과 중복 제거를 엔진이 보장할지 | 보장하지 않음 | fixture는 정렬, 중복 제거본 |
| B-Q8 | 호출 방식과 직렬화 최적화 | D-004: PyO3 직접 타입·배열 호출 + maturin 패키징, JSON 기록·검증 | 방식은 결정. 구체 API·수명/소유권·GIL·batch·zero-copy·JSON 동등성은 Phase 5 구현/검증 사항 |
| B-Q9 | 플레이어 행동 적용과 chance outcome의 열거·확률 또는 규칙에 따른 샘플링·결과 적용·RNG/seed 재현을 어떻게 표현하나 | `new_game.seed` 선택 필드만 둠. `apply_action`은 실현된 결과 하나의 예시 | 괴물 이동·랜덤 카드 드로우 등은 같은 `(state, action)`에서 결과가 갈릴 수 있음. 엔진 규칙을 AI가 추측하지 않도록 JSON/PyO3 의미와 fixture 비교 방식을 Phase 1/3/5에서 결정. 기존 `nondeterministic` fixture 취급도 함께 검토 |
| B-Q10 | 프로토콜 버전 표기 | 문자열 `"bridge-draft-0"` | 확정 시 변경 |
| B-Q11 | 정책 출력(행동 인덱스) 연결 | 다루지 않음 | Phase 6 |
| B-Q12 | authoritative game state에서 플레이어별 AI observation을 어떻게 만들고 비공개 RNG·미래 덱 정보를 어떻게 차단하나 | 미정 | 관측 필드와 불완전정보 탐색 필요성은 Phase 1/6/7에서 별도 검토. `encoded`가 내부 미래 정보를 노출해서는 안 됨 |

## C. 확신이 없는 필드 (모아 보기)

| 필드 | 왜 불확실한가 |
|---|---|
| `actionsRemaining` | 모든 fixture에서 1. 연속 행동, `timeStop`, 무료 이동 등에서 어떻게 변하는지 이 자료로는 확인 못 함 |
| `mode` | `play`, `gameover`만 확인. 드래프트 등 다른 모드는 못 봄 |
| `winner` = `"draw"` | GAME-RULES.md에만 있고 fixture에는 없음 |
| `moveCount`, `fullMove` | 값은 있으나 정의(반수/전체 수, 누가 올리는지) 미확인 |
| `captures` | 어느 편 키인지 미확인 |
| Piece의 선택 플래그 | 일부만 나열, 나머지는 통과. 객체/배열 값(`thiefVisited`, `quantum`, `callingCard`)이 있음 |
| `stars` | 같은 카드가 슬롯마다 다른 값. 실제 카드에 고정된 별 수가 있는지 미확인 |
| `deckSlots[].recovering` | 카드 1종(submerge)에서 없음 |
| 카드 `target`의 모양 | 5가지 관측, 그 밖은 열어 둠 |
| 행동 `promotion`, `shotgunReload`, `wizardSpell`, `fileSurgeSkip` | fixture에 없음. GAME-RULES.md와 `engine-merged.js`의 모양을 옮김 |
| `move.move`의 플래그들 | 목록은 관측한 것뿐이며 전부인지 모름 |
| `aiSearchBonus` 같은 AI 전용으로 보이는 `extra` 키 | 규칙 값인지 확인 안 함 |
| `apply_action` 실패 응답(`action_rejected`) | fixture의 `ok:false` 37건에서 모양을 추정. 실패 시 상태가 바뀌었는지는 확인 안 함 |

## D. 이 초안이 검증하지 않은 것

- Rust나 JS 어느 쪽도 이 초기 JSON 초안을 실행 프로토콜로 구현하지 않았습니다. 스키마와 예시가 서로 맞는지만 확인했습니다(`projects/augment-chess/contracts/tools/validate.js`).
- fixture의 상태 349개와 합법 행동 목록 전체가 스키마를 통과하는 것은 확인했지만(위 `state-and-actions.md`), 이것은 "형식이 맞다"이지 "이 형식이 엔진에 충분하다"는 뜻이 아닙니다.
- 응답 예시의 `state`는 fixture의 `stateDelta`를 적용해 만든 것이며, 적용 코드는 이 초안에서 새로 짠 작은 함수입니다. 위치 하나(`apply_action.*`)의 결과 상태가 fixture의 `signature`와 같은지까지는 대조하지 않았습니다.
- 사이트가 업데이트되면 fixture와 예시가 낡을 수 있습니다(`meta.json`의 번들 해시 참고).
