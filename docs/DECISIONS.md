# 결정 기록

팀이 합의한 중요한 아키텍처 결정을 기록합니다. 새 결정은 번호, 날짜, 결정, 이유, 대안과 영향을 남깁니다.

D-001~D-003은 기존 결정입니다. D-004~D-006과 D-003 보완은 2026-09-27 작업 방향을
기록한 변경이며, 이를 추가한 PR이 `develop`에 병합된 때부터 적용합니다. 설계 채택과
구현 완료는 별개입니다. 현재 구현과 관측한 검증 결과는 [IMPLEMENTATION](IMPLEMENTATION.md)에
기록하며, 소스 파일의 존재만으로 전체 지원 범위의 완성을 선언하지 않습니다.

## D-001: JS oracle + Rust engine + Python AI 구조 채택

- **날짜**: 2026-09-20
- **상태**: 채택
- **결정**:
  - 기존 JavaScript 엔진은 oracle/reference implementation으로 유지한다.
  - 실제 봇 탐색과 self-play에 사용할 규칙 환경은 Rust 엔진으로 포팅한다.
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
- **영향**: 책임과 의존성 방향을 유지한다. PyO3/maturin 방식은 D-004로 보완하며 구체적인 Rust API와 Python encoding은 후속 Phase에서 결정한다.

## D-002: 신경망 구조로 ResNet 채택

- **날짜**: 2026-09-23
- **상태**: 채택
- **결정**: policy/value network는 ResNet(잔차 신경망) 구조로 한다.
- **이유**: 팀 논의와 조사 결과 이 프로젝트 규모의 board-state 입력에 적합하다고 판단.
- **대안과 제외 이유**: 단순 Dense/CNN도 검토했으나, 입력 구조가 아직 확정 전이라 Phase 6(state/action encoding)에서 실제 성능으로 재확인 필요.
- **영향**: `python/`의 policy/value network 구현은 이 구조를 기준으로 시작한다. 세부 레이어 수·채널 수는 Phase 8에서 결정한다.
  LoRA·FiLM 역할과 Hypernetwork 확장 경계는 D-005로 보완한다.
- **구현 checkpoint**: 현재 기본값은 residual block 8개·channel 128개다.
  코드와 ONNX 수치 검증 범위는 [IMPLEMENTATION](IMPLEMENTATION.md)에 기록한다.

## D-003: 엔진-AI 통신 프로토콜의 기본 틀

- **날짜**: 2026-09-23
- **상태**: 채택
- **결정**: bridge를 통한 전달 정보는 다음 두 종류로 나눈다.
  - 게임 시작 시 1회: 전체 카드 정의 목록
  - 매 턴: 보드 상태, 카드(슬롯) 상태, 기타 게임 정보(턴 수, 모드 등)
  직렬화 형식은 JSON을 기본으로 한다.
- **이유**: 카드 정의처럼 게임 내내 바뀌지 않는 큰 데이터를 매 턴 반복 전송하지 않기 위함.
- **대안과 제외 이유**: 매 턴 전체 상태를 새로 정의해서 보내는 방식은 카드 정의 중복 전송 비용이 커서 제외.
- **영향**: `bridge/`의 요청/응답 규약은 이 틀을 기준으로 한다. Python encoding은 D-007,
  실제 실행·저장 계약은 [runtime-v1](../bridge/protocol/runtime-v1.md)을 따른다.
- **2026-09-27 보완(D-004와 같은 적용 시점)**: 카드 정의 1회와 위치별 상태의 논리 의미는 유지한다.
  JSON은 저장·교환·fixture·차분 검증의 기본 형식이고, 반복 탐색 호출은 PyO3 타입·배열로
  직접 전달한다. 모든 노드에 JSON 문자열 왕복을 강제하지 않는다. 기존 JSON Schema는
  과거 초안으로 보존하고 현재 v1 schema를 별도로 둔다. 구체적 바인딩 동등성은 실행 검사로 확인한다.

## D-004: PyO3/maturin 바인딩과 ONNX export

- **날짜**: 2026-09-27
- **상태**: 설계 채택, 구현 진행(검증 범위는 IMPLEMENTATION 참조)
- **결정**: Python AI → 얇은 PyO3 바인딩 → 독립 Rust 규칙 엔진으로 직접 호출한다.
  maturin은 바인딩 빌드·Python 패키징, ONNX는 신경망 export·배포를 담당한다.
  JSON의 논리 계약은 기록·검증에 유지한다. 기본 추론 backend는 `ort`, `tract`는
  명시적으로 선택하는 추가 backend다. 추론 runtime은 독립 규칙 엔진 밖에 둔다.
- **이유**: 언어 연동과 모델 배포의 책임을 분리하고 탐색의 반복 문자열 직렬화를 피한다.
- **대안과 제외 이유**: JSON 호출을 유일한 hot path로 유지하는 대신 검증 형식과 직접
  호출을 구분한다. 규칙 엔진에 ML runtime을 넣으면 의존성이 섞이므로 제외한다.
