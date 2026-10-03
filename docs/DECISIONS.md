# 결정 기록

팀이 합의한 중요한 아키텍처 결정을 기록합니다. 새 결정은 번호, 날짜, 결정, 이유, 대안과 영향을 남깁니다.

D-001~D-003은 기존 결정입니다. D-004~D-006과 D-003 보완은 2026-09-27 작업 방향을
기록한 변경이며, 이를 추가한 PR이 `develop`에 병합된 때부터 적용합니다. 설계 채택과
구현 완료는 별개입니다. 현재 구현과 관측한 검증 결과는 [IMPLEMENTATION](IMPLEMENTATION.md)에
기록하며, 소스 파일의 존재만으로 전체 지원 범위의 완성을 선언하지 않습니다.

## 재설계 D0 의미 정렬 (2026-09-29)

| 기존 결정 | 처리 | 이번 설계와의 관계 |
|---|---|---|
| D-001·D-008 | 유지 | 동결 사이트/JS oracle은 정답 비교용, Rust 엔진은 실제 탐색용이다. PR #29의 v7 게임 어댑터는 구현·검증 완료된 기준 실행층으로 재사용한다. |
| D-002 | 수정 | ResNet 단독 채택을 공통 `ObservationIR` 위의 mask-aware ResNet(A)·entity Transformer(B) 비교로 갱신한다. 성능상 우위나 최종 모델은 미정이다. |
| D-005 | 유지·확장 | FiLM 조건화와 분리 정적 LoRA 적응의 역할은 유지한다. Transformer의 LoRA 위치·rank 등은 모델별 실험 설정에 고정한다. |
| D-007 | 유지 | Python-first encoding은 이미 채택됐다. O-001을 다시 열지 않으며 실제 profiling으로 병목을 확인한 뒤에만 Rust 이전을 검토한다. |
| D-009 | 대상 갱신·단계화 | 최종 코드 GO 대상은 PR #29가 고정한 v7의 8×8 normal·chaos·grand와 공개 256장이다. v6 호환과 가변 geometry·두 모델의 기반 완료는 각각 별도 checkpoint다. |
| D-016 | 추가 | 엔진 상태와 AI 표현을 분리하고, 보드 크기 상수를 geometry로 옮기며, 검증용 N-version과 운영 오류 전파의 경계를 확정한다. |
| D-017 | 추가 | 향후 모노레포의 다른 프로젝트에서도 쓸 수 있는 객체형 어댑터 계약을 정의한다. Accelerate의 기물 이동·카드 효과는 프로젝트별 구현이며, 공유 계약에 보드·카드·게임 상태 타입을 박아 넣지 않는다. |

PR #29에는 `projects/augment-chess/engine/` 변경이 없다. 어댑터의 완료·검증 범위를 Rust의 v7 규칙 이관이나
전체 카드 조합의 동등성 완료로 확대하지 않는다. 범위별 실제 근거는
[ADAPTER-VERIFICATION](ADAPTER-VERIFICATION.md)과 [IMPLEMENTATION](IMPLEMENTATION.md)에 둔다.

## D-001: JS oracle + Rust engine + Python AI 구조 채택

- **날짜**: 2026-09-20
- **상태**: 채택
- **결정**:
  - 기존 JavaScript 엔진은 oracle/reference implementation으로 유지한다.
  - 실제 봇 탐색과 self-play에 사용할 규칙 환경은 Rust 엔진으로 포팅한다.
  - AlphaZero, MCTS, policy/value network, NNUE 및 학습 코드는 Python 계층에서 관리한다.
  - JavaScript, Rust, Python은 `packages/adapter-contract/`와 `projects/augment-chess/contracts/`에 정의한 공통·게임 전용 상태·행동·호출 계약을 사용한다.
  - JS oracle과 Rust 엔진은 `projects/augment-chess/tests/`의 differential test로 동등성을 검증한다.
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

## D-002: 공통 공개 입력 위에서 ResNet과 entity Transformer 비교

