# 프로젝트 독립 객체형 어댑터 계약과 Accelerate 이관

이 문서는 [D-017](DECISIONS.md#d-017-모노레포를-위한-프로젝트-독립-객체형-어댑터-계약)의 계약·이관 현황이다. 사용자 결정은 **언어 독립 계약 + Rust 첫 구현체**, **공통 인터페이스 + 기능별 규칙 객체**다. 기물 이동과 카드 효과는 첫 소비 사례이지 공유 어댑터의 도메인 정의가 아니다. 현재 작업 브랜치에서는 모노레포 경로 이전과 v7 규칙 이관이 진행 중이며, 부분 구현·국소 검사 성공을 전체 v7 실행 완료로 취급하지 않는다.

## 이번 PR #32 전달 범위

사용자는 봇 연동·성능 측정을 진행하지 않고 PR #32에 push한 뒤 이번 전달 목표를
완료하도록 범위를 조정했다. 구현·bounded 규칙/공통 계약 검증·구조 검사·커밋/push와
원격 SHA 확인이 현재 범위다. 설치 wheel·실제 Python 봇 연동·성능 측정은 사용자 범위
조정으로 미실행·미검증이며 후속 검증이 필요하다. 기존 검사 코드는 보존하되 실행
성공으로 기록하지 않는다. CI dispatch와 관측한 CI 상태도 구분하며 범위를 늘리지 않는다.

## 공유 계약

공유 층은 객체 식별·등록, 계약 버전 협상, capability 발견, 호출 수명, 유한 자원, 취소, 결과·오류 전달을 담당한다. 프로젝트가 입력/출력 schema와 상태·행동·시간·RNG의 의미, 객체의 실제 규칙, 트랜잭션 저장 방법을 제공한다. 공유 타입·wire schema에 Accelerate의 `GameState`, `CardSlot`, `Piece`, `Square`, `moveChunk`, 8×8, 카드 ID, 턴 정산을 넣지 않는다. 추론용 LoRA adapter와 PR #29의 JS correctness adapter도 별도 책임이다.

언어 독립 v1의 실제 wire schema와 canonical SHA-256은 `packages/adapter-contract/`에 발행한다. JSON은 wire 표현이며 Rust 내부 상태 형식이나 영구 codec으로 강제하지 않는다. `packages/adapter-runtime/`은 같은 의미의 typed Rust registry와 호출 경계를 제공한다.

공유 Rust crate는 `edition`과 최소 Rust 버전을 자체 manifest에 고정해 workspace 설정 없이 패키징·검증할 수 있다. 공유 wire 계약도 별도 npm tarball에 schema·manifest·검증기를 함께 넣을 수 있다. 로컬 `cargo package`의 독립 빌드와 오프라인 `npm pack --dry-run`은 확인했지만, npm 패키지는 현재 `private`이며 실제 레지스트리 게시·라이선스 결정·두 번째 프로젝트 소비 검증은 완료되지 않았다.

| 객체 | 공유 필드·의미 | 프로젝트가 채울 부분 |
|---|---|---|
| `AdapterDescriptor` | `contractVersion`, 충돌 없는 `adapterId`, `implementationVersion`, capability 목록, 요청/응답 schema ID·hash, 결정성·side-effect 선언, 자원 한도 | 프로젝트/rules/source/catalog 버전과 규칙 provenance |
| `AdapterRequest` | request ID, 프로젝트·객체·계약/구현 버전의 정확한 선택, capability, 요청/응답 schema ID·hash, 불투명 snapshot revision, 작업/결과 한도와 payload | 선택 대상·행동·도메인 context. 취소와 monotonic deadline은 host 호출 context로 전달 |
| `AdapterOutcome`/`AdapterResponse` | 성공/실패 구분, 결과 schema, 순서 있는 결과, 해당 시 `examined`와 cursor/`exhausted` | 후보·효과·event의 타입과 해석 |
| `AdapterError` | `unsupported`, `invalid_input`, `stale_revision`, `limit_exceeded`, `cancelled`, `execution_failed` 구분 | 도메인 오류 코드와 안전한 진단 |
| `AdapterRegistry` | `(projectId, adapterId, contract major, implementationVersion)`의 명시 선택; 중복·미등록 거부 | 버전별 객체 등록과 호출 정책 |

객체는 불변 설정으로 만들고 호출 간 임의의 전역 상태를 공유하지 않는다. RNG·시계·외부 상태는 project host가 소유권과 버전이 명확한 context로 주입한다. 결정성은 해당 capability가 실제 보장할 때만 선언한다. state 변경은 host가 가진 복제본/트랜잭션에서 실행하고 성공 시에만 원자적으로 커밋한다. 실패·취소·한도 초과 때 원본 state/RNG/history/event가 변하지 않아야 한다. 모르는 버전·capability·schema 또는 미구현 규칙을 기본 구현으로 대체하지 않는다.

`enumerate`, `validate`, `execute`는 Accelerate 규칙 객체가 우선 사용할 capability이며 모든 객체의 필수 메서드는 아니다. `enumerate`는 원문 순서와 페이지의 `examined`/`exhausted`를 보존하고 예산 초과를 성공한 전체 목록으로 위장하지 않는다. `validate`는 앞 페이지에 행동이 있었는지만 확인하는 함수가 아니라 해당 snapshot에 대한 전체 의미 검증이다. `execute`는 행동과 revision을 다시 결속해 프로젝트가 정의한 직접 효과만 수행한다. UI 힌트, 원시 실행 후보, AI 공개 의도, 왕실 위협용 내부 후보는 서로 다른 capability/결과 schema다. 내부 기물 ID·숨은 capture flag·RNG를 모델 입력에 복사하지 않는다.

게임 host는 공개 후보를 AI에 전달하기 전에 해당 Position에서 원문상 유효한 공개 intent인지 검사한다. 선택한 intent의 필드 집합과 값, 카드의 순서 있는 선택 대상이 host가 다시 투영한 Action과 정확히 같아야 한다. `TypedEncoder.encode`는 의미 특징 추출기이며 최종 행동 schema 검증기가 아니다. 공개 `ObservationIR`은 서명·visibility·8×8 geometry를 검사하는 `from_public` 경로로만 만들고, 수동 생성하거나 수정한 IR을 권위 있는 AI 입력으로 받지 않는다. 이는 PR #28 최신 public-intent/IR 경계를 이번 v7 이관의 완료 조건으로 유지한다.

Rust 첫 구현체는 객체 안전한 진입점과 프로젝트별 typed wrapper를 분리한다. 실제 핵심 API는 다음과 같다. `S`·`P`·`R`은 각 프로젝트의 상태·payload·결과 타입이다.

```rust
trait AdapterObject<S, P, R>: Send + Sync {
    fn descriptor(&self) -> &AdapterDescriptor;
    fn invoke_read(&self, state: &S, request: &AdapterRequest<P>, meter: &mut CallMeter<'_>)
        -> Result<AdapterOutput<R>, AdapterError>;
    fn invoke_write(&self, working: &mut S, request: &AdapterRequest<P>, meter: &mut CallMeter<'_>)
        -> Result<AdapterOutput<R>, AdapterError>;
    fn validate_output(&self, capability_id: &str, result: &R) -> Result<u64, AdapterError>;
}
```

공유 코어가 프로젝트 상태를 `Any`로 downcast하거나 JSON 값을 게임 상태로 소유하지 않는다. 게임 wrapper `projects/augment-chess/engine/src/adapter.rs`가 versioned envelope를 typed `GameState`와 `V7HostPosition`으로 결속하고, host의 snapshot·transaction·RNG·event 서비스를 호출한다. 봇은 `projects/accelerate/native/`에서 이 facade를 소비한다. 정적 registry로 시작하고 동적 플러그인 ABI는 제공하지 않는다. 규칙 객체마다 파일 하나를 만들지 않고 변경 이유와 소유권이 같은 구현을 함께 둔다.

## 현재 Accelerate 코드와 이관 경계

| 책임 | 현재 코드 | 이전 시 보존할 계약 |
|---|---|---|
| 상태·공개 API | `projects/augment-chess/engine/src/{state,spatial_state,v7_host,v7_new_game,adapter}.rs` | source DTO와 단일 identity·파생 점유를 모델 plane으로 혼동하지 않는다. v7 import·새 게임·공개 관측과 draft/play의 source 후보 검증을 제공한다. play는 전체 정답이 확인된 것으로 선언하지 않으며 활성 미지원 분기를 정확한 오류로 반환한다. |
| 이동 후보 | `projects/augment-chess/engine/src/{movement,movement_objects,variant_movement}.rs` | 기물별 객체는 기본 후보·flag를 맡는다. 전역 modifier·강제 기물·순서/예산은 별도 조정층이 맡는다. |
| 조합형 이동 | 게임 엔진의 `move_program.rs`가 가진 `MoveBoard`, `MoveProgramSet`, raw provenance·bounded cursor | 현재 별도 spatial 경로다. 모든 v7 이동을 자동 대체하거나 내부 `piece_id`를 공개 의도로 내보내지 않는다. |
| 카드 | 게임 엔진의 `card_registry.rs` 정의/instance/정책과 `card_effects.rs`의 객체별 후보·검증·직접 효과 | 정의와 손패 instance, UI 선택과 효과 수용, 수동/강제/패시브 실행을 구별한다. 직접 효과 객체가 사용 비용·턴 정산을 임의 결정하지 않는다. |
| 행동 입력 | `projects/augment-chess/engine/src/{v7_action_surface,v7_action_admission,v7_adapter_actions,adapter}.rs` | 내부 source envelope의 정확한 Position/action identity와 공개 intent의 필드·순서를 검사한다. draft/play 후보의 source 순서 cursor, 선택 family의 scalar 수용, eager 목록과 `legal-actions-page`를 구현 중이다. 공개 목록·봇 IR에는 host의 전체 상태 기반 action ID와 Position ID를 넣지 않는다. 새 경로의 빌드·전체 원문 대조·설치 소비자 검증은 별도 인수한다. |
| 실행·투영 | 게임 엔진의 `transition.rs`, `v7_replay.rs`, `v7_threat.rs`, `v7_turn_flow.rs`, `v7_end_move_reactions.rs`, `v7_piece_lifecycle.rs`, `v7_board_automata.rs`, `v7_queued_effects.rs`, `v7_turn_entry.rs`, `observation.rs` | actor·stale·원자성·RNG·history·result는 게임 host의 실행 흐름 책임이다. source turn callback을 끼어 있는 단계마다 연결하고 미지원 활성 분기는 구체적인 오류로 멈춘다. 위협용 `AiNoCards`와 public legal/hints를 합치지 않는다. |

고정된 공식 클라이언트는 공개 카드 256개와 보조 정의 1개, 선택 가능한 RULE 27개를 가진다. PR #29 시점의 JS oracle은 원문 최상위 초기화를 생략해 RULE 26개와 일부 phase·stars·weight·openingWeight를 잘못 기록했다. 이후 23문장을 보존해 카탈로그를 교정했으나 기물 이름 등의 다른 초기화는 여전히 생략했다. 이전 seed 19 카드/드래프트 영수증과 768-cell 감사는 **당시 로더 범위의 역사적 증거**로 남긴다.

현재 로더는 원문 SHA를 유지한 `accelerate-headless-semantic-v7-faithful-init-v1`이다. `execution-profile-20260928.json`이 보존 initializer 175개/제외 168개와 replay labels·codes 각 79개/frameKeys 222개를 고정하고, manifest SHA `d811f0232ac38af4e45e0e4f93e89c49712142dfd0f2b5fe57d36f63cd05a29f`를 composite catalogVersion `f80ebcd21759df179bccfb301415e672194de67691a6383beafc549538ffae7c`에 결속한다. 공개 원문 catalog hash와 composite identity를 혼동하지 않는다. 새 프로필로 새 게임·후보·전이·전체 state/RNG/history 자료를 생성·대조하며, 원문 내부 AI 정책과 직접 label 조회/fallback은 [인수 장부](V7-ACCEPTANCE-CLOSURE.md)의 별도 경계를 따른다. 공개 카드 정의 수나 initializer 수는 효과 실행 성공의 증거가 아니다. 단일 RULE 시작 324개는 faithful175 schema 2 source manifest와 전체 Position을 재생성해 native PASS를 관측했다. 다중 RULE·사용자 설정 7개와 이전 draft 첫 전이 6개는 각각 당시 근거와 새 프로필 검증을 구분한다.

## 단계별 인수 현황과 소유권

1. **공유 계약**: `packages/adapter-contract/`의 schema ID/hash와 `packages/adapter-runtime/`의 sealed registry, exact 버전/schema 선택, 읽기·transaction 분리, 취소·한도·rollback은 구현됐다. 서로 다른 두 가짜 프로젝트로 wire 왕복과 오류 경계를 검사했다. 게임 타입을 공유 패키지로 옮기지 않는다.
2. **게임 host와 형식**: `V7HostPosition`은 source envelope/JCS identity와 staged state·RNG·history의 원자성을 담당한다. 게임 전용 계약 자료는 `projects/augment-chess/contracts/`, v6 읽기·검증된 부분 변환은 `projects/augment-chess/tools/v6-migration/`에 둔다. 실제 원문 Position 왕복은 국소 증거이며 전이 정답은 별도로 검증한다.
3. **규칙 객체**: 이동·카드의 정적 객체 registry, 특수 기물/RULE 직접 효과와 턴 중간 callback은 기능 유형별 단일 담당으로 이관 중이다. raw 후보, UI 힌트, AI 공개 의도, `AiNoCards` 위협 후보를 서로 바꾸어 사용하지 않는다. 미지원 활성 분기는 정확한 오류를 유지한다.
4. **통합**: 공통 `GameAdapterSession`에는 source-versioned `public-observation/observe`와 `public-actions`의 `legal-actions`·`legal-actions-page`·`bind-public-intent`·`apply-public-intent`가 등록됐다. draft 외 play의 원문 순서 후보·선택 family 검증과 transaction 실행을 연결 중이며 실패·취소·한도 초과 시 state/RNG/history를 버린다. 공개 page는 발급된 불투명 cursor, page 1..4096, examined 1..65536과 snapshot identity를 결속한다. 이 통합의 bounded source comparator는 전달105에서 통과했으며 그 경계 밖의 모든 조합을 증명하지 않는다. 단일 RULE 시작 상태 324개와 source internal 21사례·43표본, normal/chaos 실제 첫 이동 2개의 faithful scoped PASS를 관측했다. 이전 복합 설정 7개·draft 국소 근거와 현재 실행을 혼용하지 않는다. 관측 기반 `public_transition_compatible`·`apply_weighted_conditioned_public`의 play 분기는 저장됐다. 한 번의 source-prior-v1 실행·실제 carried draw trace·전체 공개 projection 동등성과 opaque identity 결속의 source chance9/source prior3 및 전달105 bounded 회귀가 PASS다. 경로 질량은 IID 연속 난수 해석이며 유한 LCG seed 전체의 정확한 posterior나 모든 조합의 확률 완전성은 증명하지 않는다. Python 소비자 검사 코드는 저장됐으나 설치 wheel·실제 봇 연동은 사용자 범위 조정으로 미실행·미검증 후속 항목이다. 객체 등록을 전체 play 지원의 증거로 취급하지 않는다.
5. **이번 전달 gate**: 수정된 source loader의 legal/reject/full state/RNG/history/result와 source-reachable 분기 장부, 최종 engine/common/source 회귀·lint·구조 검사 및 PR #32 커밋/push·원격 SHA를 확인한다. 설치 wheel·실제 봇 연동·성능 측정과 미관측 CI를 포함한 전체 프로젝트 GO는 별도다. v6 내부 회귀 검사는 v7 실행 근거가 아니며 공개 v6 규칙 실행은 열지 않는다.

같은 유형의 작업에 두 구현자를 동시에 배치하지 않는다. 파일 하나마다 담당자를 나누지 않고 기능·변경 이유·API 책임으로 유형을 정한다. `lib.rs`·`transition.rs`처럼 공통 호출 순서를 바꾸는 파일은 인터페이스가 안정된 뒤 통합 담당자가 순차적으로 수정한다. N-version과 동결 JS oracle은 검증용이고 운영 fallback이 아니다. 카드별 거대 snapshot fixture를 쌓지 않고 기존 작은 사례와 저장소 밖 보고서를 이용한다.

## 증거와 아직 정할 것

- source21 내부 차분 검사는 typed `Position::legal_actions`와 운영
  `V7HostPosition`의 `legal_action_envelopes`를 모두 원문 전체 ordered envelopes와
  비교한다. 운영 host 검사는 `VerifiedV7ActionSet::complete`와
  `legal_source_actions`를 통해 draft/play/terminal의 정확한 Position/action
  identity·payload·순서를 확인한다. 초기 orthodox first-play 이동 whitelist의
  지원 범위를 일반 play의 완료 기준으로 사용하지 않으며, 실제 host 경로의
  미지원·한도 초과·admission 오류는 차분 실패로 유지한다. 이 source21의
  유한 입력 성공도 임의 규칙 조합이나 전체 v7 완료의 증거는 아니다.
- 전체 v7 범위·유형별 소유권·실제 활성 guard·source receipt와 메인 검증 명령은 [전체 규칙 인수 장부](V7-ACCEPTANCE-CLOSURE.md)에 둔다. 코드 작성, source 자료 생성, native 국소 비교와 최종 같은 SHA 통합 결과를 구분한다.
- 이전 국소 결과와 실제 84개 job(54 PASS·30 FAIL), 후속 43개(23 PASS·20 FAIL), stable50(49 PASS·1 FAIL)을 각각 보존한다. stable50은 모든 `sourceDigestBefore`/`sourceDigestAfter`가 동일한 closure이고 warnings 0이며 continuation 22개 묶음의 승급 후 `moveReplay.white` 차이만 실패했다. 실행 항목과 raw callback·whole 전이·입력 없는 단위 검사가 겹치므로 성공 수를 합산하지 않는다. provenance13의 source 생성은 실제 recipe·loader/frozen closure·계획·Node 해시를 기록했고 8개 특수 이동 recipe의 162개 자료 bytes/JCS 근거를 연결했다. Guard/campaign도 현행 identity로 새로 생성해 scoped native PASS를 관측했으며 과거 입력의 metadata를 승격하지 않았다. recheck3은 schema2 raw81·Transcendence 실제 AI/probe4·Judgment milestone2의 source 생성 exit0과 source closure 불변을 기록했다. 새 세 comparator는 checkpoint105와 전달105에서 각각 PASS였고 Otherworld는 두 depth를 명시한 새 source26으로 focus4·전달105 PASS를 관측했다. 이전 검사·입력 SHA·실행 경계는 저장된 `reports/root-v7-validation-map.json` revision 10에 보존한다. 저장하지 않은 revision 11을 근거로 인용하지 않는다. 최종 전달의 source closure와 실제 결과는 `reports/root-pr32-delivery105-jobs.json` 및 `reports/root-native-root-pr32-delivery105-jobs-summary.json`과 개별 로그를 따른다. 최종 전달105는 모두 PASS·동일 source digest이며 engine all-targets unit711/integration20과 추가104 gate를 구분한다. 104는 독립 규칙·시나리오 개수가 아니다. production dead_code11 경고를 보존한다. 뒤이은 fmt/common registry9/v6migration6은 PASS, 엄격 Clippy는 53진단·exit101이다. 진단은 인수 장부에 정확히 보존하며 lint 리팩터링을 확대하지 않는다. 실제 로컬 Rust는1.97.0이고 CIpin1.96.0 성공은 미관측이다. staged index의 구조 검사는 PASS였으며 커밋/push·원격 SHA는 메인이 확인한다. 제외한 설치 wheel·실제 봇 연동·성능과 미관측 CI까지 포함한 전체 프로젝트 GO는 선언하지 않는다.
- [IMPLEMENTATION-DIRECTIVES](IMPLEMENTATION-DIRECTIVES.md)의 P0/P2/P3/P8 계약과 [ENGINEERING-STANDARDS](ENGINEERING-STANDARDS.md)를 먼저 읽는다. 동결 v7 client SHA-256은 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`다.
- 첫 수 전후 상태·RNG, 왕실 위협 후보 순서와 미완료 경계는 Git 밖 `%APPDATA%/Accelerate/reports/v7-plain-move/`, 첫 이동 뒤 양측 공개 관측은 `%APPDATA%/Accelerate/reports/v7-postmove-observation/`, flow 국소 비교는 `%APPDATA%/Accelerate/reports/v7-delta-ledger/first-move-flow-direct.json`, Grappler 국소 원문은 `%APPDATA%/Accelerate/reports/v7-variant-grappler/`에 있다. 이 호스트의 보고서는 다른 작업자에게 따로 전달해야 한다.
- 공유 schema ID와 canonical hash는 `packages/adapter-contract/manifest.json`에 고정됐다. 게임·봇은 모노레포의 별도 프로젝트이지만 두 번째 독립 도메인 소비 사례와 별도 배포·ABI 안정성은 아직 검증되지 않았다. 현재 저장소의 게임 전용 `contracts/`를 공유 코어로 선언하지 않는다.
- faithful175에서 RULE 초기 상태 27종×3스타일×2시드×`draftDelete` 조합 324개를 원문으로 재생성하고 schema 2 manifest SHA `5742903ffa86f68e90e1bad4cf8e252c58fcdbf98ec9db04aa4922ff1d6807b5`와 전체 Position을 인증했다. 후속 native의 전체 JCS·Position ID·RNG·history·mode·적용 RULE 비교는 PASS다. 다중 RULE·다른 시드·사용자 설정 대표 7개의 이전 프로필 근거는 당시 경계로 보존하며 현행 324개와 혼용하지 않는다. 초기 생성 PASS는 런타임 RULE 조합·이동·종료의 완료가 아니다. 공개 생성자는 유효한 uint32 시드와 설정을 받되, 이 7개가 임의 조합 전체의 증명은 아니다. `GameConfig` 한도 필드는 `u32`라 JS 안전 정수 전체를 표현하지 못한다. Linux 로컬 wheel은 중간 작업 상태의 `linux_x86_64` 검증용이며 최종 commit·Windows wheel·manylinux 배포 증거가 아니다.
- 기물·카드 객체로 이전한 뒤에도 source별 RNG 소모, 자동 효과 순서, raw 후보와 공개 의도, 위협·UI·실행 context 차이가 남는다. 각 capability의 성공을 선언하기 전 직접 비교한다.

이 문서의 구현 현황은 v7 코드 GO의 증거가 아니다. 실제 학습은 완료 조건에 포함하지 않는다.