- **영향**: bridge/native의 독립 바인딩 crate와 bridge/runtime의 독립 추론 adapter를 둔다.
  오류·수명·배열 소유권·GIL·직접 호출/JSON 동등성을 설치 wheel에서 검사한다.
  API와 batch 한도는 runtime-v1 및 모델 metadata에 명시한다. zero-copy와 성능 향상은
  실측 전 보장하지 않는다. 기존 bridge-draft-0은 역사적 제안으로 보존하며 v1 실행 계약과
  혼용하지 않는다. backend의 실제 export 호환성은 실행 결과로 확인한다.

## D-005: ResNet의 FiLM 조건화와 LoRA 적응·Hypernetwork 확장

- **날짜**: 2026-09-27
- **상태**: 설계 채택, 구현 진행(검증 범위는 IMPLEMENTATION 참조)
- **결정**: FiLM은 카드·RULE·게임 상태로 ResNet 특징을 조건화한다. 조건을 ONNX 입력으로
  받고 FiLM은 그래프 내부에 유지한다. LoRA는 학습된 공통 모델의 모드·규칙 변화 적응용이다.
  학습 중 별도 어댑터로 저장하고, 성능 검증 단계에서 선택한 정적 LoRA를 기본 모델 복사본에
  병합한다. 원본을 보존하고 병합 전후 수치·성능을 별도 비교한다.
- **이유**: 국면 조건화와 모델 적응의 역할을 구분하고 학습 자산과 배포 후보를 재현한다.
- **대안과 제외 이유**: LoRA를 처음부터 기본 가중치에 덮어쓰거나 FiLM 조건을 export 시
  상수로 고정하면 원본·조건 변화의 의미를 잃으므로 제외한다.
- **영향**: 어댑터 생성·적용·병합/export를 분리해 Hypernetwork 확장을 준비한다.
  기본 모델 호환성·조건 고정 범위·병합 가능 여부를 계약에 둔다. 동적 어댑터를 하나의
  고정 가중치로 병합하지 않는다. Hypernetwork 구현·주기는 후속 범위다. 현재 기본값은
  8 block·128 channel, 각 block의 두 convolution에 rank 8·alpha 8·dropout 0,
  두 번째 BN 뒤 FiLM이다. 조건 encoding은 D-007과 EncoderSpec에 명시한다.
  두 기법의 채택과 export 수치 검증은 대전 성능 개선의 증명이 아니다.

## D-006: 저장소·에이전트 규약과 메타데이터 구조 검사

- **날짜**: 2026-09-27
- **상태**: 설계 채택(이 변경의 develop 병합 시 적용)
- **결정**: 기존 책임 영역을 유지하고 새로운 루트·생성물 예외를 명시적으로 등록한다.
  Windows 생성물은 `%APPDATA%\Accelerate`, CI는 `$RUNNER_TEMP/Accelerate`를 사용한다.
  WSL 일반 개발을 허용하고 Linux 재생성 캐시·가상환경·중간 빌드만 별도 고정 루트에 둔다.
  NASA/JPL 원칙을 연구·규칙·FFI 경계에 맞게 조정한다. 작은 Node 검사로 Git index를 검사한다.
  커밋·push는 검증된 논리 단위로 공유한다. SRP는 기능별 응집도를 기준으로 적용하고
  파일 크기 테스트로 분할을 강제하지 않는다. 회귀 검사·fixture는 재발 위험과 유지비를
  보고 선택하며 일회성 버그의 큰 자료를 영구 누적하지 않는다.
- **이유**: 디렉터리·산출물 파편화와 검증 범위의 과장 없이 자율 개발을 지원한다.
- **대안과 제외 이유**: 모든 WSL 호출의 supervisor 강제, C 안전 규칙의 전면 적용,
  새 대규모 관리자 도입과 기존 코드 일괄 이동은 현재 단계에 불필요하다.
- **영향**: [AGENTS](../AGENTS.md), [개발 기준](ENGINEERING-STANDARDS.md), 구조 정책과 CI가
  기준이다. 생성기 경로 전환은 해당 코드 수정 시 적용한다. 구조 검사는 모든 쓰기를
  통제하거나 모든 개발 규약의 준수를 증명하지 않는다.

---

## D-007: Python에서 관측·행동 encoding을 먼저 구현

- **날짜**: 2026-09-27
- **상태**: 사용자 채택(O-001 해결)
- **결정**: Python의 NumPy/PyTorch 계층에서 공개 Observation·공개 history·belief 요약과
  후보 행동의 의미 payload를 encoding한다. 실제 프로파일링으로 병목을 확인한 뒤에만
  Rust 이동을 검토한다. full Position, private RNG, positionId를 신경망 특징에 넣지 않는다.