- **날짜**: 2026-09-23
- **상태**: 2026-09-29 비교 결정으로 갱신; A만 구현됨
- **기존 결정의 처리**: ResNet을 최초 구현과 비교의 A 기준선으로 유지한다.
  ResNet만을 최종 policy/value 구조로 지정한 부분은 아래 비교 결정으로 대체한다.
- **결정**: Rust 엔진이 허용한 같은 공개 Observation·history·규칙/행마 descriptor·
  candidate action에서 공통 `ObservationIR`을 만든다. A는 가변 크기 mask-aware ResNet,
  B는 기물·카드·활성 규칙·지형·효과와 geometry를 표현하는 entity Transformer다.
  두 모델은 같은 허용 정보, 후보별 policy logit, 같은 decision actor 관점의 value를 사용한다.
- **이유**: 엔진 저장 방식과 모델 입력 표현을 분리하고, 가변 보드를 CNN과 entity 모델
  양쪽에서 검증하기 위함이다. 모델별 입력 표현과 backbone을 함께 비교하므로 결과를
  attention 단독 효과 또는 token화 단독 효과로 해석하지 않는다.
- **대안과 제외 이유**: 칸 token Transformer는 원인 분해가 필요할 때의 추가 대조군이다.
  Transformer의 우위나 미학습 크기·카드 조합에 대한 일반화는 미리 결정하지 않는다.
- **영향**: 모델별 encoder·artifact는 분리하되 공개 의미 필드·history·descriptor·
  후보 의미를 맞춘다. 모델용 token/plane을 규칙 엔진의 저장 구조로 사용하지 않는다.
  FiLM·LoRA의 역할은 D-005, Python 인코딩 소유권은 D-007을 따른다.
- **구현 checkpoint**: 기존 A는 8×8·residual block 8개·channel 128개이며 새 A/B와
  가변 크기 입력·두 backend 검증은 아직 구현되지 않았다. 실제 검증 범위는
  [IMPLEMENTATION](IMPLEMENTATION.md)에 기록한다.

## D-003: 엔진-AI 통신 프로토콜의 기본 틀

- **날짜**: 2026-09-23
- **상태**: 채택
- **결정**: bridge를 통한 전달 정보는 다음 두 종류로 나눈다.
  - 게임 시작 시 1회: 전체 카드 정의 목록
  - 매 턴: 보드 상태, 카드(슬롯) 상태, 기타 게임 정보(턴 수, 모드 등)
  직렬화 형식은 JSON을 기본으로 한다.
- **이유**: 카드 정의처럼 게임 내내 바뀌지 않는 큰 데이터를 매 턴 반복 전송하지 않기 위함.
- **대안과 제외 이유**: 매 턴 전체 상태를 새로 정의해서 보내는 방식은 카드 정의 중복 전송 비용이 커서 제외.
- **영향**: 게임 계약의 요청/응답 규약은 이 틀을 기준으로 한다. Python encoding은 D-007,
  실제 실행·저장 계약은 [runtime-v1](../projects/augment-chess/contracts/protocol/runtime-v1.md)을 따른다.
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
- **영향**: `projects/accelerate/native/`의 독립 바인딩 crate와
  `projects/accelerate/runtime/`의 독립 추론 adapter를 둔다.
  오류·수명·배열 소유권·GIL·직접 호출/JSON 동등성을 설치 wheel에서 검사한다.
  API와 batch 한도는 runtime-v1 및 모델 metadata에 명시한다. zero-copy와 성능 향상은
  실측 전 보장하지 않는다. 기존 bridge-draft-0은 역사적 제안으로 보존하며 v1 실행 계약과
  혼용하지 않는다. backend의 실제 export 호환성은 실행 결과로 확인한다.

