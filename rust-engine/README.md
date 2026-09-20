# Rust Engine

## 목적

실제 봇 탐색과 self-play에 사용할 고성능 증강체스 규칙 엔진을 Rust로 제공합니다.

## 책임 범위

장기적으로 `GameState`, `Action`, `legal_actions()`, `apply_action()`, `is_terminal()`, `result()`에 해당하는 독립 규칙 API를 담당합니다.

## 다른 영역과의 관계

동작은 `infra/`의 JS oracle을 기준으로 differential test에서 검증합니다. Python AI는 `bridge/`의 계약을 통해 이 엔진을 사용하며, Rust 엔진은 Python을 알거나 의존하지 않습니다.

## 포함하지 않는 코드

Python 학습 코드, MCTS, 신경망, JS oracle, 언어별 orchestration을 두지 않습니다.

## 현재 상태

**구현 전**입니다. Cargo 프로젝트나 Rust 소스는 아직 생성하지 않았습니다.
