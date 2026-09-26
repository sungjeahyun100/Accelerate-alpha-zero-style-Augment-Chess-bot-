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

하네스는 구현됨: `infra/tools/fixtures/run-differential.js` + `.github/workflows/differential.yml`(push/PR마다 자동 실행). `rust-engine/`에 아직 후보 코드가 없어서 지금은 오라클 서버 자신을 후보로 돌려 하네스/정답지 자체만 검증 중이다. `rust-engine/`이 `run-differential.js` 헤더에 적힌 stdin/stdout 프로토콜을 구현한 바이너리를 내놓으면, 워크플로의 `CANDIDATE_CMD` 한 줄만 바꾸면 실제 교차 검증이 시작된다.

## Fixture 종류

- `fixtures/oracle-v1.jsonl.gz`: JS oracle(`engine-merged.js`) 기준 2,072개.
- `fixtures/site-reference-v1/`: **사이트 규칙 코드 기준** reference fixture 349개(약 5.5 MB). Rust 포팅의 정답지로 쓸 것. 형식은 oracle-v1의 상위 호환이며 생성 방법·커버리지·한계는 그 폴더의 README.md 참고.