## D-005: FiLM 조건화와 LoRA 적응·Hypernetwork 확장

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
- **2026-09-29 보완(D-002와 함께 적용)**: FiLM은 A/B 모두에서 명시적인 공개 조건으로
  특징을 조절하고 ONNX 그래프에 남긴다. LoRA는 계열별 별도 정적 adapter다.
  Transformer의 적용 projection·rank·alpha·dropout은 **모델별 실험 설정과 artifact**에
  기록해 해당 실행 동안 고정한다. 현재 ResNet의 convolution rank 8을 모든 모델에
  강제하거나 다른 계열의 adapter를 자동 호환으로 취급하지 않는다. 병합은 원본을
  보존한 복사본에서만 수행하며 Hypernetwork의 국면별 동적 adapter는 병합 대상이 아니다.

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
- **2026-09-29 적용**: `ObservationIR`에서 A의 plane/condition과 B의 entity/condition을
  만드는 운영 encoder도 Python이 소유한다. 독립 참조판은 검사 전용이다.
  인코딩·언어 경계 전달·추론 시간을 구분해 실측하고 Python 인코딩이 실제 병목일 때만
  Rust 이전을 새 결정으로 검토한다. O-001은 해결 상태를 유지한다.

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
- **2026-09-29 대상 갱신**: 이번 최종 코드 GO의 원문은 PR #29의
  `augment-site-20260928-e5ed84fcf8e72a24`/headless-v7이다. 2026-09-27 동결본과
  v6 snapshot·계약은 호환·회귀 범위로 유지하되, v6 결과를 v7 규칙 동등성의 증거로
  대체하지 않는다. 두 버전의 source hash·catalog·관측 정책·실행 profile은 각각 고정한다.

## D-009: 공개 관측 기반 탐색과 이번 코드 완성 판정

- **날짜**: 2026-09-27
- **상태**: 사용자 채택
- **결정**: D-008의 v7 동결본에 대한 8×8 normal·chaos·grand 및 공개 256 카드의
  전체 규칙을 최종 구현 범위로 삼는다. 기존 v6 snapshot·replay·바인딩은 호환
  범위로 유지하며 별도 v7 포팅 완료로 간주하지 않는다.
  탐색은 공개 Observation/history·실제 화면에서 제공하는 hint·belief에 기반한 ISMCTS로
  한다. 선택한 상대 카드는 사이트에서 공개되므로 임의로 숨기지 않는다.
  private full Position은 환경 실행 경계에서만 사용한다.
- **완료 기준**: GO/NO-GO는 전체 코드·계약·한도·의미 비교 검증을 기준으로 한다.
  실제 학습, 장시간 자가대국, 대전 실력과 학습된 모델의 승격은 이번 구현에서 제외한다.
  짧은 synthetic optimizer/export 및 bounded rollout 검사는 학습 성능의 증거가 아니다.
- **영향**: source catalog 전 범위의 legal/reject/초기 draft/phase/terminal과 normalized
  full next state·result·RNG를 비교한다. signature·몇 개 기본 기물·구조 검사만으로 GO를
  선언하지 않는다. 아직 실패·미구현·근거 부족인 항목은 실행 보고에 유지한다.
- **2026-09-29 단계화**: D-016의 가변 geometry와 대표 규칙·두 모델·native 추론이
  연결된 기반 checkpoint는 전체 규칙 GO와 구분한다. 고정 v7의 8×8 사이트 범위의
  코드 완료 기준은 축소하지 않는다. 붕괴처럼 유효 칸이 바뀌는 현재 규칙과 합성
  외곽 확장·축소 검증의 출처도 분리한다.

## D-010: immutable snapshot 비교의 headless 실행 context 명시

- **날짜**: 2026-09-28
- **상태**: 기존 immutable Position 요구를 위한 구현 선택 채택; 전체 카드 재검증 진행 중
- **결정**: `accelerate-headless-semantic-v3`는 성공한 restore/newGame admission에서
  원문의 activePieceAnimationUntil Map과 clockDisplayAnchor를 cold 초기화한다. action
  내부와 queued settlement에서는 둘 다 유지하고 복원 실패는 state·RNG·두 context·callback을
  보존한다. v2는 renderer Map만 초기화했던 과거 실행 profile로 보존한다.
