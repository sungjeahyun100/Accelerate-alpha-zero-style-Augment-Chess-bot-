# Python AI

## 목적

AlphaZero 방식의 탐색, 신경망, self-play, 학습과 평가를 연구하고 구현하는 계층입니다.

## 책임 범위

- `alphazero/`: 전체 AlphaZero 흐름 조정
- `mcts/`: policy/value 기반 탐색
- `network/`: state encoder와 policy/value network
- `selfplay/`: self-play와 replay data 생성
- `training/`: 학습과 평가·승격 흐름
- `nnue/`: NNUE 관련 연구
- `tests/`: Python 계층 단위·통합 검사

## 다른 영역과의 관계

Python은 `bridge/` 계약을 통해 `rust-engine/`의 규칙 API를 사용합니다. `infra/`의 JS oracle은 Rust correctness 검증용이며 Python 탐색 루프의 환경으로 직접 사용하지 않습니다.

## 포함하지 않는 코드

증강체스 규칙의 별도 Python 구현, JS oracle, Rust 규칙 구현, bridge 규약 자체를 두지 않습니다.

## 현재 상태

**구현 전**입니다. 하위 디렉터리는 향후 책임 위치를 표시하는 빈 구조입니다.