- **이유**: 입력 의미와 조건 구조가 변하는 단계의 실험·수정을 단순하게 유지한다.
- **대안과 영향**: Rust가 미리 encoding하는 방식은 성능 근거가 생길 때 재검토한다.
  [ENCODING-EVIDENCE](ENCODING-EVIDENCE.md)는 참고 증거이며 이번 구현의 실측을 대체하지 않는다.
- **관측 v2 보완**: 동결 source 화면이 제공하는 기물 상태·보드 표시·관계를 정책 schema로
  정의하고 encoder의 16개 직렬화 필드에 `observation_policy_hash`를 포함한다. ONNX와 replay는
  정책 원문 및 JCS SHA-256을 보존하며 native runtime은 compiled 정책과 일치를 검사한다.
  `public-utf8-v2`·`public-film-v2`는 이전 artifact를 자동으로 변환하지 않는다. 이는 규칙·catalog
  동결 버전의 변경이 아니며 실제 관측 coverage와 배포 검증은 IMPLEMENTATION에서 확인한다.

## D-008: 최초 동결 사이트 본체를 규칙 정답으로 고정

- **날짜**: 2026-09-27
- **상태**: 사용자 채택(O-002 해결)
- **결정**: 이번 최초 snapshot의 실제 client 초기화·드래프트·검증·전이·종료를 정답으로
  삼는다. 256개 공개 카드 catalog를 지원 범위로 고정하고 이후 사이트 변경은 별도
  rules/catalog 버전 도입으로 다룬다. raw bundle은 Git에 넣지 않는다.
- **이유**: 기존 engine-merged.js 및 과거 worker fixture와 실제 사이트 사이의 불일치가
  관측됐다. worker의 useful-action filter와 AI target truncation은 전체 legal rules가 아니다.
- **대안과 영향**: 매 실행의 최신 사이트를 자동 흡수하거나 과거 239개 카드 범위로
  축소하지 않는다. 동결 SHA·URL·시간과 비교 차이는 IMPLEMENTATION에 기록한다.

## D-009: 공개 관측 기반 탐색과 이번 코드 완성 판정

- **날짜**: 2026-09-27
- **상태**: 사용자 채택
- **결정**: 8x8 normal·chaos·grand 및 동결 256 카드의 전체 규칙을 구현 범위로 삼는다.
  탐색은 공개 Observation/history·실제 화면에서 제공하는 hint·belief에 기반한 ISMCTS로
  한다. 선택한 상대 카드는 사이트에서 공개되므로 임의로 숨기지 않는다.
  private full Position은 환경 실행 경계에서만 사용한다.
- **완료 기준**: GO/NO-GO는 전체 코드·계약·한도·의미 비교 검증을 기준으로 한다.
  실제 학습, 장시간 자가대국, 대전 실력과 학습된 모델의 승격은 이번 구현에서 제외한다.
  짧은 synthetic optimizer/export 및 bounded rollout 검사는 학습 성능의 증거가 아니다.
- **영향**: source catalog 전 범위의 legal/reject/초기 draft/phase/terminal과 normalized
  full next state·result·RNG를 비교한다. signature·몇 개 기본 기물·구조 검사만으로 GO를
  선언하지 않는다. 아직 실패·미구현·근거 부족인 항목은 실행 보고에 유지한다.

## D-010: immutable snapshot 비교의 renderer 실행 context 명시

- **날짜**: 2026-09-28
- **상태**: 기존 immutable Position 요구를 위한 구현 선택 채택; 전체 카드 재검증 진행 중
- **결정**: `accelerate-headless-semantic-v2`는 성공한 restore/newGame admission에서
  원문의 activePieceAnimationUntil module Map을 cold 초기화한다. action 내부와 queued
  settlement에서는 유지하고 복원 실패는 state·RNG·cache·callback을 보존한다.
- **이유**: 같은 serialized snapshot·RNG도 snapshot 밖의 renderer Map에 따라
  animatedPieceIds가 달랐다. reused oracle의 일치를 독립 snapshot 전이 증거로 사용할 수 없다.
- **영향**: serialized field를 삭제하거나 비교에서 제외하지 않는다. 최초 source hash와
  rules/catalog/관측 정책 버전은 유지하며 실행 profile과 증거의 context를 별도로 기록한다.
  기존 v1·renderer audit 증거는 당시 범위로 보존하고 v2의 fresh full-state·RNG 재검증과
  구분한다. 이 경계는 populated browser의 미래 RNG 동등성이나 전체 코드 GO의 근거가 아니다.

## 열린 질문

O-001과 O-002는 각각 D-007과 D-008로 해결했다. 새로운 의미 또는 성능 선택이 필요하면
현재 사용자 결정과 혼동하지 않고 별도의 열린 질문을 기록한다.
## 새 항목 추가 형식

### D-XXX: 제목 (확정된 결정)

- **날짜**:
- **상태**:
- **결정**:
- **이유**:
- **대안과 제외 이유**:
- **영향**:

### O-XXX: 제목 (아직 미결정, 열린 질문)

- **날짜**:
- **상태**: 미결정
- **논의 내용**:
- **필요한 것**:
- **영향**:
