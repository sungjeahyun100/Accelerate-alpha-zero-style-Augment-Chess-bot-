# 전체 구현 계획과 실행 checkpoint

현재 전체 판정은 **NO-GO**다. 이는 코드 완성·정답 동등성에 대한 판정이며 실제 학습 또는
봇 실력의 판정이 아니다. 사용자 채택 요구는 [DECISIONS](DECISIONS.md), 상세 작업·경계·
단계별 검증 지시는 [IMPLEMENTATION-DIRECTIVES](IMPLEMENTATION-DIRECTIVES.md)에 있다.
기존 v6 실행 계약은 [runtime-v1](../bridge/protocol/runtime-v1.md)에 보존한다.
아래 관측한 검사와 계획을 혼동하지 않는다.
작업 브랜치는 feature/full-stack-implementation, 출발점은 bfc85c886b489f21f6f1037bc291ab0157c3dc6f다.

## 2026-09-29 비어댑터 병렬 구현 checkpoint

카드 효과와 기물 이동의 규칙 객체화는 [별도 담당자 이관](RULE-ADAPTER-HANDOFF.md)에 따라
동결했다. 이번 단계는 공통 상태·입력·배포 계약과 독립 검증을 보강한 것이다. 공개 v7
`Position`의 import/legal/bind/apply와 플레이 행동자의 후속 힌트는 계속 미지원이다.

| 범위 | 이번 변경과 국소 증거 | 완료하지 않은 범위 |
|---|---|---|
| Rust 상태·흐름 | 명시 anchor와 파생 점유의 충돌을 거부하고, 손실 있는 레거시 export를 막는다. 가변 직사각형 replay의 빈·ragged 입력 및 크기 변경은 상태 변경 전에 거부한다. v7 deathmatch 설정을 원문 수치 규칙으로 정규화하고 좁은 seed 37 직접 호출에서 전체 상태·RNG를 대조했다. 왕실 위협은 내부 `AiNoCards` 후보·포획 실행이 검증될 때까지 닫았다. | 자연 대국 전이와 모든 source-reachable 상태·카드·기물 규칙. |
| 공개 AI 입력·실행 | 공개 이력의 최근 8개와 집계 일관성을 검증한다. 탐색의 루트 posterior를 조건화하지 않은 자식 posterior로 표기하지 않고, hidden opening의 확률·native 지원이 없으면 거부한다. replay의 중복·actor 불일치와 CLI의 실행 오류·예산 종료를 구분한다. | 공개 v7 native Position을 이용한 세 모드 bounded 탐색·자가대국. |
| 모델·추론 | A/B typed v3 ONNX에 model-config SHA-256을 정확히 한 번 기록하고 양쪽 로더가 검사한다. NumPy 소유 복사 전에 dtype/shape/총 원소 수/명시 byte 한도를 검사하며 관계 수 0을 기존 A/B 패리티 검사에 포함했다. Python 두 모델 검사 24개가 로컬에서 통과했다. | 최종 설치 wheel의 두 OS ORT·tract 검사. 중간 메모리 값은 검증된 exporter 계열에 대한 추정이며 임의 ONNX의 hard bound가 아니다. 기존 typed v3 bundle은 재export가 필요하다. |
| 차등 gate | 원문 행동을 새 Position에 재사용하면 거부되는지 검사하고 native bind/apply 뒤 원자성을 확인한다. 응답 건수·순서·실패 상태가 잘못되면 성공으로 보고하지 않는다. 동결 원문 전용 9국면 생성은 `oracle-only`·NO-GO로 끝났다. | native v7 import 미지원으로 실제 9국면 동등성 판정 불가. |

로컬 통합 검사에서 Rust 엔진 142개, 통합 1·4·8개, runtime 4개와 workspace
strict Clippy/rustfmt가 통과했다. WSL의 native 라이브러리 테스트 링크는 호스트의
`libpython3.12` 부재로 실행되지 않았고, 설치 wheel은 CI에서만 판단한다. Python
공개 IR·탐색·session 소스 검사 49개는 7개 native 의존 사례를 제외하고 통과했다.
이 단계의 검사 개수는 전체 규칙 GO나 모델 성능 증거가 아니다. 실제 학습은 수행하지 않았다.

## 2026-09-29 구현 checkpoint

이번 checkpoint는 동결 v7 규칙을 실행 가능하게 선언하는 단계가 아니다. Rust `Position`의
v7 생성·실행 guard와 `try_observe`의 동적 v7 투영 guard는 유지한다. 확인한 내용은 다음과 같다.

| 범위 | 구현·관측한 근거 | 남은 경계 |
|---|---|---|
| 규칙 기반 | checked geometry와 단일 기물 identity·파생 점유, 가변 합성 보드·붕괴, 출처와 순서를 보존한 MoveProgram/lazy cursor, 테스트 전용 N-version을 추가했다. Rust 엔진 lib 102개 및 통합 13개가 로컬에서 통과했다. | v7 전체 public legal/bind/apply 및 source full state/history/RNG 동등성. |
| 카드·턴 | v7 정의 256개와 보조 1개를 registry로 확인하고 초기 normal/chaos/grand 제안·첫 선택의 순서/RNG를 고정 원문에 국소 비교했다. Othello의 즉시·턴 완료 전향과 종료 흐름의 일부를 연결했다. | source-reachable 카드·RULE 전체 효과, 누락된 턴 경계 효과 조합. 미지원 조합은 오류로 닫는다. |
| correctness adapter | Portal Gun·Hypocrisy·Panic의 순서 있는 선택을 원문처럼 열거한다. 새 직접 source 검사 4개와 관련 Node 검사 50개가 로컬에서 통과했다. | 모든 카드 전제조건·특수 phase와 Rust 차분 검증. |
| 공개 AI 입력·모델 | v7 공개 관측과 합성 직사각형 상태를 분리한 Python typed IR, A mask-aware ResNet, B entity Transformer를 추가했다. 실제 source normal·chaos·grand의 행동자 관측/후보 2개씩을 IR→두 모델에 넣어 유한 출력을 확인했다. | v7 native Position에서 검증된 공개 관측·후보를 공급하는 실행 경로. |
| 배포·탐색 | 별도 v3 typed ONNX manifest와 A/B ORT·tract 추론을 Linux 설치 wheel에서 검사했고, P6 검증 스냅샷의 runtime 검사 11개가 통과했다. Python의 유한 search/session·replay/CLI 경로를 확대했다. | 후속 소스 변경을 포함한 정확한 최종 wheel·Windows 설치 검증과 v7 native 3모드 bounded 실행. |
| 통합 판정 | v7 normal·chaos·grand의 draft/play 6국면 차분 진입점은 원문 후보를 모두 소진하고 native v7 import가 `UnsupportedFeature`임을 명시적으로 보고한다. | 최종 동일 SHA의 Windows/Linux CI 성공, 전체 source-reachable 규칙 coverage와 코드 GO. |

실제 학습은 수행하지 않았다. 위 수치는 서로 다른 국소 검사 범위의 관측값이며, 새 PR
커밋의 CI 성공이나 규칙 전체 GO를 뜻하지 않는다.

## 2026-09-29 후속 통합 checkpoint

이 절은 위 기반 checkpoint 이후의 **로컬 작업 트리**에서 확인한 범위다. 동결 v7
클라이언트와 독립 N-version의 결과를 분리하고, 공개 native `Position`의 v7
import·play legal·bind·apply를 아직 열지 않는다.

| 범위 | 추가 구현과 확인한 증거 | 남은 경계 |
|---|---|---|
| 공간·행마 | v7 `collapseEdges`는 보드 외곽 8×8을 유지하며 영향받은 다중 셀 기물의 identity 전체를 제거한다. `blackHole`은 별도 위험 지형이다. 독립 N-version의 낡은 부분 footprint 잔존 기대도 원문에 맞게 수정했다. seed 19 normal Relay 직후에는 검증된 profile에서 swap 128개와 기본 행마 20개, 합계 **148개 후보의 payload·순서**가 원문과 일치한다. Siege Ram 경로와 Don Quixote의 왕 회피 자동 턴 진입도 각각 좁은 원문 상태에서 대조했다. | Relay 교환 실행·다른 조합의 이동·붕괴 후 상위 승패·일반 v7 legal 전체. |
| 카드·드래프트·턴 | 257개 정의의 표시 필드를 원문 SHA에 묶고, seed 37 normal·chaos·grand 초기 상태/전체 draft 후보 **3·3·28개**/첫 선택 후 전체 상태·history·RNG를 대조했다. seed 19에서 Relay·Reposition, grand의 실제 12회 선택 후 Taunt, chaos의 Queen's Gambit을 해당 도달 상태에서 대조했다. 10·20턴 MIDDLE·END 자동 드래프트는 normal·chaos의 **합성** 네 상태에서 전체 상태·RNG가 일치했다. | 나머지 카드·RULE 상호작용, 자연 대국으로 10·20턴 도달하는 전체 전이. |
| 공개 계약 | v7 드래프트·종료·비행동자 관측을 버전별 공개 정책으로 검사하며, 이력에 viewer 투영이 없으면 panic 대신 `InvalidState`를 반환한다. 마지막 draft 선택의 내부 공개 event 기록은 active-play 힌트와 분리했다. | 플레이 행동자의 `legalHints`와 모든 선택의 적용이 완성되기 전에는 공개 v7 `Position`을 열 수 없다. 기존 v6 전용 infallible `observe`도 v7에 사용하지 않는다. |
| AI·설치본 | source 공개 IR에 카드 alias와 belief 요약을 명시한 15필드 인코더를 적용했다. 로컬 Python 관련 검사는 **62 pass·7 native 선택 제외**였다. 중간 Linux sdist→설치 wheel에서 15필드 검증, 추론 검사 **11 pass**, A/B × ORT·tract 활성화 네 건과 수치 대조가 통과했다. | 같은 최종 커밋의 Windows/Linux 설치본, v7 native Position을 통한 세 모드 bounded 탐색. |
| 통합 검사 | 로컬 WSL `cargo test --workspace --locked`에서 엔진 lib **125개**와 통합 **1·4·8개**가 모두 통과했고 ignored는 0개였다. workspace 전체 대상 strict Clippy, 전체 rustfmt, 저장소 정책 Node 검사 14개와 v7 ordered adapter 검사 4개가 통과했다. | 동일 SHA CI 및 source-reachable 전체 규칙 coverage. 고정 v7 차분의 native import는 아직 명시적 `Unsupported`이므로 최종 코드 판정은 **NO-GO**다. |

Linux 전체 workspace 테스트의 PyO3 링크에는 WSL 시스템의 `libpython3.12.so.1.0`을
가리키는 재생성 가능한 외부 캐시 symlink를 사용했다. 모델 학습 캠페인은 실행하지 않았다.

