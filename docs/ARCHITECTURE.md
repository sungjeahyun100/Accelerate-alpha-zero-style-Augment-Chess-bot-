# 아키텍처

이 문서는 현재 책임과 다음 구현 인터페이스를 구분한다. 독립 Rust 규칙,
PyO3/maturin 패키지, Python 공개 정보 탐색·replay·CLI, ResNet·FiLM·LoRA와
실제 ONNX 추론은 기존 실행 경로다. PR #29의 고정 v7 사이트 게임 어댑터는
구현·검증 완료된 correctness 기준 실행층이다. 공통 `ObservationIR`과 가변
ResNet/entity Transformer의 두 경로는 기반 구현이 있으나 최종 동일 SHA의 설치본과
전체 게임 규칙 입력까지 통합 검증되지 않았다.
게임 실행 계약은 [runtime-v1](../projects/augment-chess/contracts/protocol/runtime-v1.md)이며 기존 draft schema는 과거
자료다. 전체 규칙·카드·visibility 포팅은 진행 중이고 전체 판정은 NO-GO다. 구현·관측한
검증·남은 조건은 [IMPLEMENTATION](IMPLEMENTATION.md), 채택 결정은 [DECISIONS](DECISIONS.md)에 둔다.

## 책임과 실행 방향

```text
동결 사이트/PR #29 JS oracle ── differential validation ──> Rust 규칙 엔진

Python MCTS/self-play ── PyO3 직접 호출 ──> 독립 Rust 규칙 엔진
                         ↑
                  maturin 빌드·패키징

Rust 허용 관측·공개 이력 ──> Python ObservationIR·후보 행동
  ├─> plane/condition ──> A: mask-aware ResNet (FiLM + 별도 LoRA)
  └─> entity/condition ──> B: entity Transformer (FiLM + 별도 LoRA)
       └─> 계열별 모델·adapter·조건/규칙 메타데이터
            └─> 검증용 복사본의 정적 LoRA 병합·수치 비교
                 └─> ONNX (명시적 FiLM 조건 + 후보별 policy/value)
                      └─> 독립 Rust 추론 adapter: ort 기본 / tract 명시 선택
```

JS→Rust 화살표는 검증 관계다. 실제 탐색은 Python MCTS가 Rust 환경을 호출한다.
JS oracle·독립 참조 구현은 실패한 Rust 규칙 실행의 대체 경로가 아니다.
추론 adapter는 모델 실행 경계에 두고 순수 규칙 엔진을 의존자로 만들지 않는다.
추론 adapter는 `projects/accelerate/runtime/`, Python 경계는 `projects/accelerate/native/`에 둔다.

## 영역별 책임

| 영역 | 소유하는 책임 | 넣지 않는 것 |
|---|---|---|
| `packages/adapter-contract/`, `packages/adapter-runtime/` | 게임 비종속 schema·호출·등록 | 특정 게임 상태·규칙 |
| `projects/augment-chess/engine/`, `projects/augment-chess/contracts/` | GameState·Action·규칙과 게임 전용 catalog | PyO3·Python·MCTS·학습·ort/tract 의존성 |
| `projects/augment-chess/oracle/`, `projects/augment-chess/tests/` | 동결 client adapter·차분 fixture와 검증 | 운영 데이터셋·모델 |
| `projects/augment-chess/reference/` | 기존 JS oracle·NNUE 실험 도구와 C++ 초안 | 새 AlphaZero 탐색의 주 실행 환경 |
| `projects/accelerate/` | PyO3·ONNX runtime·encoding·MCTS·모델·학습 | Python으로 다시 쓴 게임 규칙 |
| `docs/`, `.github/` | 설계·규약과 저장소 CI | 원시 실행 로그·무거운 학습 실행의 자동 시작 |

새 디렉터리·생성물·WSL2는 [AGENTS](../AGENTS.md)와
[개발 기준](ENGINEERING-STANDARDS.md)을 따른다. 기존 JS 도구의 내부 상대 배치는 유지한다.

## 엔진 상태와 공개 입력의 경계

현재 Rust `GameState`는 8×8 dense board와 일부 동적 필드를 사용한다. 목표 구조는
크기·좌표 경계의 geometry, 변화하는 유효 칸·지형·연결, 단일 기물 identity와
anchor/footprint, 파생 점유 조회를 분리한다. 기존 8×8 사이트 초기 배치·홈 영역의
의미는 규칙 구성에 남기되 순회·경계·인덱스 계산의 크기는 geometry에서 유도한다.
보드 크기가 다른 합성 상태는 사이트의 해당 규칙 지원이 확인됐다는 근거로 쓰지 않는다.

동결 v7 사이트의 붕괴는 외곽 칸을 사용할 수 없게 하고 해당 기물을 제거하지만 보드
배열의 외곽 크기는 유지한다. 별도 합성 profile은 대국 중 실제 외곽 확장·축소의
좌표·점유·참조·snapshot·행동 무효화 계약을 검증한다. 축소로 잘리는 기물이나 활성
관계의 처리 규칙이 지정되지 않았다면 전이를 거부한다.

엔진은 viewer별 허용 Observation과 공개 history를 제공한다. Python은 D-007에 따라
이 입력을 공통 `ObservationIR`로 해석하고 모델별 plane/entity 배열로 변환한다.
실제 빈칸·비가시 칸·붕괴 칸·batch padding을 구분하며 private RNG, full Position,
positionId를 신경망 특징으로 전달하지 않는다. 두 모델은 같은 공개 history·행마/효과
descriptor·public candidate action을 입력으로 받는다. Rust로 encoder를 옮기는 작업은
실제 Python 인코딩 병목을 profiling으로 확인한 뒤에만 재검토한다.

