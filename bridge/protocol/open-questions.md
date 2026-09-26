# 미결정 사항과 확신 없는 필드 (DRAFT bridge-draft-0)

> 이 문서는 초안이 **결정하지 않은 것**을 모아 둔 목록입니다. 번호 `B-Q*`는 이 초안 안에서만 쓰는 임시 번호이며, 팀이 다룰 가치가 있다고 보면 `docs/DECISIONS.md`의 열린 질문(O-XXX)으로 옮기면 됩니다(이 PR은 DECISIONS.md를 수정하지 않았습니다).

## A. 이미 열려 있는 결정과의 관계

| 항목 | 이 초안의 처리 |
|---|---|
| **O-001** encoding을 Rust와 Python 중 어디서 하나 | 결정하지 않음. 기본 메시지는 원문 상태만 다룹니다. 확장 지점으로 요청의 선택 필드 `encode`, 응답의 선택 필드 `encoded`(`schemas/encoded.schema.json`)만 열어 두었습니다. Python이 인코딩하기로 하면 이 두 필드는 쓰지 않으면 됩니다. sparse(인덱스+값) 모양은 `docs/ENCODING-EVIDENCE.md`(별도 PR)의 제안을 예시로 따랐을 뿐입니다. |
| **D-003** 카드 정의 1회 + 매 턴 상태, JSON | 그대로 따름: `new_game`에서 `cards` 1회, 이후 요청에 `GameState`. |
| **D-001** 책임 분리 | 영향 없음. bridge에는 규칙 로직을 넣지 않았습니다. |

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
| B-Q8 | 전송 수단(FFI, 표준입출력, 소켓 등)과 직렬화 최적화 | 정하지 않음 | ARCHITECTURE.md: Phase 1 결정 사항 |
| B-Q9 | 확률 요소가 있는 행동(예: brutus의 룩 선택, randomRoulette)의 난수/시드 처리 | `new_game.seed` 선택 필드만 둠 | fixture는 난수 결과가 갈리는 경우를 별도로 다룸(`nondeterministic`)하며 이 초안은 다루지 않음 |
| B-Q10 | 프로토콜 버전 표기 | 문자열 `"bridge-draft-0"` | 확정 시 변경 |
| B-Q11 | 정책 출력(행동 인덱스) 연결 | 다루지 않음 | Phase 6 |

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

- Rust나 JS 어느 쪽도 이 프로토콜을 실제로 구현해 보지 않았습니다. 스키마와 예시가 서로 맞는지만 확인했습니다(`bridge/tools/validate.js`).
- fixture의 상태 349개와 합법 행동 목록 전체가 스키마를 통과하는 것은 확인했지만(위 `state-and-actions.md`), 이것은 "형식이 맞다"이지 "이 형식이 엔진에 충분하다"는 뜻이 아닙니다.
- 응답 예시의 `state`는 fixture의 `stateDelta`를 적용해 만든 것이며, 적용 코드는 이 초안에서 새로 짠 작은 함수입니다. 위치 하나(`apply_action.*`)의 결과 상태가 fixture의 `signature`와 같은지까지는 대조하지 않았습니다.
- 사이트가 업데이트되면 fixture와 예시가 낡을 수 있습니다(`meta.json`의 번들 해시 참고).