## 2026-09-29 첫 플레이 계약 확장 checkpoint

이 절의 구현은 source v7에서 도달 가능한 **seed 19의 제한된 상태**와 공통 입력 경계를
넓힌 것이다. 공개 Rust `Position`의 v7 import·bind·apply는 계속 닫혀 있고,
처음 이동 이후의 일반 왕실 위협·합법 행동·종료 판정은 원문 동등성이 확인되지 않았다.

| 범위 | 이번 검증 가능한 진전 | 아직 허용하지 않는 범위 |
|---|---|---|
| 공간 | 실제 원문 전이에서 blackHole은 사용 가능한 위험 지형으로 남고, 주기적 붕괴는 8×8 extent를 유지하며 외곽 셀의 사용 가능성과 기물 identity를 바꾼다. 실제 draft의 2×2 `bigBishop` 및 중립 `football`도 단일 identity·파생 점유에서 원문 64셀과 일치했다. | 이 기물의 전체 행마·카드 상호작용 및 일반 resize의 사이트 규칙 동등성. |
| 행동 후보·카드 | normal·chaos·grand 첫 플레이의 기본 이동 20개와 카드 행동 **1·2·16개**를 각각 원문 전체 순서·payload 해시와 대조했다. normal Relay 뒤의 148개 이동 후보 및 수용된 교환 두 건의 64셀 보드 효과도 비교했다. `symmetry`·`ice-sheet`·`quantum-mechanics`와 grand 일부 활성 카드의 직접 효과·후보는 좁은 도달 상태에서 검사했다. | 첫 플레이 후보를 공개 `Position`으로 승격하는 것, 일반 카드·이동 적용과 전체 state/history/RNG parity. |
| 공개 관측·AI 입력 | seed 19 세 첫 플레이 상태의 카드 표적 힌트를 실제 UI 표시 순서로 투영한다. `miracle`의 빈 힌트, `grappler`의 첫 클릭 한 칸과 합법 복합 행동 네 개를 분리했다. 세 모드의 행동자·비행동자 **6개 전체 Observation v2 JCS와 `informationStateKey`**가 동결 원문과 바이트 단위로 일치한다. Python의 공식 사이트 공개 관측은 8×8로 검증하고 가변 geometry는 합성 profile에만 둔다. 검색의 v7 카드 필터는 첫 클릭만 대조하며 v6 복합 선택 방식은 보존한다. | 첫 플레이 이후 모든 도달 상태의 공개 관측, source-reviewed 운영용 MoveProgram descriptor. |
| 바인딩·CI | 서명이 맞지만 형식이 잘못된 v7 Position envelope는 native 경계에서 입력 오류로 구분한다. 직전 공유 SHA `766e55d8`의 Windows/Linux CI는 Rust·설치 wheel pytest 각 **87 pass** 뒤 v7 차분 9/9에서 import 미지원으로 정확히 **NO-GO**였다. | 이번 로컬 변경을 포함한 새 동일 SHA 설치 wheel·두 OS CI, v7 규칙 실행 GO. |

첫 보통 이동 `a2→a3`을 실제 Rust 전이에 넣는 조사에서는 아직 미지원인
`threat::play_move_sound`의 v7 합법 이동 조회에서 종료됐다. 원문은 여기서 세 번의
왕실 위협 후보 평가를 하고, chaos는 자동 효과로 RNG가 추가 진행한다. 결과를 상수로
대체하거나 v6 행마로 우회하지 않으며, 완성되지 않은 임시 실행 helper는 남기지 않는다.
원문 전체 snapshot·일회성 대조 자료는 Git 밖 `%APPDATA%\Accelerate\reports`에 둔다.

## 2026-09-29 규칙 객체 이관 checkpoint

