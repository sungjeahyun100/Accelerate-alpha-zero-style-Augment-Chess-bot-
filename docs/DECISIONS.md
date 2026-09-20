# 결정 기록

팀이 합의한 중요한 아키텍처 결정을 기록합니다. 새 결정은 번호, 날짜, 결정, 이유, 대안과 영향을 남깁니다.

## D-001: JS oracle + Rust engine + Python AI 구조 채택

- **날짜**: 2026-09-20
- **상태**: 채택
- **결정**:
  - 기존 JavaScript 엔진은 oracle/reference implementation으로 유지한다.
  - 실제 봇 탐색과 self-play 환경은 Rust 엔진으로 포팅한다.
  - AlphaZero, MCTS, policy/value network, NNUE 및 학습 코드는 Python 계층에서 관리한다.
  - JavaScript, Rust, Python은 `bridge/`에 정의한 공통 상태·행동·호출 계약을 사용한다.
  - JS oracle과 Rust 엔진은 `tests/differential/`의 differential test로 동등성을 검증한다.
- **이유**:
  - Rust로 실제 탐색과 self-play 성능을 확보할 수 있다.
  - 검증된 기존 JS 동작을 버리지 않고 correctness 기준으로 활용할 수 있다.
  - Python의 ML 생태계를 활용할 수 있다.
  - 규칙, 통신, 탐색·학습, 레거시 검증의 책임을 분리할 수 있다.
  - 여러 기여자가 영역별로 독립적으로 개발하기 쉽다.
- **의존성 원칙**:
  - 실제 실행은 `Python AI → bridge → Rust engine` 순서다.
  - Rust 엔진은 Python AI에 의존하거나 이를 알지 않는다.
  - JS oracle은 실제 AlphaZero 탐색 루프에 참여하지 않고 Rust correctness 검증에만 사용한다.
- **대안과 제외 이유**:
  - JS 엔진을 MCTS 환경으로 직접 사용: 초기 재사용은 쉽지만 목표 탐색 성능과 장기 책임 분리에 맞지 않는다.
  - Python에 규칙을 다시 구현: ML 통합은 단순하지만 규칙 구현이 중복되고 oracle과의 drift 위험이 커진다.
- **영향**: 구체적인 bridge 방식, Rust API, Python encoding은 후속 Phase에서 별도 결정한다. D-001은 구현 방식이 아닌 책임과 의존성 방향만 확정한다.

## 새 결정 형식

### D-XXX: 제목

- **날짜**:
- **상태**:
- **결정**:
- **이유**:
- **대안과 제외 이유**:
- **영향**:
