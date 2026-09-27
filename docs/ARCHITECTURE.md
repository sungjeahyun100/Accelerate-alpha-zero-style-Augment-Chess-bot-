# 아키텍처

이 문서는 책임과 목표 인터페이스를 정의한다. JS oracle·fixture·차분 하네스·C++ 초안은
존재하지만 Rust 엔진, Python AI, PyO3/maturin 패키지, 신경망·ONNX 추론은 구현 전이다.
bridge의 JSON Schema와 예시는 `bridge-draft-0` 초안이다. 새 설계 D-004~D-006은 해당
변경의 `develop` 병합 시 적용한다. [DECISIONS](DECISIONS.md)에서 채택·미결정을 구분한다.

## 책임과 실행 방향

```text
JS oracle + fixture ── differential validation ──> Rust 규칙 엔진

Python MCTS/self-play ── PyO3 직접 호출 ──> 독립 Rust 규칙 엔진
                         ↑
                  maturin 빌드·패키징

Python ResNet 학습 (FiLM + 별도 LoRA)
  └─> 모델·어댑터·조건/규칙 메타데이터
       └─> 검증용 복사본의 정적 LoRA 병합·수치 비교
            └─> ONNX (FiLM 조건 입력 + policy/value)
                 └─> 추론 adapter 후보: ort / tract
```

JS→Rust 화살표는 검증 관계다. 실제 탐색은 Python MCTS가 Rust 환경을 호출한다.
추론 adapter는 모델 실행 경계에 두고 순수 규칙 엔진을 의존자로 만들지 않는다.
backend 선택과 Rust 추론 adapter의 구체 배치는 모델 검증 단계에서 확정한다.

## 영역별 책임

| 영역 | 소유하는 책임 | 넣지 않는 것 |
|---|---|---|
| `rust-engine/` | GameState, Action, legal/apply/terminal/result 규칙 | PyO3·Python·MCTS·학습·ort/tract 의존성 |
| `bridge/` | 논리 계약, JSON 기록·검증, 얇은 PyO3 바인딩 | 규칙·탐색·학습의 중복 구현 |
| `python/` | encoding, MCTS, network, self-play, training/evaluation/export | Python으로 다시 쓴 게임 규칙 |
| `infra/` | 기존 JS oracle·NNUE·검증·실험 도구 | 새 AlphaZero 탐색의 주 실행 환경 |
| `tests/differential/` | oracle 비교 fixture와 후보 검증 | 운영 데이터셋·모델 |
| `pre_cpp_engine_code/` | Rust 포팅 참고용 C++ 초안 | 완성된 증강체스 구현이라는 보장 |
| `docs/`, `.github/` | 설계·규약과 저장소 CI | 원시 실행 로그·무거운 학습 실행의 자동 시작 |

새 디렉터리·생성물·WSL2는 [AGENTS](../AGENTS.md)와
[개발 기준](ENGINEERING-STANDARDS.md)을 따른다. 기존 경로를 일괄 재배치하지 않는다.

## Bridge와 PyO3/maturin

`bridge/`는 공통 GameState·Action·결과·카드 정의와 언어 간 해석을 소유한다.
PyO3 바인딩은 향후 이 영역의 얇은 독립 crate에 두며 Rust 규칙 crate를 호출한다.
maturin은 이 바인딩의 빌드·Python 패키징 도구이고 모델 export 도구가 아니다.

반복 호출은 PyO3 타입·배열을 직접 전달한다. JSON은 저장·교환·fixture·차분 검증에
유지하며 탐색 노드마다 요청/응답 문자열을 만들도록 강제하지 않는다. 카드 정의의 1회
등록과 위치별 상태·행동의 의미는 두 경로에서 같아야 한다. 바인딩의 구체 타입·배열
소유권·batch·오류·GIL 해제는 Phase 5에서 검사한다. zero-copy나 성능 향상은 실측 전
보장하지 않는다. 엔진의 stateless/stateful 방식은 여전히 bridge의 미결정 사항이다.

Rust는 독립적으로 `GameState`, `Action`, `legal_actions()`, `apply_action()`,
`is_terminal()`, `result()`에 해당하는 규칙 API를 제공할 계획이다.
기존 JSON 스키마는 초안이며 아직 엔진에 구현하지 않았다.

