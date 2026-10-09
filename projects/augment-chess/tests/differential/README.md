# Differential Tests

## 목적

동일한 `GameState`와 `Action`을 JS oracle과 Rust 엔진에 입력해 Rust 포팅의 규칙 동등성을 검증합니다.

## 책임 범위

legal actions, action 적용 결과, 보드·턴·카드·특수 기물 상태, 종료 여부, 승패 결과를 비교합니다. 공통 fixture와 비교 결과 형식은 `projects/augment-chess/contracts/` 계약을 따릅니다.

## 다른 영역과의 관계

`projects/augment-chess/oracle/`은 expected behavior를, `projects/augment-chess/engine/`은 actual behavior를 제공합니다. `projects/augment-chess/contracts/`는 양쪽이 공유하는 상태·행동 데이터 규약을 정의합니다.

## 포함하지 않는 코드

게임 규칙 구현, 성능 benchmark, MCTS, 신경망, 학습 검사를 두지 않습니다.

## 현재 상태

기존 fixture 하네스는 `projects/augment-chess/reference/infra/tools/fixtures/run-differential.js` +
`.github/workflows/differential.yml`(관련 경로의 push/PR에 실행)이다. 그 CI는
2,072개 중 300개를 오라클 서버 자신을 후보로 돌려 검사한다. Rust 엔진 코드는
별도로 존재하지만 이 기존 JSON-lines 후보 경로의 성공은 JS↔Rust 동등성 증거가
아니다. v7 네이티브 비교는 아래의 별도 명시적 진입점과 실제 설치된 바인딩을
사용한다. [생성물 경로 규약](../../../../AGENTS.md#생성물-경로)도 적용한다.

## Fixture 종류

- `fixtures/oracle-v1.jsonl.gz`: JS oracle(`engine-merged.js`) 기준 2,072개.
- `fixtures/site-reference-v1/`: **사이트 규칙 코드 기준** reference fixture 349개(약 5.5 MB). 규칙 차이 분석과 Rust 포팅의 참고 데이터. JS와 사이트가 다를 때의 최종 처리 기준은 [O-002](../../../../docs/DECISIONS.md#o-002-정답-기준을-사이트-원본으로-명시할지)에 남아 있다. 형식은 oracle-v1의 상위 호환이며 생성 방법·커버리지·한계는 그 폴더의 README.md 참고.

## v7 행동 표면 경계

`node --test projects/augment-chess/tests/differential/v7-action-surface.test.cjs`는 SHA-256으로 고정된
2026-09-28 client와 `accelerate-headless-semantic-v7` 프로필에서 Portal Gun,
Hypocrisy, Panic의 순서 있는 선택을 검사한다. 실제 source draft를 끝낸 뒤
목표 칸만 좁힌 작은 합성 국면을 사용해 각 후보 표면을 전부 소진한다.
각 순서를 원문에 직접 적용하고 adapter의 전체 상태·RNG와 대조한다.
Hypocrisy의 큰 후보 공간은 전체 `actions()`의 명시적 예산 오류와
`actionStream()`의 제한된 페이지 진행을 따로 검사한다. 이는 JS oracle의
행동 표면 검사이며 Rust 규칙 parity를 대신하지 않는다. 원시 조사 보고서는
저장소 밖 `Accelerate/reports`에 둔다.

## 규칙 의미 비교 초안

[Headless 규칙 동등성 계약](../../../../docs/HEADLESS-RULE-SEMANTICS.md)은 RNG·표현 상태를 제외하는 보수적 투영과 조건부 결합분포 비교 도구의 범위·미지원 항목·사용자 실행 명령을 기록한다. 아래의 전체 상태 비교와 별개이며 현재 자동 최종 gate가 아니다.

## v7 네이티브 비교 진입점

`node projects/augment-chess/tests/differential/v7-native-differential.cjs --python <maturin으로 설치한 Python>`은
고정된 client SHA·v7 프로필·catalog·공개 관찰 정책을 먼저 확인한다. normal,
chaos, grand 각각에서 seed 37의 초기 draft와 첫 play를 만들고, seed 19에서는
각 draft offer의 모든 카드가 catalog상 `ACTIVE`인 첫 선택을 반복해 첫 play를
재생성한다. seed 19 세 국면의 Position digest와 합법 후보 수는 같은 SHA의
**최상위 초기화를 수행한 원문**에서 다시 계산한 작은 고정값에 맞춰 검사한다.
원시 Position은 Git fixture로 복사하지 않는다.

각 국면에서 `actionStream()`의 **전체** 합법 후보를 한도 안에서 소진하고,
네이티브 PyO3 `Position.from_snapshot()`·`action_stream()`의 후보 집합뿐 아니라
**순서와 전체 action envelope**를 비교한다. 모든 합법 후보를 payload 및
snapshot 경로로 다시 bind하고, 원문이 거부한 잘못된 행동자 선택의 거부·원본
상태 보존을 확인한다. 처음/마지막 후보와 별도의 첫 card 후보가 있으면 이를
각각 적용해 전체 Position(상태·공개 이력·RNG·identity), 행동자·턴 변화,
결과 envelope 및 양측 viewer의 public Observation v2를 대조한다. JSON object의
키 순서와 정수/실수 표기 차이만 정규화하고, 누락 필드·배열 순서·값 차이는
불일치로 판정한다. 후보 prefix나 worker useful-action filter는 전체 합법 후보의
대체물이 아니다.

`--playout=normal:0:128`처럼 스타일·seed·최대 의사결정 수를 지정하면, 실제 원문
행동을 선택해 이어진 각 국면을 같은 방법으로 대조한다. 한 호출에는 최대 18개
스타일/seed 조합을 넣고, 각 경로는 최대 128개 의사결정에서 멈춘다. 후보가 없는
게임 종료 국면도 결과·양측 관측·빈 합법 목록으로 비교한다. 각 국면은 16 MiB
요청 및 8개 사례 이내의 배치로 네이티브 worker에 전달한다. 한도 도달은 원문
무승부로 기록하지 않는다. 한 배치가 실패해도 요청한 유한 사례의 나머지 배치를
검사해 사례별 실패와 첫 실패 상태를 함께 남긴다. worker가 일부 사례만 반환하면
그 배치를 `probe-error`로 기록한다.

`--source <절대 경로>`로 고정 client cache를 명시할 수 있다. `--oracle-only`는
네이티브 엔진 없이 원문 입력 생성을 조사할 때 사용하며 종료 코드는 실패다.
`--export-cases`는 동일한 원문 전체 Position·RNG·history·후보·적용 사례를
생성물의 `source-cases.jsonl`에 기록하고 보고서에 파일 SHA-256과 사례 수를 남긴다.
이 파일은 테스트 전용 Rust 내부 차등 조사 입력이며 Git에 넣지 않는다. 파일과 보고서의
해시·사례 수가 일치하지 않으면 일부만 생성된 입력으로 보고 거부한다.
네이티브 wheel 부재, v7 import/규칙 미지원, source 또는 native 스트림의
페이지·후보·examined 예산 초과와 비교 불일치는 모두 성공으로 처리하지 않는다.
고정 위치의 작은 요약만 `%APPDATA%/Accelerate/reports/v7-native-differential/report.json`
(CI: `$RUNNER_TEMP/Accelerate/reports/...`)에 쓴다. 원시 상태나 fixture는 저장하지
않는다. 미실행·미지원·불일치 상태의 보고서 `decision`은 `NO-GO`이며 명시적
`--oracle-only`도 종료 코드가 실패다. 기본 21개 국면에는 세 스타일의 draft/play,
seed 19의 첫 `ACTIVE` play, 원문으로 만든 합성 왕실 포획·상대 행동 불가의 종료 전후
국면, `disarmed`·`severed`·`iceSheet`·`staked`의 턴 종료 감소 전후가 포함된다.
상태 효과 사례는 원문 게임에서 시작해 해당 속성만 주입하고 새 Position ID를 만든 뒤,
원문 합법 행동으로 정산한다. 자연 seed와 합성 국면을 구분해 기록하며, 합성 사례를 자연
도달성의 증거로 해석하지 않는다. 선택형 연속 진행 역시 실제로 만난 분기에
한정되므로 통과해도 모든 규칙의 P8 완료나 프로젝트 GO 판정은 아니다.

### 공개 gate가 닫혀 있을 때의 Rust 내부 조사

공개 `Position::from_snapshot()`이 v7을 거부하는 동안에도, 명시적으로 생성한
`source-cases.jsonl`은 [테스트 전용 내부 probe](../../engine/src/tests/v7_internal_differential.rs)에
전달할 수 있다. `ACCELERATE_V7_INTERNAL_CASES`와
`ACCELERATE_V7_INTERNAL_SOURCE_REPORT`에는 같은 실행에서 나온 **외부 절대 경로**를,
`ACCELERATE_V7_INTERNAL_REPORT`에는 Git 밖 결과 경로를 준다. 원문 보고서의
SHA/profile·21개 사례·JSONL SHA를 검사한 뒤 명시적으로 `--ignored`를 붙여 실행한다.

```text
cargo test -p augment-chess-engine --lib v7_internal_differential -- --ignored --nocapture
```

이 probe만 Rust의 `#[cfg(test)]` 내부에서 source DTO를 typed state로 보아 draft 후보,
제한된 첫 이동 후보, 전이 뒤 full Position/RNG/history와 관측을 비교한다. 공개
`Position` 생성/전체 legal gate를 우회해 제품 capability를 여는 코드가 아니다.
`complete-legal`의 미지원, 카드·위협·UI 선택의 미관측, 합성 상태의 자연 도달성
부재를 사례별 실패 또는 부분 근거로 남긴다. 정상 `cargo test`는 이 외부 자료 의존
probe를 실행하지 않으며, 명시적 검사 실패도 전체 검증의 성공으로 바꾸지 않는다.

## v7 원문 범위 장부

`node --test projects/augment-chess/tests/differential/v7-source-inventory.test.cjs`는
고정 client SHA에서 읽은 257개 원문 정의와 256개 공개 카드의 ID·효과·실제
드래프트 분류·수동/패시브 분류를 대조한다. 원문 `ruleCardPool()`의 기본
deathmatch 설정 27개 RULE과 deathmatch 비활성 26개(`revelation` 제외)를
구별한다. 예전 기본 설정 26개 결과와 `capture-the-flag` 비가용 판정은 원문 최상위
초기화 누락에 따른 것이었다.

`node projects/augment-chess/tests/differential/v7-source-inventory.cjs --seeds=0,1,7,19,42,20260928`
는 세 스타일에서 유한한 실제 원문 오프닝 드래프트를 실행하고, 카드별 정의·RULE
선택 가능 여부·실제 제공/선택 목격을 서로 다른 증거 단계로 기록한다. 첫 플레이
행동은 최대 16개 수락·256개 조사 후보의 접두만 관측하며 전체 합법 목록으로
간주하지 않는다. 기물은 초기 보드에서 목격한 것만 따로 표시한다. 보고서는 생성물 `reports/v7-source-inventory`
에 보관한다. 미목격 카드·기물은 사용 불가능하다는 뜻이 아니며, 이 장부는 효과
동등성·중후반 드래프트·UI 선택·왕실 위협·종료를 완료로 표시하지 않는다.

### 미검증 분기와 수용 조건

| 분기 | 현재 원문 근거와 증거 한계 | 차등 수용 조건 |
|---|---|---|
| 카드 획득·효과 | SHA 고정 원문에는 공개 카드 256개와 보조 정의 1개가 있다. 장부의 자연 오프닝 `openingOfferWitness`/`openingSelectionWitness`는 제공·획득만 증명하고 `effectParity`는 모두 미검증이다. | 각 카드의 실제 전제조건을 만족하는 source-reachable 국면에서 수동 대상·자동/강제 효과·실패를 원문과 전체 Position/RNG/history로 비교한다. |
| RULE | 원문 `ruleCardPool()`은 기본 27개, deathmatch 비활성 26개(`revelation` 제외)를 반환한다. 풀 소속은 즉시 효과·지속 효과 실행 증거가 아니다. | 27개 선택의 획득, 적용 전후, 유지/해제와 조건부 분기를 각각 비교한다. |
| 기물 | catalog의 `pieceTypes`는 초기 기물과 카드 효과·설치물·8×8 미분류 ID를 함께 포함한다. 장부는 자연 초기 보드에서 본 `initialBoardWitness`만 참으로 둔다. | 생성 전제조건과 실제 이동/위협/포획 전이를 재현하고 source의 후보 순서·상태·결과를 비교한다. |
| 원시 후보와 공개 선택 | 원문 adapter의 `actionStream()`과 `publicHints()`는 별도 API다. 장부의 첫 플레이 행동은 16개 수락·256개 조사 이내 접두이고 `publicIntent`는 미검증이다. | 필요한 국면마다 raw stream을 완전 소진하고 ordered target, 실제 bind/apply, 양측 viewer의 공개 intent·hint를 따로 비교한다. 예산 초과는 실패로 둔다. |
| UI 선택 | catalog의 `sourceUiChoices`에는 `chooseRuleTicket`, `chooseJokerCard`, `chooseBarricadeDirection`이 선언돼 있다. 선언은 실제 선택 도달성이나 Rust 지원 증거가 아니다. | 각 선택이 발생하는 원문 국면에서 표시 순서, 입력 검증, 선택 뒤 전이를 비교한다. |
| 왕실 위협 | `AiNoCards`는 공개 후보와 별도의 내부 위협 문맥이다. 이 장부와 기본 13개 국면은 그 분기 동등성을 증명하지 않는다. | 카드 사용을 제외한 위협 후보, 체크·무행동·왕실 포획의 경계와 공개 관측을 원문과 비교한다. |
| 턴 정산·종료 | 기본 하네스의 상태 효과 감소 전후 8개와 왕실 포획·상대 무행동 종료 전후 4개 국면은 원문으로 만든 **합성** 사례다. 네이티브 실행 성공이나 자연 도달성 증거가 아니다. | 동일 국면의 네이티브 full state/RNG/history/result/양측 관측을 통과하고 자연 진행의 종료·상태 효과 경로도 별도 목격한다. |

각 행의 미충족은 해당 영역의 `NO-GO`다. 카드 수×모드 수 같은 단순 곱이나
기존 768-cell 접두 조사를 전량 source-reachable 규칙 증거로 승격하지 않는다.