- **이유**: 같은 serialized snapshot·RNG도 snapshot 밖의 renderer Map이나 과거
  clockDisplayAnchor에 따라 animatedPieceIds 또는 다음 clock commit이 달랐다. reused oracle의
  일치를 독립 snapshot 전이 증거로 사용할 수 없다.
- **영향**: serialized field를 삭제하거나 비교에서 제외하지 않는다. 최초 source hash와
  rules/catalog/관측 정책 버전은 유지하며 실행 profile과 증거의 context를 별도로 기록한다.
  기존 v1·renderer audit·v2 증거는 당시 범위로 보존한다. v3에서는 fresh/reused 및 후보 조회
  유무의 시계 반례 16개가 전체 상태·history·RNG에서 일치했고 기존 Node 계약 17개가 통과했다.
  `draftDeleteEnabled`라는 별도의 newGame 모듈 상태 의존성은 D-012에서 다룬다.
  populated browser의 미래 RNG 동등성이나 전체 코드 GO도 이 증거로 주장하지 않는다.

## D-011: snapshot에서 계산하는 Deathmatch 공개 경고 자격

- **날짜**: 2026-09-28
- **상태**: 기존 공개 관측 요구를 위한 source 기반 구현 선택 채택; 새 정책 배포 검증 진행 중
- **결정**: local explicit 실행에서 `publicState.deathmatchStatus`의 `active`·`warning` 두
  bool을 제공한다. `warning`은 동결 source의 현재 경고 조건이며 다음 턴의 종료·승패
  예측이 아니다. source의 owner/online guard를 다른 실행 context로 일반화하지 않는다.
- **이유**: 기존 관측은 실제 source 경고가 달라지는 두 snapshot을 같은 정보 상태로
  취급했다. 반면 toast가 사라진 뒤 남는 DOM 문구와 notice dedup cache는 snapshot에
  없으므로 정확한 현재 DOM 표시를 복원할 수 없다. 두 bool은 source에서 계산할 수 있는
  공개 의미만 전달하고 raw counter·notice ID·외부 UI cache를 특징에 넣지 않는다.
- **영향**: 관측 envelope v2와 tensor 용량을 유지하고 projection을
  `source-visible-20260927-v3`로 갱신한다. 정책·encoder hash가 달라지므로 기존 artifact와
  replay의 계약 불일치를 명시적으로 거부하고 초기 관측 및 모든 복원 trace frame을
  spec에 따라 검증한다. 동결 rules/catalog/source와 모델 설정은 유지한다.
  미분류 52개 source 감사의 종료는 해당 규칙 구현이나 전체 코드 GO를 뜻하지 않는다.

## D-012: 새 게임의 원문 모듈 상태를 staging VM에서 원자적으로 초기화

- **날짜**: 2026-09-28
- **상태**: headless profile v4 구현 선택 채택; 전체 카드 재검증 진행 중
- **결정**: `accelerate-headless-semantic-v4`는 새 게임의 설정·seed·tape를 먼저 검증한
  뒤 별도의 동결 source VM에서 설정의 `draftDeleteEnabled`를 원문 초기화 전에 적용한다.
  상태 snapshot까지 성공하면 VM과 RNG callback을 함께 교체하며, 실패하면 기존 VM·상태·RNG·
  renderer Map·clock anchor·callback을 유지한다. D-010의 cold admission 의미를 계승한다.
- **이유**: 같은 `{draftDelete: true}`와 seed 41에서도 이전 원문 모듈값에 따라
  middle/end draft 완료 flag와 replay 기본 frame이 달랐다. 원문의 `resetGame`이 모듈값을
  읽은 뒤 어댑터가 state field만 덮어쓰는 순서로는 snapshot 전이 동등성이 성립하지 않는다.
- **영향**: 새 게임 세 모드의 전체 상태·RNG를 원문 직접 초기화와 비교했고 Node 계약
  17개, cold/warm 반례 및 주입한 실패 원자성 검사가 통과했다. 관측 정책·rules/catalog·
  동결 원본은 유지한다. v3 설치 wheel의 실제 ONNX 세 모드 통과는 v4 adapter를 포함하지
  않은 별도 근거이므로 혼합해 보고하지 않는다. 수치 설정 `starWinLimit`와
  `deathmatchLimitTurns`를 후속 적용하면 initial replay 기본 frame이 이전 값을 보유하는
  경계가 추가로 발견됐다. v4를 모든 설정의 새 게임 closure로 주장하지 않으며 D-013에서
  원문 기록시점을 따르는 수정을 구분한다.

## D-013: 초기 replay frame을 수치 설정 적용 후 원문 helper로 재기록

- **날짜**: 2026-09-28
- **상태**: headless profile v5 구현 선택 채택; 전체 카드 재검증 진행 중
- **결정**: v5의 staging VM에서 수치 설정을 적용한 뒤 초기 board history가 1개이고
  label이 initial이며 replay event와 nonce가 모두 0인 경우에만 원문의
  `captureReplayFrame()`과 `cloneReplayValue()`로 replay 기본 frame과 tail을 재캡처한다.
  예상 밖 replay 형태는 오류로 처리하고 기존 Oracle을 그대로 둔다.
- **이유**: 원문 `resetGame`은 사용자 수치 설정을 state에 반영하기 전에 초기 replay
  frame을 기록한다. 후속 state만 바꾸면 `starWinLimit=50`·`deathmatchLimitTurns=3`의
  현재 상태와 replay base/tail의 기본값 45·10이 갈라진다.
- **영향**: 같은 seed 41의 cold/warm 모듈 context 반례에서 전체 Position·RNG가 일치하고
  base/tail에 설정 수치가 남는 것을 확인했다. 기존 Node 17개와 기본 세 모드 직접 원문
  초기화 비교, 실패 원자성 검사도 통과했다. v3 wheel의 추론 검증과는 별도 근거이며
  관측 정책·rules/catalog·동결 원본은 유지한다.

## D-014: 공개 관측 후 행동 조회의 viewer 실행 context 복원

- **날짜**: 2026-09-28
- **상태**: headless profile v6 구현 선택 채택; 변형기물 전체 실행 검증 진행 중
- **결정**: `accelerate-headless-semantic-v6`는 동결 source의 원래 localViewColor와
  boardViewColor를 보존하고, 성공한 Position restore admission에서 상태 decode·relink 후
  두 값을 다시 결합한다. 관측 호출은 요청 viewer를 적용할 수 있지만 뒤따르는 새 행동
  조회는 해당 Position의 원문 턴 context에서 시작한다. restore 실패는 기존 viewer context를
  유지한다.
- **이유**: 같은 football Position에서도 `observe(black)`을 먼저 호출하면 원문의
  숨김 판정이 boardViewColor=black을 사용해 White의 합법 킥 3개를 0개로 만들었다.
  `observe(white)` 뒤에는 다시 3개가 나타나 snapshot만으로 행동을 정할 수 없었다.
- **영향**: cold·black 관측 후·white 관측 후 동일 Position의 행동 3개와 입력 불변을
  비교했고 기존 Node 17개가 통과했다. 관측 정책·rules/catalog·동결 원본을 바꾸지
  않는다. 앞선 v5 wheel의 세 모드 추론과는 별도 oracle 검증이며 전체 변형기물 지원의
  완료 근거는 아니다.

## D-015: 자동 OPENING 카드의 복구 snapshot을 공개 관측에서 제외

- **날짜**: 2026-09-28
- **상태**: source 분류 및 정책 구현 채택; 새 정책의 전체 배포 검증 진행 중
- **결정**: `firstMoveUndo`를 관측 정책의 내부 bookkeeping 필드로 명시한다. 이 필드는
  공개 viewer 관측·모델 특징·리플레이 공개 frame에서 제외하고 raw Position의 원문
  상태·실패 복구 의미는 유지한다.