## 신경망·LoRA·FiLM·Hypernetwork

policy/value network는 ResNet이다. FiLM은 카드·RULE·게임 상태 조건을 이용해 잔차
특징을 조절한다. 조건은 **명시적인 ONNX 입력**이고 FiLM 연산은 그래프 안에 유지한다.
조건 벡터의 의미·순서·dtype·shape와 인코딩 버전을 모델 메타데이터에 연결한다.

LoRA는 학습된 공통 모델의 모드·규칙 변화 적응용이다. 학습 중 기본 가중치와 별도
어댑터를 보존한다. 성능 검증 시 선택한 정적 어댑터를 기본 모델 **복사본**에 병합하고
병합 전후의 policy/value 수치와 실제 처리 성능을 따로 비교한다. 원본 모델·어댑터를
덮어쓰지 않는다. FiLM 조건이 바뀌는 기능은 LoRA 병합으로 제거하지 않는다.

Hypernetwork는 후속 확장이다. 어댑터의 생성, 적용, 병합/export를 분리하고 기본 모델
호환성·조건 고정 범위·병합 가능 여부를 계약에 둔다. 국면마다 바뀌는 어댑터는 하나의
고정 가중치로 병합할 수 없다. 미지원 동적 어댑터를 정적 LoRA 경로로 처리하지 않는다.
Hypernetwork 본체·생성 주기·추론 그래프는 이번 설계에서 구현하거나 확정하지 않는다.

세부 ResNet 층·채널, LoRA rank·적용 층, FiLM 삽입 위치, encoder 언어(O-001),
정책 행동 표현과 backend는 후속 단계의 결정이다. 기존 NNUE의 가치 출력이나 압축
self-play 기록을 새 policy/value와 완전한 게임 상태의 계약으로 간주하지 않는다.

## ONNX·추론 검증

ONNX는 모델 배포 형식이며 규칙·MCTS·데이터 계약을 대신하지 않는다. ort/tract는
같은 모델 입력·출력 계약의 후보 backend다. 어느 쪽이 더 빠르거나 지원하는 연산이
충분한지는 실제 모델과 목표 OS·batch 설정에서 확인한다.

모델에는 기본 모델·어댑터 식별, encoder/action 버전, 규칙·카탈로그 버전, FiLM 조건
명세, dtype/shape/layout, 출력 의미와 value 관점, opset·export 설정·파일 hash를 연결한다.
PyTorch 기준 출력, ONNX 출력, backend 출력을 동일 입력으로 비교한다. FP32부터
수치 기준을 확보하고 유한값·mask·batch·종료 상태·조건 변화와 LoRA 병합 오차를 검사한다.
backend 속도와 대전 승률은 별도 근거로 기록한다.

## 차분 검증과 학습 흐름

현재 하네스는 Rust 후보가 없으면 JS oracle 서버를 후보로 선택한다. CI는 기존
2,072개 fixture 중 300개를 검사한다. 이 성공은 JS 자체 회귀이며 JS↔Rust 검증이 아니다.
Rust 후보를 연결할 때 빌드 산출물 경로와 candidate 명령도 새 출력 계약에 맞춰 함께 바꾼다.
검증 대상은 legal actions, 적용/거절 결과, 전체 상태, 턴·카드·기물 상태와 종료 결과다.
사이트와 JS가 다를 때의 기준은 O-002로 남아 있다.

목표 학습 흐름은 `(state/observation, legal actions, visit policy, result, seed,
model/encoder/rules versions)`를 보존하는 self-play → Python 학습 → 후보 모델의
정확성·독립 대전 평가 → 승격이다. 중단 결과를 실제 무승부로 취급하지 않는다.
추가 행동·확률·은신에 필요한 상태와 관측 의미는 Phase 1/6/7에서 명시한다.

## 근거

- [maturin 프로젝트 구성](https://www.maturin.rs/project_layout.html)
- [PyO3 병렬 실행과 GIL](https://pyo3.rs/main/parallelism)
- [LoRA 원 논문](https://arxiv.org/abs/2106.09685)
- [FiLM 원 논문](https://arxiv.org/abs/1709.07871)
- [ort](https://github.com/pykeio/ort), [tract](https://github.com/sonos/tract)
