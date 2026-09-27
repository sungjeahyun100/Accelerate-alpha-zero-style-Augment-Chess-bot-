# 실행 계약 v1

현재 실행·저장·차분 비교의 기준은 [runtime-v1.schema.json](../schemas/runtime-v1.schema.json)과
[runtime-contract.js](../tools/runtime-contract.js)다. Position·Action envelope는 v1을 유지하고,
공개 관측은 아래 Observation v2 계약을 사용한다.
기존 bridge-draft-0 문서와 예시는 과거 검토 자료로 보존한다. v1 채택은 전체 규칙 구현의
완료를 뜻하지 않으며, 실제 검사와 남은 범위는 [IMPLEMENTATION](../../docs/IMPLEMENTATION.md)에 있다.

## 위치와 행동

`Position`은 `{protocolVersion, rulesVersion, catalogVersion, state, rng, history, positionId}`의
불변 snapshot이다. `state`는 실제 사이트의 전체 실행 상태와 unknown 내부 규칙 필드를
손실 없이 보존한다. 8x8 보드·행동자·phase와 JSON 경계는 검사한다. JSON은 저장·교환·검증
경로이고 PyO3의 반복 호출은 소유한 native 객체를 사용한다.

`rng`는 `{algorithm:'lcg32-v1', state:uint32, cursor:safeInteger, tape:[0≤x<1]}`다.
매 draw에서 `state=(1664525*state+1013904223) mod 2^32`, `cursor++`를 수행한다.
tape에 현재 cursor의 값이 있으면 출력만 대체하며 LCG 상태는 그대로 한 번 전진한다.
cursor overflow는 거부한다. snapshot 복원은 상태·tape·cursor를 함께 복원한다.

`positionId`는 나머지 Position 필드의 RFC 8785/JCS SHA-256이다. full state·RNG가 들어 있는
private identity이므로 신경망 특징·공개 이력·informationStateKey에 넣지 않는다.
동결 source의 효과는 같은 대형 기물 ID를 떨어진 여러 square에 남길 수 있다. snapshot
admission은 같은 frame의 ID/type/color/전체 속성 일관성을 검사하고 이 source 상태를 보존한다.
배치 모양을 2x2로 제한하는 추가 가정으로 source snapshot을 거부하지 않는다. 새 기물의
정상 배치·이동 legality는 규칙 helper가 검사한다. 서로 다른 history/replay frame의 같은 ID는
별도 객체이며, JS oracle 복원도 frame 간 객체를 공유하지 않는다.
`Action`은 `{protocolVersion:'accelerate-action-v1', positionId, actionId, payload}`다.
`actionId`는 **payload만** JCS SHA-256으로 계산한다. 다른 위치에서 동일 payload는 동일
actionId를 가지며, 실행 시 positionId가 맞지 않으면 stale action으로 거부한다.

payload는 사이트 행동의 기물/카드 instance identity, 특수 flags, 선택 순서를 보존한다.
현재 10종은 move, card, promotion, promotionChoice, shotgunReload, wizardSpell,
fileSurgeSkip, draftPick, draftBundlePick, trolleyChoice다. 좌표는 row/col 0..7이다.
RULE 선택은 card.target.ruleId, Joker 재사용은 card.target.cardInstanceId, Barricade 방향은
card.target.direction에 들어간다. 실제 `chooseRuleTicket`, `chooseJokerCard`,
`chooseBarricadeDirection`은 활성화 전에 정보를 선택하며 그 사이 RNG·행동자 변화가 없어
하나의 최종 card 행동으로 정규화한다. `draftDelete`는 draft 생략 설정이며 행동 종류가 아니다.