- **이유**: 동결 source는 첫 수 자동 카드의 rollback을 위해 이전 보드·포획·합법 수 등을
  이 필드에 보관하고, 성공 뒤 null로 정리한다. renderer·AI 후보·공개 관측에서 직접
  읽는 경로는 없다. 성공한 chaos·grand 첫 수 뒤 `firstMoveUndo:null`만 남아도 기존
  엄격한 관측 정책은 미분류 오류를 냈다.
- **영향**: projection v3와 schema 2, tensor 크기는 유지하지만 정책 JCS hash는
  `5e17b5622f1e761d6e0719187006aaaac336150c4ae2372f76ef0080da0ab037`로 바뀐다.
  기존 BFB artifact·replay를 새 정책과 혼용하지 않는다. source 첫 수의 raw 전체 상태·RNG,
  양측 공개 관측·이력 기록을 새 정책에서 별도로 비교하고, 최종 ONNX metadata·설치
  wheel·두 OS CI는 새 hash로 다시 검증한다.

## D-016: 모델 독립 가변 보드와 검증 전용 N-version

- **날짜**: 2026-09-29
- **상태**: 설계 채택, 구현·검증 진행 중
- **결정**: 엔진은 크기·좌표 경계를 소유하는 geometry, 변화하는 유효 칸·지형·연결,
  단일 기물 identity와 footprint, 파생 점유 조회를 구분한다. 보드 크기를 규칙 코어의
  고정 8×8 상수로 취급하지 않는다. 규칙 상태에서 허용 관측을 만든 뒤 D-007의 Python
  encoder가 모델 입력으로 변환한다. 동결 v7 붕괴는 외곽 칸의 활성 상태를 바꾸고,
  별도 합성 profile에서 실제 외곽 확장·축소를 검증한다.
- **이유**: 규칙 의미를 plane/token 배열과 분리하고 원문 8×8 외의 보드 변화에도
  좌표·기물·행동·관측의 의미를 일관되게 유지하기 위함이다.
- **N-version 경계**: geometry·점유·행마 등 핵심 로직은 독립 참조 구현과 Rust 운영
  구현을 차분 검사할 수 있다. 참조판과 PR #29의 JS oracle은 운영 탐색에 참여하지
  않는다. 불일치는 원문·계약으로 판단하며 다수결로 실행하지 않는다.
- **실패 정책**: Rust 규칙 구현이나 선택한 ONNX backend가 실패해도 JS·Python 규칙
  구현 또는 다른 backend로 자동 전환하지 않는다. 미지원/한도 오류를 호출자에게 전파한다.
- **영향**: 기존 8×8 경로는 의미를 보존하면서 geometry에서 크기를 유도하도록 정리한다.
  새 구조는 명시 선택으로 병행하고, 규칙 미지원 상태를 전체 지원으로 표시하지 않는다.
  실제 학습과 전체 사이트 카드의 가변 보드 의미 확정은 이번 기반 checkpoint가 아니다.

## D-017: 모노레포를 위한 프로젝트 독립 객체형 어댑터 계약

- **날짜**: 2026-09-29
- **상태**: 언어 독립 wire 계약과 Rust 첫 구현체를 모노레포에 구현 중. 게임 v7 전체 실행 GO는 별도 검증 대상
- **이번 이관 범위**: PR #28의 마지막 커밋을 기준으로 한 어댑터 PR은 공유 계약·모노레포와 함께
  D-009의 동결 v7 규칙 전체 이식을 목표로 한다. 단계별 국소 검증은 중간 증거일 뿐,
  공개 256 카드와 source-reachable 이동·턴·종료·RNG·history 전체의 GO를 대신하지 않는다.
