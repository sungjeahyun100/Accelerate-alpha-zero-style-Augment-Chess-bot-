# 정답지(oracle) 기준 정리

> 상태: 설명 문서입니다. 팀이 확정한 결정은 아니며, 제안은 [DECISIONS.md의 O-002](DECISIONS.md)에 열린 질문으로 올려 두었습니다.

## 1. "oracle / 정답지"가 여기서 뜻하는 것

시험 채점에 비유하면 이렇습니다.

- **Rust 엔진** = 수험생. 규칙을 새로 풀어서 답을 냅니다.
- **oracle(정답지)** = 채점 기준표. "이 판에서 둘 수 있는 수는 이것들이고, 이 수를 두면 판이 이렇게 바뀐다"를 미리 적어 둔 답안입니다.
- **differential test** = 채점. 같은 문제(상태와 행동)를 수험생과 기준표에 똑같이 주고 답이 같은지 비교합니다.

기준표가 틀렸다면 만점을 받아도 소용이 없습니다. 그래서 "기준표 자체는 무엇을 근거로 만들었나"를 분명히 해 두는 것이 이 문서의 목적입니다.

## 2. 기준이 이어지는 순서 (reference chain)

```text
① 사이트 원본 worker         (진짜 정답, ground truth)
        │  이 코드를 돌려서 뽑음
        ▼
② 생성된 fixture 데이터      (PR #19, tests/differential/fixtures/site-reference-v1)
        │  비교
        ▼
③ Twist의 engine-merged.js   (infra/engine-merged.js, 참고용 구현. 틀릴 수 있음)
        │
④ Rust 포팅                  (differential test로 ②와 대조해 검증)
```

| 단계 | 무엇인가 | 믿어도 되는 정도 |
|---|---|---|
| ① 사이트 원본 worker | 사이트(augmentchess.org)가 실제로 쓰는 AI worker의 `generateActions` / `applyAction`. 사용자가 실제로 겪는 규칙이 이것입니다. | 기준. 다만 사이트가 업데이트되면 바뀝니다. |
| ② fixture | ①을 시드 고정으로 돌려 뽑은 데이터(JSON). 사이트 코드 자체는 들어 있지 않습니다. `meta.json`에 어느 사이트 번들에서 뽑았는지 해시가 적혀 있습니다. | ①에서 뽑은 시점의 스냅샷. 저장소 안에서 가장 믿을 만한 기준. |
| ③ engine-merged.js | Twist 프로젝트가 규칙을 JS로 다시 구현한 것. 실험, 학습 도구, 현재 differential 하네스의 임시 정답지로 쓰이고 있어 편해서 보존합니다. | **틀릴 수 있음.** ①과 다른 곳이 실제로 관측되었습니다(아래 3절). |
| ④ Rust 엔진 | 앞으로 만들 실행용 엔진. | ②와 대조해 검증합니다. ③과 ②가 다르면 ②를 따릅니다. |

기존 differential 하네스(`infra/tools/fixtures/run-differential.js`)와 `oracle-v1` 데이터는 ③ 기준으로 뽑은 것이고, PR #19가 ① 기준으로 뽑은 fixture를 같은 형식으로 추가합니다.

## 3. 알려진 engine-merged.js ↔ 사이트 불일치

출처는 PR #19의 `known-differences.json`과 fixture README입니다. 이 표는 그 자료에 나온 항목만 옮긴 것이며 새로 조사한 것이 아닙니다. 대부분은 "어떤 카드/규칙에서 갈라졌다"까지만 확인되었고, 왜 갈라지는지(원인)는 다시 조사하지 않았습니다.

### 3-1. 원인이 명시된 항목

| 카드/규칙 | 무엇이 다른가 | 어떻게 관측했나 |
|---|---|---|
| `locustSwarm` (개시 OPENING 카드) | 사이트 worker는 이 카드를 게임 중 카드로 처리하지 않고 무시하는데, engine-merged는 적용합니다. | 40~60수 대국 비교(`parity-playout`)에서 "actions-differ after card:locustSwarm" 4건 (`known-differences.json`의 knownCauses, README). |

### 3-2. 갈라진 것은 관측되었으나 원인은 미조사인 항목

관측 방법 세 가지와 건수는 아래와 같습니다. 각 항목의 원인은 문서에 "not root-caused again in this PR"라고 되어 있으며, 분류 문서는 Twist 저장소 `tools/site-parity/TRIAGE.md`(2026-09-19 기준)입니다.

| 관측 방법 (명령) | 결과 | 갈라진 항목 |
|---|---|---|
| 카드 행동 목록 비교 (`parity-actions.js - 400 12345`) | 400개 중 11개 불일치(2.8%) | `card:recurrence`(32로 기록됨), `card:randomRoulette`(3), `card:trolley`(1) |
| 행동 1개 적용 비교 (`parity-apply.js - 300 777`) | 297개 중 6개 불일치 | `zugzwang`(1), `blackMagic`(2), `binaMate`(1), `randomRoulette`(1), `falseStart`(1) |
| 여러 수 대국 비교 (`parity-playout.js - 60 4242 60`) | 60게임 중 25게임이 어딘가에서 갈라짐 (총 1130수) | 일반 이동 후 상태 차이 6, 일반 이동 후 행동 목록 차이 4, `locustSwarm` 후 행동 목록 차이 4, `checker` 후 상태 차이 3, `falseStart` 2, `zugzwang` 1, `randomRoulette` 1, `missionary` 1, `brutus` 1, `royalShield` 후 행동 목록 차이 1, `knightmate` 후 행동 목록 차이 1 |
| PR #19 fixture 349개를 develop의 `infra/engine-merged.js`에 돌림 (`run-differential.js`) | 328개 일치, 21개 불일치 | 불일치 출처 중 확인된 것(12종): `playout:5`(2), `piece:king`, `piece:alfil`, `card:blackBox`, `card:blueJeans`, `card:holdout`, `card:missionary`, `card:scarecrow`, `card:zugzwang`, `card:blackMagic`, `card:fieldPromotion`, `card:recurrence` 각 1 |

주의할 점:

- 위 수치는 그 시점 무작위 표본에서 나온 값이라 고정된 성능 지표가 아닙니다. 코드가 바뀌거나 표본이 달라지면 변합니다. PR #19 본문에도 "참고치"라고 적혀 있습니다.
- `recurrence`의 32는 문서에 적힌 숫자 그대로이며, 표본 400개보다 큰 값입니다. 무엇의 개수인지 원문에 설명이 없어 확인이 필요합니다.
- 마지막 표는 불일치 21개 중 처음 12종만 원문에 나열되어 있어 나머지 몇 개는 어느 항목인지 알 수 없습니다.
- 이 목록은 "engine-merged가 사이트와 다르다"는 뜻일 뿐, 어느 쪽 구현 문제인지를 가르지는 않습니다. 기준은 사이트이므로 관례상 engine-merged 쪽 차이로 봅니다.

**결론: Rust 포팅은 fixture(사이트 기준)를 따르고, engine-merged.js와 fixture가 다르면 fixture가 맞습니다.**

## 4. 사이트가 바뀌었을 때 갱신 절차

사이트가 업데이트되면 ①이 바뀌므로 ②도 낡습니다. 다음 순서로 갱신합니다.

1. **감지**: `site-watch` 워크플로(`.github/workflows/site-watch.yml`, 현재 수동 실행)가 `infra/tools/site-parity/check-site-update.js`로 사이트의 메인 번들 이름과 `aiWorker.js` SHA-256을 `last-seen.json`과 비교합니다. 바뀌었으면 `CHANGED`로 표시합니다.
2. **가져오기**: 같은 워크플로가 `fetch-real-worker.js`로 사이트 worker를 내려받습니다(사이트 코드는 이 저장소에 커밋하지 않습니다).
3. **동등성 검사(parity test)**: `parity-actions.js`, `parity-apply.js`, `parity-playout.js`로 engine-merged와 새 worker를 무작위 판에서 비교하고, 결과 보고서가 아티팩트로 올라옵니다. 차이가 늘었으면 규칙이 바뀐 것이므로 사람이 검토합니다.
4. **fixture 재생성**: Twist의 `tools/site-parity/gen-reference-fixtures.js`(같은 시드면 byte 단위로 같은 결과)로 `site-reference-v1` 데이터를 다시 만듭니다. 생성기와 절차는 PR #19 README에 있습니다.
5. **해시 기록**: `meta.json`의 사이트 번들 이름, `aiWorkerSha256`, `realWorkerJsSha256`, 파일별 sha256을 새 값으로 바꾸고, 검토 후 `check-site-update.js --save`로 `last-seen.json`도 갱신합니다.
6. 필요하면 Rust 엔진의 동작을 새 fixture에 맞게 고치고, `known-differences.json`의 수치는 "수집 당시 값"이므로 다시 측정해 갱신합니다.

이 절차 중 자동화된 부분은 1~3단계이고, 4~5단계는 사람이 실행합니다(이 문서 작성 시점 기준).

## 5. 이 문서가 다루지 않는 것

- 표의 각 불일치의 원인 규명과 수정
- 사이트 코드 사용에 대한 운영자 동의 확인(`NOTICE.md`에 증빙이 아직 없다고 PR #19에 적혀 있음)
- 정답 기준을 어떻게 명시할지의 결정 (DECISIONS.md O-002, 팀 확인 필요)
