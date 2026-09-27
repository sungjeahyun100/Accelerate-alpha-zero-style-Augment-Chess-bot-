# Python AI

## 목적

AlphaZero 방식의 탐색, 신경망, self-play, 학습과 평가를 연구하고 구현하는 계층입니다.

## 책임 범위

- `alphazero/`: 전체 AlphaZero 흐름 조정
- `mcts/`: policy/value 기반 탐색
- `network/`: state encoder와 policy/value network
- `network/`는 ResNet·FiLM 조건화·별도 LoRA 어댑터와 ONNX export의 모델 계약을 담당합니다.
  FiLM 조건은 ONNX 입력으로 유지하며, 검증용 정적 LoRA 병합은 원본 복사본에서 수행합니다.
  Hypernetwork는 생성·적용·병합 가능 여부의 확장 경계만 설계합니다.
- `selfplay/`: self-play와 replay data 생성
- `training/`: 학습과 평가·승격 흐름
- `nnue/`: NNUE 관련 연구
- `tests/`: Python 계층 단위·통합 검사

## 다른 영역과의 관계

Python은 `bridge/` 계약을 통해 `rust-engine/`의 규칙 API를 사용합니다. `infra/`의 JS oracle은 Rust correctness 검증용이며 Python 탐색 루프의 환경으로 직접 사용하지 않습니다.

반복 호출은 PyO3 타입·배열, 빌드·패키징은 maturin을 사용할 계획입니다. JSON은
저장·교환·검증 계약으로 유지합니다. 세부 모델 파라미터와 encoder 언어는 아직 미정입니다.
자세한 설계는 [ARCHITECTURE](../docs/ARCHITECTURE.md), 경로·WSL 기준은 [AGENTS](../AGENTS.md)를 따릅니다.

## 포함하지 않는 코드

증강체스 규칙의 별도 Python 구현, JS oracle, Rust 규칙 구현, bridge 규약 자체를 두지 않습니다.

## 현재 상태

**구현 전**입니다. 하위 디렉터리는 향후 책임 위치를 표시하는 빈 구조입니다.
PyO3/maturin 패키지·신경망·ONNX export는 이 설계 변경으로 구현되지 않았습니다.