- **결정**: 공통 인터페이스를 가진 어댑터 객체를 구성하고 각 기능은 별도 객체로
  구현한다. 이 계약은 Accelerate 전용 규칙 엔진의 내부 API가 아니라 향후
  모노레포의 다른 프로젝트에서도 사용할 수 있는 **프로젝트 독립 경계**다.
  계약은 언어 독립적인 버전·capability·요청/응답·오류 의미로 기술하고 첫 구현체는
  Rust로 작성한다. 기물 이동·카드 효과는 첫 소비 사례이며, 공유 코어는 `GameState`, `CardSlot`,
  8×8 `Square`, v7 카드 ID나 source-specific 턴 정산을 알지 않는다. 프로젝트별
  구현이 입력·상태·행동·결과의 의미와 버전, 실행 context를 제공한다.
- **이유**: 프로젝트마다 비슷한 기능을 별도 분기문으로 확장하면 계약, 소유권,
  취소·오류·자원 한도 및 검증 근거가 흩어진다. 재사용 코어의 수명·호출·capability
  경계와 도메인 규칙을 분리하면 여러 프로젝트에서 동일한 확장 절차를 적용할 수 있다.
- **대안과 제외 이유**: Accelerate의 기물·카드 분기를 공유 패키지로 통째로
  옮기거나 한 거대 객체가 모든 규칙을 실행하게 하지 않는다. Rust `MoveBoard`와
  카드 registry 같은 현재 구조도 재사용 API의 확정 근거로 간주하지 않는다.
  객체마다 파일을 하나씩 만드는 방식도 요구하지 않는다.
- **영향**: 언어 독립 JSON schema와 발행 hash는 `packages/adapter-contract/`,
  첫 typed registry·호출 구현은 `packages/adapter-runtime/`에 둔다. 게임 전용
  catalog·schema·Rust 엔진·검증용 JS oracle은 `projects/augment-chess/`, 봇의
  native/runtime/Python 소비자는 `projects/accelerate/`에 둔다. 공유 패키지는
  게임 규칙을 import하지 않고 정적 등록으로 시작한다. 기존 v6 입력은 읽기·검증된
  부분 변환 자료로만 취급하고 공개 규칙 실행은 열지 않는다. 동결 v7의
  legal/reject/apply/full state·RNG·history 비교를 단계별로 통과한 capability만
  등록하며, 미지원 규칙은 오류로 남기고 운영 중 JS/N-version으로 자동 대체하지 않는다.
  상세 계약·현황·다음 작업 수용 기준은 [RULE-ADAPTER-HANDOFF](RULE-ADAPTER-HANDOFF.md)에 둔다.

## D-018: 현재 모델 입력은 고정 8×8 공간 경로와 의미 단위 entity 경로로 분리

- **날짜**: 2026-10-02
- **상태**: 구조·입력 의미 채택. 새 경로의 학습·ONNX·운영 MCTS 연결은 별도 검증 대상.
- **결정**: D-016의 엔진과 `ObservationIR` geometry 일반화는 유지한다. 현재 비교할
  ResNet 입력 `fixed8-spatial-v1`만 원점 (0,0)의 정확한 8×8을 허용하며 다른 크기는
  거부한다. Transformer `entity-token-v1`은 한 logical piece를 한 token으로 표현하고
  footprint를 별도 점유 tensor와 관계로 보존한다. 두 계열은 같은 공개 IR, 카드·규칙·
  효과·지형·이력 의미와 같은 `typed-input-v1` 후보 행동 tree를 사용한다. 후보별 logit과
  관측 viewer의 value를 반환하며 padding은 별도 mask로 구분한다.
- **이유**: 현재 학습 대상의 보드 크기를 고정하면서 규칙 엔진의 합성 가변 geometry
  검증을 계속할 수 있다. 동적 기물 embedding을 여러 점유 칸에 평균 집계하고 추가
  공간 상태 채널을 사용하면 종류별 one-hot plane과 마지막 기물 덮어쓰기를 피한다.