공개 정보 탐색의 선택 형식은 `public-decision-intent-v1`이다. `Action.public_intent()`가
source UI에서 선택할 수 있는 정보만 투영하며, 현재 일반 이동은
`{type:'move', color, from:{row,col}, destination:{row,col}}`을 사용한다. 카드 instance와
공개 선택·ordered target은 선택 의미에 맞춰 보존한다. 실제 환경의
`Position.bind_public_intent(intent)`가 화면의 선택 순서대로 실행 payload를 해결한다.
그때 결정되는 숨은 기물·capture flag·private position identity는 신경망·정보집합 키에 넣지 않는다.
아직 미지원인 특수 선택을 Python에서 임의 flag 제거로 대신하지 않는다.
trolley 공개 intent는 `{type:'trolleyChoice', color, doomedIndex}`다. 원문의 `windowId`는
시간·난수로 만든 실행/만료 확인용 식별자이므로 공개 관측과 intent에는 넣지 않는다.
실제 환경은 현재 window에 intent를 bind하고 lossless 실행 Action에 해당 `windowId`를 보존한다.

ONNX encoder 계약은 `exact-payload`와 `public-decision-intent-v1`의 action 정책을 각각
기록하고 별도 hash를 갖는다. 봇 탐색은 공개 intent 계약을 사용하며 실행 Action의 lossless
직렬화는 위 v1 envelope로 유지한다. 원본 public trace/replay를 보존하면서 신경망 history만
`full` 또는 `public-history-summary-v1`로 명시할 수 있다. 서로 다른 정책의 artifact를
자동으로 호환 처리하지 않는다.

JCS 입력은 유한 수, safe integer, 유효 Unicode, dense array, 일반 JSON object로 제한한다.
최대 depth 64, node 100000, 경계 byte budget 8 MiB를 적용한다. unknown wrapper 필드,
NaN/Infinity, lone surrogate, sparse array, cursor overflow와 malformed history는 명시적으로
거부한다. canonical key 순서는 UTF-16이고 숫자는 ECMAScript JSON 표기를 따른다.
Python/Rust 저장 identity 역시 JCS로 맞춘다.

## 관측과 힌트

Observation은 `{protocolVersion:'accelerate-observation-v2', viewer, board, turn, ownCards,
opponentHandCount, publicState, history, informationStateKey}`의 단일 형식이다.
`informationStateKey`는 나머지 공개/관찰 가능한 필드만 JCS SHA-256으로 계산한다.
동일한 viewer 관측·공개 history를 가진 서로 다른 hidden state/RNG는 같은 key를 가져야 한다.
환경의 full Position은 탐색 환경 실행 경계의 내부 자료다. encoder와 belief 탐색이 그것을
신경망 입력·정보집합 키·정답 hidden state로 사용하는 것은 계약 위반이다.

관측의 공개 여부는 실제 화면을 따른다. 선택한 양측 library 카드는 공개이므로
`revealedOpponentCards`에 현재 상대 카드 identity·효과·별·used와 공개 slot을 보존한다.
빈 slot을 제거하더라도 원래 library index는 `slot`으로 유지한다. 선택 전 상대 draft offer,
미래 RNG, 숨은 기물과 상대 비밀 계획은 제외한다. owner가 이미 아는 premove 계획은
`ownPlans`의 순서 있는 from/to로 보존하되 RNG planId/pieceId를 공개하지 않는다.

board는 실제 `pieceVisibleToColorAt`으로 hiddenFrom·camouflage·fog를 가리고
`visiblePieceType`으로 hallucination·시각 변형을 투영한다. 타입/색과 검토된 공개 기물 필드만
허용한다. `get_public_hints`와 publicState.legalHints는 `{moves:[{from,destinations}],
cardTargets:[{cardInstanceId,targets}]}`의 좌표만 제공한다. 실제 getLegalMoves→moveHighlightKeys,
getVisibleCardTargetSquares의 UI 정보를 따르며 worker 후보나 capture flags를 공개하지 않는다.
UI가 보여 주는 legal hint는 그 자체가 관찰 가능한 정보다.

