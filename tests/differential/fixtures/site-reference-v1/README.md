# site-reference-v1: 사이트 규칙 코드 기준 reference fixture

Rust 포팅(ROADMAP Phase 2 "기준 fixture", Phase 4 differential test)의 정답지입니다. **사이트가 실제로 쓰는 규칙 코드**(AI worker의 `generateActions` / `applyAction`)에서 뽑은 값이며, `engine-merged.js`(JS oracle 복제본)에서 뽑은 값이 아닙니다. 기존 `../oracle-v1.jsonl.gz`(engine-merged 기준)와 형식이 같아서 `infra/tools/fixtures/run-differential.js`에 그대로 넣을 수 있습니다.

**이 폴더에는 사이트 코드가 들어 있지 않습니다.** 데이터(JSON)만 있습니다. 생성기는 Twist 저장소에 있고, 사이트 코드 사용에 대한 운영자 동의 증빙이 NOTICE.md에 아직 없으므로 이 저장소에는 복사하지 않았습니다.

## 파일

| 파일 | 크기 | fixture 수 | 내용 |
|---|---|---|---|
| `pieces.jsonl` | 0.91 MB | 45 | 기물 종류별 1개. 그 기물이 놓인 무작위 희소 보드 (`source: piece:<type>`) |
| `cards.jsonl` | 3.21 MB | 241 | 카드 239종 각각 1개(그 카드를 손에 쥔 무작위 보드, 카드 행동이 나올 때까지 최대 40번 보드 재추첨) + brutus 다중 룩 보드 2개 (`card:<id>`, `card:brutus:random-rook`) |
| `playouts.jsonl` | 0.80 MB | 29 | 시작 배치 + 특수 기물에서 시드 무작위 대국을 두다가 5/15/31수째에서 표본 (`playout:<게임>:<수>`) |
| `gameover.jsonl` | 0.54 MB | 34 | 게임 종료: 종료 직전 위치(마지막 수를 applied에 강제 포함) `gameover:*` 와 종료된 위치 `terminal:*` 쌍 17개 (대국에서 나온 것 + 왕 잡기가 가능한 구성 보드) |
| `meta.json` | 0.04 MB | - | 시드, 생성기 커밋, 사이트 번들 해시, 파일별 sha256, 개수, 기물/카드별 커버리지 |
| `known-differences.json` | 2 KB | - | Twist `engine-merged.js`와 사이트의 알려진 차이(수집 당시 수치) |

합계 약 5.5 MB(비압축). 크기 선택 이유: 카드 239종을 모두 손에 쥔 위치가 최소 1개씩은 있어야 해서(fixture 하나가 상태 약 5 KB + 전체 합법 행동 목록 + 적용 결과를 가짐) 카드 파일만 3.2 MB로, 사실상 하한입니다. 나머지는 그 위에서 기물 1개씩, 대국 표본, 종료 위치만 얹었습니다. gzip 하지 않은 평문 JSONL로 둔 이유는 생성이 byte 단위로 재현되는지 diff로 확인하기 쉽고, git이 알아서 압축하기 때문입니다.

## 형식

한 줄이 fixture 하나(JSON)입니다. `oracle-v1`과 같은 필드 + 확장 필드:

```text
{ id, source, seed, oracle: "site", color,          // color = 이번에 두는 쪽
  state,                                            // 사이트 worker의 전체 상태 JSON (약 156개 최상위 키)
  expected: {
    legalActions: [키, ...],                        // generateActions 결과. 정규화한 행동(JSON 문자열), 정렬, 중복 제거
    applied: [ { action, key, ok,                   // action = 원본 행동, key = 정규화 키
                 result,                            // [확장] applyAction 반환값(정규화)
                 signature,                         // oracle-v1 서명 (run-differential.js가 비교하는 값)
                 stateDelta } ],                    // [확장] 적용 후 전체 상태 (아래 delta 형식)
    nondeterministic: [ { action, key, ok } ],      // [확장] 결과가 Math.random에 의존하는 행동 (이번 생성에서는 0개)
    skippedNondeterministic: 정수,
    terminal: { mode, winner }                      // 이 위치 자체가 종료 상태인지
  } }
```

