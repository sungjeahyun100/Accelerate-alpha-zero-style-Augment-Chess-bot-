# 아키텍처

이 문서는 책임과 구현 인터페이스를 정의한다. 독립 Rust 규칙, PyO3/maturin 패키지,
Python 공개 정보 탐색·replay·CLI, ResNet·FiLM·LoRA와 실제 ONNX 추론을 연결했다.
공통 계약은 [runtime-v1](../bridge/protocol/runtime-v1.md)이며 기존 draft schema는 과거
자료다. 전체 규칙·카드·visibility 포팅은 진행 중이고 전체 판정은 NO-GO다. 구현·관측한
검증·남은 조건은 [IMPLEMENTATION](IMPLEMENTATION.md), 채택 결정은 [DECISIONS](DECISIONS.md)에 둔다.

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
                 └─> 독립 Rust 추론 adapter: ort 기본 / tract 명시 선택
```

JS→Rust 화살표는 검증 관계다. 실제 탐색은 Python MCTS가 Rust 환경을 호출한다.
추론 adapter는 모델 실행 경계에 두고 순수 규칙 엔진을 의존자로 만들지 않는다.
추론 adapter는 `bridge/runtime/`, Python 경계는 `bridge/native/`에 둔다.

## 영역별 책임

| 영역 | 소유하는 책임 | 넣지 않는 것 |
|---|---|---|
| `rust-engine/` | GameState, Action, legal/apply/terminal/result 규칙 | PyO3·Python·MCTS·학습·ort/tract 의존성 |
| `bridge/` | 논리 계약, JSON 기록·검증, 얇은 PyO3 바인딩과 독립 ONNX 추론 adapter | 규칙·탐색·학습의 중복 구현 |
| `python/` | encoding, MCTS, network, self-play, training/evaluation/export | Python으로 다시 쓴 게임 규칙 |
| `infra/` | 기존 JS oracle·NNUE·검증·실험 도구 | 새 AlphaZero 탐색의 주 실행 환경 |
| `tests/differential/` | oracle 비교 fixture와 후보 검증 | 운영 데이터셋·모델 |
| `pre_cpp_engine_code/` | Rust 포팅 참고용 C++ 초안 | 완성된 증강체스 구현이라는 보장 |
| `docs/`, `.github/` | 설계·규약과 저장소 CI | 원시 실행 로그·무거운 학습 실행의 자동 시작 |

새 디렉터리·생성물·WSL2는 [AGENTS](../AGENTS.md)와
[개발 기준](ENGINEERING-STANDARDS.md)을 따른다. 기존 경로를 일괄 재배치하지 않는다.

## Bridge와 PyO3/maturin

`bridge/`는 공통 GameState·Action·결과·카드 정의와 언어 간 해석을 소유한다.
PyO3 바인딩은 `bridge/native/`의 얇은 독립 crate에서 Rust 규칙 crate를 호출한다.
maturin은 이 바인딩의 빌드·Python 패키징 도구이고 모델 export 도구가 아니다.

반복 호출은 PyO3 타입·배열을 직접 전달한다. JSON은 저장·교환·fixture·차분 검증에
유지하며 탐색 노드마다 요청/응답 문자열을 만들도록 강제하지 않는다. 카드 정의의 1회
등록과 위치별 상태·행동의 의미는 두 경로에서 같아야 한다. immutable Position/Action과
소유하는 NumPy 배열, 타입 오류·stale action·GIL 경계를 설치 wheel에서 검사한다.
엔진은 위치마다 독립 snapshot을 소유하고 적용 결과를 새 Position으로 반환한다.
zero-copy나 성능 향상은 실측 전 보장하지 않는다.

Rust는 독립적으로 GameState/Position/Action, legal 조회·직접 검증·apply·observe·result와
페이지 단위 action stream을 제공한다. 공개 intent의 숨은 실행 flag는 native 결속 경계에서
해석한다. positionId·private RNG·실제 환경의 전체 상태는 탐색 특징으로 전달하지 않는다.
기존 draft 스키마와 운영 runtime-v1을 자동 호환으로 취급하지 않는다.

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

구현 기본값은 ResNet 8 block·128 channel, block의 두 convolution에 LoRA rank/alpha 8,
dropout 0, 두 번째 BN 뒤 FiLM이다. encoding은 Python-first이고 봇의 행동·이력 정책은
public-decision-intent-v1/public-history-summary-v1이다. 기존 NNUE의 가치 출력이나 압축
self-play 기록을 새 policy/value와 완전한 게임 상태의 계약으로 간주하지 않는다.

## ONNX·추론 검증

ONNX는 모델 배포 형식이며 규칙·MCTS·데이터 계약을 대신하지 않는다. ort/tract는
같은 FP32 opset 18 모델 입력·출력 계약의 실제 backend다. 기본 ort와 명시 선택 tract는
자동 fallback을 하지 않는다. 동적 batch/action 축, condition 연결·artifact hash·shape/dtype·
유한값·자원 예산을 검사한다. backend 성능은 목표 OS·batch의 실제 측정으로 별도 확인한다.

모델에는 기본 모델·어댑터 식별, encoder/action 버전, 규칙·카탈로그 버전, FiLM 조건
명세, dtype/shape/layout, 출력 의미와 value 관점, opset·export 설정·파일 hash를 연결한다.
PyTorch 기준 출력, ONNX 출력, backend 출력을 동일 입력으로 비교한다. FP32부터
수치 기준을 확보하고 유한값·mask·batch·종료 상태·조건 변화와 LoRA 병합 오차를 검사한다.
backend 속도와 대전 승률은 별도 근거로 기록한다.

## 차분 검증과 학습 흐름

historical JS fixture CI는 원래 JS 서버를 명시하고 기존 자료에서 300개를 검사한다.
이 성공은 JS 자체 검증이며 JS↔Rust 검증이 아니다. 신규 본체 oracle은 최초 동결 client와
pinned parser를 실행하고 명시 headless profile의 의미를 보존한다. 초기·draft 일부는 실제
Rust와 비교했으며 전체 catalog의 normalized full state/RNG 동등성은 아직 미완료다.
검증 대상은 legal actions, 적용/거절 결과, 전체 상태, 턴·카드·기물 상태와 종료 결과다.
사이트와 기존 JS가 다를 때는 D-008의 최초 동결 client를 기준으로 한다.

목표 학습 흐름은 `(state/observation, legal actions, visit policy, result, seed,
model/encoder/rules versions)`를 보존하는 self-play → Python 학습 → 후보 모델의
정확성·독립 대전 평가 → 승격이다. 중단 결과를 실제 무승부로 취급하지 않는다.
실제 학습·대전 성능 캠페인·모델 승격은 이번 코드 구현에서 실행하지 않는다. 공개 trace와
독립 future RNG의 particle posterior, 실제 decision actor의 value 관점, bounded PUCT와
leaf batch를 사용한다. 완료한 코드 경로의 검증을 전체 규칙 의미 coverage와 구분한다.

## 근거

- [maturin 프로젝트 구성](https://www.maturin.rs/project_layout.html)
- [PyO3 병렬 실행과 GIL](https://pyo3.rs/main/parallelism)
- [LoRA 원 논문](https://arxiv.org/abs/2106.09685)
- [FiLM 원 논문](https://arxiv.org/abs/1709.07871)
- [ort](https://github.com/pykeio/ort), [tract](https://github.com/sonos/tract)
