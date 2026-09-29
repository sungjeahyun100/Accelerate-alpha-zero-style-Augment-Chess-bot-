# Differential Tests

## 목적

동일한 `GameState`와 `Action`을 JS oracle과 Rust 엔진에 입력해 Rust 포팅의 규칙 동등성을 검증합니다.

## 책임 범위

향후 legal actions, action 적용 결과, 보드·턴·카드·특수 기물 상태, 종료 여부, 승패 결과를 비교합니다. 공통 fixture와 비교 결과 형식은 `bridge/` 계약을 따릅니다.

## 다른 영역과의 관계

`infra/`는 expected behavior를, `rust-engine/`는 actual behavior를 제공합니다. `bridge/`는 양쪽이 공유하는 상태·행동 데이터 규약을 정의합니다.

## 포함하지 않는 코드

게임 규칙 구현, 성능 benchmark, MCTS, 신경망, 학습 검사를 두지 않습니다.

## 현재 상태

기존 fixture 하네스는 `infra/tools/fixtures/run-differential.js` +
`.github/workflows/differential.yml`(관련 경로의 push/PR에 실행)이다. 그 CI는
2,072개 중 300개를 오라클 서버 자신을 후보로 돌려 검사한다. Rust 엔진 코드는
별도로 존재하지만 이 기존 JSON-lines 후보 경로의 성공은 JS↔Rust 동등성 증거가
아니다. v7 네이티브 비교는 아래의 별도 명시적 진입점과 실제 설치된 바인딩을
사용한다. [생성물 경로 규약](../../AGENTS.md#생성물-경로)도 적용한다.

## Fixture 종류

- `fixtures/oracle-v1.jsonl.gz`: JS oracle(`engine-merged.js`) 기준 2,072개.
- `fixtures/site-reference-v1/`: **사이트 규칙 코드 기준** reference fixture 349개(약 5.5 MB). 규칙 차이 분석과 Rust 포팅의 참고 데이터. JS와 사이트가 다를 때의 최종 처리 기준은 [O-002](../../docs/DECISIONS.md#o-002-정답-기준을-사이트-원본으로-명시할지)에 남아 있다. 형식은 oracle-v1의 상위 호환이며 생성 방법·커버리지·한계는 그 폴더의 README.md 참고.

## v7 행동 표면 경계

`node --test tests/differential/v7-action-surface.test.cjs`는 SHA-256으로 고정된
2026-09-28 client와 `accelerate-headless-semantic-v7` 프로필에서 Portal Gun,
Hypocrisy, Panic의 순서 있는 선택을 검사한다. 실제 source draft를 끝낸 뒤
목표 칸만 좁힌 작은 합성 국면을 사용해 각 후보 표면을 전부 소진한다.
각 순서를 원문에 직접 적용하고 adapter의 전체 상태·RNG와 대조한다.
Hypocrisy의 큰 후보 공간은 전체 `actions()`의 명시적 예산 오류와
`actionStream()`의 제한된 페이지 진행을 따로 검사한다. 이는 JS oracle의
행동 표면 검사이며 Rust 규칙 parity를 대신하지 않는다. 원시 조사 보고서는
저장소 밖 `Accelerate/reports`에 둔다.

## v7 네이티브 비교 진입점

`node tests/differential/v7-native-differential.cjs --python <maturin으로 설치한 Python>`은
고정된 client SHA·v7 프로필·catalog를 먼저 확인한다. normal, chaos, grand 각각에서
seed 37의 초기 draft와 원문 선택으로 draft를 끝낸 첫 play를 만든다. 국면마다
`actionStream()`의 **전체** 합법 후보를 한도 안에서 소진한 뒤 네이티브 PyO3
`Position.from_snapshot()`·`action_stream()`의 후보 multiset과 action envelope를
비교한다. 원문이 거부한 잘못된 행동자 선택의 거부·원본 상태 보존도 확인하고,
처음/마지막 합법 행동을 적용해 전체 Position(상태·공개 이력·RNG·identity),
행동자·턴 변화·결과를 대조한다. 후보의 prefix나 worker의 useful-action filter는
전체 합법 후보의 대체물이 아니다.

`--source <절대 경로>`로 고정 client cache를 명시할 수 있다. `--oracle-only`는
네이티브 엔진 없이 원문 입력 생성을 조사할 때 사용하며 종료 코드는 실패다.
네이티브 wheel 부재, v7 import/규칙 미지원, source 또는 native 스트림의
페이지·후보·examined 예산 초과와 비교 불일치는 모두 성공으로 처리하지 않는다.
고정 위치의 작은 요약만 `%APPDATA%/Accelerate/reports/v7-native-differential/report.json`
(CI: `$RUNNER_TEMP/Accelerate/reports/...`)에 쓴다. 원시 상태나 fixture는 저장하지
않는다. 이 6개 국면은 draft/play의 move·card·draft 선택과 ongoing 결과만 다루므로,
통과해도 특수 행동·종료 결과·모든 규칙의 P8 완료 판정은 아니다.
