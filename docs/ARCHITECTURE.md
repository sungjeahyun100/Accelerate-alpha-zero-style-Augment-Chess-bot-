# 아키텍처

이 문서는 구현 언어별 책임과 의존성 방향을 정의합니다. 현재 저장소는 구조와 문서만 확정한 Phase 0이며, Rust 엔진·Python AI·bridge는 아직 구현 전입니다. **differential test는 하네스(`infra/tools/fixtures/run-differential.js` + `.github/workflows/differential.yml`)가 이미 구현되어 push/PR마다 자동 실행되지만, `rust-engine/`에 아직 후보 코드가 없어 지금은 오라클 서버 자신을 후보로 자체 검증만 함 — 실제 JS↔Rust 교차 검증은 Rust 바이너리가 생긴 뒤 워크플로의 `CANDIDATE_CMD` 한 줄만 바꾸면 시작됨.**

## 전체 구조

```text
JS Oracle (infra/)
        │
        │ differential validation
        ▼
Rust Engine (rust-engine/)
        ▲
        │ bridge (bridge/)
        ▼
Python (python/)
 ├─ MCTS
 ├─ Policy/Value Network
 ├─ Self-play
 └─ Training
```

위 그림의 JS → Rust 방향은 **검증 관계**입니다. 실제 AlphaZero 실행 경로는 아래와 같습니다.

```text
Python AI ──> bridge ──> Rust engine
```

Rust 엔진은 Python AI의 존재를 알 필요가 없습니다. Python이 bridge의 공통 계약을 사용해 Rust 엔진을 호출하며, JS oracle은 이 탐색 루프에 참여하지 않습니다.

## 1. Bridge

`bridge/`는 JavaScript oracle, Rust 엔진, Python AI가 공유할 경계입니다.

- 공통 `GameState`와 `Action` 표현
- 요청/응답 규약과 직렬화 형식
- 언어 간 호출 인터페이스
- differential test 데이터 규약

게임 규칙, MCTS, 신경망, 학습 로직은 bridge에 두지 않습니다. 구체적인 전송 방식(FFI, 프로세스 통신 등)은 Phase 1에서 결정합니다.

## 2. JavaScript oracle

`infra/engine-merged.js`를 포함한 기존 JavaScript 코드는 oracle/reference implementation으로 유지합니다.

- 실제 사이트와의 규칙 비교 및 site parity
- Rust 엔진 포팅의 expected behavior 제공
- 기존 실험, 데이터 수집, NNUE/자가대국 도구 보존

이 코드는 앞으로 실제 봇이나 AlphaZero self-play의 주 실행 엔진으로 사용하지 않습니다. 기존 도구의 경로와 동작은 보존하며, 상세 파일 지도는 [infra/FILE-GUIDE.md](../infra/FILE-GUIDE.md)에 있습니다.

## 3. Rust 규칙 엔진

`rust-engine/`는 실제 봇 탐색과 self-play에 사용할 고성능 환경입니다. 장기적으로 다음과 같은 독립 API를 제공합니다.

```text
GameState
Action
legal_actions()
apply_action()
is_terminal()
result()
```

규칙 동작은 JS oracle을 기준으로 검증합니다. Rust 계층에는 Python의 MCTS, 신경망, 학습 로직을 넣지 않습니다.

## 4. Python AI

`python/`는 ML 및 탐색 연구 계층입니다.

- state/action encoding
- AlphaZero 방식 MCTS
- policy/value network
- self-play와 replay buffer
- training과 evaluation
- NNUE 관련 실험

Python은 게임 규칙을 별도로 재구현하지 않고 bridge를 통해 Rust 엔진을 사용합니다.

## 5. Differential test

향후 `tests/differential/`에서 동일한 `GameState`와 `Action`을 JS oracle과 Rust 엔진에 입력해 다음 결과를 비교합니다.

- legal actions
- action 적용 결과와 보드 상태
- 턴과 카드 상태
- 특수 기물 상태
- 종료 여부와 승패 결과

Phase 0에서는 테스트 구현이나 fixture를 추가하지 않고 계획과 위치만 확정합니다.

## 6. AlphaZero 데이터 흐름

Rust 엔진과 Python AI가 연결된 뒤의 목표 흐름입니다.

```text
자가대국 (Python MCTS + Network, Rust rules)
  └─> 데이터 (state, visit policy, result)
       └─> Python training
            └─> 후보 모델
                 └─> evaluation / promotion
                      └─> 반복
```

state/action encoding, 정책 출력 공간, 네트워크 구조, MCTS 파라미터는 이후 단계에서 결정하고 [DECISIONS.md](DECISIONS.md)에 기록합니다.
