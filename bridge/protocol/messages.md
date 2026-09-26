# 메시지 (DRAFT bridge-draft-0)

> 초안입니다. 확정 아님. 스키마는 `../schemas/request.schema.json`, `../schemas/response.schema.json`, 전체 예시는 `../examples/`에 있습니다. 아래 JSON은 지면을 위해 `...`로 줄인 부분이 있습니다(실제 예시 파일에는 전체가 들어 있음).

## 공통 규칙

- 모든 메시지: `protocol`(현재 `"bridge-draft-0"`), `type`, `id`(요청이 정하고 응답이 그대로 되돌려 줌).
- 응답 `type`은 요청 `type` + `_response`. 실패는 `type: "error"`, `ok: false`.
- 좌표는 `{row, col}`이고 `board[row][col]`입니다. fixture에서 row 0이 흑의 뒷줄, 백 폰이 row 6에서 시작합니다.
- 색은 `"white"` / `"black"`.

## 1. new_game: 카드 정의를 1회 등록 (D-003)

요청 (예시 파일 `new_game.request.json`):

```json
{
  "protocol": "bridge-draft-0",
  "type": "new_game",
  "id": 1,
  "cards": [ { "id": "acceleration", "effect": "acceleration" }, "... 239개 ..." ],
  "initialState": { "...": "GameState (선택)" },
  "seed": 20260926
}
```

응답:

```json
{
  "protocol": "bridge-draft-0",
  "type": "new_game_response",
  "id": 1,
  "ok": true,
  "gameId": "g-1",
  "cardCount": 239,
  "state": { "...": "시작 GameState" }
}
```

- `cards`의 항목 모양은 [state-and-actions.md](state-and-actions.md#카드-정의)에 있습니다. fixture에서 확실히 얻은 것은 `id`, `effect`뿐이라 예시도 그 두 개만 채웠습니다.
- `initialState`를 생략했을 때 엔진이 기본 시작 상태를 만드는지, 덱 배정과 드래프트를 누가 하는지는 미정입니다(open-questions).

## 2. get_legal_actions: 합법 행동 목록

요청 (`get_legal_actions.request.json`):

```json
{ "protocol": "bridge-draft-0", "type": "get_legal_actions", "id": 2, "gameId": "g-1", "state": { "...": "GameState" } }
```

응답 (`get_legal_actions.response.json`, 실제 fixture `playout-00001`의 24개 중 앞부분):

```json
{
  "protocol": "bridge-draft-0",
  "type": "get_legal_actions_response",
  "id": 2,
  "ok": true,
  "actions": [
    { "type": "card", "color": "white", "cardId": "hallucination", "cardInstanceId": "white-hallucination-2" },
    { "type": "move", "color": "white", "from": { "row": 5, "col": 0 }, "move": { "row": 4, "col": 0, "sirenMove": true } },
    "..."
  ]
}
```

- 종료된 위치에서는 `actions`가 빈 배열입니다(fixture의 종료 위치 17개가 모두 그랬음).
- 목록의 순서와 중복 제거는 정하지 않았습니다. fixture는 정규화한 키로 정렬, 중복 제거한 목록입니다(비교할 때는 그렇게 정렬해서 비교해야 함).

## 3. apply_action: 행동 적용

요청 (`apply_action.move.request.json`):

```json
{
  "protocol": "bridge-draft-0",
  "type": "apply_action",
  "id": 3,
  "gameId": "g-1",
  "state": { "...": "적용 전 GameState" },
  "action": { "type": "move", "color": "white", "from": { "row": 5, "col": 0 }, "move": { "row": 4, "col": 0, "sirenMove": true } }
}
```

응답 (`apply_action.move.response.json`): 적용 후 전체 상태.

```json
{ "protocol": "bridge-draft-0", "type": "apply_action_response", "id": 3, "ok": true, "state": { "...": "적용 후 GameState" } }
```

- 응답 상태는 fixture의 `stateDelta`를 입력 상태에 적용해 만든 값이라, 사이트 규칙 코드가 실제로 낸 결과입니다(`apply_action.*.response.json`).
- 카드 행동은 `target`이 있을 수도 없을 수도 있습니다(`apply_action.card-no-target.*`, `apply_action.card-cell-target.*`).
- 게임을 끝내는 행동을 적용하면 `state.mode`가 `"gameover"`, `state.winner`가 승자가 됩니다(`apply_action.ends-game.*`).
- 엔진이 합법 목록에 낸 행동인데도 적용이 실패한 경우가 fixture에 37개 있어서, 실패 응답을 규약에 넣었습니다.

실패 응답 (`response.error.action-rejected.json`, 손으로 쓴 예시):

```json
{ "protocol": "bridge-draft-0", "type": "error", "id": 8, "ok": false, "error": { "code": "action_rejected", "message": "..." } }
```

`error.code`: `invalid_request`, `unknown_game`, `action_rejected`, `internal_error`.

## 4. get_result: 종료 여부와 승패

요청 (`get_result.request.json`): `state`만 보냄.

응답 (`get_result.response.json`, fixture 종료 위치 `gameover-00004`):

```json
{ "protocol": "bridge-draft-0", "type": "get_result_response", "id": 7, "ok": true, "terminal": true, "mode": "gameover", "winner": "white" }
```

- `winner`: 아직 안 정해졌으면 `null`(사이트 내부 표현은 빈 문자열). 무승부 `"draw"`는 GAME-RULES.md에 근거한 값이며 fixture에는 나오지 않았습니다.
- 이 정보는 `apply_action` 응답의 `state.mode`, `state.winner`에도 이미 들어 있습니다. 별도 메시지로 둘지는 open-questions에 적었습니다.

## 5. 선택 확장: encoded (O-001)

인코딩 위치(Rust vs Python)가 미결정이라 기본은 **원문 상태만** 주고받습니다. Rust가 인코딩하기로 정해지면, 요청의 `"encode": "<형식 이름>"`에 응답의 `"encoded": {format, size, indices, values}`를 붙이는 식으로 확장할 수 있게만 열어 두었습니다(`../schemas/encoded.schema.json`, 예시 `../examples/encoded.sparse-example.json`). 이 필드의 이름과 sparse 모양은 예시이지 결정이 아닙니다. Python이 인코딩하기로 정해지면 쓰지 않습니다.
