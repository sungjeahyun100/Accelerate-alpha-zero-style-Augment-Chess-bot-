# 프로젝트 독립 객체형 어댑터 계약과 Accelerate 이관

이 문서는 [D-017](DECISIONS.md#d-017-모노레포를-위한-프로젝트-독립-객체형-어댑터-계약)의 다음 작업자용 계약·현황 기록이다. 사용자 결정은 **언어 독립 계약 + Rust 첫 구현체**, **공통 인터페이스 + 기능별 규칙 객체**다. 기물 이동과 카드 효과는 Accelerate에서 이 계약을 사용하는 첫 사례이지 공유 어댑터의 도메인 정의가 아니다. 이 저장소를 지금 모노레포로 이전하거나 새 최상위 패키지를 만들지 않는다.

## 공유 계약

공유 층은 객체 식별·등록, 계약 버전 협상, capability 발견, 호출 수명, 유한 자원, 취소, 결과·오류 전달을 담당한다. 프로젝트가 입력/출력 schema와 상태·행동·시간·RNG의 의미, 객체의 실제 규칙, 트랜잭션 저장 방법을 제공한다. 공유 타입·wire schema에 Accelerate의 `GameState`, `CardSlot`, `Piece`, `Square`, `moveChunk`, 8×8, 카드 ID, 턴 정산을 넣지 않는다. 추론용 LoRA adapter와 PR #29의 JS correctness adapter도 별도 책임이다.

언어 독립 v1은 다음 객체의 의미를 먼저 고정한다. JSON은 설명 가능한 초기 wire 표현이지만 Rust 내부 상태 형식이나 영구 codec으로 강제하지 않는다. schema ID와 canonical hash를 향후 공유 패키지에서 버전과 함께 발행한다.

| 객체 | 공유 필드·의미 | 프로젝트가 채울 부분 |
|---|---|---|
| `AdapterDescriptor` | `contractVersion`, 충돌 없는 `adapterId`, `implementationVersion`, capability 목록, 요청/응답 schema ID·hash, 결정성·side-effect 선언, 자원 한도 | 프로젝트/rules/source/catalog 버전과 규칙 provenance |
| `AdapterRequest` | request ID, adapter ID, capability/operation, 불투명 snapshot revision, 한도·취소/deadline, schema에 맞는 payload | 선택 대상·행동·도메인 context |
| `AdapterResponse` | 성공 여부, 결과 schema 버전, 순서 있는 결과, 해당 시 `examined`와 cursor/`exhausted` | 후보·효과·event의 타입과 해석 |
| `AdapterError` | `unsupported`, `invalid_input`, `stale_revision`, `limit_exceeded`, `cancelled`, `execution_failed` 구분 | 도메인 오류 코드와 안전한 진단 |
| `AdapterRegistry` | `(projectId, adapterId, contract major, implementationVersion)`의 명시 선택; 중복·미등록 거부 | 버전별 객체 등록과 호출 정책 |

객체는 불변 설정으로 만들고 호출 간 임의의 전역 상태를 공유하지 않는다. RNG·시계·외부 상태는 project host가 소유권과 버전이 명확한 context로 주입한다. 결정성은 해당 capability가 실제 보장할 때만 선언한다. state 변경은 host가 가진 복제본/트랜잭션에서 실행하고 성공 시에만 원자적으로 커밋한다. 실패·취소·한도 초과 때 원본 state/RNG/history/event가 변하지 않아야 한다. 모르는 버전·capability·schema 또는 미구현 규칙을 기본 구현으로 대체하지 않는다.

`enumerate`, `validate`, `execute`는 Accelerate 규칙 객체가 우선 사용할 capability이며 모든 객체의 필수 메서드는 아니다. `enumerate`는 원문 순서와 페이지의 `examined`/`exhausted`를 보존하고 예산 초과를 성공한 전체 목록으로 위장하지 않는다. `validate`는 앞 페이지에 행동이 있었는지만 확인하는 함수가 아니라 해당 snapshot에 대한 전체 의미 검증이다. `execute`는 행동과 revision을 다시 결속해 프로젝트가 정의한 직접 효과만 수행한다. UI 힌트, 원시 실행 후보, AI 공개 의도, 왕실 위협용 내부 후보는 서로 다른 capability/결과 schema다. 내부 기물 ID·숨은 capture flag·RNG를 모델 입력에 복사하지 않는다.

Rust 첫 구현체는 객체 안전한 진입점과 프로젝트별 typed wrapper를 분리한다. 다음 코드는 인터페이스 **형태**의 스케치이며 crate API나 패키지 경로를 확정하지 않는다.

```rust
trait AdapterObject: Send + Sync {
    fn descriptor(&self) -> &AdapterDescriptor;
    fn invoke(&self, request: &RequestEnvelope, call: &CallLimits)
        -> Result<ResponseEnvelope, AdapterError>;
}
```

공유 코어가 프로젝트 상태를 `Any`로 downcast하거나 JSON 값을 게임 상태로 소유하지 않는다. Accelerate wrapper가 versioned envelope를 `GameState`/`Action`/규칙 context로 변환하고, project host의 snapshot·transaction·RNG·event 서비스를 호출한다. 정적 registry로 시작할 수 있으며 동적 플러그인 ABI는 요구가 생기기 전까지 약속하지 않는다. 규칙 객체마다 파일 하나를 만들지 않고 변경 이유와 소유권이 같은 구현을 함께 둔다.

## 현재 Accelerate 코드와 이관 경계

| 책임 | 현재 코드 | 이전 시 보존할 계약 |
|---|---|---|
| 상태·공개 API | `state.rs`의 source DTO, `spatial_state.rs`의 가변 geometry, `lib.rs`의 `Position` | source DTO와 단일 identity·파생 점유를 모델 plane으로 혼동하지 않는다. 공개 v7 import/legal/bind/apply는 아직 닫혀 있다. |
| 이동 후보 | `movement.rs::piece_moves`, `variant_movement.rs::base_moves`, `movement.rs::ActionCursor` | 기물별 객체는 기본 후보·flag를 맡는다. 전역 modifier·강제 기물·순서/예산은 별도 조정층이 맡는다. |
| 조합형 이동 | `move_program.rs`의 `MoveBoard`, `MoveProgramSet`, raw provenance·bounded cursor | 현재 별도 spatial 경로다. 모든 v7 이동을 자동 대체하거나 내부 `piece_id`를 공개 의도로 내보내지 않는다. |
| 카드 | `card_registry.rs`의 정의/instance/정책, `card_effects.rs`의 ID 우선 plan·후보/검증/직접 효과 | 정의와 손패 instance, UI 선택과 효과 수용, 수동/강제/패시브 실행을 구별한다. 직접 효과 객체가 사용 비용·턴 정산을 임의 결정하지 않는다. |
| 실행·투영 | `transition.rs`의 move/card 적용·턴/phase·replay, `threat.rs`의 내부 조회, `observation.rs`의 공개 투영 | actor·stale·원자성·RNG·history·result는 Accelerate 프로젝트 host의 실행 흐름 책임이다. 위협용 `AiNoCards`와 public legal/hints를 합치지 않는다. |

`CardRegistry`가 256개 공개 v7 정의와 보조 정의를 파싱한다는 사실은 256개 효과가 실행된다는 뜻이 아니다. 카드 `symmetry`·`ice-sheet`·`quantum-mechanics`의 국소 직접 효과는 커밋 `796a3da`로 공유·동결됐다. seed 19 첫 플레이 세 모드의 전체 ordered 후보와 양측 Observation v2 여섯 개는 **제한된 내부 상태**에서 원문과 일치하지만, 공개 v7 `Position`은 여전히 `UnsupportedFeature`다. 첫 일반 수의 왕실 위협 후보는 원문 대조 9/9 배열이 일치해도 가상 상태·관련성·후속 포획 실행 계약이 없어 위협 경로를 열지 않았다. Chaos 무행동 검사는 미지원 카드 `qxe1`에서 닫힌다.

첫 일반 이동 `a2→a3` 뒤의 비행동자 White 관측은 세 모드에서 원문과 일치하지만, 행동자 Black의 공개 힌트는 아직 닫혀 있다. 이를 열려면 이동 구현이 `getVisibleMoveSquares`의 **표시 순서와 출발점별 목적지 묶음**을, 카드 구현이 `getVisibleCards` 표시 순서와 `getVisibleCardTargetSquares`의 **첫 선택 대상 묶음**을 각각 제공해야 한다. 빈 대상 묶음은 의미가 있을 때 보존하고 대상 없는 카드를 임의로 힌트에 추가하지 않는다. 관측 계층은 이 두 투영을 조합하고 공개 정책을 검증한다. 전체 legal action이나 실행용 카드 후보를 UI 힌트로 바꿔 쓰지 않는다. 왕실 위협은 별도의 순서·출처가 보존된 내부 `AiNoCards` 후보와 검증된 포획 실행을 요구한다. 이 세 소비 계약은 서로 대체할 수 없으며, 현재 모두 공개 v7 실행을 여는 근거가 아니다.

프로젝트 host는 후보를 AI 입력에 넘기기 전에 source-valid 공개 intent의 정확한 필드와
순서 있는 선택 대상을 검증해야 한다. `TypedEncoder.encode`는 의미 특징 추출기이며,
카드 후보의 추가 필드까지 거부하는 최종 행동 schema 검증기가 아니다. 공개
`ObservationIR`도 `from_public`의 원본 서명·visibility-policy 검사를 거친 출력으로만
사용한다. 투영된 IR을 직접 구성·변경하면 원본 관측 전체를 다시 검증할 수 없다.

## 다음 작업자의 순서와 소유권

1. **공유 계약 담당자 한 명**이 서로 다른 가짜 프로젝트 타입 두 개로 descriptor/envelope/error/capability의 schema·버전 협상·취소/한도·중복 등록을 검증한다. Accelerate 타입을 공유 패키지에 import하지 않는다. 실제 모노레포 경로·패키지명은 구조가 정해진 뒤 선택한다.
2. **Rust 구현체 담당자 한 명**이 객체 registry와 invocation 경계를 만든다. 객체 수명, thread safety, 호출별 context, 원자적 state 변경을 검증한다. 언어 독립 계약과 Rust 구현을 같은 의미 사례로 대조한다.
3. **Accelerate 이동 담당자 한 명**이 source 검증 기물의 기본 후보를 객체로 감싼다. 기존 ordered payload·flag·cursor와 v6 호환을 먼저 대조한다. 정통 첫 플레이, Relay, Grappler 등 변형 기물과 왕실 위협 내부 조회는 각각 별도 단계이며 이때 일반 v7 공개 gate를 열지 않는다.
4. **Accelerate 카드 담당자 한 명**이 source 검증 효과 하나를 객체로 감싼다. registry 정의·instance·사용 비용/정산은 host에 남기고 UI 후보·직접 효과·자동 발동을 구별한다. 현재 카드 효과 추가는 동결됐으므로 다음 담당자에게 넘긴다.
5. **통합 담당자 한 명**이 `Position` legal/bind/apply와 observation, native binding, 검색 연결을 순차 검증한다. 객체를 등록했다는 이유만으로 공개 실행을 허용하지 않는다. source legal/reject/full state/RNG/history/result 및 최종 동일 SHA의 Windows/Linux 설치 wheel로 지원 범위를 확정한다.

같은 기능/파일에 두 구현자를 동시에 배치하지 않는다. `lib.rs`·`transition.rs`처럼 공통 호출 순서를 바꾸는 파일은 인터페이스가 안정된 뒤 통합 담당자가 수정한다. N-version과 동결 JS oracle은 검증용이고 운영 fallback이 아니다. 카드별 거대 snapshot fixture를 쌓지 않고 기존 작은 사례와 저장소 밖 보고서를 이용한다.

## 증거와 아직 정할 것

- [IMPLEMENTATION-DIRECTIVES](IMPLEMENTATION-DIRECTIVES.md)의 P0/P2/P3/P8 계약과 [ENGINEERING-STANDARDS](ENGINEERING-STANDARDS.md)를 먼저 읽는다. 동결 v7 client SHA-256은 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`다.
- 첫 수 전후 상태·RNG, 왕실 위협 후보 순서와 미완료 경계는 Git 밖 `%APPDATA%/Accelerate/reports/v7-plain-move/`, 첫 이동 뒤 양측 공개 관측은 `%APPDATA%/Accelerate/reports/v7-postmove-observation/`, flow 국소 비교는 `%APPDATA%/Accelerate/reports/v7-delta-ledger/first-move-flow-direct.json`, Grappler 국소 원문은 `%APPDATA%/Accelerate/reports/v7-variant-grappler/`에 있다. 이 호스트의 보고서는 다른 작업자에게 따로 전달해야 한다.
- 공유 schema ID·canonical form, 두 번째 실제 프로젝트 소비 사례, 모노레포 패키지 경로·release/ABI 정책은 아직 확정되지 않았다. 임의 값으로 고정하거나 현재 저장소의 `bridge/`를 공유 패키지라고 선언하지 않는다.
- 기물·카드 객체로 이전한 뒤에도 source별 RNG 소모, 자동 효과 순서, raw 후보와 공개 의도, 위협·UI·실행 context 차이가 남는다. 각 capability의 성공을 선언하기 전 직접 비교한다.

이 문서는 어댑터 구현이나 v7 코드 GO의 증거가 아니다. 실제 학습은 완료 조건에 포함하지 않는다.