[D-017](DECISIONS.md#d-017-모노레포를-위한-프로젝트-독립-객체형-어댑터-계약)은
Accelerate 전용 Rust 분기문을 공통 패키지로 옮기는 지시가 아니다. 다른 프로젝트도
사용할 **언어 독립 계약과 Rust 첫 구현체**를 계획하고, Accelerate의 기물 이동과
카드 효과는 기능별 객체를 통한 첫 소비 사례로 둔다. 현재 소스 경계와 다음 담당자의
작업 순서·수용 기준은 [이관 문서](RULE-ADAPTER-HANDOFF.md)에 정리했다.

카드 직접 효과의 좁은 배치는 `796a3da`로 커밋·공유하고 추가 카드 편집을 동결했다.
별도 source 대조에서는 seed 19 첫 일반 이동의 왕실 위협용 순서 후보 **9/9 배열**이
기존 Rust 생성 결과와 일치했지만, 가상 상태·관련 후보 실행 미검증 때문에 위협
경로를 열지 않았다. 종료 검사는 normal·chaos·grand 3/3 전체 상태/RNG가 일치했고,
무행동 검사는 normal·grand만 일치했다. chaos는 미지원 카드 `qxe1`에서 fail-closed다.
실제 도달한 Grappler 한 국면의 기본 이동 4개도 원문 순서와 일치하지만 포획·전역
제약·전체 이동 적용을 입증하지 않는다. 추가 조사 자료는 Git 밖 reports에 둔다.
이 이관은 공개 v7 `Position`의 import/legal/bind/apply 지원이나 코드 GO를 선언하지 않는다.

## D0 의미 정렬과 적용 상태 (2026-09-29)

아래는 [DECISIONS](DECISIONS.md#재설계-d0-의미-정렬-2026-09-29)의 채택 관계다.
결정의 변경과 코드 구현·검증은 별개이며, 아래의 과거 checkpoint 숫자는 당시 실행
범위에 한정한다.

| 결정 | 유지·수정·대체 | 이 계획에서 확인할 결과 |
|---|---|---|
| D-001·D-008 | 유지 | PR #29에서 완료한 고정 v7 사이트/JS 게임 어댑터를 correctness 기준으로 사용한다. Rust 탐색 규칙 실행으로 대체하지 않는다. |
| D-002 | ResNet 단독 최종 구조 지정을 대체 | 같은 허용 `ObservationIR`·public history·descriptor·candidate action에서 A: mask-aware ResNet과 B: entity Transformer를 비교한다. |
| D-005 | 역할 유지, 적용 계열 확장 | FiLM은 ONNX 내부 조건화, LoRA는 분리 적응이다. B의 LoRA 위치·rank 등은 실험별 설정에 고정한다. |
| D-007 | 유지 | Python-first encoding을 계속 사용한다. 실제 profiling으로 Python 인코딩 병목을 확인한 경우에만 Rust 이전을 재검토한다. O-001은 해결 상태다. |
| D-009 | 최종 대상 v7로 갱신 | 가변 보드·두 모델·대표 규칙의 기반 완료와 동결 v7 전체 규칙의 최종 GO를 구분한다. v6 호환은 유지하고 실제 학습은 제외한다. |
| D-016 | 새 설계 | 엔진 상태와 모델 입력 분리, 내부 보드 크기 상수 제거, 검증 전용 N-version, 자동 runtime 대체 금지. |
| D-017 | 새 설계 | 프로젝트 독립 언어 중립 어댑터 계약과 Rust 첫 구현체를 계획하고, 기물·카드별 객체는 Accelerate 소비 구현으로 분리한다. |

PR #29의 변경 파일에는 `rust-engine/`이 없다. 구현 완료는 게임 어댑터와 고정 v7
검증 계층에 대한 것이며, 아래 기록처럼 현재 Rust 엔진은 v7 Position을 실행하지
못한다. PR #29의 bounded 256카드×3모드 표면 조사는 전체 조합 규칙 coverage의
증거가 아니다. 완료된 adapter 재구현을 이번 작업에 추가하지 않는다.

## 채택 범위와 작업 순서

1. PR #29의 고정 v7 사이트 본체의 8×8 normal/chaos/grand, 공개 256 카드 catalog와
   별도 게임 어댑터를 최종 correctness 기준으로 유지한다. 최초 동결 v6는 호환 범위다.
   이미 완료한 adapter를
   재구현하지 않고, 검증하지 않은 조합·행동 경계를 원문과 비교한다.
2. 순수 Rust 독립 규칙 엔진에 전체 기물·카드·RULE·특수 행동·초기/draft/종료를 포팅하고
   client oracle과 normalized full state/result/RNG를 비교한다. worker useful filter를 legal로 쓰지 않는다.
   이 전체 목표 전에 geometry·단일 기물 상태·조합형 행마·대표 규칙을 연결한다.
3. bridge의 독립 PyO3 crate를 maturin으로 패키징하고 immutable 객체, 배열 소유권, 오류,
   snapshot roundtrip, JSON/direct 호출 의미와 GIL 경계를 검증한다.
4. Python에서 viewer 관측·공개 history·belief와 행동 의미 payload를 encoding한다.
   공통 `ObservationIR`에서 A/B의 입력을 만들고 raw full Position·private RNG·positionId가
   특징에 들어가지 않는 검사를 유지한다. Python 인코딩 병목의 실측 전에는 Rust로 옮기지 않는다.
5. 같은 공개 입력 계약 위에 A: 가변 mask-aware ResNet과 B: entity Transformer를 연결한다.
   FiLM 조건은 두 모델의 ONNX 입력/그래프에 남기고 static LoRA 병합은 원본을 보존한
   복사본에서 한다. B의 LoRA 적용 위치·rank 등은 모델별 실험 설정과 artifact에 고정한다.
   기본 ort와 명시 선택 tract의 입력 dtype·shape·유한값·수치 차이를 검사한다.
6. 실제 decision actor·확률·숨은 정보·공개 hint에 기반한 bounded ISMCTS, legal iterator와
   progressive widening, batch inference·취소·실행 예산을 구현한다.
7. bounded self-play/replay/CLI·dataset 기록·optimizer/evaluation checkpoint와 계약 version을
   연결한다. 이어 CI/독립 오류 경로 리뷰·전체 code GO checklist를 수행한다.

실제 학습·장시간 self-play·대전 성능·모델 승격은 이번 구현에서 제외한다. 작은 synthetic
optimizer/export와 bounded rollout은 구현 경로 검증이며 학습 성능으로 보고하지 않는다.
Hypernetwork는 향후 extension의 생성·적용·병합 가능 여부만 준비한다.

### 먼저 완료할 재설계 기반 checkpoint

이는 D-009의 전체 코드 GO를 대체하지 않는다. 기존 8×8 실행 경로는 보존하면서
보드 크기를 뜻하는 `8`·`7`·`64`의 좌표/순회/메모리 계산을 geometry에서 유도한다.
사이트의 초기 배치·홈/승격·캐슬링 조건과 모델 rank 등 다른 의미의 숫자는
기계적으로 치환하지 않는다. 새 조합형 Rust 경로는 명시 선택하고 실패나 미지원에서
기존 Rust 경로·JS oracle·Python 참조판으로 자동 전환하지 않는다.

| 단계 | 전달할 코드와 계약 | 기반 checkpoint의 증거 |
|---|---|---|
| P1 geometry·상태 | 크기·좌표·유효 칸·단일 기물/footprint·파생 점유, 붕괴와 별도 외곽 확장·축소 | 기존 8×8 결과 보존, 직사각형·보드 변경의 원자성·stale action 검사 |
| P2 조합형 규칙·N-version | `moveChunk`의 원래 출발점·형제 독립·부모 거리, 대표 카드·제약; 별도 독립 참조 구현 | JS 원문과 검증 가능한 부분의 legal/apply/state/RNG 비교, 참조판과의 불일치 설명 |
| P3 공개 IR·두 모델 | Python의 공통 허용 관측→plane/entity 입력, 같은 public history·descriptor·후보·value 관점 | 숨은 정보 비누출, empty/unknown/collapse/padding 구분, 순열·후보 분할·유한 출력 |
| P4 export·파인튜닝·탐색 | 모델 계열별 typed ONNX bundle, 두 backend, 기존 ResNet 호환 가중치 이전, 유한 탐색 | 실제 설치 wheel의 두 backend, 복사 병합·resume/warm-start 검사, 보드 변화 전후 실행 |
| P5 통합 | 같은 최종 커밋의 관련 검사와 미지원 항목 기록 | Windows/Linux 결과와 재설계 기반 완료를 확인; 전체 규칙 GO와 따로 보고 |

고정 v7 붕괴는 외곽 칸의 사용·기물 상태를 바꾸지만 외곽 배열 크기는 유지한다.
대국 중 실제 크기 변경은 별도 합성 profile에서 검증한다. 외곽 축소로 잘리는 기물·
footprint·링크·예약 효과에 지정된 처리 결과가 없으면 적용을 거부한다. 변경된 Position에
이전 행동·cursor를 재사용하지 않는다. 합성 크기 변경을 공식 사이트 카드의 의미
동등성 증거로 쓰지 않는다.

핵심 geometry·점유·행마의 독립 N-version은 테스트에서만 실행한다. Python 참조판은
운영 규칙 엔진이 아니고, 불일치를 다수결로 해결하지 않는다. PR #29의 JS 어댑터는
고정 사이트와 Rust의 correctness 비교용이다. 운영 탐색은 선택한 Rust 엔진과 선택한
ONNX backend 하나를 호출하고 실패·한도·미지원 오류를 그대로 전파한다.

기존 ResNet checkpoint는 동일 계약의 optimizer/RNG/cursor **resume**과 새 입력 계약으로의
**warm-start**를 구분한다. 후자는 원본을 보존하고 의미·shape가 맞는 가중치만 이전하며
새 입력·조건·행동층은 초기화한다. Transformer는 별도 base를 사용한다. 실제 학습 없이
작은 synthetic 갱신·저장/재개·원본 보존 검사만 수행한다.

## 관측한 checkpoint와 아직 필요한 증거

| 영역 | 실제 구현/관측 | 코드 완성·의미 coverage의 남은 조건 |
|---|---|---|
| 사이트·계약 | 동결 loader, v1 schema/JSON validators, 실제 client 초기/draft/전이/종료 adapter 구현; 관측 미분류 52개 원문 감사 완료, Deathmatch 경고 자격을 projection v3에 반영 | 전체 256 효과·선택 순서·특수 phase·lazy action 경계의 동등성 미완료 |
| Oracle 검사 | depth0 settlement·potion 정리·bounded terminal microtask·lazy premove·client 전용 loader, cold renderer·clock admission의 Node 계약 17개 통과; 시계 반례 16개 일치; profile v5의 원자적 새게임·replay 재캡처, profile v6의 관측 후 행동 3개 반례 통과 | 전체 catalog·populated browser의 future RNG 동등성은 별도 조건 |
| PR #29 v7 어댑터 | 공식 client hash 고정, 8×8 normal/chaos/grand 원문 실행·관측 검증, 256카드×3모드 bounded 표면 조사와 Ubuntu/Windows adapter CI 성공 | 어댑터 구현은 완료. 모든 카드 전제조건·조합의 `completeRuleCoverage` 및 Rust v7 포팅 증거는 아님 |
| 과거 fixture | 349개, 757 sampled action을 최신 worker에 비교 | worker hash 불일치; 최신 client 전체 정답을 대신할 수 없음 |
| Rust 규칙 | dcc0667과 default-flow의 기본 전이, Potion production 44개·양측 관측 88개, 별도 quiet/enPassant 12개(후속 이동 7개)·Quiet 지원 62개, 첫 수 자동 OPENING 세 모드의 전체 state/RNG/history 비교가 각 고정 소스에서 통과 | 검증하지 않은 나머지 획득·효과·변형기물과 catalog 전체 legal/reject/full-next-state/result/RNG 비교 미완료 |
| PyO3/maturin | dcc0667 Windows/Linux CI 설치 검사 각 34/34·skip 0; 5e17 정책·profile v6의 Guard 수정 Linux sdist→wheel 설치 foundation 19/19·CI helper 34/34·skip 0, 정책 JCS·wheel/import 일치 | 새 정책의 최종 두 OS 배포 검증 필요 |
| Python 모델·encoding | envelope v2·projection v3·16개 EncoderSpec 필드·네 surface 정책; BFB 정책의 source 관측 20개×양측 전체 JCS/key 일치, 52개 감사 종료, replay 전체 trace 관측 검증; 첫 수 복구 필드의 후속 5e17 정책을 설치 wheel에서 확인 | 새 정책의 전체 규칙 실행·모델 입력 통합과 최종 두 OS 검증 필요 |
| ort/tract | dcc0667 Windows/Linux에서 두 backend·두 history 계약·full 8x128 base/adapter 48개 비교 통과; 5e17 정책의 Linux 재export도 설치본 48개 비교 통과하고 Guard 수정본의 실제 ort 세 모드 bounded 실행 완료 | 새 정책의 최종 두 OS 확인과 전체 규칙 coverage 필요 |
| ISMCTS·replay·CLI | BFB+Potion2 및 후속 5e17/v6 Guard 수정 고정 wheel에서 각각 weighted search/session 15/15·skip 0과 세 모드 bounded 실제 ort 실행 통과 | 전체 hidden chance·카드·변형기물 coverage와 최종 두 OS 검증 필요 |
| CI | dcc0667의 구조·기존 engine·historical JS와 Windows/Linux native workflow 전체 성공; 실제 설치 검사 각 34/34·skip 0 | 후속 default-flow·weighted 탐색·관측 revision과 전체 규칙을 완성한 최종 공통 commit 검증 필요 |

### 2026-09-28 통합 checkpoint

[PR #29](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/29)의
고정 v7 게임 어댑터는 일반 merge commit `f7d7d896`으로 [PR #28](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/pull/28)에
세 원본 커밋 그대로 들어갔다. PR #28은 `develop` 대상 draft로 유지한다. 이 head의
[원격 CI](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/36402517741)에서
v7 어댑터는 Ubuntu·Windows 모두 성공했다. 256장·768개 bounded probe cell 중 27개는
후보가 없고 모든 shard가 `completeRuleCoverage=false`다. 같은 CI의 native 설치 검사는
각 OS 44개 중 1개 실패했다. Ubuntu는 CHAOS belief의 5초 한도, Windows는 동일 JSON
대상에 대한 동시 `os.replace` 테스트의 `WinError 5`였다. 이 실행을 전체 통합 성공으로
표시하지 않는다.

후속 구현은 공개 관측 인코더와 ONNX manifest에 고정 v6·v7 규칙/정책 조합을 명시하고
혼합 입력을 거부한다. 실제 v7 어댑터의 normal·chaos·grand 양측 관측 6개는 별도 로컬
실행에서 인코딩했고, 작은 v7 ONNX 모델은 ort·tract 양쪽에서 수치 검사를 통과했다.
저장된 단위 테스트의 v7 관측은 v6 입력의 메타데이터를 바꾼 synthetic 사례이므로 이
수동 6개 관측이나 전체 규칙 동등성의 자동 회귀 증거로 취급하지 않는다. 로컬 고정
sdist에서 세 crate를 다시 빌드한 Linux wheel의 native/model/runtime/search/session
검사는 47/47 통과했다. 이 로컬 결과는 최종 head의 양 OS 원격 CI와 별개다.

고정 v6 원문과 독립 대조한 seed 12345의 세 모드 상태 8개 중 draft 6개는 합법 행동
집합 67개가 일치했다. play 2개는 Rust의 `fanatic` 미지원으로 비교가 중단됐고, draft
적용에서도 5개 시도 중 2개만 전체 state·RNG·history가 일치했다. 세 모드의 새 게임
초기 full state는 draft/replay frame/clock 차이로 불일치한다. 이 부분 결과를 전체
규칙 parity나 v7 실행 근거로 확대하지 않는다.

Rust의 기본 실행은 계속 v6이며 v7 Position 생성은 명시적으로 거부한다. 현재 Portal
Gun은 직접 행동 결합·적용과 예약 전이 일부가 있지만 행동 스트림이 미지원이다. 고정 v6
원문은 포탈 두 칸의 양방향 클릭을 모두 허용하고 결과가 달라지는 반면, 병합된 어댑터의
후보 열거는 순서 없는 조합만 내므로 이 경계의 수정과 원문 대조가 남아 있다. v7 규칙
포팅, 전체 카드/이동·상태·RNG 동등성, 최종 두 OS 통합 검증 전까지 판정은 **NO-GO**다.

다른 영역의 작은 test count나 compile 성공은 해당 checkpoint이며 전체 GO로 승격하지 않는다.
CI 요청/관측한 성공, Windows/Linux 확인, 모델 export 수치, 실제 semantic coverage를 따로 기록한다.

Linux sdist에서 만든 wheel 설치 검사 결과는 native 6 + model 7 + runtime 6 = 19개 통과,
skip 0이다. full history와 summary history의 public-intent 모델을 각각 24개 사례로 검사했고
최대 절대 오차는 둘 다 1.2218952178955078e-6이다(`atol=1e-5`, `rtol=1e-4`). 보고서는
`%APPDATA%\Accelerate\reports\native-bot\Linux`에 보관한다. 이 배포 경로와 수치 검사는
현재 native 규칙 경계의 checkpoint이며 전체 카드 의미 coverage의 증거는 아니다.

공유 커밋 804d9f4의 원격 native CI에서도 Windows/Linux 각각 19개 검사(skip 0)가
통과했다. full/summary public-intent의 full ResNet 수치 비교 48개에서 최대 절대 오차는
Linux 1.043081283569336e-6, Windows 1.1324882507324219e-6이다. 이후 두 job 모두
현재 사이트의 `aiWorker.js`와 동결 원본의 hash가 달라 다운로드 gate에서 실패했다.
본체 oracle 검사는 그 실행에서 시작되지 않았으며 workflow 전체를 성공으로 표시하지 않는다.

후속 공유 커밋 f41e45d의 native CI 실행 36336831808은 Windows 2025와 Ubuntu 24.04에서
전체 성공했다. 각 OS에서 실제 sdist→설치 wheel의 34개 검사(skip 0)를 실행했으며
Linux 44.798초, Windows 50.626초다. 두 history 계약의 full ResNet·두 backend 48개
비교에서 최대 절대 오차는 Linux 1.0356307029724121e-6, Windows 1.2218952178955078e-6이다.
동결 main과 pinned parser를 실제 실행 의존성으로 검사했고 원래 worker/index는
executed:false provenance로 보존한다. 이는 전체 baseline을 다시 동결하거나 worker
로딩의 원래 hash 검사를 제거한 변경이 아니다. 원격 보고서는 Git 밖의
`%APPDATA%\Accelerate\reports\full-stack-implementation\ci-f41`에 보관한다.
34개 검사의 normal/chaos 범위는 초기 역조건화와 첫 draft/posterior까지이며 실제 CLI는
명시 draftDelete:true를 사용했다. 이 성공을 전체 draft→play나 256개 효과의 완성으로 확대하지 않는다.

후속 2B 기본 이동 checkpoint는 동결 client의 실제 getLegalMoves와 Rust base_moves를
657개 구성 사례에서 전체 JSON 값으로 비교해 순서·중첩 선택·실행 flags까지 일치했다.
양색 기본 이동 288개, 기억 이동 186개, trickster 171개, 명시 이동 mode 12개이며
글로벌 modifier를 비활성화한 세 원점의 kernel 검사다. 실제 효과 실행·글로벌 legality·
지원 등록의 완료 근거는 아니다. Rust 1.96 focused 4개와 strict clippy를 통과했고
임시 비교 검사는 소스에서 제거했다. 재현 입력·script·source/module hash는 Git 밖
`%APPDATA%\Accelerate\reports\variant-movement\checkpoint.json`에 기록한다.
전체 상태 검증에서는 moveReplay를 후속 카드가 읽는 경로와 2x2 기물의 snapshot 복원
alias를 확인하고 있다. raw replay/notation 필드를 이름만으로 제외하거나 객체 identity가
깨진 oracle의 결과를 정답으로 사용하지 않는다.

진행 중인 후속 규칙 checkpoint에서 담당자가 Rust 1.96의 library 34개와 bounded CLI 1개,
strict lint 통과를 확인했다. common 이동 6개는 raw full next state·RNG·history를 맞췄고,
opening 8개는 카드 정의의 name/text/art만 정규화해 비교했다. 54개 targeted 카드 정의의
primitive 111개에서는 승인 107개의 상태·RNG·history, UI 후보 111개, 거부 4개를 비교했다.
이 비교는 UI 후보를 먼저 실행한 동일 oracle의 renderer cache가 남아 있는 환경이다.
후속 fresh-realm 비교에서 grappler의 animatedPieceIds가 달라져, 독립 스냅샷만으로
전체 상태 전이를 재현한다는 증거로 사용하지 않는다.
이는 production wrapper의 finishCard·replay 정산이나 전체 256개 효과의 완료가 아니다.
wrapper 검사에서 남은 full-state 차이와 Unsupported는 담당자가 실패·미완료로 유지한다.
54개 정의의 검증된 source는 외부 checkpoint로 보존했고 다음 카드 구현을 같은 담당자가 진행한다.
이후 대형 기물 exile·전령의 queued endGame 정산을 연결한 생성 비교 167개에서는 승인 143개의
전체 상태·RNG·history가 같은 reused-oracle 환경에서 일치했다. 완성 UI 행동·첫 클릭 후보·불변 검증은 각각 167개가
일치했고 정상 거부 23개와 원문 예외 1개는 구분했다. 이 primitive 경계의 성공을 미완료
production wrapper·global modifier 조합이나 전체 카드 지원으로 확대하지 않는다.

공개 관측은 envelope v2를 유지하고 source-visible projection v3로 확장했다. 기물 상태·보드 표시·관계 schema와 정책 hash를
Python encoder·replay·ONNX metadata·Rust runtime에 연결한다. root의 얇은
`site_observation_policy()` API는 Rust 1.96 compile·fmt·strict lint를 통과했고, 설치 wheel의
정책을 실제 sdist와 JCS로 비교하는 CI smoke도 추가했다. 당시 최종 projection과 encoder를 함께
고정한 sdist→wheel의 설치 검사는 수행 전이었으며, 후속 로컬 검사는 아래에 별도로 기록한다.
담당자의 중간 정책 비교에서는 60개 source 상태×양측 viewer의 공개 관측 120개가 정규화 없이
전체 JCS·informationStateKey까지 일치했다. 별도로 실제 createPieceElement의 표시·배지·설명
88개를 확인했고 owner-only 정보와 전체 공개 count를 보존했다. 이후 정책 보강분을 포함한
최종 hash·projection 재검사를 마친 뒤 공통 코드 checkpoint로 묶는다.

root의 별도 thin API 후보 검사에서는 캡처한 sdist를 Linux release wheel로 빌드해 독립
Python 환경에 설치했다. 정책의 sdist/compiled JCS 일치·owned copy·명시 encoder 생성·계약
복원을 확인했고, 기존 실제 runtime 검사 6개도 84.889초에 통과했다(skip/error/failure 0).
두 history 계약의 8x128·rank/alpha 8 base/비영 LoRA 복사본 병합 모델을 ort/tract에서 비교한
48개 사례의 최대 절대 오차는 1.2218952178955078e-6이다. 소형 모델의 48개 사례도 별도로 통과했다.
이 후보는 당시의 3개 surface schema 정책이며 이후 overlay·공개 카드 결과 확장은 포함하지 않는다.
최종 관측 의미·정책 hash·Windows 배포·전체 규칙 검증은 계속 미완료다. 후보의 미사용 helper
경고 4개도 최종 lint 통과로 처리하지 않았다. 원본 archive/wheel/test source SHA와 JUnit·수치
요약은 Git 밖 `reports/full-stack-implementation/policy-api-candidate-packaging.json`에 보관한다.
같은 설치 후보에서 보강한 패키징 smoke도 실행했다. Python 모듈 10개는 sdist·wheel·설치 파일의
SHA가 모두 일치했고 실제로 import한 native 바이너리도 wheel member와 일치했다. 보고서는
`policy-api-candidate-installed-wheel.json`이며 최종 4-schema 정책의 배포 증거로 확대하지 않는다.

대형 기물 exile의 복원 실패는 source 결함과 adapter 검사를 분리해 조사했다. 담당자의
실제 원문 relink/normalize 실행은 disconnected 4개 cell의 JCS·RNG와 객체 alias를 보존했고,
추가 geometry guard가 있던 adapter만 거부했다. snapshot admission의 2x2 가정을 제거하고
same-frame 전체 속성 일관성·ID·frame independence를 유지하는 수정을 적용했다. 기존 Node
검사 17개가 모두 통과했고 실제 원문의 bigRook/Bishop exile→normalize→restore→nullification
두 경로는 전체 상태·RNG 동등성을 확인했다. 이 검사는 관측 v2의 최종 정책 완료와 구분한다.
정상 배치 legality는 규칙 helper의 책임이며, 이 복원 거부를 원문 결함이나 지원 제외 근거로 쓰지 않는다.

후속 관측 v2 checkpoint는 policy JCS SHA
`14fd1c134efd56ed66fa9bd5fc1966e3a8fbcd88a75ce1bd5cb79242cd1c7388`로 고정했다.
139개 raw public value schema와 기물 상태·보드 표시·관계·overlay의 네 surface schema를
연결했고 실제 공개 카드 결과와 전체 설명 count를 보존했다. 트롤리의 시간·난수 window ID는
공개 관측·intent·history에서 제외하고 실행 Action에 보존한다. 기존 Node 17개, Windows
모델·탐색·세션 pure 검사 15개와 실제 renderer 88개를 확인했다. Windows의 15개 실행에서는
native 7개를 의도적으로 제외했으며 Windows 설치 wheel의 통합 성공으로 보고하지 않는다.

검증된 core·모델 소스를 캡처해 만든 최종 Linux sdist→설치 wheel에서도 정책·catalog JCS,
Python 모듈 10개와 native 바이너리의 배포 일치를 확인했다. 실제 Rust 1.96 workspace
fmt·all-target strict clippy와 36개 library + 1개 CLI 검사가 통과했고, 설치 wheel의 통합
34개 검사도 49.85초에 통과했다(skip/error/failure 0). 두 history 계약의 full ResNet과 실제
ort/tract의 base/비영 LoRA 복사본 병합 모델 비교 48개에서 최대 절대 오차는
1.2218952178955078e-6이다. 공개 관측의 최종 source 비교 130개도 전체 JCS·key가 일치했다.
sdist SHA는 `517f29f68efc39739bb939c7d3862e089f0ea26238fcbadbce5eb456689d2cfb`,
wheel SHA는 `6650a8b5ae3e0413c89cdec9135e417ce973068153a842d981c3b965c7f04c7c`다.
원본·검사 입력 SHA와 보고서는 Git 밖 `reports/full-stack-implementation/policy-v2-*`에 있다.
로컬 offline ONNX 링크 실패와 Python 개발용 링크 누락은 성공과 분리해 남겼고, 잠금된
의존성과 소유 빌드 캐시의 기존 Python runtime 링크로 같은 source 검사를 완료했다.

이 source를 공유한 ba8a1ad94edbe774358734e5d1bd16fcfcec2f96의
[원격 native CI 36346564006](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/36346564006)은
Windows 2025·Ubuntu 24.04 모두 전체 성공했다. 각 OS의 설치 wheel 검사는 34/34·skip 0이고
Linux 58.548초, Windows 63.691초였다. 각 OS에서 두 history 계약의 full ResNet·ort/tract
48개 비교가 통과했고 최대 절대 오차는 Linux 1.043081283569336e-6,
Windows 1.2218952178955078e-6이다. installed Python 모듈 10개·native 바이너리·관측 v2
정책의 배포 일치와 동결 client/parser 단계까지 통과했다. 구조 정책·기존 engine CI와
historical JS harness도 성공했으며, 이 증거는 아래 후속 conditioned-search 소스와 구분한다.

이 checkpoint의 targeted 카드 primitive 승인 143개는 reused-oracle의 renderer cache가
남아 있는 비교 환경에서 전체 상태·RNG·history가 일치했다. 원문의 activePieceAnimationUntil
module Map은 serialized snapshot에 없으며, 동일 snapshot·RNG로도 fresh realm과 재사용한
realm의 animatedPieceIds가 달라진다. 별도 fresh-source 대표 12개 중 baby-bear·grappler·
judgment 계열 6개가 이 한계를 보였고 RNG는 12개 모두 일치했다. 이는 finishCard만의
문제로 분류하지 않으며, 명시적인 실행 context와 full-state 재검증이 필요한 미완료 경계다.
실제 production wrapper의 별도 outpost 두 경우와 monochrome-off siren은 3개 모두 일치했다.
후속 17-family wrapper에서는 16개만 전체 상태가 일치했고 baby-bear의 animatedPieceIds 차이
1개를 미완료로 유지한다. colossus·활성 monochrome의 Unsupported도 통과와 구분한다.
최종 공개 관측 정책의 52개 audit 항목, 전체 256 효과·27 RULE·variant 실행·기본 세 모드의
전체 흐름은 아직 미완료다. 설치 검사의 성공을 전체 코드 GO로 확대하지 않는다.

후속 conditioned-search의 별도 Linux sdist→설치 wheel은 최소 Rust 1.96 fmt·strict clippy·
workspace 36+1, installed provenance와 기존 native 6개(새 stale/seed/conversion 경계 포함)를
통과했다. 실제 search/session 15개는 12개 통과·3개 실패·skip 0이었다. session 4개와
draftDelete 3개는 통과했고 default normal은 두 선택 후 play 관측까지 도달했으나 다음
선택자의 hidden offer 조건화가 미지원이었다. chaos는 첫 묶음의 source proposal 밀도가
미지원이고 grand는 11개 선택·양측 posterior를 통과한 뒤 마지막 선택의 democracy 획득
효과에서 멈췄다. 예산·skip/xfail로 완료 조건을 낮추지 않는다.
초기 hidden White offer의 source/proposal 확률 보정과 새로운 공개 Black draw의 likelihood는
별도 경계다. StepResult만 반환하는 조건부 draw 복원은 public compatibility 증거이며,
latent별 draw 분포의 차이를 보정하는 완전한 posterior의 증거로 사용하지 않는다.
명시적인 weighted conditional-step metadata와 source trace 밀도 구현을 이어서 검증한다.

후속 cold20 캡처는 성공한 oracle admission에서 원문의 renderer Map만 비우는
headless semantic v2를 기준으로 한다. 원래 224개 생성 사례 중 source 승인 187개에서
지원한 161개는 전체 state·RNG·history가 일치했고 26개 emergency 경로는 Unsupported로
남았다. 정상 거부 36개와 원문 예외 1개는 별도로 기록했다. global modifier를 비활성화한
20개 확장 효과의 production wrapper 23개와 독립 fresh/reused 대표 12개도 전체 상태와
RNG를 맞췄다. 74개 local effect ID가 구현되어 있으나 전체 lifecycle·RULE 지원 수는 아니다.
immutable 원본과 일회성 비교 데이터는 `reports/full-stack-implementation/card-effects-checkpoint/cold20`에 있다.

그 core와 weighted filter v3·네 native 조건화 helper를 고정한 별도 Linux sdist→설치 wheel은
Rust 1.96 fmt·strict clippy·workspace 37+1과 native 6개를 통과했다. installed Python 모듈 10개·
native 바이너리·정책 JCS의 원본 일치도 확인했다. wheel SHA는
`661e5e9badd4676dbb99702eac5cd112dad237f7813bca1a1aa0c0e71aadcac3`이다.
실제 search/session 검사 15개는 13개 통과·2개 실패·skip 0, 41.895초였다.
기본 normal의 두 획득·양측 posterior·첫 play와 session 4개는 통과했고, chaos hidden-offer
density와 grand의 두 번째 White→Black shared-pool 조건화에서 실패했다. 기존 예산을 유지했다.

같은 설치 wheel에서 고정된 관측 v2 summary 계약의 full 8x128 ONNX를 실제 Rust ort로
실행했다. normal의 두 draft 선택 뒤 4 iteration·depth 1·최대 64 후보·5초 예산으로 첫 play
intent를 선택하고, 실제 bind/apply·양측 공개 posterior·미완료 replay 왕복을 2.86초에 확인했다.
학습 label은 만들지 않았으며, 이는 한 제한된 normal 흐름의 증거다. chaos/grand의 미지원
전파와 미완료 replay 보존은 별도 확인했지만 두 모드의 NN play 완료로 세지 않는다.
이후 실제 무관찰 normal draw의 p=q<1 metadata와 grand shared-pool guard를 고정 source에서
수정한 별도 wheel도 fmt·strict clippy·workspace 37+1·native 6개·배포 일치를 통과했다.
wheel SHA는 `f1cfdbf1c33b62c54009eb1b26f2b47a6161d0afaa4cb8bef308bb75f2a37089`다.
같은 예산의 search/session은 12개 통과·3개 실패·skip 0, 27.723초였다. grand guard는
해소되어 마지막 선택의 democracy 효과까지 진행했다. normal은 NN의 a2→a4 Move 선택과
실제 적용까지 성공했으나 후속 posterior가 숨은 Card 후보의 미지원 밀도에서 실패했다.
chaos는 처음 선택한 묶음 이후 새 Black offer를 White viewer에게 조건화하는 밀도가 미지원이다.
이 설치본의 normal 완료로 이전 wheel의 성공을 재사용하지 않는다. 원본 wheel과 실패
보고서는 덮어쓰지 않고, 정확한 행동과 공개 경계를 규칙 담당자가 계속 검증한다.
이 증거는 finite LCG seed posterior나 브라우저 future RNG의 정확한 재현을 뜻하지 않는다.

원격 공유용 `cold-card-native-checkpoint`는 검증된 cold profile·core·native API를
유지하면서 windmill·bribe·queens-gambit·chain 네 local handler를 함께 캡처했다.
네 효과의 별도 source 35개 중 승인 21개의 전체 state·RNG·history와 정상 거부 14개의
불변 상태, UI/첫 클릭/검증 35개가 일치했다. local effect ID는 78개이며 새 네 효과의
wrapper·expiry·이동 제한 lifecycle은 완료 수에 포함하지 않는다. 해당 모듈만 고정 core에
교체해도 같은 비교가 통과했고 다른 미완료 규칙 코드에 의존하지 않았다.
Python search와 test_search는 원격 ba8a1ad의 기존 구현·검사를 그대로 유지한다.
실패 중인 후속 weighted v3 소스와 완료 gate는 live 작업 및 immutable 캡처에 보존했으며
삭제·완화하지 않는다. 이 두 탐색 버전의 검증 범위는 서로 대체할 수 없다.

이 공유 단위의 최소 Rust 1.96 workspace fmt·strict clippy·37+1 검사와 실제
sdist→설치 wheel의 기존 CI helper 검사 34/34가 통과했다(skip/error/failure 0,
46.824초). full ResNet·두 history·ort/tract 비교 48개 최대 절대 오차는
1.2218952178955078e-6이다. sdist SHA는
`6b7e48ee73521569d2718a99631da7d4e81777c850cbea56252f00049474f96c`,
wheel SHA는 `da408dc25476c647863abde52337cfa0c98109ab5dd9ab05959a0cac851b542d`다.
보고서는 Git 밖 `reports/full-stack-implementation/cold-card-native-checkpoint`에 둔다.
root runner가 없는 test_session_pipeline.py 경로를 지정해 pytest 시작 전에 실패한 기록도
보존했다. 실제 캡처한 CI helper가 기존 test_session.py를 포함해 검사했고 빌드는 재실행하지 않았다.
공유 커밋 dcc06671d26b1e72e3680acaba9be57cccee0dc5의
[원격 native CI 36350746198](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/36350746198)은
Windows 2025·Ubuntu 24.04 모두 전체 성공했다. 설치 검사 각 34/34·skip 0이며 Linux
60.152초, Windows 40.137초였다. 각 OS의 full 모델 수치 비교 48개 최대 절대 오차는
Linux 1.043081283569336e-6, Windows 1.1324882507324219e-6이다. Python 모듈 10개·
native 바이너리·정책 배포 일치와 frozen client/parser, 구조·기존 engine·historical JS도
성공했다. 원격 보고서는 `reports/full-stack-implementation/ci-dcc`에 보존하며 후속
weighted v3·새 관측 revision의 증거와 구분한다. 전체 코드 판정은 계속 NO-GO다.

후속 default-flow 고정 소스는 grand6 효과와 기존 관측 정책 `14fd1c...`을 사용한다.
최소 Rust 1.96의 37 library + CLI 1개, fmt·strict clippy가 통과했고, 동결 client의
normal 2·chaos 2·grand 12개 전이가 전체 next state·RNG·history와 일치해 play에 도달했다.
비교에서 카드 정의의 name/text/art만 정규화하고 missing/null 차이를 보존했다.
양측 viewer의 관측 6개도 일치했다. 이 고정 소스로 weighted v3 탐색을 포함한 별도
sdist→wheel 검증을 진행했다. 설치된 세 모드의 기본 draft와 양측 posterior는 완료했으며,
이어지는 전체 기본 play gate의 성공은 아래 결과처럼 미완료다.
Potion과 새 관측 revision은 이 고정 단위에 포함하지 않는다. 증거와 고정 source manifest는
Git 밖 `reports/full-stack-implementation/default-flow-weighted-checkpoint`에 보존한다.

별도 Linux wheel `0f7e7f2c7df0d339c33b68826235c138fa841ecce71f7e6f09f56323b996272c`의
설치 foundation 검사는 19/19·skip/error/failure 0, 37.872초였다. full 모델·두 history·
ort/tract 48개 비교의 최대 절대 오차는 1.2218952178955078e-6이다. 같은 설치 패키지의
search/session은 13개 통과·2개 실패·skip 0, JUnit 32.081초였다. 실제 8x128 모델·Rust ort로
normal의 draft 두 전이와 play 이동을 선택·실행한 뒤 양측 posterior 전체 frame 및
미완료 replay roundtrip/examples 0을 확인했다. chaos도 draft 두 전이, grand도 draft
열두 전이와 양측 posterior까지 완료했으나 다음 실제 탐색에서 각각 suspiciousPotion과
enPassantBang Unsupported로 멈췄다. 기존 gate와 실행 예산은 완화하지 않았다.
이 설치 결과는 기존 정책 `14fd1c...`와 고정 source에 대한 것이며, 새 관측 정책이나
후속 Potion 구현의 성공이 아니다. 실제 archive·설치 Python/native SHA와 source 확률
metadata·JUnit·각 mode phase는 `reports/full-stack-implementation/default-flow-weighted-native`에 있다.

관측 정책의 미분류 52개는 실제 renderer·hint·status 경로에 따라 공개 파생 33·내부
기록 13·소유자/private 2·미지원 context 2·presentation 2개로 분류했다. 확인된 누락은
로컬 Deathmatch 경고 자격이다. 경고 toast가 사라진 뒤 남는 DOM 문구와 dedup cache는
snapshot으로 재구성할 수 없으므로 별도 revision은 원문에서 계산한 `active`·`warning`
두 bool을 전달한다. `warning`은 현재 원문 경고 조건이며 미래 종료·승패 예측이 아니다.
동결 규칙·catalog와 tensor capacity를 유지하면서 projection/policy hash를 함께 갱신하고
기존 artifact를 명시적으로 거부한다. source 감사 보고서는 `reports/visibility-audit`에 두며,
분류 감사 완료와 새 정책의 코드·패키지 검증을 구분한다.

새 정책 `bfb5c0a3...`와 headless profile v3, Potion2 고정 소스를 합친 203개 파일의
Linux 패키지 checkpoint를 별도로 검증했다. sdist `b413d509...`, wheel `b06b2b98...`의
실제 SHA와 설치 출처를 확인했고 fmt·strict clippy·workspace 검사, 설치 foundation
19/19·skip/error/failure 0을 통과했다. 새 정책으로 다시 export한 full 8x128 ResNet의
두 history·ort/tract 48개 비교 최대 절대 오차는 1.2218952178955078e-6이다. 동일한
설치 wheel에서 기존 search/session 15/15·skip 0을 통과했다. 새 정책과 encoder hash가
포함된 ONNX 모델을 사용한 bounded Rust ort 실행은 normal·chaos·grand 모두 기본 draft부터
play의 NN 선택·실제 bind/apply, 양측 posterior 전체 공개 frame, 미완료 replay examples 0까지
통과했다. 실행 한도는 search 4/depth 1/candidates 64/5초, particles 2/proposals 16,
최대 16 step/30초이며 actual learning은 없다. source·archive·JUnit·mode 결과는 Git 밖
`reports/full-stack-implementation/public-contract-v3-native`와
`reports/full-stack-implementation/public-contract-v3-verification`에 있다. 새 source의
enPassant 후속 이동, Target4 wrapper, 전체 256 카드와 변형기물 실행은 이 bounded 경로에서
검증되지 않았다. 새 정책의 Windows 원격 CI도 아직 실행하지 않았다.

후속 headless profile v6, 첫 수 자동 OPENING 전이와 `firstMoveUndo` 내부 분류 정책
`5e17b562...`를 결합한 203파일 고정 소스는 별도 Linux 설치 시도에서 Rust fmt·strict
clippy·workspace 검사, maturin sdist→wheel 설치, foundation 19/19·skip 0, full 8x128
ort/tract·두 history 수치 비교 48개를 통과했다. 최대 절대 오차는
1.2218952178955078e-6이며 동결 oracle/runtime Node 계약 17개도 통과했다. 그러나 같은
설치 wheel의 필수 search/session은 13/15·skip 0이었다. CHAOS의 belief 재구성은 기존
5초 한도를 초과했고 GRAND는 첫 이동 후 흑색 posterior의 `guard` 후보에서 미지원
weighted chance trace가 전파됐다. 실제 ort 실행은 normal·chaos의 draft→play·양측
posterior·미완료 replay까지 통과했지만 grand는 같은 후보에서 중단됐다. 검사 예산을
완화하거나 실패를 건너뛰지 않았다. 고정 source manifest, archive SHA, JUnit과 자세한
단계별 결과는 Git 밖 `reports/full-stack-implementation/opening-v6-native` 및
`reports/full-stack-implementation/opening-v6-verification`의 attempt1 기록에 있으며,
이 시도를 완료된 통합 checkpoint 또는 전체 코드 GO로 계산하지 않는다.

후속 Guard 수정은 `guard` 단독 카드 사용이 white 턴·moveCount 0을 유지하는 반면
첫 폰 이동과 함께 자동 사용되면 black 턴·moveCount 1이 되는 원문 차이를 확인하고,
불가능한 단독 카드 후보만 공개 턴 조건으로 제거한다. Guard 효과 전체의 weighted
chance density를 추정하거나 일괄 허용하지 않는다. 이 수정과 profile v6·5e17 정책을
합친 최종 Linux 고정 소스는 Rust fmt·strict Clippy·workspace 검사, maturin
sdist `e699232d...`→wheel `3203ded4...` 설치, foundation 19/19·skip 0을 통과했다.
설치한 full 8x128 ResNet의 두 history·ort/tract 비교 48개 최대 절대 오차는
1.2218952178955078e-6이다. 같은 wheel의 필수 search/session은 기존 한도에서
15/15·skip 0, 실제 Rust ort는 normal·chaos·grand 모두 draft→play·양측 공개 posterior·
미완료 replay까지 완료했다. 캡처한 CI helper 전체 범위도 34/34·skip 0이다. 문서만
최종 검사 결과에 맞춰 갱신했으며 wheel을 만든 실행 소스의 코드 바이트는 그대로다.
고정 소스와 설치·JUnit·mode 보고서는 Git 밖
`reports/full-stack-implementation/opening-v6-guard-final-native` 및
`reports/full-stack-implementation/opening-v6-guard-final-verification`에 있다. 전체 256 카드·
변형기물 의미 coverage는 별도 남은 조건이다.

커밋 `5d3ff9c`의 [원격 native CI](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/36375077487)는
두 OS 모두 Rust 검사·sdist→wheel 설치까지 통과했지만 설치본 테스트에서 실패했다.
Ubuntu는 34개 중 CHAOS belief 재구성 1개, Windows는 CHAOS·GRAND 재구성 2개가
기존 5초 한도에 걸렸다. 각각 33/34, 32/34이며 오류·skip은 0이다. 같은 커밋의
repository policy·engine·historical JS workflow는 성공했다. 다운로드한 양측 JUnit과
실패 분류는 Git 밖 `reports/full-stack-implementation/ci-5d3ff9c`에 보존했다.
따라서 로컬 34/34 통과를 원격 성공으로 승격하지 않고, 한도나 검사를 완화하지 않은
성능 수정과 두 OS 재검증을 진행한다.

후속 성능 수정은 동결 draft 가중치의 첫 ID 일치 결과를 읽기 전용으로 캐시하고,
native 드래프트 스트림에서 이미 현재 Position에 묶인 Action의 중복 재바인딩을
생략한다. 일반 이동·카드·synthetic factory는 기존 경로를 사용한다. 캐시는 100개
seed의 NORMAL·CHAOS 전체 GameState가 수정 전후 일치했고, 드래프트 Action 282개는
재바인딩 payload와 모두 같았다. 두 수정만 `5d3ff9c`에 overlay한 고정 소스에서
maturin sdist `892c4926...`→wheel `7b415230...` 설치·정책 검증과 CI 범위 설치본
34/34·skip 0이 통과했다. 기존 5초 한도의 CHAOS·GRAND 사례는 추가 3회씩 모두
통과했다. 이 결과는 Linux 로컬 검사이며 최종 커밋의 Windows/Linux 원격 CI와
전체 규칙·카드 coverage를 대신하지 않는다. 고정 source manifest와 JUnit·packaging
보고서는 Git 밖 `reports/full-stack-implementation/ci-perf-v1`에 보존한다.
실제 `85854c1`의 [두 OS native CI](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/36378523934)는
wheel 설치까지 성공했지만 Ubuntu 33/34(CHAOS), Windows 32/34(CHAOS·GRAND)에서
같은 5초 belief 한도를 초과했다. skip은 0이며 policy·engine·historical JS workflow는
성공했다. 실패 JUnit은 Git 밖 `reports/full-stack-implementation/ci-85854c1`에
보존했다. 추가 native 성능 수정과 두 OS 재검증이 필요하다.

다음 공개 계약 수정은 완료된 replay 결정의 `chosen_intent`를 해당 actor trace의
`ownIntent`와 대조하고, Python ONNX manifest loader도 Rust와 같은 2 MiB 한도를
읽기 전에 검사하며 실제 읽기를 `2 MiB + 1` 바이트로 제한한다. 상대 viewer에게
숨겨진 의도와 실행 전 중단된 결정은 계속 허용한다. `85854c1` 코드에 이 두 수정만
추가한 고정 소스의 maturin sdist `113cbd09...`→wheel `d1259935...`를 설치해
CI 범위 34/34·skip 0을 확인했다. 거대 manifest fixture와 실제 학습은 만들지 않았다.
이 Linux 로컬 설치 검사는 최종 커밋의 두 OS CI와 전체 코드 GO를 대신하지 않으며,
보고서는 Git 밖 `reports/full-stack-implementation/ci-contract-v1`에 보존한다.

추가 고정 성능 후보는 불변 `Position`의 양쪽 공개 관측을 전이 후보 사이에 재사용하되,
상태가 바뀌면 캐시를 새로 만든다. 전이의 공개 history를 양 viewer에 각각 반영하고
키를 다시 계산한다. 검증 중 빈 승자 값이나 기물 ID가 정규화되면 전이 전 관측 캐시를
버리고 검증된 상태에서 재투영한다. 고정 소스의 Rust 42/42·fmt·strict Clippy와
100개 seed의 전체 상태·가중치 동등성을 확인했다. 이 코드의 maturin sdist
`bbd41a36...`→wheel `d7d7dfd5...` 설치본은 기존 CI 범위 34/34·skip 0,
CHAOS·GRAND 기본 사례 추가 3회씩 모두 통과했다. 정확한 소스 목록과 JUnit은 Git 밖
`reports/full-stack-implementation/ci-perf-v5`에 고정했다. 로컬 Windows에서는 같은
후보보다 이전 고정 소스의 sdist까지 생성했으나, 이 호스트의 긴 MSVC 출력 경로와
애플리케이션 제어 정책으로 wheel·설치 검사를 실행하지 못했다. 이를 코드 실패나
Windows 원격 CI 결과로 계산하지 않는다. 실제 커밋 `1aff4b3`의
[원격 native CI](https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-/actions/runs/36384650640)는
Ubuntu 설치본 34/34·skip 0으로 성공했고 Windows는 wheel 설치 후 32/34·skip 0으로
실패했다. Windows 실패 두 건은 CHAOS·GRAND의 기존 5초 belief 재구성 한도 초과이며,
각 테스트 전체 시간 12.830초·10.670초를 한도 검사 순간의 경과 시간으로 해석하지
않는다. 같은 head의 policy·engine·JS/Rust differential workflow는 성공했다. 양측
JUnit과 분류는 Git 밖 `reports/full-stack-implementation/ci-1aff4b3`에 보존했다.
최종 두 OS native 성공과 전체 규칙·카드 coverage는 별도 완료 조건이며 실제 학습은
시작하지 않았다.

## 구현 연결 순서와 완료 기준

작업별 담당자는 하나로 유지하고 공통 계약은 담당자끼리 확인한다. 다음 순서는 의존 관계이며,
각 단계의 통과를 전체 구현 완료로 보고하지 않는다.
이번 구현 세션에서 외부 어댑터는 검증 시간이 길어지는 경우 작업 하나에 최대 두 담당자를
할당할 수 있다. 구현과 독립 검증의 소유 범위를 먼저 나누고 같은 파일을 동시에 수정하지 않는다.
규칙 검증은 규칙 담당자가 진행하며 주 담당자는 계약·완료 조건을 조율하고 다른 구현을 병행한다.

Rust 규칙 작업은 기능 경계로 2A·2B·2C를 나누어 각 담당자 하나를 둔다. 2A는 초기·draft·
phase·공통 legality·효과 실행·모듈 연결을 담당한다. 2B는 variant 기물의 기본 이동과
이동 payload 생성만 `variant_movement.rs`에 구현한다. 공통 이동 파일·상태·전이의 소유권은
2A에 유지하고, 2B의 반환 flag를 실제 전이에서 처리한 뒤에만 해당 기물을 지원 목록에 넣는다.
기능별 책임 분리이며 파일 크기를 기준으로 추가 파일을 만들지는 않는다.
2C는 targeted 기물 변환과 속성 부여 카드의 target 생성·검증·상태 변경을
`card_effects.rs`에 함께 구현한다. 사용 상태·notation RNG·턴 종료·공통 정산은 2A가 연결하고,
변환된 기물의 기본 이동은 2B와 실제 source flag 의미를 확인한다.

| 작업 | 전달할 구현 | 완료를 판단하는 코드 증거 |
|---|---|---|
| 1. 동결 source와 oracle | 실제 client 실행, public projection, 정확한 legal iterator, queued settlement | rule에 영향을 주는 UI 정리와 RNG를 보존하고 동결 source에서 legal/reject/state/result/RNG 비교가 재현됨 |
| 2. 순수 Rust 규칙 | 초기·draft·256 카드·RULE·84개 catalog type과 reachable 상태 전이 | 채택된 8x8 normal/chaos/grand의 reachable 기능에 Unsupported나 대체 구현이 남지 않고 source 비교가 통과함 |
| 3. Python 연동 | immutable Position/Action, owned 배열, direct/JSON 경계, maturin sdist/wheel | Windows/Linux에서 실제 배포 wheel을 설치하고 동일 의미·오류·소유권을 확인함 |
| 4. 인코딩과 모델 | Python 공통 `ObservationIR`·공개 이력·descriptor·후보 행동, A mask-aware ResNet/B entity Transformer, 계열별 static LoRA·FiLM·checkpoint/export | 같은 허용 의미 정보, 숨은 상태 불변성, 계열별 adapter·복사 병합, 명시적 condition 입력과 계약 hash를 확인함 |
| 5. Rust 추론 | 기본 ort, 명시적 tract, 계열별 typed 입력과 strict artifact 검증 | 두 실제 backend에서 base/adapter와 B/A/H/W/N의 지원 shape·dtype, FP32 오차·condition 효과·오류 전파를 확인함 |
| 6. 공개 정보 탐색 | public trace, source-conditioned particles, availability PUCT, bounded search | 세 mode에서 실제 native 규칙과 연결되고 private 환경 상태·seed 없이 선택·재구성·취소가 작동함 |
| 7. 실행과 자료 보존 | 유한 CLI, replay/dataset, optimizer/RNG checkpoint, 평가 코드 | 작은 synthetic 검증으로 중단·복원·미완료 판정·version 경계를 확인하고 실학습을 시작하지 않음 |
| 8. 통합 검증 | 구조 검사, Rust lint/test, wheel 설치, source parity와 오류 경로 리뷰 | 최종 공통 commit에서 Windows/Linux CI 성공과 전체 미완료 항목의 해소를 관측함 |

우선 oracle의 의미 보존과 Rust의 public-conditioned 초기화·행동 의도 경계를 완성해 탐색에 연결한다.
규칙 전체 포팅과 병행해 모델/추론의 확정 계약 검사를 마친 다음 replay·CLI와 최종 CI를 연결한다.
중간 단계에서 미지원 기능을 명시적으로 거부하는 것은 허용하지만, 이를 전체 지원이나 GO로
보고하지 않는다. 검사 개수나 문서 채택, workflow 요청만으로 GO를 내리지 않는다.

카드별 큰 snapshot을 영구 추가하는 방식으로 coverage를 늘리지 않는다. 작은 공통 시나리오와
프로그램으로 만든 선택·오류 입력을 재사용하고 대규모 비교 결과는 외부 reports에 보관한다.

### 공개 선택과 모델 계약

실제 Action의 의미 payload는 실행·직렬화에서 손실 없이 보존한다. 탐색과 봇용 모델은 source UI에서
선택할 수 있는 `public-decision-intent-v1`을 사용한다. 숨은 occupant에 따라 달라지는 capture flag나
실행 정보는 `Position.bind_public_intent`에서 해결하고 탐색 입력으로 되돌리지 않는다.
실제 환경 Position으로 탐색 후보를 사전 필터링하거나 Python에서 임의로 flag를 제거하지 않는다.

EncoderSpec은 history와 action 정책을 각각 명시한다. 봇용 기본은
`public-history-summary-v1`과 `public-decision-intent-v1`이다. summary는 전체 public history의
event count·JCS digest·actor/change 집계와 최근 8개 event를 담고 원본 trace/replay는 보존한다.
full history 및 exact execution payload용 모델은 별도 계약 hash를 가지며 자동 호환으로 취급하지 않는다.
탐색의 value 시점은 물리적인 turn 대신 실제 decision actor의 `observation.viewer`를 따른다.

## 동결 source와 재현

이 절의 2026-09-27 source·profile·검사 수치는 v6 호환 경로의 역사적 근거다.
이번 최종 GO의 v7 source는 [최신 어댑터 검증](ADAPTER-VERIFICATION.md)의
`main-OahWs0tU.js`/`e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`와
`augment-site-20260928-e5ed84fcf8e72a24`를 사용한다. 두 동결본을 서로의 규칙 동등성
증거로 대체하지 않는다.

동결 시각은 **2026-09-27T14:37:10.842Z**다. 날짜가 바뀌어도 이 source를 재동결하지 않는다.
규칙 버전은 augment-site-20260927-abfe01a035813875, 공개 catalog hash는
yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4다.

| 원문 | URL | SHA-256 |
|---|---|---|
| index | https://augmentchess.org/ | 4b541aa23cd0f763b0835091cd044b8f3d5b4bd009f7c5bcfb4f7ce29b02fe0c |
| client | https://augmentchess.org/assets/main-CqkYwJX4.js | abfe01a035813875772d8eeaf8e300a1df0348888ff48778d4a1789b76ae492f |
| worker | https://augmentchess.org/assets/aiWorker.js | 930746c09446dea014d346a06fe86ddc4c1decc9749e7ec40c035af9bd5c0c52 |

원문은 `%APPDATA%\Accelerate\cache\site-baseline` 고정 slot에만 보관한다.
CI는 `$RUNNER_TEMP/Accelerate/cache/site-baseline`, 다른 OS는 명시적 경로를 사용할 수 있다.
`frozen-site.js freeze`는 기존 manifest를 덮어쓰거나 최신 변경을 자동 흡수하지 않고 검증한다.
최초 bootstrap용 Acorn 8.15.0 단일 파일도 이 외부 cache에 놓고 SHA
fdb08546776ec6228b03e8d02b40d4ab3255bae5f401adba7ff5dad927ac5c9c로 검사한다.
parser와 raw 사이트 번들을 source에 vendoring하지 않는다. 오프라인 실행은 외부 network와
background timer를 막고 presentation hooks를 분리한다. VM 자체를 보안 sandbox라고 주장하지 않는다.

본체 oracle은 `loadMain`을 실행하므로 CI의 실행 의존 파일은 위 동결 client와 pinned parser다.
CI는 별도 고정 slot `site-baseline-client`에 이 둘의 원래 hash·bytes·동결 시각을 검사한다.
index와 worker는 최초 발견 시점의 provenance를 보존하고 현재 hash·크기·변경 여부를
`executed: false` 보고서로 분리한다. 현재 HTML의 새 main URL이나 현재 worker를 실행하지 않는다.
기존 전체 `site-baseline` cache를 덮어쓰지 않으며, 전체 baseline 검사와 `loadWorker`는
원래 worker가 있어야 통과한다. client 전용 cache에서 worker 비교를 요청하면 명시적으로 실패한다.

오라클의 현재 실행 profile은 `accelerate-headless-semantic-v6`다. 성공한 restore/newGame
admission에서 snapshot 밖의 activePieceAnimationUntil renderer Map과 clockDisplayAnchor를
cold 초기화한다. action 내부·queued settlement 중에는 두 context를 유지하며 복원 실패는
state·RNG·context와 callback을 보존한다. 시계 반례 16개에서 fresh/reused 및 후보 조회
유무의 전체 상태·history·RNG가 일치하고 기존 Node 계약 17개가 통과했다. v1의 warm-cache와
v2의 renderer-only 증거는 각 profile의 범위로 유지한다. 정책의 renderer88 당시 v1 evidence도
덮어쓰지 않는다. v4의 새 게임은 별도 동결 source VM에서 `draftDeleteEnabled`를 원문 초기화
전에 설정하고 snapshot 성공 후 VM·RNG callback을 원자적으로 교체한다. 실패하면 이전 상태와
두 context·callback을 보존한다. v5는 `starWinLimit`와 `deathmatchLimitTurns`를 반영한 뒤
원문 helper로 초기 replay 기본 frame과 tail을 재캡처하며, 예상 밖 초기 history/event 형태는
staging 실패로 처리한다. 기존 v3 wheel의 실제 ONNX 검증에는 v4·v5가 포함되지 않는다.
v6는 성공한 restore에서 원문의 localViewColor·boardViewColor를 되돌려, 같은 football
Position이 `observe(black)` 뒤에도 White의 킥 3개를 유지하도록 한다. 이 Node 반례의
통과는 변형기물 전체 행동·전이의 완료 증거가 아니다.
원래 `renderAll`이 수행하는
`pruneBoardPotionEffects`를 실행하고 render-local simulation depth를 즉시 복구한다.
종료 rule ticket 정리와 원래 queued replay settlement를 보존하며 snapshot 전에 최대 256개
microtask를 FIFO로 처리한다. 이전 position을 복원하면 그 position의 미실행 callback을 버린다.
조건부 notation ID 생성의 실제 RNG 소비도 보존한다. 순수 renderer라고 추정해 전체 hook을
무조건 no-op으로 처리하지 않는다.

DOM animation·update-log의 난수와 render probe/UI 상태, editor UI·timer·network persistence는
이 profile에서 제외한다. grand insertion은 원래 null-ghost reveal 경로를 실행한다.
`browserFutureRngEquality: false`를 명시하고 저장된 상태의 난수 ID를 정규화하지 않는다.
비교는 이 명시 headless profile을 양측에 적용하며 모든 renderer의 전이적 순수성이나
populated browser의 미래 RNG까지 증명했다고 보고하지 않는다. seed 11의 실제 초기 cursor는
normal 122, chaos 212, grand 112다. trolley와 black-box availability predicate가 각각
14 draw를 소비하는 것처럼 규칙 availability에 필요한 source RNG는 표시용 RNG와 함께 제거하지 않는다.

core22와 최종 Python 연결의 별도 sdist→설치 wheel에서는 native 6 + search/session 15개가
통과했다(skip·제외 0). 이어 같은 설치 wheel에서 새 CI의 통합 검사 34개가 52.186초에
통과했고 full ResNet의 두 history 계약·두 실제 backend 수치 비교 48개도 통과했다.
normal/chaos는 초기 공개 조건화와 첫 draft 선택·posterior 동기화를
검사했다. CLI는 명시 `draftDelete: true`에서 실제 Rust ort의 4 leaf batch 탐색과 1 ply
미완료 replay, tract 평가, 명시적 artifact 선택, 취소·전체 deadline 경계를 확인했다.
SIGINT는 exit 130, 전체 selfplay/train deadline은 exit 2로 전달하며 완료된 탐색 뒤 취소되면
pending decision을 보존하고 환경 전이를 실행하지 않는다. 개별 search 시간 예산의 정상
종료와 전체 작업 deadline은 구분한다. synthetic optimizer 1 step과 RNG·shuffle·optimizer
복원 검사는 학습 캠페인이 아니다. 세 default mode의 전체 게임 통합 증거로 확장하지 않는다.

실제 본체 resetGame/beginInitialGameFlow는 초기 259개 root field를 만든다. 여러 최신 card 효과는
root field를 lazily 추가하므로 초기 기본값만으로 전체 field 목록이 완성되지는 않는다.
`bridge/catalog/`에는 공개 256 metadata, semantic CARD_DEFS 257(shotgun-king 보조 정의 포함),
initial defaults, draft weights/exclusive sets, 명시적 관측 정책을 보관한다. 큰 art/text/원문은 넣지 않는다.

## 실제 규칙·visibility 근거

[공식 규칙](https://augmentchess.org/rules/)과 동결 client가 기본 근거다. 체크메이트 대신 킹
포획·행동 불능과 별 비교 종료를 사용한다. normal은 각 3개 offer에서 1장, chaos는 2장 묶음
3개에서 1묶음을 초기·중간·종반에 고르며 grand는 공통 28장 중 양측 6장씩 초기 선택한다.
실제 source resetGame(65813), beginInitialGameFlow(66150), startDraft(66960),
createGrandDraftPool/startGrandDraft(66676/66725), completeDraftStep(67011),
getLegalMoves/finalizeLegalMoves(95533/95888), endGame(109677)를 실행한다.

collectValidAiActions(83093)의 useful move 제한과 collectAiCardTargets(83721)의 target truncation은
AI 선택 전략이다. 오라클은 이를 전체 rules로 취급하지 않고 UI/rule helpers로 선택 표면을
보완하며 실제 applyCard/movePiece/finishCard로 수용 여부를 확인한다.
aiSimulationDepth는 실제 전이에 **0**을 사용한다. 1로 설정하면 applyPassiveCardOnDraft가
획득 passive를 생략하므로 이전 depth1 검사 결과는 최종 의미 증거에서 제외했다.

상대 acquired library는 renderOpponentLibrary→compareLibrarySlots→playerDeck에서 공개다.
pieceVisibleToColorAt/visiblePieceType은 hiddenFrom·camouflage·fog·hallucination을 투영한다.
getVisibleCardTargetSquares는 숨은 상대 occupant를 표시 target에서 제외하고,
moveHighlightKeys/visibleMoveCells는 실제 UI highlight cell을 반환한다.
full worker 행동의 capture flags·private 기물 identity를 공개 후보로 복사하지 않는다.
renderCaptureList는 양측 공개 capture type/color를 마지막 12개 표시한다. trolley의 choice.pieces는
타입/색을 양측 UI에 보여준다. viewer가 알고 있는 premove 순서/좌표는 owner 관측에 유지한다.

관측 나머지 field는 무조건 private로 분류하지 않는다. 직접 공개·viewer projection·비공개 또는
viewer 제한·presentation/다른 mode·visibilityReviewPending을 구분한다. 현재 85개 pending field에
대한 renderer/정보 시점 검토가 남아 있으며 이 수치는 관측 의미 완성의 증거가 아니다.

## 검사 결과와 coverage gaps

검사 명령은 다음과 같다. offline 통합 검사는 채택된 외부 baseline이 없으면 skip하지 않고 실패한다.

```text
node --test bridge/tools/runtime-contract.test.js infra/tools/site-parity/offline-oracle.test.js
node bridge/tools/validate.js
node infra/tools/site-parity/compare-frozen-fixtures.js
node infra/tools/site-parity/audit-card-surface.js
```

현재 checkpoint의 depth0 계약/통합 검사는 14개 통과, skip 0이다. 3종 실제 draft lifecycle, passive
settlement, 초기 20개 이동/공개 hint, RNG snapshot 복원, wrong actor 거부, private RNG/hidden
기물 변화에 대한 관측 불변성, 실제 킹 포획 terminal을 검사했다. schema 예시는 18 valid+6 invalid,
전체 8 schemas를 검사한다. 변경된 strict public event·safe integer/depth/node 경계도 기존 검사에 보탰다.
potion cleanup, terminal chain 정리·replay 기록·conditional notation RNG·중복/stale microtask와
callback 한도는 기존 검사 파일의 작은 생성 입력으로 확인했다. 전체 게임 fixture를 추가하지 않았다.

foundation native CI는 Windows 2025와 Ubuntu 24.04에서 Rust 1.96의
`clippy::nonminimal_bool`에 실패해 wheel/backend 단계가 실행되지 않았다. 로컬 Rust 1.97의
성공을 minimum toolchain의 성공으로 간주하지 않는다. 기존 differential workflow의 Cargo.toml
자동 탐지는 존재하지 않는 placeholder binary를 실행했으므로 원래 JS 서버를 쓰는
`Historical JS fixture harness`로 범위를 명시했다. 이 harness의 300개 성공은 과거 fixture와
JS harness 검증이며 실제 동결 client↔Rust 정답 비교 gate를 대체하지 않는다.

후속 core checkpoint는 실제 Rust 1.96의 단위 검사 21개, strict clippy와 formatting이
통과했다. source의 availability predicate와 weighted draw를 포팅해 normal/chaos 각각
seed 0·1·11·37·71·91에서 초기 offer의 종류·순서·instance ID와 RNG 전체가 일치했다.
chaos seed 0의 금지 묶음 교체는 cursor 242, 다른 검사 chaos는 212, normal은 122다.
seed 11의 두 모드·양측 viewer 공개 frame 4개도 JCS 및 informationStateKey까지 일치했다.
이는 초기 draw 경계의 증거이며 선택 뒤 효과, 전체 private state 또는 전체 catalog의 증거가 아니다.
JCS 기준으로 같은 공개 숫자가 `0.0`/`0`으로 직렬화돼도 conditioning과 native snapshot이
같은 의미로 복원되도록 비교하며 실제 의미·shape·필드가 바뀐 입력은 계속 거부한다.
검증한 소스를 캡처한 sdist에서 만든 Rust 1.96 Linux wheel의 native 검사 6개도 통과했고
skip은 없다. root가 같은 캡처 소스의 workspace 전체에 실행한 Rust 1.96 strict clippy와
formatting도 통과했다. 진행 중인 live checkout의 이후 변경이나 Windows 검증으로 확대하지 않는다.

과거 fixture는 349개/757 sampled action, 행동 목록 340/349, 행동 수용 757/757,
result 753/757, full state 0/757이다. 신규 revolvingDoorGuard/september27CopyPools 같은 root
field drift를 제거해 성공으로 표시하지 않았다. 기존 239 효과와 최신 256 catalog 사이에 17개
미관측 효과가 있고 초기/draft/public history/RNG tape가 빠져 있다. 보고서는 historical-reference-only다.

256개 card surface probe는 초기 표준 보드에서 카드당 앞 2개 후보만 적용한다. 첫 실행은
228개에 후보가 있었고 신규 동적 field의 미분류 오류 및 premove의 100000 후보 한도 초과를
발견했다. 후속 실행은 245개 후보와 premove 예산/neutral wall 두 경계 오류를 기록했다. 확인된 공개 owner flag와 taboo square marker 및 actual neutral wall은 정책에 보완했다. 이 probe는 전체
카드 규칙의 정답 비교나 모든 유효 선택의 coverage를 증명하지 않는다. 최신 실행 보고서를 따른다.

premove는 1..3개 distinct piece 계획을 **제출 순서대로** resolvePendingFreeMovesAfterTurn(93413)가
실행한다. 조합만 열거하면 순서를 잃으므로 순열을 보존한다. 전체 materialization이 한도를 넘으면
명시 오류를 내며 정확한 lazy iterator/progressive widening이 후속 구현 조건이다.
다른 multi-target 카드의 순서 동등성과 모든 특수 window도 추가 비교가 필요하다.

조사 report는 `%APPDATA%\Accelerate\reports\full-stack-site`의 고정 파일로 교체 보관한다.
raw 로그·큰 새 fixture·데이터셋을 Git에 누적하지 않는다. source engine을 단순 fixture lookup,
공통 compile 결과나 signature 비교로 대체하지 않는다.