captured piece는 사이트 renderCaptureList가 viewer/hiddenFrom 필터 없이 양측에 보여 주는
마지막 12개의 type/color/시각 변형만 제공한다. collapsedCells, 공개 card owner flags, 현재
promotion/trolley 선택 phase와 clock의 공개 잔여값도 보존한다. raw publicState 복사는 금지한다.
필드별 allowlist와 viewer projection/실제 private/presentation/아직 검토 중인 분류는
`bridge/catalog/observation-20260927.json`에 있다. 새 unknown 필드는 검토 없이 무시하지 않는다.
visibilityReviewPending은 비공개라는 결론이 아니며 전체 관측 coverage의 미완료 범위다.

관측 v2는 동결 source renderer가 보여 주는 기물 상태, 보드 표시와 관계를 별도로 투영한다.
기물의 `status`와 publicState의 `boardMarks`·`relationships`·`overlays`는 정책에 있는 strict schema를
따른다. 효과의 내부 ID나 raw deadline을 복사하는 대신 실제 화면의 flag·남은 횟수·표시 좌표를
제공한다. 소유자와 시간 정보가 화면에 나타나는 경우에는 그 공개 의미를 유지한다.
정책은 schemaVersion 2·projectionVersion `source-visible-20260927-v2`이며, 규칙·catalog의
동결 source 버전은 바꾸지 않는다. 정확한 필드별 구현 범위와 남은 검증은 IMPLEMENTATION에 기록한다.

`EncoderSpec`은 `observation_policy_hash`를 포함한 16개 필드를 직렬화한다. Python 생성자는
catalog와 관측 정책을 명시적으로 받고, encoder 계약·ONNX metadata·replay에는 그 정책도
포함한다. 정책 hash는 JCS bytes의 SHA-256이며 native runtime은 배포 metadata의 정책을
compile-time 정책과 비교한다. encoder/condition 버전은 `public-utf8-v2`·`public-film-v2`다.
이전 관측 v1이나 정책이 누락된 artifact를 자동으로 보완하지 않는다. 설치 패키지의
`site_catalog()`·`site_observation_policy()`는 서로 같은 wheel에 포함된 원본의 owned copy를 제공한다.

## 전이·이력·결과

Position.history는 임의 object 배열이 아니라 다음 game event 배열이다.

```text
{ protocolVersion: 'accelerate-game-event-v1', actor,
  action: <exact semantic payload>, turnChanged: boolean,
  public: { white: <PublicTransition>, black: <PublicTransition> } }
```

PublicTransition은 `{kind:'transition', actor, nextActor, phase, boardChanges, ownCards,
revealedOpponentCards, captures, result}`다. boardChanges는 viewer의 전후 board 투영이
다른 square와 공개 before/after만 기록한다. hidden→hidden 이동의 원본 좌표·flags는 여기에
들어가지 않는다. Observation.history는 `event.public[viewer]`만 선택하며 내부 action,
actionId, positionId, RNG와 control identity를 복사하지 않는다. history 전체를 event에 재중첩하지
않는다. Python belief tracker의 관측 frame 이력은 이 환경 event history와 별도 책임이다.

`actor`/`nextActor`는 draft·promotion·trolley의 실제 decision actor를 따르며 단순 physical
`state.turn`으로 대체하지 않는다. `turnChanged`는 physical turn 변화다. value backup·정책 선택은
실제 actor 전환을 사용해야 한다.

StepResult는 `{protocolVersion:'accelerate-step-v1', ok, position, result, event? , error?}`다.
거절 시 원래 immutable Position과 RNG/history가 유지되고 ACTION_REJECTED를 반환한다.
unsupported 기능·한도 초과·source 실행 오류는 실패로 드러내며 legal action으로 위장하지 않는다.
GameResult는 `{protocolVersion:'accelerate-result-v1', status:'ongoing'|'terminal'|'unfinished',
winner:null|'white'|'black', outcome:null|'white'|'black'|'draw', reason:string}`이다.
bounded rollout 종료는 unfinished이며 실제 gameover와 구분한다. terminal draw는 winner=null이다.
reason은 사이트 결과 이유이며 1024자 한도를 검사한다.

## 공개 trace의 조건부 제안 확률