- **대안과 제외 이유**: D-002의 가변 ResNet은 과거 비교 방향으로 기록을 보존하지만
  현재 모델 v1의 요구가 아니다. JSON 필드별 token화와 BPE는 채택하지 않는다.
- **영향**: 기존 `PolicyValueNetwork`, `TypedEncoder`, `MaskResNetPolicyValueNetwork`,
  `EntityTransformer`와 배포 artifact의 버전·API는 유지한다. 새 버전의 spec·projection·
  모델은 추가 경로이며, 훈련 checkpoint와 ONNX manifest는 기존 것을 새 모델로
  해석하지 않는다. 자세한 현황·미완료 경계는 [MODEL-ARCHITECTURE](MODEL-ARCHITECTURE.md)에 둔다.

## D-019: 확률적 전이를 지원하는 AlphaZero 스타일 탐색

- **날짜**: 2026-10-01
- **상태**: 설계 채택, chance 경계와 전체 게임 통합 검증 진행
- **결정**: 실제 Rust 규칙 엔진 + policy/value network + MCTS 방향을 유지하고,
  MCTS를 **chance-aware AlphaZero-style MCTS**로 설계한다. 괴물의 무작위 이동이나
  랜덤 카드 드로우처럼 같은 `(state, action)`에서도 여러 후속 상태가 가능하므로 전이는
  `P(next_state | state, action)`으로 본다. 플레이어가 고르는 action의 decision node와
  엔진 규칙이 정하는 chance outcome의 chance node를 구분한다. 개념적 흐름은
  `decision state → player action → afterstate → chance event → next decision state`다.
  `afterstate`는 설명용이며 필수 저장 타입으로 확정하지 않는다.
  하나의 player action이 0개, 1개 또는 여러 개의 chance event를 연쇄적으로 발생시킬 수 있으며, 위 흐름은 개념적 모델이지 chance node 개수를 하나로 제한하지 않는다.
- **이유**: 순수 결정론적 `state + action → next_state`와 action마다 단일 child를
  가정하면 환경의 무작위 결과를 플레이어 선택으로 잘못 취급할 수 있다. Decision node에는
  policy prior·visit count·Q-value·PUCT를 적용할 수 있지만 chance node는 유리한
  outcome을 골라서는 안 된다. 확률은 엔진의 실제 게임 규칙을 따른다. 개념적 기대값은
  `Q(s,a) = Σ_o P(o | s,a) V(s'_o)`다.
- **엔진·AI 경계**: Rust 엔진이 합법 행동, 플레이어 행동 적용, 확률 사건의 가능한
  결과·확률 또는 규칙에 따른 샘플링, 결과 적용의 의미를 소유한다. AI는 규칙이나
  확률을 재구현하지 않는다. 실제 API 이름·타입은 Phase 1/3/5에서 기존
  `apply_action` 초안과 연결해 정한다. 엔진의 authoritative game state와 AI observation은
  구분한다. 내부 RNG 상태·셔플된 미래 덱 순서 등 플레이어에게 알려지지 않은 정보는
  신경망 입력이나 해당 플레이어의 관측에 노출하지 않는다.
- **대안과 제외 이유**: 결정론적 단일 후속 상태 탐색은 위 규칙을 표현하지 못한다.
  Stochastic MuZero로의 전환이나 NNUE를 확률 처리 수단으로 쓰는 방향은 채택하지 않는다.
  NNUE는 향후 축적한 대국·탐색 데이터로 별도 학습할 수 있는 평가 모델 후보다.
- **영향·미결정**: explicit chance expansion과 simulation마다 실제 분포를 따르는
  sampled chance outcomes를 모두 구현 후보로 둔다. 확률 열거·샘플링·RNG 재현 및 게임 계약/PyO3 표현은
  구현 단계에서 결정한다. 숨은 정보의 실제 범위와 불완전정보 탐색 도입 여부도 별도
  검토한다. 기존 JS oracle의 검증 역할과 D-001~D-005의 책임·모델 결정은 유지한다.

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
