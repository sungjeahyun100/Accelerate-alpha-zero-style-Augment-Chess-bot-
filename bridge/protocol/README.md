# Bridge 프로토콜 (DRAFT bridge-draft-0)

> **초안(DRAFT)입니다. 팀이 확정한 것이 아닙니다.** D-003(카드 정의는 게임 시작 시 1회, 매 턴 보드/카드 슬롯/기타 정보, JSON)만 확정 사항이고, 나머지 필드 이름과 메시지 모양은 검토용 제안입니다. 팀이 정하지 않은 것은 정하지 않고 [open-questions.md](open-questions.md)에 남겼습니다.

## 문서 구성

| 파일 | 내용 |
|---|---|
| [messages.md](messages.md) | 메시지 5종(new_game, get_legal_actions, apply_action, get_result, error)과 JSON 예시 |
| [state-and-actions.md](state-and-actions.md) | `GameState`, `Action`, 카드 정의의 필드 설명과 근거, 사이트 내부 상태와의 대응 |
| [open-questions.md](open-questions.md) | 아직 정해지지 않은 것, 확신 없는 필드 목록, O-001 확장 지점 |
| `../schemas/*.json` | JSON Schema(draft 2020-12) |
| `../examples/` | 스키마를 통과하는 예시(대부분 실제 fixture에서 뽑음)와 일부러 틀린 예시 |
| `../tools/validate.js` | 예시가 스키마에 맞는지 검사하는 의존성 없는 Node 스크립트 |
| `../tools/make-examples-from-fixtures.js` | 예시를 fixture에서 만든 일회성 변환기 |

## 큰 그림 (제안)

```text
Python AI ──(요청 JSON)──> 엔진 ──(응답 JSON)──> Python AI
            new_game        카드 정의를 1회 등록
            get_legal_actions / apply_action / get_result   매 턴 GameState를 함께 보냄
```

- **엔진은 위치 단위로 상태가 없습니다(stateless).** 매 요청에 `state`가 들어 있고, 서버가 대국을 기억하지 않아도 됩니다. 이렇게 하면 MCTS에서 임의의 위치로 되돌아가도 새 요청 하나면 되고, differential test의 fixture(상태 -> 합법 행동, 상태+행동 -> 결과 상태)와 모양이 같습니다. 대신 `gameId`는 등록해 둔 카드 정의를 가리키는 손잡이입니다.
- **카드 정의는 `new_game`에서 한 번만** 보냅니다(D-003). 이후 요청은 `gameId`만 붙입니다.
- **전송 수단은 정하지 않았습니다**(FFI, 표준입출력, 소켓 등, Phase 1/5에서 결정). 메시지 하나는 JSON 값 하나입니다.
- 인코딩(신경망 입력) 위치는 O-001이 미결정이므로 기본 응답에는 원문 상태만 있고, 선택 필드 `encoded`/`encode`를 확장 지점으로만 열어 두었습니다.

## 근거로 삼은 자료

1. **사이트 규칙 코드에서 뽑은 reference fixture** (PR #19, 브랜치 `feature/reference-fixtures`, `tests/differential/fixtures/site-reference-v1`). 349개 위치, 12,462개 합법 행동. 상태는 사이트 worker의 내부 표현(최상위 키 156개)입니다.
2. Twist 프로젝트의 자가대국 기록(`selfplay-worker-merged.js`의 `compactBoard`/`compactDeck`)은 보조 참고였습니다. 이 기록은 기물 상태를 필요한 것만 남기는 압축 형식이라 프로토콜의 상태 표현으로는 정보가 부족해서(예: 카드 `instanceId` 없음) 그대로 쓰지 않았습니다. 기물 플래그 목록이 어떤 것이 있는지 보는 용도로 참고했습니다.
3. `docs/GAME-RULES.md`(행동 종류, 카드 상태, 승패).

사이트 코드는 이 저장소에 없고, 여기 있는 예시는 fixture 데이터(JSON)만 사용해 만들었습니다.

## 검사 방법

```bash
node bridge/tools/validate.js --verbose
```

Node 18 이상이면 됩니다(외부 패키지 없음). 통과하면 마지막 줄이 `PASSED: 18 valid + 6 invalid examples ...`로 나옵니다. 스크립트는 JSON Schema 전체를 구현한 것이 아니고 이 스키마들이 쓰는 키워드만 지원하며, 그 밖의 키워드를 스키마에 쓰면 오류를 냅니다. 검사 대상과 기대 결과(유효/무효)는 `bridge/examples/manifest.json`에 있고, 목록에 없는 예시 파일이 있으면 실패합니다.

예시를 새로 만들려면(일반적으로 필요 없음):

```bash
git fetch origin feature/reference-fixtures   # PR #19
git show origin/feature/reference-fixtures:tests/differential/fixtures/site-reference-v1/pieces.jsonl   # 등 4개 파일을 한 폴더에 저장
node bridge/tools/make-examples-from-fixtures.js <fixture 폴더>
```
