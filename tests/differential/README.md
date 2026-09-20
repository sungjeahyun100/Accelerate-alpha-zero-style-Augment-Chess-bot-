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

**구현 전**입니다. Phase 0에서는 테스트 계획과 위치만 정의합니다.