기존 Rust 경로와 조합형 경로는 실행 시 명시적으로 선택한다. geometry·점유·행마의
독립 참조판은 차분 검사에서만 사용한다. 선택한 운영 규칙 구현에서 오류가 발생하면
호출자에게 전파하고 JS oracle·Python 참조판으로 전환하지 않는다.

## 계약과 PyO3/maturin

`packages/adapter-contract/`와 `packages/adapter-runtime/`는 게임을 모르는 객체 호출
계약을 소유한다. `projects/augment-chess/contracts/`는 게임 전용 상태·행동·결과·카드
정의를 소유한다. PyO3 바인딩은 `projects/accelerate/native/`의 얇은 독립 crate에서
Rust 규칙 crate를 호출한다.
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

## 두 policy/value 모델·LoRA·FiLM·Hypernetwork

현재 구현된 모델은 고정 8×8 ResNet이다. D-002의 다음 비교는 같은 `ObservationIR`과
공개 history·descriptor·후보 행동 계약 위의 A: mask-aware 가변 크기 ResNet,
B: entity Transformer다. 두 모델은 후보별 policy logit과 같은 decision actor 관점의
value를 출력한다. 한 모델에만 추가 공개 정보를 주지 않으며 성능이나 일반화의 우위를
아직 주장하지 않는다. B의 기물 token만으로 빈칸·비가시 영역·지형·연결을 잃지 않도록
geometry와 관계 정보를 보존한다.

D-018은 위 D-002의 과거 가변 ResNet 비교 범위를 현재 모델 v1에서 **고정 8×8**로
좁힌다. 엔진과 `ObservationIR`의 가변 geometry는 유지한다. 새 입력 경로는
`ObservationIR → EntityTokenEncoder / Fixed8x8SpatialEncoder →
EntityTokenTransformer / Fixed8x8ResNet → 공통 CandidateScorer`다. 기존
`typed-input-v1`과 배포 artifact는 이전 버전 그대로 유지한다. 새 경로의 구현 범위와
검증되지 않은 운영 연결은 [모델 아키텍처 현황](MODEL-ARCHITECTURE.md)에 적는다.

FiLM은 카드·RULE·게임 상태의 공개 조건으로 각 모델의 특징을 조절한다. 조건은
**명시적인 ONNX 입력**이고 FiLM 연산은 그래프 안에 유지한다. 조건 벡터의 의미·순서·
dtype·shape와 인코딩 버전을 모델 메타데이터에 연결한다.

LoRA는 학습된 공통 모델의 모드·규칙 변화 적응용이다. 학습 중 기본 가중치와 별도
어댑터를 보존한다. 성능 검증 시 선택한 정적 어댑터를 기본 모델 **복사본**에 병합하고
병합 전후의 policy/value 수치와 실제 처리 성능을 따로 비교한다. 원본 모델·어댑터를
덮어쓰지 않는다. FiLM 조건이 바뀌는 기능은 LoRA 병합으로 제거하지 않는다.

Hypernetwork는 후속 확장이다. 어댑터의 생성, 적용, 병합/export를 분리하고 기본 모델
호환성·조건 고정 범위·병합 가능 여부를 계약에 둔다. 국면마다 바뀌는 어댑터는 하나의
고정 가중치로 병합할 수 없다. 미지원 동적 어댑터를 정적 LoRA 경로로 처리하지 않는다.
Hypernetwork 본체·생성 주기·추론 그래프는 이번 설계에서 구현하거나 확정하지 않는다.

현재 ResNet 기본값은 8 block·128 channel, block의 두 convolution에 LoRA rank/alpha 8,
dropout 0, 두 번째 BN 뒤 FiLM이다. Transformer의 LoRA 적용 projection·rank·alpha·
dropout은 모델별 실험 설정과 artifact에 기록해 해당 실행 동안 고정한다. ResNet의
rank를 B의 전역 불변값으로 사용하거나 계열 간 adapter를 자동 호환하지 않는다.
기존 봇의 행동·이력 정책은 public-decision-intent-v1/public-history-summary-v1이다.
새 IR·모델 입력에는 별도 encoder/IO 계약 버전을 사용한다. 기존 NNUE의 가치 출력이나
압축 self-play 기록을 새 policy/value와 완전한 게임 상태의 계약으로 간주하지 않는다.

## ONNX·추론 검증

ONNX는 모델 배포 형식이며 규칙·MCTS·데이터 계약을 대신하지 않는다. 현재 ResNet bundle은
세 float32 입력·고정 8×8, 동적 batch/action 축, opset 18을 검증한다. 새 A/B bundle은
family별 입력 이름·float32/int64/bool dtype·shape·mask와 B/N/A/H/W의 실제 지원 범위를
별도 계약으로 검증한다. 기본 ort와 명시 선택 tract는 자동 fallback을 하지 않는다.
condition 연결·artifact hash·유한값·자원 예산을 검사하며 두 backend의 지원은 실제
export·실행 결과로 확인한다. backend 성능은 목표 OS·batch에서 별도 측정한다.

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
PR #29의 별도 고정 v7 게임 어댑터는 구현·검증 완료된 JS 실행층으로 재사용한다.
해당 adapter의 768개 카드/모드 표면 조사는 bounded probe이며 전체 카드 조합의
`completeRuleCoverage`를 뜻하지 않는다. PR #29에는 Rust 엔진의 v7 규칙 포팅이 없다.
geometry·점유·행마의 독립 N-version은 추가 correctness 검사이며 운영 다중 실행이나
다수결 판정이 아니다.

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