- **정규화**: 행동에서 `id`, `instanceId`, `pieceId`를 모든 깊이에서 제거(Twist `common.js`의 `norm()`과 동일). 상태는 손으로 만든 보드에서 시작하고 기물에 id가 없어서 별도 정규화가 필요 없었습니다(카드의 `id`/`instanceId`는 결정적인 값이라 그대로 둠).
- **서명(`signature`)**: `infra/tools/fixtures/generate-fixtures.js`의 `signature()`와 동일합니다(보드 셀 문자열, 카드 슬롯 사용/회복 상태, 턴 정보, 대기 효과 등). 이 저장소에는 원본이 언급하는 `docs/PORTING-GUIDE.md`가 없으므로 그 함수가 서명의 정의입니다.
- **`stateDelta`**: 적용 후 상태를 입력 `state` 대비 차이로 저장한 것입니다. 노드는 셋 중 하나입니다.
  - `{"=": 값}` 통째로 교체
  - `{"o": {키: 노드}, "d": [삭제된 키]}` 객체 패치
  - `{"a": {인덱스: 노드}}` 길이가 같은 배열 패치
  루트에서 위 규칙으로 적용하면 적용 후 상태가 됩니다(키 순서는 보존되지 않으니 비교는 키 순서 무관하게 하세요). 생성기의 `stateApply()`가 참조 구현이고, 생성 시 왕복 검사를 합니다. 전체 상태를 그대로 넣으면 fixture가 3배 이상 커져서 delta로 했습니다.
- **`ok: false`** 인 행동(합법 목록에서 나왔지만 적용은 실패)도 37개 들어 있으며, 그때 `signature`는 `null`이고 `stateDelta`는 없습니다.

## 러너로 실행

`run-differential.js`는 파일 하나만 받으므로 합치거나 파일별로 돌립니다.

```bash
cd tests/differential/fixtures/site-reference-v1
cat pieces.jsonl cards.jsonl playouts.jsonl gameover.jsonl > /tmp/site-reference-v1.jsonl
node ../../../../infra/tools/fixtures/run-differential.js \
  --fixtures=/tmp/site-reference-v1.jsonl --candidate="<후보 명령>" --report=report.json
# 파일별: --fixtures=cards.jsonl  /  카드만: --source=card:
```

`run-differential.js`는 `legalActions`, 각 `applied[i].ok`, `applied[i].signature`만 비교합니다. `result`와 `stateDelta`는 러너가 아직 비교하지 않는 확장 필드입니다(디버깅과 Rust 쪽 자체 테스트용). 후보가 이것까지 검증하려면 러너 확장이 필요하고, 이 PR에서는 `infra/`를 건드리지 않았습니다.

## 생성 방법

생성기: Twist 저장소(`Vamp-pire/Augment-Chess-Engine-Twist`) `tools/site-parity/gen-reference-fixtures.js`, 커밋 `f2c085b7ee4c69b3303e1264e88969bc18cf55f6`.

```bash
git clone https://github.com/Vamp-pire/Augment-Chess-Engine-Twist && cd Augment-Chess-Engine-Twist
git checkout f2c085b7ee4c69b3303e1264e88969bc18cf55f6
node tools/site-parity/fetch-real-worker.js --force     # 사이트 worker를 내려받아 .cache/real-worker.js 생성 (네트워크)
node tools/site-parity/gen-reference-fixtures.js --out=out --seed=20260926      # 기본 인자 = 이 폴더의 값
```

- 같은 시드와 같은 worker면 출력이 byte 단위로 같습니다(타임스탬프 없음). 사이트가 worker를 바꾸면 내용도 바뀌므로, 재현하려면 `meta.json`의 `realWorkerJsSha256`(= `.cache/real-worker.js`의 SHA-256)과 `site.aiWorkerSha256`이 같은지 먼저 확인하세요.
- `--worker=fast`는 Twist의 `make-fast-worker.js`가 만든 속도 개선판(동작은 같음, `diff-fast-worker.js`로 검증)을 씁니다. 두 worker로 생성한 결과가 byte 단위로 같다는 것을 확인했습니다.
- 사이트 번들(`meta.json`의 `site`): `main-saUeM6OL.js`, AI worker SHA-256 `5f73282964575ff2c63a863b1a2d1c9778d8109900e7f8d5c4e6fb5ec97340ea`.
- 재생 검증: `node tools/site-parity/gen-reference-fixtures.js --verify=<이 폴더> [--worker=orig|fast]`. 후보 모드: `--serve`(run-differential.js 프로토콜, 사이트 worker가 후보 역할).

## 커버리지