belief 재구성은 실제 환경의 private Position·RNG를 받지 않는다. 공개 frame 이력과
독립 seed로 source-valid particle을 생성하고, 공개 전이와 일치하는 조건부 제안의 밀도를
보정한다. 현재 chance prior는 독립적인 source 추첨이다. 유한 LCG seed의 posterior나
브라우저의 미래 RNG와 정확히 같은 분포라는 주장은 이 계약에 포함하지 않는다.

native의 `condition_hidden_opening_draft(expected_next_public, independent_seed)`는
`{position, importance_weight, source_probability, proposal_probability}`를 반환한다.
`apply_weighted_conditioned_public(action, expected_observation, independent_seed)`는
`{step, importance_weight, source_probability, proposal_probability}`를 반환한다.
두 반환 mapping에는 이 네 key만 존재한다. Position과 StepResult는 별도로 소유하며,
확률은 유한한 (0, 1] 값이고 weight는 양수인 유한 `source_probability / proposal_probability`다.
binding은 상대 허용 오차 1e-10으로 그 비율을 검사한다. seed는 독립적인 `u32`이고
기존 action의 stale guard와 입력 변환 한도를 그대로 적용한다.

Python filter는 hidden-offer 제안과 관찰된 후속 draw의 weight를 함께 반영한다.
조건화 전 공개 frame 전체를 보존하고, 조건화 후 공개 frame 전체를 관찰값과 비교한다.
숨은 상대 intent를 열거할 때 source-proven 공개 불일치 후보를 먼저 제외해도 원래 intent
개수의 prior 분모는 유지한다. 확률 metadata나 실행 전용 ID를 신경망 특징으로 복사하지 않는다.
후속 weighted Python filter 캡처는 `source-importance-filter-v2`·`public-particle-summary-v3`·
`availability-puct-v3`로 이를 구분한다. 이 캡처의 기본 세 모드 검증은 아직 미완료이며,
native API를 공유하는 중간 checkpoint에서는 기존 원격 Python 탐색을 유지한다.
관측 v2, Position/Action/공개 trace v1의 직렬화 형식은 유지한다.

`apply_conditioned_public`의 StepResult만으로는 제안의 likelihood를 알 수 없다.
weighted filter가 이 호출이나 raw child를 확률 1의 제안으로 대체하지 않는다.
입증하지 못한 chance family는 명시적인 Unsupported 오류로 남긴다. deterministic trace의
p=q=1, 실제 source 추첨에서 p=q<1인 무보정 trace, 관찰값에 조건화해 p와 q가 달라지는
trace를 구분한다. 구현·관측한 범위와 남은 default-mode 흐름은 IMPLEMENTATION에 기록한다.

## 비교와 지원 범위

규칙 정답은 2026-09-27 최초 동결 client 본체다. 카드 catalog 256개, CARD_DEFS 257개 중
추가 shotgun-king은 별도 모드용 보조 정의다. 84개 piece identifier는 83개 asset/value identifier와 실제 neutral wall의 superset이며
84종이 모두 기본 8x8 생성 대상이라는 뜻이 아니다. 역할/미분류를 catalog에 명시한다.

비교는 초기화·각 draft·phase·legal 후보·거절·normalized full next state/result/RNG/history를
다룬다. 카드 name/text/art는 규칙이 아닌 presentation 정의로 별도 정규화할 수 있으며,
id/instanceId/flags/효과 owner/행동 선택 순서와 RNG는 제거하지 않는다. 추가 정규화 예외는
근거와 정확한 field list를 남긴다. private positionId 단독 비교나 signature만으로 성공을
판정하지 않는다. 과거 worker fixture는 AI filter와 catalog drift가 있어 최신 client 전체 정답을
대신하지 못한다.

완전 열거가 한도를 넘는 premove 등에는 정확한 lazy iterator와 progressive widening 연결이
필요하다. 현재 materialized API는 한도를 넘으면 명시적으로 실패한다. 시간/개수에 맞추기 위해
조용히 truncate하지 않는다. 아직 이 경계와 전체 catalog semantic parity가 완료된 상태는 아니다.