| 항목 | 값 |
|---|---|
| fixture | 349개 (합법 행동 12,462개 전부 기록, 최대 139개/위치, 합법 행동 0개인 위치 17개 = 종료 위치) |
| 적용 결과 기록 | 758개 (성공 721 / 실패 37). 그중 카드 행동 300개 |
| 기물 종류 | 45종 모두 보드에 등장. 41종은 그 기물의 행동이 applied에 들어감 (alfil, merchant, parrot, scarecrow는 applied 없음) |
| 카드 | 풀 239종(`POOL` 240줄 중 중복 1개) 모두 손에 쥔 위치 있음. 226종은 합법 카드 행동이 나왔고 모두 applied에 기록됨 |
| 카드 행동이 한 번도 안 나온 13종 | canceling, checker, exile, guard, homecoming, icbm, joker, judgment, freeMove, queenCavalry, summonColossus, fleetingDream, replayMove (선행 상태가 필요한 반응형·개시 카드로 보이며, 사이트 worker가 이 위치들에서 행동을 만들지 않았음. 원인 미조사) |
| 게임 종료 | 종료된 위치 17개(백 8, 흑 9), 종료 행동이 applied에 든 경우 66개 |

기물별/카드별 상세 수치는 `meta.json`의 `coverage`에 있습니다.

## 난수 의존 카드 처리 방식

사이트 worker에서 `Math.random`을 쓰는 곳은 카드 경로 1곳(brutus의 룩 선택)과 탐색용 1곳뿐입니다. 생성기는 `Math.random`을 시드 고정 스트림으로 바꿔 두고, 적용 결과(반환값 + 전체 상태)가 서로 다른 4개 난수 스트림에서 모두 같을 때만 `applied`에 기록합니다. 다르면 `expected.nondeterministic`에 넣어 합법성만 검증하고 결과는 검증하지 않습니다(oracle-v1의 skippedNondeterministic과 같은 취지). 이번 생성에서 worker가 `Math.random`을 21번 호출했지만 결과가 갈린 행동은 0개였습니다(brutus 다중 룩 보드 2개 포함). 즉 이번 데이터에서 `nondeterministic`은 비어 있고 이 장치는 안전망일 뿐 실데이터로는 작동을 확인하지 못했습니다. randomRoulette는 결과 기물이 행동 안에 이미 들어 있어서(worker가 난수를 안 씀) 결정적으로 기록됩니다. dice류 카드는 별도 확인하지 않았습니다.

## 알려진 한계

- **상태가 사이트 내부 표현 그대로**입니다(약 156개 최상위 키). Phase 1에서 정할 bridge의 `GameState`/`Action` 스키마와는 아직 다릅니다. bridge 스키마가 정해지면 변환기를 거쳐 다시 만들어야 할 수 있습니다.
- 비교 지표는 `signature` 기준이라 서명에 없는 상태 필드(예: 대기 중인 효과 대부분, 카드별 세부 플래그)는 러너가 자동으로 잡지 못합니다. 이 부분은 `stateDelta`로 사람이 대조할 수 있게만 해두었습니다.
- 위치는 손으로 만든 보드와 무작위 대국에서 나온 것이라 실제 대국의 분포를 대표하지 않습니다. 대국은 무작위 정책이라 조기에 왕이 잡혀 끝나는 경우가 많습니다.
- 카드 239종 중 13종은 카드 행동 자체를 검증하지 못했습니다(위 표).
- 개시(OPENING) 카드는 사이트 worker가 게임 중 카드로는 처리하지 않는 경우가 있습니다(예: locustSwarm). fixture는 worker가 실제로 한 대로 기록되어 있습니다.
- 규칙 카드(chess960 등)와 특수 모드, 드래프트, 3회 동형반복·45수 규칙은 대상이 아닙니다. 그 근거(`GAME-RULES.md`)와 대조하는 작업은 하지 않았습니다.
- 사이트가 업데이트되면 fixture는 낡을 수 있습니다. `meta.json`의 번들 해시로 어느 버전인지 알 수 있습니다.

## Twist `engine-merged.js`와의 알려진 차이

`known-differences.json` 참고. 요약: 무작위 표본에서 카드 액션 목록 불일치 2.8%(400개 중 11개), 단일 액션 적용 불일치 6/297, 40~60수 대국에서 25/60 게임이 어딘가에서 갈라짐. 이 저장소 develop(`16cf318`)의 `infra/engine-merged.js`를 후보(`generate-fixtures.js --serve`)로 이 fixture를 돌리면 349개 중 328개 일치, 21개 불일치입니다(Twist `b297ab3`로 동기화한 작업본으로 잠깐 돌렸을 때는 330/19였으나 그 브랜치는 이 PR과 무관하고 고정된 값이 아니므로 참고치입니다). **fixture는 언제나 사이트 기준**이며 engine-merged와 다르면 fixture가 맞습니다.
