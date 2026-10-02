# v7 전체 규칙 인수 장부와 남은 구현 지시

이 문서는 [구현 지시서](IMPLEMENTATION-DIRECTIVES.md)의 P0/P2/P3/P8과
[객체형 어댑터 이관](RULE-ADAPTER-HANDOFF.md)의 인수 장부다. 2026-10-01의 호출 경로
감사와 2026-10-02 후속 검증 상태를 구분해 기록한다. 이전 실행에서 읽은
로그와 메인 실행자가 전달한 최신 결과를 구분하며, 전체 v7 인수는 진행 중이다. 구현이 존재하거나
정의 수가 맞는다는 이유로 source-reachable 기능을 지원 완료로 처리하지 않는다.

## 이번 PR #32 전달 범위

2026-10-01 사용자는 봇 연동과 성능 측정을 진행하지 않고 PR #32에 push한 뒤 이번
전달 목표를 완료하도록 범위를 조정했다. 2026-10-02 후속 요청은 core/common/JS/source
회귀와 strict lint·구조 검사, PR #28 base 통합, Windows/Linux `core_only` CI 및
검증된 커밋의 PR #32 공유·원격 SHA 확인을 포함한다. 실제 Python 봇 연동,
sdist/wheel 빌드와 설치, 성능 측정은 **현재 범위에서 제외**한다. 기존 관련 검사 코드를 저장한 사실이나
이전 중간 wheel 결과를 최종 설치·봇 연동 성공으로 기록하지 않는다.

이 문서의 전체 프로젝트 GO와 이번 PR 검증 완료는 서로 다른 판정이다. 2026-10-01
전달105의 PASS·Rust 1.97·production dead_code 11건·Clippy 53진단/exit 101은 당시
source closure의 역사적 증거로 보존한다. 아래의 새 실행 상태와 혼동하지 않는다.

## 2026-10-02 후속 검증 상태

다음 결과는 메인이 실제 실행해 확인한 로컬 검증이다. 원격 SHA와 같은 commit의
Windows/Linux CI 관측은 PR #32에 별도로 결속하며 전체 프로젝트 GO와 구분한다.

| 항목 | 현재 관측 상태 |
|---|---|
| Rust 최소 toolchain | 실제 Rust 1.96의 fmt와 core 4개 crate all-targets strict Clippy(`-D warnings`) PASS |
| Node 관련 회귀 | 6개 파일·62개 검사 PASS, skip 0 |
| CI helper·구조 | CI helper 17개와 저장소 구조 검사 테스트 14개 PASS |
| Python 변경 입력 | 8개 파일 AST PASS. 실제 소비자 실행·봇 연동·설치 검증은 아님 |
| workflow 정적 검사 | Actionlint 1.7.12로 2개 workflow PASS |
| 최종 native105 | `reports/root-pr32-finish-final105-jobs.json` 전체 105개 PASS. engine all-targets 단위 713개·통합 20개, 일반 실행의 source-input 53개 ignored. 공통 registry 9개·v6 변환 6개 PASS. 전 job source digest 불변·컴파일 경고 0건 |
| PR #28 base 통합 | base `45edfe1`의 충돌을 해소한 source로 검증했다. merge checkpoint·push·원격 SHA는 PR #32에 기록 |
| Windows/Linux CI | 같은 commit의 `core_only=true, adapter_only=false` 관측 결과를 PR #32에 기록. 자동 PR의 봇·wheel 실행을 제외하려고 `[skip ci]` 뒤 수동 실행하며 전체 CI 성공으로 확대하지 않음 |

첫 all-targets 시도의 부모 회귀 2건을 실패 기록으로 보존한다. overtime 새 게임의 이전
golden `c2af18…`은 faithful 원문을 재생성한 `4dca89…`과 달랐다. 메인이 대조한 새 원문
결과는 현재 native와 정확히 일치했으며 overtime 값 `1.6`을 `2`로 정규화한다.
다른 검사는 `AiNoCards` 미지원 오류를 기대하지만 현재 지원된 실행 경로와 충돌한다.
overtime golden을 새 원문 결과로 갱신하고, 위협 검사는 실제 v7 생성 상태의 malformed
`pendingOtherworld`가 정확한 `InvalidState`를 전파하며 원본 state/RNG를 보존하는 경계로
바꿨다. 이후 최종105 전체 재실행은 모두 PASS다. 최초 실패 요약
`reports/root-native-root-pr32-finish105-jobs-summary.json`의 SHA는
`1b94c563d730b8dfaee192d4195d00b8fd1c19941015fa52cb7ff2456788b908`로 보존한다.

핵심 원문 21국면을 새로 생성했다. cases bytes SHA-256은
`ec6fcbcbd3067eec61838298a600d49c808038ef206db48d5f99ee943d0e6471`, report bytes SHA-256은
`b111c386130bf3ac5b362352fc2a233e6a9893ca7832ce9709a8cac6557d4c8c`다.
원문 생성 자체는 `oracle-only`·exit 1(native 비교 명시적 제외)이며, 이후 동일 입력으로
Rust의 21국면·42 apply 표본을 비교해 `bounded-internal-pass`를 관측했다.
내부 비교 보고서 SHA는 `3701f49b4e86c36ee5f26a554fd726e20a2b8f9ba87283f88431065bffc9892b`다.
최종105 계획 SHA는 `2e404be2b7fe1c7881a48bf0ec531cc155814d9225f114b070c07826d807950f`,
요약 `reports/root-native-root-pr32-finish-final105-jobs-summary.json`의 SHA는
`73fda7786b22c2cde00ca8b0cc48abcab7e09e8c0f0932efb6a6a182befcc471`다.
전 job source digest는 `5638d6c8f812a41a3a0b6fb20c3f471f59c7d624a8774232e9698545a3b15e13`로
동일하다. 105는 실행 작업 수이며 독립 규칙·시나리오 수가 아니다. 원문 입력을 요구하는
53개 ignored를 자동 PASS로 세지 않으며 각 제공 입력·필터의 실제 실행 결과를 따른다.

## 정답과 최종 범위

정답은 `projects/augment-chess/contracts/catalog/site-20260928.json`의
`augment-site-20260928-e5ed84fcf8e72a24`와
`accelerate-headless-semantic-v7-faithful-init-v1`이다. 원문 client SHA-256은
`e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`이며
parser·catalog·관측 정책의 버전과 hash도 함께 결속한다.
`execution-profile-20260928.json`의 SHA-256은
`d811f0232ac38af4e45e0e4f93e89c49712142dfd0f2b5fe57d36f63cd05a29f`이며,
보존한 최상위 초기화 175문장과 제외한 168문장, 원문 순서·의존성·bootstrap·replay
metadata를 고정한다. 현재 composite `catalogVersion`은
`f80ebcd21759df179bccfb301415e672194de67691a6383beafc549538ffae7c`다.
원문 공개 catalog hash `yt63f_Lcq4xUPu4GbbdwAyv5IUTuC079nNxvF3IFrj4`는 별도 값으로
보존한다. 이전 선언 전용/23-initializer loader의 RULE 26/27개나 과거 768-cell prefix
조사로 최신 전체 규칙을 승인하지 않는다. 원문 bytes가 같아도 실행 초기화와 composite
identity가 바뀌었으므로 영향을 받는 영수증을 새 프로필로 생성·비교해야 한다.

| 대상 | 인수해야 하는 의미 | 정의·등록만으로 알 수 없는 것 |
|---|---|---|
| normal·chaos·grand, 로컬 8×8 | 새 게임, draft, play, 선택, 정산, 종료와 공개 양측 관측 | 한 시드의 초기 보드가 임의의 조합·전이를 증명하지 않는다. |
| 공개 카드 256 + 보조 정의 1 | 획득/자동·수동·가상 실행, 실패·재시도, 직접 효과와 지속 효과 | ACTIVE non-RULE 162 + PASSIVE 67 + RULE 27의 분류는 실행 증거가 아니다. |
| 선택 가능한 RULE 27 | 초기 활성화, 런타임 추가, 해당 룰의 이동·정산·종료 상호작용 | 초기 상태 324개 일치가 런타임 27종의 완료를 뜻하지 않는다. |
| 카탈로그 기물 84 | 생성·변환·이동·공격·방어·자동 효과·승급·소멸의 실제 호출 경로 | 타입이 등록되거나 일반 이동 하나가 존재해도 특수 기물은 미완료일 수 있다. |
| 행동 10종 | 전체 원문 순서 후보, 선택 family 검증, 거절, apply와 결과 | 이동/card/draft 표본만으로 특수 결정 행동을 승인하지 않는다. |
| 원문 UI 선택 3종 | Rule Ticket, Joker, Barricade의 선택 순서와 원자적 카드 intent | 원문 AI combination과 브라우저의 중간 클릭 상태는 다른 입력이다. |
| 공통 host | Position identity, RNG, replay·공개 history, 결과, stale/취소/한도/실패 원자성 | raw 직접 효과의 국소 성공이 public legal/bind/apply 또는 설치 wheel 성공은 아니다. |

보조 `shotgun-king`은 공개 라이브러리 256장과 별개로 장부에 유지한다. piece 효과
소유자와 raw·virtual·manual·강제 첫수 dispatch 연결이 추가됐으며 통합 검증 대기다.
보조 정의를 분모에서 빼서 전체 인수를 완료하지 않는다. 동적 캠페인 `blood`와
`black-tower`는 공개 정적 풀과 별도로 정의·instance 할당·검증·실행 provenance를
보존한다. Black Tower의 정의·후보·수동/가상·첫수 실행 연결도 새 검증 대상이다.

원문 초기화의 보존 범위는 카탈로그와 로그 모두에 영향을 준다. `TYPE_LABELS`의
기본 선언은 원문 `main:50295`의 30개 이름이며, `main:50332–50373`의 직접 대입 40개와
`Object.assign`의 9개를 합하면 브라우저 초기화 후 이름은 79개다. 카탈로그 기물 84개 중
`bomb`, `logRolling`, `platform`, `portal`, `wall`에는 이 이름 표의 key가 없다. 해당
추가 문장은 UTF-16 offset `9664390` 이후에 있으며, 이전 23-initializer loader는
`9788208` 이후만 보존해 이 이름 추가를 실행하지 않았다. 당시 labels 30개 결과는
그 실행 범위의 역사적 증거다. 현재 faithful175 프로필과 엔진의 공통 v7 replay
accessor는 labels 79개·codes 79개·frameKeys 222개를 같은 manifest로 결속한다.
다만 내장 legacy metadata를 사용하는 `replay::piece_label`과 이를 부르는 일부 v7
소비자가 남아 있다. accessor의 구현만으로 모든 로그가 이행됐다고 보지 않으며,
Judgment/Necromancy·승급·Desperado·queued/incoming의 실제 label 조회를 각각 확인한다.
이는 이름 key가 없는 5종에 새 이름을 만들어 넣는다는 뜻이 아니다. 원문의
`TYPE_LABELS[type]` 직접 조회와 `|| type` 등의 fallback을 각각 보존한다. 선언·등록
개수와 초기화 후 실제 값, loader 구현 준비와 동적 실행 성공을 구분하고, full state·
history·Position identity의 의존 영수증을 다시 생성·비교한다. import stub·timer·UI
hook을 포함한 headless 실행 범위는 manifest의 제외 이유와 정책을 따르며, 이 프로필로
브라우저의 미래 RNG나 모든 UI 동작까지 같다고 주장하지 않는다.

비8×8 editor·wide campaign의 회귀 geometry, 외부 권위 서버의 제출 상태, 충돌하는
identity/alias, 표현할 수 없는 JCS Unicode 등은 정확한 입력/범위 경계로 기록한다.
grand의 기본 보드는 8×8이다. 비8×8 guard가 있다는 이유로 grand 전체를 미지원으로
분류하거나 guard를 없애 일부 칸만 처리하지 않는다.

## 실제 실행 경로와 소유권

공통 계약·호출 순서·통합·빌드·테스트·원문 실행·커밋·push는 메인 담당자가 소유한다.
아래 기능 유형에는 각각 한 구현 담당자만 둔다. 한 파일에 여러 유형의 접점이 있어도
별도 구현자가 같은 유형을 중복 작성하지 않는다. 공통 `lib.rs`·`transition.rs`와
adapter schema의 변경은 메인 담당자가 순차적으로 연결한다.

```text
v7_new_game / draft
  → RULE 초기 활성화 / passive 획득·실패 / 선택·복제·가상 카드
  → V7HostPosition / GameAdapterSession
  → 공개 관측 / SourceActionCursor / 공개 intent 페이지
  → 선택 family의 원문 후보 검증 / exact bind / transaction apply
  → 이동 또는 카드 직접 효과 / 캡처·제거·승급 콜백
  → endMove 중간 정산 / count 경계 / incoming 정산
  → replay / 공개 양측 history / result / Position identity
```

UI 힌트, 원문 raw 행동, AI 공개 intent와 위협용 `AiNoCards`는 목적·context·결과 schema를
따로 유지한다. 공통 검증에 편리하다는 이유로 하나의 목록으로 바꾸지 않는다. 내부
위협·free-move·가상 카드 실행 context는 Position DTO에 임의의 private 필드로 넣지 않는다.

원문 내부 AI 정책은 `v7_ai_card_candidates.rs`로 분리한다. 공개 intent의 전체 선택
공간과 위협·no-action 판정의 source AI 후보는 서로 다른 집합이다. Oracle의
`withPublicEnumerationPolicy`는 공개 수집 호출 동안만 완성 후보를 적용하고,
`aiSimulationDepth` 또는 `kingThreatProbeDepth`가 양수인 내부 판정과 기본
playability 조회에는 원문 collector를 사용한다. paused iterator가 제어권을 반환하기
전에 원문 함수를 복구한다. 관련 Node 검사와 전달105의 내부 21국면·43개 sampled apply는
통과했다. 이 유한 표본을 모든 내부 판정·규칙 조합의 증거로 확대하지 않는다.

| 기능 유형 | 실제 코드 경계 | 다음 인수 작업 |
|---|---|---|
| 공개 행동·수용 | `v7_action_surface.rs`, `v7_action_admission.rs`, `v7_adapter_actions.rs` | raw 이동→손패 순서 카드→raw가 전혀 없을 때 friendly fallback을 보존한다. 후보 거절과 미구현 오류를 구별하고 page/scalar/eager 결과를 대조한다. |
| 공유 facade·페이지 | `adapter.rs`, 공통 packages, native/Python 소비자 | 공개 목록/응답에서 내부 action/Position ID·source payload를 누출하지 않는다. 발급된 불투명 cursor, snapshot 변경, 취소·한도, 설치 wheel을 검증한다. |
| 기물 이동 조회 | `movement.rs`, `variant_movement.rs`, `v7_special_piece_moves.rs` | generic source 후보를 개별 modifier·공격 mask·특수 이동과 연결하고 관측·AI·위협 context별 순서를 대조한다. |
| 위협·턴 흐름 | `v7_threat.rs`, `v7_turn_flow.rs` | probe의 강제 연속 행동·자동 효과·간접 왕실 제거와 실제 no-action 판정의 카드 가용성을 구현·대조한다. |
| 카드 dispatch·후보 | `card_registry.rs`, `card_effects.rs`, `card_target_hints.rs` | source raw 후보와 효과 수용, virtual/forced/passive 정책, 실패 결과·RNG 재시도, 동적 blood와 보조 정의를 결속한다. |
| 카드 효과 유형 | `v7_card_{piece,status,board,topology,turn,choice}.rs` | 직접 효과뿐 아니라 공통 begin/finishCard, 후속 hazard·제거·승급·replay와 지속 상태의 사용/만료를 검사한다. |
| 패시브·첫수 | `v7_card_passive.rs`, `opening.rs` | 획득 66종과 White Box를 분리하고, 발동 실패·첫수 undo 재시도·Clone·RNG 순서·형성 후속을 대조한다. |
| 특수 결정 | `v7_decision_actions.rs`, 승급·draft host | WizardSpell, ShotgunReload, FileSurgeSkip, TrolleyChoice와 promotion/PromotionChoice의 원문 조건·후속 정산을 검사한다. |
| 특수 이동 실행 | `v7_move_execution.rs`, 공통 transition 접점 | log/shotgun·swap·portal·Mistake·Merchant/Grappler·large/castle·Football·Colossus를 source 분기 순서로 연결한다. timer raw noop과 host completion을 구별한다. |
| 일반 이동의 기물·trait 후속 | `v7_move_piece_effects.rs`, 공통 transition 접점 | 착지 승급 전후와 bomb/feudal/Trojan 뒤 생존 단계의 변환·trait 정산을 각각 호출한다. query API의 성공으로 이 실행 단계의 완료를 대신하지 않는다. |
| 이동 연속 정산 | `v7-card-coverage/move-continuations-probe.cjs`의 22 합성 사례 | `frozen_move_continuations_when_receipts_are_supplied`; `ACCELERATE_V7_MOVE_CONTINUATION_CASES`. stable50의 승급 후 `moveReplay.white` 실패를 보존하고 수정 뒤 checkpoint105와 전달105의 22개 comparator PASS를 관측했다. stationary direct HP의 Desperado와 모든 HP의 Frenzy, 이동/승급 후 연속 정산의 bounded 경계이며 임의의 모든 조합 증명은 아니다. |
| 캡처 반응 | `v7_capture_reactions.rs`, 공통 transition | 방패/HP, 회피/패링, 곰/목마, 봉건, 예고장, Imperial Study, 마나·Blood를 실제 이동/제거 경계와 연결한다. |
| 보드·위험·종료 | `v7_board_automata.rs`, `v7_board_hazards.rs`, `v7_rule_bombs.rs`, `v7_passive_terminal.rs` | conveyor/collapse/crown 및 환경 제거의 회귀·왕실·Reaper·다중 alias와 최종 result/history를 대조한다. |
| 큐·endMove | `v7_queued_effects.rs`, `v7_end_move_reactions.rs` | count/prophecy/winter/capture reset 전후의 큐·same-turn 조기 반환·상태 만료 순서를 지킨다. |
| Otherworld 복귀 | `v7_otherworld_returns.rs`, 공통 advance 연결 | scheduled return의 남은 count·배치·RNG와 terminal replay settle을 분리하고 위협 probe의 pendingOtherworld 후속을 연결한다. |
| incoming·수명·승급 | `v7_piece_lifecycle.rs`, `v7_turn_entry.rs`, `v7_incoming_reactions.rs`, `v7_promotion.rs` | 회귀/Undead/Lunchbox, Vanishing/Siren/ICBM/Panic/Trolley, RULE 추가, Recycling/holdout과 종료 콜백을 연결한다. |
| 관측·양자 상태 | `observation.rs`, `v7_quantum_state.rs` | 정규화→양자 관측→표적 재조회→후속 정산 순서와 양측 visibility·정보 불변성을 검증한다. |
| 인수 감사 | source inventory, internal differential, 이 문서 | 원문 도달성·입력 경계·코드 작성·국소 증거·전체 통합 증거를 구분하고 미완성 분기를 장부에서 유지한다. |

## 이전 프로필에서 관측한 국소 결과

메인 실행자가 생성한 `reports/root-receipts/root-adapter-receipt-checks.json`과
해당 `root-receipt-<filter>.log`를 읽어 다음 결과를 확인했다. 결과는 이 실행의
작업트리 snapshot과 이전 실행 프로필에 한정된다. faithful175 이행 이후의 현재 결과나
최종 commit의 검증을 대신하지 않는다.
아래 source 사례 수는 comparator의 제한된 자료 범위다. 공개 admission·completeMove·
실제 도달 가능 조합 전체를 수행하지 않은 raw callback은 그대로 표시한다.

| 실행한 필터 | 관측 결과와 범위 |
|---|---|
| `adapter::tests` | 6 passed, 0 failed. facade의 국소 계약 검사이며 설치 wheel·최종 CI는 별도다. |
| `source_pinned_quantum_state_rng_history_and_identity_match` | comparator 1 passed. 고정 source 16사례의 전체 Position·RNG·history·identity 범위다. |
| `frozen_capture_callbacks_when_source_receipt_supplied` | comparator 1 passed. 10사례·19 callback stage의 returned와 전체 envelope를 비교했다. ordinary 이동 전체의 증거는 별도다. |
| `frozen_promotion_callbacks_match_full_state_rng_and_history` | comparator 1 passed. 기존 36사례이며 새 착지 승급 source-slice 17사례와 일반 이동 접합은 별도다. |
| `frozen_geometry_queries_when_receipt_is_supplied` | comparator 1 passed. 고정 source 72 query 사례이며 직접 효과·정산의 완료는 별도다. |
| `frozen_incoming_lifecycle_callbacks_match_full_positions` | comparator 1 passed. 고정 source 19 callback 사례이며 실제 전체 턴의 증거는 별도다. |
| `current_v7_miracle_selects_one_bishop_and_preserves_unselected_bishop` | 1 passed. 당시 profile의 선택한 비숍 한 개 실행과 나머지 보존을 확인했다. |
| `temporary_source_otherworld_matrix` | comparator failed. raw callback 26사례는 일치했으나 terminal 정산 3사례의 `replayEvents[0].delta.fields` 순서/내용 차이가 남았다. |
| `temporary_source_passive_matrix` | comparator failed. 30사례에서 BigRook/BigBishop의 raw `monoShade` 누락 2건, 같은 settled 차이와 terminal replay·수치 표현을 포함한 총 10개 진단이 나왔다. |

이전 전체 lib 실행은 555 passed·33 failed·20 ignored인 구 snapshot이다. 새 국소 PASS로 이전
실패 전체를 닫거나, ignored 검사의 개수만으로 규칙 인수율을 계산하지 않는다.
Otherworld 일반 단위 4개의 성공과 source callback 26사례의 terminal 정산 실패도
각각 보존한다. PASSIVE의 `4`/ `4.0` 진단은 첫 차이 위치이며, 전체 canonical
digest와 replay 차이의 원인까지 확인하기 전에 표현 차이로 치부하지 않는다. 이 실패
목록은 이후 수정의 재검증 대상이며, 새 프로필에서도 같은 실패가 난다는 의미는 아니다.

## faithful175 이행 뒤 메인이 전달한 진행 결과

아래는 2026-09-30~10-01(한국시간) 메인 실행자가 전달한 결과다. source 자료 생성, native 비교,
소스가 바뀐 실행의 snapshot을 구분하며 최종 SHA의 완료 근거로 확장하지 않는다.

| 경계 | 전달받은 결과 | 아직 닫지 않은 범위 |
|---|---|---|
| 기존 ID 13개 재산출 | source 실행 exit 0. 이전 full state·RNG·history·legacy ID와 일치하며 composite catalog 변경으로 새 ID를 구했다. | ID 이행만으로 최신 native 전이가 성공한 것은 아니다. |
| seed 19 양측 전체 관측 6개 | faithful source 생성 exit 0, native comparator exit 0·1 passed | 실행 중 source closure가 바뀌어 최종 근거 재사용에서 제외했다. 고정 6개 관측의 모든 필드 일치는 관측했으며 안정화 snapshot 재검증·다른 국면·최종 전체 batch는 별도다. |
| White Box·Clone / Queen's Gambit 2개 | faithful source 생성 exit 0. callback 1개와 draft 2회→full move 1개의 자료다. | native 재실행과 일반 패시브 전체 조합 |
| Grand Roulette 14 action | faithful source 생성 성공, running clock 차이 수정 뒤 native comparator exit 0·1 passed | draft 12회·이동 1회·Roulette 1회의 고정 전이다. 공개 facade 전체·다른 Grand 규칙·최종 전체 batch는 별도다. |
| 이동 query 452개 | faithful source 생성 성공, Campfire 후보 순서 차이 수정 뒤 native comparator exit 0·1 passed | 합성 snapshot의 ordered query·flags·공개 hint·불변성을 비교한 근거다. 연속 대국이나 해당 카드의 자연 생성 조합 전체로 확장하지 않는다. |
| 양자 UI 10개 | faithful source 생성 성공, large-piece animation 차이 수정 뒤 native comparator exit 0·1 passed | 고정 query·binding·전이 10개와 ordinary landing 전체 증거를 구별한다. |
| Spy 전체 이동 4사례 | continuous VM과 fresh restore 자료 생성 성공, deck 정규화·stationary HP metadata 수정 뒤 fresh native comparator exit 0·1 passed | fresh 4사례·5단계의 state/RNG/history/Position ID·private context 비교다. continuous VM 4개 source witness와 다른 전이·최종 전체 batch는 별도다. |
| Football 고정 비교 | comparator 1 passed | 그 필터의 유한 자료만 확인했으며 다른 특수 이동·공개 admission은 별도다. |
| 전체 lib | 660 passed·11 failed·39 ignored | 실행 중 engine source digest가 바뀌어 최종 검증으로 재사용하지 않는다. 통합 소스를 고정한 뒤 관련 실패와 전체 gate를 다시 실행한다. |

위 개별 native comparator의 성공은 메인이 실제 exit 0과 test 1 passed를 확인해
전달한 결과다. 이 primary 5개 중 Grand·이동 query·양자 UI·Spy의 실행 전후 source
closure는 같았고, 전체 관측 6개 실행은 closure 변경으로 최종 근거 재사용에서 제외했다.
최종 전체 batch는 아직 별도다. Don은 source 16/16·실패 0·exit 0이고 Othello 12개와
Evacuation sound도 source 생성 exit 0을 확인했지만 native 성공과 구별한다. 새 source
21개는 생성을 마쳤고 oracle-only의 의도된 NO-GO exit 1을 유지한다. migration audit의
exit 0·ready true·실패 0·차이 0은 이전 입력·상태 보존 이행의 근거이며 전체 규칙 완료가 아니다.

새 영수증에 profile 명칭을 적는 것만으로 이전 자료가 이행되지는 않는다. 정확한
source/parser SHA, 현재 composite catalog, 실행 manifest와 실제 생성 경계를 연결한다.
standalone profile header가 없는 raw JSONL도 이 연결 근거를 확인한 뒤 분류한다.

## 실제 84회 실행과 후속 43/50회 입력 감사

메인의 `reports/root-receipts/root-native-84-summary.json`은 84개 job 중 exit 0인
54개와 실패 30개를 기록했다. 복사된 입력은 250개이며, 감사자가 실행 요약의 SHA-256과
파일 bytes를 읽기 전용으로 대조해 모두 일치함을 확인했다. 각 job의 실제 필터·소비 파일·
현재 comparator의 검사 경계는 `reports/root-v7-input-authority-audit.json`에 둔다.
폴더 전체 fingerprint에 들어간 파일 수를 그 필터가 실행한 사례 수로 세지 않는다.

입력 중 현행 composite catalog를 담은 파일은 67개, loader identity를 담지 않은 raw
source 파일은 178개, faithful profile 문자열만 담은 raw source 파일은 1개다.
이전 공개 catalog hash를 execution identity로 사용한 파일 3개와 native 진단 산출물
1개도 포함돼 있다. 정확한 client SHA와 rulesVersion은 이전/현행 프로필이 공유하므로
그 값만으로 자료를 legacy 또는 faithful로 판정하지 않는다. raw before/after에서 native가
현행 catalog와 Position ID를 합성하는 비교도 생성 당시 loader provenance를 대신하지
않는다. 해당 자료는 실제 generator·실행 profile·manifest·파일 digest 근거를 연결한다.

| 실제 입력 연결 | 84회 실행에서 관측한 경계 | 후속 판정 |
|---|---|---|
| `v7-card-coverage/board-direct-cases.jsonl` | 이전 catalog의 20개 basic 자료를 26개 extended 필터에 연결했다. `sourceUiTargets` 필드 부재로 효과 실행 전 중단됐다. | 별도 `board-direct-faithful26.jsonl`의 현행 26개를 재생성했고 후속 43회 실행에서 comparator 1 passed를 관측했다. 이전 입력 panic을 생산 effect 26개 불일치로 기록하지 않는다. |
| `v7-card-coverage/source-card-cases.jsonl` | 4행 중 Guard를 고른 raw 비교는 통과했지만 envelope catalog는 이전 공개 hash였다. 현행 catalog·Position ID·loader authority는 검사하지 않았다. | 별도 `faithful-authority6/faithful-authority-probe.cjs`로 현행 source 4행을 생성하고 stable50에서 현행 identity·전체 envelope 비교 PASS를 관측했다. Guard 필터가 실제 실행한 native raw 효과는 1개이며 나머지 3개 source 공개 전이를 native 성공으로 세지 않는다. |
| `v7-card-coverage/board-campaign-direct-cases.jsonl` | QG/Scarecrow 합성 callback 2개 raw 비교는 통과했지만 같은 이전 catalog와 identity 검사 누락이 있다. | 같은 새 슬롯의 현행 source 2개를 생성하고 stable50에서 현행 identity·전체 envelope 비교 PASS를 관측했다. normal 보드에 magicParty context를 붙인 합성 callback으로 실제 campaign 초기화와 구분한다. |
| `v7-move-execution-port/*.native-after.json` | 진단 파일이 폴더 fingerprint에 포함됐다. | 원문 영수증·사례 수·생성 provenance에서 제외한다. |
| Taboo capture callback | 해당 job은 exit 0이지만 실행 전후 source digest가 바뀌었다. | 이 실행을 안정화된 변경의 재사용 가능한 최종 근거로 쓰지 않는다. |

특수 이동 162사례의 생성 계획 `root-v7-faithful-source-jobs.json`과 종료 요약을
대조했다. stationary 22·missionary 10·swap 21·action 23·Football 15·large/castle
20·context 23·Siege/Mistake 28의 source 생성은 각각 exit 0이고 stdout의 사례명·
after JCS digest가 실제 84회 입력과 모두 연결된다. 후속 앙파상 2개는 별도
`root-v7-input-boundary-source-jobs.json`과 stdout의 새 digest로 연결한다.
실행 당시 recipe SHA·loader closure SHA는 이 기록에 없으므로 현재 recipe의 해시를
과거 실행 해시로 추정하거나 raw 영수증을 메타데이터만 바꿔 faithful로 승격하지 않는다.

이후 별도 `root-source-provenance13-summary.json`의 13개 source job은 모두 exit 0이다.
실제로 실행한 recipe·oracle/contracts/frozen 파일 집합·Node `v22.23.2`·계획 SHA와
실행 전후 source closure를 기록했고 13개 모두 closure가 같았다. summary bytes SHA는
`1eeb16af7a645fb2147caa4685b4ba420f245ecb3b2840113684f59edd26c783`다.
이 phase의 8개 특수 이동 recipe가 만든 162개 파일은 기록된 stdout의 after JCS와
모두 일치하고 후속 43회 실제 입력 bytes SHA와도 같다. 새 생성 근거를 연결한 것이며
fingerprint가 없던 과거 실행에 같은 metadata가 있었다고 보고하지 않는다.
Journey의 setup 완료 before 복원 10개, Guard/source authority 4행과 합성 campaign
2개, RULE 환경 before 복원 36개, terminal queue 복원 3개, private replay global
trace 12개도 같은 phase에서 생성했다. trace 12개는 source 내부 진단 자료이고 전용
native comparator가 없어 native 성공 사례로 세지 않는다. execution manifest의
canonical SHA와 원본 JSON 파일 bytes SHA도 서로 구분한다.

후속 입력 index `root-followup-inputs.json`의 357개 파일도 기록된 SHA와 모두 일치했다.
이 중 schema 2 manifest에 연결된 새 게임 Position은 324개다. 기존·후속 파일과
실행 수는 겹치므로 파일 수·생성 수·성공 수를 합산하지 않는다.

후속 `root-native-root-followup43-jobs-summary.json`은 43개 job 중 23개 PASS·20개
FAIL을 기록했고 43개 모두 실행 전후 source closure가 같았다. 이 43개에는 외부 비교를
수행한 job과 영수증 입력이 없는 단위 검사 4개가 함께 있다. 앞선 84개와 재검증 항목이
겹치므로 수치를 더하거나 전체 규칙 인수율로 바꾸지 않는다. 84회 실행에서 primary
관측 6·Grand 14·이동 query 452·양자 UI 10·Spy fresh 4사례/5단계는 모두 다시 exit 0을
관측했고 해당 실행의 closure도 같았다. 그 이전 관측 실행의 closure 변경 기록은 보존한다.

그 뒤 `root-receipts/root-native-root-stable50-jobs-summary.json`의 실제 50개 job은
49개 exit 0·1개 exit 101이며 warnings는 0이다. 모든 job의 `sourceDigestBefore`와
`sourceDigestAfter`가 같고 동일한 closure
`1c41381dc759708118688efe8f6db585233f39c5db0faa73df02f9ae17d1a1e2`다.
native 요약의 이 두 필드를 source 생성 요약의 `sourceClosureStable`와 혼동하지 않는다.
summary bytes SHA는
`6e9a89d5329cc346838477f653de975bf010650860fd380c825ba28d812616dc`다.
fresh index `root-stable50-fresh-inputs.json`의 원본 192개도 기록된 SHA와 모두 일치했다.
이 index는 reports의 원본 슬롯을 가리키며 이전 Windows root-receipts 복사본에는
미복사·이전 버전 파일이 있으므로 이름이 같은 mirror를 임의로 검증 입력으로 선택하지 않는다.
실행 이후 생산 소스의 caller·callee 수리가 계속되므로 이 성공을 현재 소스의 최종
coherent 검증으로 재사용하지 않는다. 아래 상태는 서로 겹치는 실행을 합산하지 않은
해당 유한 경계의 최신 관측이다.

| 최신 유한 검증 경계 | 실제 native 결과 | 인수 범위 |
|---|---|---|
| PASSIVE interaction 30 / Otherworld 26 | 84회 실행의 각 comparator PASS | exported-before 재수입 후 raw callback·정산의 국소 비교다. PASSIVE 66개 전체 재생성·자연 도달 조합·공개 whole move와 별도다. |
| 관측 body/full/hint 21사례 | body와 hint는 84회에서 PASS, full 125/126 차이를 수정한 뒤 후속 전체 126 비교 PASS | 동일 bounded corpus의 양측 projection이며 모든 대국 국면의 관측 완료가 아니다. |
| visibility 10×2 / incoming 20 / Othello 12 | 후속 및 stable50의 각 comparator PASS | 각각 renderer/per-color fog, microtask 전 incoming callback, whole endMove의 서로 다른 경계를 유지한다. |
| board direct 26 / 결정 행동 26 / 기물 trait 18 / 실행 context 23 | stable50의 각 comparator PASS | direct effect·결정·source slice·private helper의 경계다. 공개 facade 전체 완료로 확장하지 않는다. |
| 단일 RULE 새 게임 324 | source 324/324·error 0·unrun 0, 후속 및 stable50 native comparator PASS | schema 2 manifest와 전체 Position 324개를 비교했다. manifest SHA는 `5742903ffa86f68e90e1bad4cf8e252c58fcdbf98ec9db04aa4922ff1d6807b5`다. 런타임 RULE 조합·이동·종료와 별도다. |
| local Janggi query 42 / weighted second acquisition 2 / Judgment return 10 / Don A* 1 / Janggi cold constructor 4 | stable50의 각 native comparator PASS | 조회·draft·raw 반환·planner와 cold 전체 생성 경계다. Janggi는 이전 after-reset 4개 PASS와 구별하여 native newGame→preview→공통 reset→initializer→constructor 전체 Position·RNG·history를 대조했다. warm preview/UI·온라인 authority는 별도다. |
| nominal capture/placement 48 / weighted first move 2 / Colossus timer+history 22 | 후속 FAIL을 보존하고 수정 뒤 stable50의 각 native comparator PASS | bool 조회, 두 스타일의 실제 첫 이동, timer/history projection 범위다. weighted 첫 이동 성공은 play에서 관측 기반 belief 조건화 API가 구현됐다는 뜻이 아니다. |
| Journey 10 / 직접 turn flags / threat 60 / topology 29 / raw 특수 이동 162 / 복원 RULE 환경 36 / 선택한 terminal queue | 해당 stable50 comparator PASS | callback·helper·source danger probe와 공통 내부 전이를 각각 기록한다. source 복원 경계와 공개 host admission·전체 공개 턴을 혼합하지 않는다. |
| source internal 21사례 / 이동 continuation 22사례 | internal은 43개 표본 비교 PASS, continuation comparator는 FAIL | continuation의 첫 차이는 `promotion-reposition-precedes-underpromotion`의 `state/moveReplay/white`가 source null과 달리 native 객체가 되는 것이다. 구현된 정산의 오류·후속 재검증으로 기록하며 Unsupported placeholder로 분류하지 않는다. |

Journey는 cold-live VM, newGame을 복원한 뒤 setup, setup 완료 before Position을 복원한
뒤 callback이라는 세 경계를 구별한다. 앞의 자료는 보존하고 마지막 경계용 recipe와
checkpoint 인증 자료를 새로 생성해 stable50에서 전체 10개 native PASS를 관측했다. 현행 프로필의 자료라도 native fresh import와 다른
실행 경계면 같은 검증으로 합치지 않는다. threat 60의 테스트 전용 raw importer는 exact
envelope·catalog·Position ID·8×8 DTO·RNG·history·round-trip을 검사하며 callback 뒤
private context를 복구하고 replay를 정산한다. 공개 host spatial admission을 변경하지
않았으므로 이 비교의 성공을 공개 입력 수용·whole public turn의 성공으로 보고하지 않는다.

`reports/root-lib-source-recheck3-summary.json`은 schema 2 raw 81개, Transcendence AI/probe
4맥락, Judgment milestone 2개를 새로 생성한 세 job의 exit 0과 source closure 불변을
기록한다. 각 출력은 faithful175·현행 composite catalog와 exact bytes SHA로 연결했다.
이것은 source 생성 근거이며 새 native comparator의 성공이나 최종 동일 closure 검증
결과가 아니다. 이후 checkpoint105에서 새 raw81·AI/probe4·milestone2 native gate는
모두 PASS였고, 별도 최종 전달105에서도 같은 gate의 PASS를 관측했다.

source 생성 성공과 native 성공을 각각 기록한 실행 지도는
`reports/root-v7-validation-map.json`에 유지한다. 전체 v7 인수는 여전히 NO-GO다.

## 2026-10-01 checkpoint105와 전달105의 관측 결과

`reports/root-native-root-core-checkpoint105-jobs-summary.json`과 해당 105개 로그를
읽어 lib 699 passed·0 failed·53 ignored, 추가 104개 gate 중 103 PASS·Otherworld
한 묶음 FAIL을 확인했다. 당시 source digest는
`87a45c050d31d0f642f39f7d42ba29982fcb2ea4aa7c078fab46e6bb4d76c801`이며 모든
job의 전후 값이 같고 warnings 0이었다. 이 수치는 해당 lib 전용 snapshot의 근거다.

Otherworld 실패는 `royal-threat-probe-spawn-cause`의 source AI1/probe1을 native
비교기에서 AI0/probe1로 복원한 문맥 차이였다. `pendingNotation`, RNG cursor,
정산 후 replay nonce를 포함한 6개 진단을 보존했으며 생산 코드의 AI 전용 replay 조건을
완화하지 않았다. 새 `v7-otherworld-differential/source-context-explicit.cjs`는 기존
26개 입력·seed·before 전체 JCS를 보존하면서 두 private depth를 명시했다. 새 source
JSON SHA는 `9c28c2b5fba252d9046b71e858682a05a0c141ea5534e6acaa0f6baaa26d3dab`다.

`reports/root-native-root-prior-integration4-jobs-summary.json`과 네 로그에서는
source chance 9개·source prior 3개·Quantum 14개(ignored 2개), 새 Otherworld26의
comparator 1개가 모두 PASS였다. 이 실행과 최종 전달105의 source digest는
`cecfc80a1fbb091f3ff06d869a00962834ec70022e686862f0c2df8d736ad7b2`다.

당시 `reports/root-native-root-pr32-delivery105-jobs-summary.json`의 bytes SHA는
`7700c014560b70168d1a3063efaf3f3fea2ae00f6e0be7ab6b27c2956019fe48`다. 실제 105개
로그의 footer가 모두 이 요약과 일치했고 모든 job이 exit 0·동일 source digest를
유지했다. core `--all-targets --locked`는 unit 711 passed·0 failed·53 ignored와
integration 20 passed·0 failed였다. 추가 104개 gate도 모두 PASS지만 원문 comparator와
기존 등록·회귀 단위의 재실행이 섞이므로 독립 시나리오 수나 전체 규칙의 분모로 쓰지 않는다.
Otherworld26·continuation22·관측126·visibility20·schema2 raw81·contexts4·milestone2·
RULE324·internal21/43표본의 성공도 각 comparator 경계에 한정한다.

당시 all-targets production lib에는 `dead_code` 경고 11건이 실제 발생했고 실행 요약은
집계 행을 포함해 12행을 보존한다. 뒤이은 `reports/root-pr32-core-checks-summary.json`은
fmt `--all --check` PASS, common registry 9개·v6 migration 6개 PASS와 엄격한
Clippy exit 101을 기록했다. Clippy 로그 `reports/root-pr32-core-clippy.log`의 SHA는
`df19b80e566cd9cf6d382e0fd53299facea50c623dfcaa14a8322735fadbae05`이며 최종 footer는
engine lib 53 errors였다. 진단은 `dead_code` 11, `collapsible_if` 16,
`too_many_arguments` 13, `manual_is_multiple_of` 3, `to_digit_is_some` 2,
`manual_clamp` 2, `large_enum_variant` 2, `manual_range_contains` 1,
`needless_option_as_deref` 1, `identity_op` 1, `double_ended_iterator_last` 1이다.
이 전달 당시 경고 일괄 허용·숨김·코드 삭제는 적용하지 않았다. 당시 로컬 toolchain은
rustc/cargo 1.97.0이며 CI pin 1.96.0에서 성공했다는 증거로 쓰지 않는다.
이전 lib/focus의 warnings 0을 당시 all-targets의 warnings 0으로 재사용하지 않는다.
2026-10-02 후속 요청으로 lint를 정리했고 실제 Rust1.96 fmt·strict Clippy PASS를
메인이 관측했다. 새105·merge commit·원격 SHA·CI의 결과는 상단 후속 상태를 따르며
역사적 진단을 삭제하거나 변경 후 실행의 실패로 재표기하지 않는다.

Roulette의 cold/warm 원문 자료도 별도로 보존한다.
`reports/card-dispatch-roulette-cold-warm-source.json`의 SHA는
`009410b2ff3859f93b6f1f30bbe41bfa4eb60b4541258a1b54c490ed594df428`다. 동일 before/action의
독립 VM restore→apply는 state `560912c3…`·Position ID `f31bb38d…`로 native와 일치하고,
같은 VM의 actions→apply는 private UI clock controller를 유지해 state `3cef5568…`가 된다.
RNG/history는 같으며 서로 다른 실행 경계를 metadata 변경으로 합치지 않는다.

## 최종 인수 차단의 구분

`대기`라는 표현은 다음 네 상태로 나누어 사용한다. 유한 표본이라는 사실만으로
구현 누락을 주장하지 않으며, 구현이 저장됐다는 사실만으로 검증 성공을 주장하지 않는다.

| 구분 | 실제 경로와 현재 판단 |
|---|---|
| play 공개 조건화의 저장된 구현·국소 검증 | `v7_conditioning.rs`의 play 일괄 거절은 source prior 분기로 교체됐다. 독립 seed로 한 번 실행하고 `source_chance_trace.rs`가 실제 carry된 경로의 semantic mass·opaque identity·불변 소비·그룹을 분류한다. 미분류 draw는 callsite와 RNG cursor를 포함한 정확한 오류다. 전체 공개 history를 보존하고 공개 projection의 차이는 opaque identity 결속 외에는 거절한다. source chance 9·source prior 3 및 전달105의 bounded 회귀가 PASS지만 모든 확률 family·규칙 조합의 완전성이나 설치 봇 성공은 선언하지 않는다. |
| 이전 실패·강화된 gate의 당시 결과 | stable50 continuation22 실패와 checkpoint105 Otherworld 문맥 차이를 보존했다. 이후 continuation22·schema2 raw81·Transcendence4·milestone2·명시 문맥 Otherworld26의 comparator는 2026-10-01 전달105와 2026-10-02 최종105에서 각각 PASS였다. 이전 PASS를 새 metadata/gate에 승격한 결과가 아니며 입력 fingerprint와 실제 재실행 경계를 따른다. |
| 정확한 입력·결정·범위 오류 | activeTrolley가 남은 endMove 거절은 선택 대기 경계다. `v7_decision_actions`가 bundle 선택을 마치면 activeTrolley를 null로 바꾼 뒤 정산한다. serialized private context, 원문에 없는 legacy queue, 잘못된 ID·alias·버전·catalog, 알려진 factory가 만들지 않는 object/array 숫자 coercion, 비8×8 및 정해진 실행 한도 초과도 해당 오류 계약으로 남긴다. 그 문자열을 정상 source 경로의 포팅 누락으로 합산하지 않는다. |
| 이번 후속 확인과 제외 범위 | 과거 전달105·Rust1.97·Clippy 53진단/exit101은 역사로 보존한다. 현재 Rust1.96 fmt·strict Clippy·최종105·common과 Node/helper/구조/AST/workflow 검사는 PASS다. base 통합 merge checkpoint·원격 SHA와 같은 commit의 Windows/Linux `core_only` CI 관측 근거는 PR #32에 별도로 기록한다. 실제 봇 연동·sdist/wheel 빌드와 설치·성능 측정은 현재 범위에서 제외한다. finite callback/source slice의 일반적 한계는 실제 미구현 placeholder와 분리한다. |

현재 읽기 전용 호출 경로 감사에서는 정상 factory가 만든 cold 8×8 core 카드·RULE·
기물·행동의 추가 미구현 Unsupported placeholder를 확정하지 못했다. 이 판단은
알려진 누락 목록이며 256+aux1·27 RULE·84 기물·10 행동·3 UI family 전체의 실행
완료 선언이 아니다. 2026-10-01의 알려진 comparator 실패는 당시 bounded gate의 재검증으로
닫혔다. 변경 후 새105 결과·원격 공유·두 OS CI와 이번 범위에서 제외한 후속 검증은
별도로 남긴다. 현재 strict lint의 PASS는 전체 source 회귀 성공을 대체하지 않는다.
source prior 경로 질량은 분류된 실제 carry 경로의 IID 연속 난수 해석이며, 유한 LCG
seed 전체를 열거한 정확한 posterior나 모든 규칙 조합의 확률 완전성 증명이 아니다.

## 남은 실질 경계

다음은 source를 읽어 실제 호출 경로에서 확인한 조건과 담당자가 전달한 진행 상태다.
작성 중인 변경으로 guard가 제거돼도 해당 실행 비교가 성공하기 전에는 검증 대기로
남긴다. `UnsupportedFeature` 문자열 검색 수를 완료율로 사용하지 않는다.

| 활성 조건 또는 의미 | 현재 상태와 완료 조건 |
|---|---|
| Imperial Study를 획득한 royal이 행마를 학습한 다음 조회 | source `main:76001–76008`은 정상 capture에서 `imperialMoves`를 만들고 `main:95880/96071–96075`는 legal에 학습한 행마를 추가한다. movement 담당자가 nonempty 배열 blanket guard를 source type switch·후보 순서·flags 구현으로 대체했다. 저장된 구현과 현재 프로필의 이동/공격·전이 비교 성공은 구분한다. |
| Ice Sheet와 일반 movement modifier | Ice Sheet의 `main:103402–103411`은 상대 원거리 기물에 remaining 3을 생성하고 `main:98217/100875–100908`은 최대거리 집합으로 제한한다. blanket guard가 이 필터로 교체됐으나 최신 검증은 대기다. bishopInfiltration/killerKing/substitution/locustSwarm·portal/crown 등의 이전 broad guard도 제거돼 구현·통합 검증 대기로 옮긴다. |
| 위협의 FileSurge/RookLift quiet continuation | source `main:105245–105253` 카드 획득으로 활성화되고 `main:85475–85488`은 quiet move도 위협 probe에 포함한다. 해당 blanket guard를 제거하고 `v7_move_transition`의 중간 callback 뒤 연속 이동을 연결했다. 후속 `main:92560–92587`의 턴·선택·추가 이동과 source 위협 60사례를 현재 프로필로 대조한다. |
| 기억한 행마가 있는 Trickster와 활성 Portal | source `main:100221/99708`의 변환과 `main:72656–72663`은 정상 `tricksterMoveType`을 만든다. 공격 원문 `main:98698–98705`는 raw `tricksterMoves`와 SiegeRam highlight를 사용한다. 이전 Trickster Portal guard는 공유 source geometry projection으로 교체됐다. 저장된 구현과 현재 프로필의 공격·위협 결과는 별도로 검증한다. |
| 일반 착지의 trait·변환·승급·연속 이동 | `v7_move_transition`이 stage API와 `v7_move_piece_effects`·`v7_move_continuations`를 호출하도록 접합됐다. Spy가 승급/Transcendence 뒤 owner를 바꾸는 경우 초기 actor·현재 actor·턴·capture ledger·기보·후속 callback의 기준을 각각 대조한다. source-slice의 국소 성공은 ordinary 전체 이동이나 공개 admission의 성공이 아니다. |
| 특수 이동 실행 | swap·portal/Mistake 재귀·Merchant/Grappler·large/castle·Football/Colossus, SiegeRam pre-capture·MadKnightUndo·Roller/Metal/Medium 실행 context API가 저장됐으며 메인이 일반 전이와 연결했다. Football의 위 국소 PASS를 다른 실행의 완료로 확대하지 않는다. 생존 Colossus timer raw noop과 명시 host completion도 구별한다. |
| source sacrifice raw `{row,col}`, 완성 UI compound, AI recipient 선택 | local headless의 raw 효과 false, targeted 후보 존재에 따른 no-action 가용성, AI-controlled recipient RNG와 UI completion을 각각 검증한다. 이번 profile은 local play mode라 AI-controlled 분기의 실제 도달성을 먼저 확인한다. raw 거절을 효과 누락으로 단정하거나 compound 성공으로 모든 context를 승인하지 않는다. |
| Black Box와 untargeted raw simulation의 source-false 재시도 | raw false 결과의 partial state/RNG와 source 정책 collector를 보존하는 연결이 추가됐다. 바깥 transaction의 실패 원자성과 내부 false 결과를 구별하고 미감사 effect family·재시도 조합을 확인한다. |
| Judgment 환경 제거 및 즉시 Reaper 종료 | `judgment_remove`의 v7 early branch는 환경 callback을 호출한다. ordinary capture에도 pending defeat·Reaper destination·notation flush가 연결됐다. 뒤쪽 legacy guard를 v7 활성 blocker로 세지 않으며 Vigilance·capture pool·royal·Reaper·history의 통합 비교는 대기로 유지한다. |
| PASSIVE 형성과 Otherworld terminal 정산 | 이전 관측 표의 monoShade와 replay delta 실패를 보존한다. exported-before 재수입 raw/settled 경계의 faithful PASSIVE 30개·Otherworld 26개는 84회 실행에서 comparator PASS를 관측했다. 기존 PASSIVE 66개 자료의 현행 재생성·전체 조합·공개 whole move는 별도다. |
| 큐·count·endMove 자동 효과 | physical monster는 Otherworld 뒤 counted 경계에 연결되고 logDir 자동 이동·Lobster/timeTraveler/Gomoku/PawnStorm/delayedHazards·Taboo placement는 구현됐다. 28 callback의 원자료와 같은 before를 복원한 terminal 3개 자료를 구별한다. stable50에서 선택한 delayed/log/monster 및 복원 terminal callback의 PASS를 관측했으며 전체 28개·whole endMove의 최종 closure gate로 확대하지 않는다. source가 생성하지 않는 legacy `pendingUndead`·`pendingRuleMonsters` guard와 선택을 기다리는 activeTrolley는 별도의 오류 계약이다. |
| incoming·수명·clock | 이전 19 callback PASS와 현재 faithful 20개 source 생성·후속 native comparator PASS를 구분한다. Trolley live clock은 공통 pause API에 위임돼 이전 blanket guard가 없어졌다. Twins large/neutral은 원문 정상 target predicate가 제외하지만 후속 변환의 도달성까지 증명된 것은 아니므로 입력 불변식으로 남긴다. aiSimulationDepth/probe 억제와 실제 전체 턴을 별도로 확인한다. |
| Don Quixote의 일반 incoming | 이전 exact turn/RNG·5기물·단일 route witness 제한이 일반 callback으로 교체됐다. 16 raw callback과 별도 A* planner 1개 source 생성·native comparator PASS를 관측했다. 양색·다중 Don·풍차와 실제 A* path의 고정 경계이며 whole turn·모든 조합 완료와 구별한다. |
| Othello와 preceding callback 조합 | 이전 `turn_effects_v7::ensure_preceding_source_effects_inert`는 conveyor·잔존 Lunchbox·platform·crown·pendingGales·democracy의 유지되는 rule flag를 거절했다. 메인 담당자가 이 blanket guard를 제거했다. 후속 whole endMove 12개 comparator PASS를 관측했다. 이 12개 밖의 조합·실패 원자성과 전체 incoming은 별도로 확인한다. |
| faithful labels 소비자 이행 | 공통 `source_piece_label`은 현재 manifest의 79개 이름을 쓰지만 내장 legacy `piece_label`은 30개다. Judgment/Necromancy·승급·Desperado·queued·incoming 소비자는 원문 fallback에 맞게 이행됐으며 새 코드의 검증은 별도다. 보드 환경·기물 효과·결정 행동과 Red Card 영구 제거 로그 등에 남은 legacy 호출은 각 원문 직접 조회와 `|| type`/`|| "기물"` fallback을 보존하며 이행·full state/history/Position ID를 대조한다. legacy helper 전체를 바꿔 v6 의미를 변경하지 않는다. |
| Vigilance 부여 뒤 counter 정산 | 이전 정수 JSON 표현 제한은 공통 JS number 변환과 finite decrement로 교체됐다. 원문 `main:877–892`의 생성·decrement 경로에 따라 `1`과 `1.0`을 같은 숫자로 처리한다. capture→turn 정산의 재검증은 별도이며 정상 내부 생성 숫자를 malformed 경계로 제외하지 않는다. |
| Crown과 Missionary 배치 | `movement::open_alibaba_placement`의 v7 early branch가 실제 ground-crown 칸만 제외하도록 연결됐다. 뒤 legacy `crownRule` blanket guard는 v7 blocker로 세지 않는다. 원문 `main:100683/105442–105443`에 따라 해당 칸과 다른 빈칸의 후보 순서·RNG·spawn을 현재 프로필로 대조한다. |
| Judgment 추방기물과 MIDDLE/END draft | `v7_card_board::apply_judgment`가 정상 `judgmentExiles`를 생성하고 원문 `main:95081–95096/99283–99345`는 milestone에서 복귀를 처리한다. owned `return_judgment_exiles_for_draft`가 공통 `start_draft`의 clock 정지 전에 연결됐고 flow의 nonempty 큐 guard는 제거됐다. fresh faithful callback 10개 source 생성과 후속 native comparator PASS를 관측했다. raw 반환 callback과 추방→10/20턴→복귀·정산·기보 전체 경계를 각각 검증한다. |
| 로컬 Janggi 8×8 캠페인 | 원문 `main:66462`의 `setupJanggiSide`는 home row 7/0과 col 0..7의 로컬 보드를 만들고 `main:66616`의 setup switch가 호출한다. query 42개와 이전 after-reset 4개 PASS 뒤 비교기를 native cold newGame(seed/config)→preview→공통 normal reset→initializer→constructor 전체 Position으로 강화해 stable50에서도 4개 PASS를 관측했다. cold local single/offline 범위이며 warm preview/UI·온라인 권한은 별도다. D-009의 비8×8 제외로 처리하지 않으며 `main:95568`의 온라인 authority/recipientLegalMoves 경로는 별도 계약이다. |
| 공개 UI 창 재개와 내부 AI 후보 정책 | exact 미사용 Rule Ticket/Joker/Barricade/targeting 창을 atomic intent로 완성하는 native 확장과 ordered UI prefix가 연결됐다. AI PawnStorm의 제한과 UI 최대 64 선택을 구별한다. 관련 공개 경계 회귀와 내부 21국면·43개 sampled apply는 전달105에서 PASS였으며, 전체 UI/controller·임의 조합의 인수로 확대하지 않는다. 모순 창·소비된 카드·외부 serverAuthoritative/submitting는 명시 오류다. |

정당한 입력 경계도 남긴다. 예를 들어 ID 없는/서로 불일치하는 large alias, 비8×8
recurrence geometry, malformed color/counter/queue, 불명확한 버전·catalog, JCS가 허용하지
않는 lone surrogate는 지원 완료를 위해 임의 해석하지 않는다. source factory가 생성한
정상 8×8 값에서 실제로 도달하는지 먼저 증명한다. exotic JS object/array coercion은
정상 source 생성 값의 의미와 입력 계약을 조사한 뒤 분류한다.

UI 중간 상태는 frozen correctness adapter의 snapshot import에서 atomic card target으로
정규화하도록 거절한다. native 창 재개는 동결 브라우저 UI의 클릭 순서에서 별도 근거를
만드는 확장이다. oracle의 거절을 없애거나 AI 조합을 UI 클릭 순서로 바꿔 증명하지 않는다.

## 27 RULE과 84 기물의 호출 경로 인수

| RULE 묶음 | 고정 ID | 런타임 인수 경로 |
|---|---|---|
| 초기 배치·geometry | `chess-960`, `chess-344200`, `chess-n-pow-30`, `diagonal-chess`, `monochrome-chess` | 초기 배치 외에 movement/threat, 형성·승급·변환 후 유지되는 제한과 raw runtime 추가를 검사한다. |
| 보드 자동 변화 | `conveyor`, `periodic-collapse`, `platform`, `crown` | endMove/incoming 경계·착지·강제 승급·왕관 이전/복구·붕괴 제거 후속을 검사한다. |
| 위치·특수 목표 | `portal`, `high-ground`, `highway`, `transcendence`, `capture-the-flag`, `football` | 순서 있는 입출구·거리/높이·사다리/벽, 상대 관측, 중립 기물과 승리 판정을 검사한다. |
| 위험·시간 환경 | `black-hole`, `rule-bombs`, `monster`, `winter-kingdom` | 반복 제거·HP·Vigilance·회귀·royal/Reaper, 종료 조기 반환과 다음 incoming을 검사한다. |
| 행동·정보·캡처 정책 | `acceleration`, `macho-chess`, `cool-guy`, `saturation`, `camouflage-color`, `mistake`, `recycling`, `revelation` | 행동 크레딧/강제 연속·공격 거절/실패·숨김·다시 사용 가능한 capture pool·승급/결과를 검사한다. |

84종은 `site-20260928.json.pieceTypes`의 고정 집합으로 감사한다. 생성되었는지,
`pieceAbilityType`/royal identity가 무엇인지, generic 이동과 특수 행동·자동 반응 중
어느 호출 경로를 사용하는지 기물별로 기록한다. source inventory의 초기 6종이나
공개 관측 84종은 이동·전이 84종의 실행 근거와 구별한다. 기물 변환 뒤에도 footprint,
ability/traits, ongoing 상태, hidden identity, RNG·종료가 맞아야 한다.

## 메인 담당자가 실행할 검증

아래 표에서 전달105·focus4 등의 PASS는 2026-10-01까지의 해당 입력·source closure에
한정한 기록이다. 2026-10-02 변경 후 새105의 최종 결과는 상단에서 별도로 확인하며,
과거 비교기의 성공을 새로 생성한 source 입력이나 변경한 native 소스에 자동 승격하지 않는다.

아래 자료 루트는 Windows `%APPDATA%/Accelerate/reports`, CI
`$RUNNER_TEMP/Accelerate/reports`다. WSL에서는 Windows 루트를 변환하거나 승인된
외부 reports를 명시한다. 동결 source 루트는 `ACCELERATE_SITE_BASELINE` 또는 해당
generator가 지원하는 `_LATEST`에 지정한다. 원문·영수증·전체 실행 로그는 Git 밖에 두고
검토된 범위·digest·실행 명령·owner만 문서와 기존 검사에 남긴다.

기본 필터는 `cargo test -p augment-chess-engine --lib <filter>`다. source receipt가 필요한
검사는 `-- --ignored --nocapture`와 아래 환경변수를 함께 사용한다. 필터에 여러 callback이
있으면 개별 receipt 경로를 바꾸어 유한하게 실행하고 원문 예외·실패·skip을 그대로 보고한다.

개별 실행의 정확한 env→generator→출력→필터→case 수→raw/whole/restored 경계는
외부 `reports/root-v7-validation-map.json`에 유지한다. 저장된 revision 10은 이전 source와 실행 근거를 읽은
53개 ignored 검사(원문 비교 49·진단 3·외부 입력 없는 대기 1), 환경변수 선택형 17개
검사와 Blood 19개 외부 Python comparator의 지도다. Colossus timer/history 22개와
RULE 환경의 ignored 이행, Janggi 생성의 새 cold boundary 및 실제 84/43/50 실행 입력을
반영한 역사적 snapshot이다. revision 11은 저장하지 않았으며, revision 10을 최종 source의
검증 근거로 사용하지 않는다. 최종 전달 실행은 `reports/root-pr32-delivery105-jobs.json`과
`reports/root-native-root-pr32-delivery105-jobs-summary.json` 및 개별 로그에 결속한다.
이후 비교기·생성 경계가 바뀌면 기존 PASS의 적용 범위를 다시 대조한다.
mismatch assertion이 없는 진단 필터와 env 없이 조기 반환하는 검사를
영수증 parity PASS로 세지 않는다. 큐의 최신 개별 32개 job 지도는
`reports/root-queued-native-jobs.json`이다. Lobster 3개와 capture callback 4개는 queued
comparator, 나머지 closure 25개는 end-move comparator로 연결하며 whole endMove 완료와
구별한다. 프로필 stamp가 이전 값인 recipe와 헤더 없는 raw 자료는 생성 provenance 및
현재 composite identity를 먼저 확인한다.

| 대상 | source recipe / 영수증 | native 필터·환경변수 또는 다음 조치 |
|---|---|---|
| 전체 차분 진단 | tracked `v7-native-differential.cjs`→`v7-native-differential/source-cases.jsonl`, `report.json`의 faithful 21사례 | `source_pinned_v7_internal_differential_21_cases`; `ACCELERATE_V7_INTERNAL_CASES`, `_INTERNAL_SOURCE_REPORT`, `_INTERNAL_REPORT`. source 입력 보존 audit는 ready true·differences 0이고 source 21개 생성은 완료됐다. oracle-only의 전체 coverage 미충족 exit 1과 실제 생성 오류를 구분한다. 최종 전달105에서 내부 21국면의 sampled apply 43개는 PASS였다. 이전 native 실패와 JSONL·manifest는 당시 경계로 보존하며, 유한 표본 밖의 `completeRuleCoverage=false`를 유지한다. |
| source inventory | `projects/augment-chess/tests/differential/v7-source-inventory.cjs` | 같은 source scope로 정의/제시/획득/초기 생성/도달성만 갱신한다. native parity는 별도다. `node --test .../v7-source-inventory.test.cjs`는 구조 계약 검사다. |
| 이동 query·위협·flow | `v7-movement-query/movement-query-source.cjs` 452개, `v7-threat-flow/source-probe.cjs` 60개 | `frozen_v7_movement_queries_match_complete_source_payloads`에 `ACCELERATE_V7_MOVEMENT_RECEIPTS`, `frozen_threat_flow_when_receipts_are_supplied`에 `ACCELERATE_V7_THREAT_FLOW_CASES`. fresh sparse/editor query·공개 전체 후보·원문 내부 collector를 구별하고 descriptor 순서·불변성과 거절·후속 state를 검사한다. royal probe 내부 move 실행은 outer whole-turn 영수증과 구별한다. |
| 로컬 Janggi·nominal query | `v7-movement-query/janggi-query-source.cjs` 42개, `nominal-placement-source.cjs` 48개 | `frozen_v7_local_janggi_move_capture_and_attack_queries_match_source` / `ACCELERATE_V7_JANGGI_QUERY_RECEIPTS`, `frozen_v7_nominal_capture_and_placement_queries_match_source` / `ACCELERATE_V7_NOMINAL_PLACEMENT_RECEIPTS`. Janggi의 ordered movement/hints·royal/capture·64칸 attack과 attacker 없는 capture/Alibaba placement의 bool·query 불변성을 별도로 비교한다. 원문 sparse snapshot의 조회 자료이며 카드 효과·전체 이동·온라인 권한의 완료와 구별한다. source 생성은 두 자료 모두 성공했고 최종 전달105에서 native Janggi 42·nominal 48은 모두 PASS였다. 이전 nominal 실패는 당시 경계로 보존한다. |
| geometry query | source geometry 72사례 | `frozen_geometry_queries_when_receipt_is_supplied`; `ACCELERATE_V7_RULE_GEOMETRY_CASES`. 이전 프로필의 국소 PASS와 현재 프로필 재생성을 구별하며 이동·위협 전체를 대신하지 않는다. |
| RULE 초기 생성·환경 정산 | `v7-new-game/collect-rule-opening-matrix-faithful175.cjs`의 324개와 `v7-rule-environment-review/collect-restored-source-cases.cjs`의 36개 | `v7_all_single_rule_openings_match_pinned_source_matrix_when_supplied` / `ACCELERATE_V7_RULE_OPENING_SOURCE_MANIFEST` 및 `_MANIFEST_SHA256`, `frozen_rule_environment_when_receipt_is_supplied` / `ACCELERATE_V7_RULE_ENVIRONMENT_CASES`. faithful schema 2 manifest SHA `5742903ffa86f68e90e1bad4cf8e252c58fcdbf98ec9db04aa4922ff1d6807b5`와 전체 Position 324개, 원래 before 36개를 정확히 복원한 환경 callback 자료의 stable50 PASS를 관측했다. 이전 raw36 실패 경계와 source 입력을 보존하며 초기 생성·raw callback을 whole runtime turn 완료로 합치지 않는다. |
| normal/chaos weighted draft·첫 이동 | `v7-threat-flow/weighted-draft-source-probe.cjs`→`weighted-draft-cases.jsonl` 2스타일·총 6 whole source 전이 | `frozen_weighted_normal_and_chaos_second_acquisition_when_receipts_are_supplied`, `frozen_normal_and_chaos_first_play_move_when_receipts_are_supplied`; `ACCELERATE_V7_WEIGHTED_DRAFT_PLAY_CASES`. 원문 자연 draft 2회·첫 이동 1회의 full envelope·양측 관측과 conditioned public frame·독립 future seed 94·p/q 합성을 구별한다. source 2스타일·총 6전이 생성은 성공했고 second-acquisition 2와 first-move 2의 stable50 native PASS를 관측했다. 이전 43 실행의 first-move 실패는 당시 경계로 보존한다. |
| Blood 공개 계약 | `v7-campaign/source-recipe.cjs`→`source-cases.json` 19개와 `source-manifest.json` | `compare-native.py`에 source JSON을 stdin으로 전달한다. 설치한 native의 Position·ordered legal·양측 관측·actor 거절·선택 전이·RNG/history·snapshot/payload bind를 비교하며 별도 cargo env gate로 대체하지 않는다. Python comparator의 faithful profile·composite identity는 이행됐고 메인이 실행한다. |
| 카드 등록·직접 효과 | `v7-card-coverage/board-direct-faithful26.jsonl`, `faithful-authority6/`의 준비 자료와 family별 JSONL | `card_registry::tests`, `card_effects::tests`, `card_target_hints::tests`; 각 family ENV receipt와 common finish/replay를 별도로 대조한다. |
| Judgment 추방 복귀 | `v7-card-coverage/judgment-return-probe.cjs`→`judgment-return.jsonl` 10개 | `frozen_judgment_returns_when_receipts_are_supplied`; `ACCELERATE_V7_JUDGMENT_RETURN_CASES`. env가 있으면 fresh faithful callback의 full envelope·RNG/history·Position ID·return count를 비교하고 없으면 조기 반환한다. raw callback 10개 native PASS를 관측했다. MIDDLE/END 공통 draft 접합과 전체 milestone 전이는 별도로 검증한다. |
| passive·첫수 | `v7-passive-differential/source-opening-interactions.cjs`의 faithful 30개, `source-passive-goldens-faithful.cjs`의 2개, `v7-passive-first-move/source-probe.cjs` | `v7_card_passive::tests`, `opening::tests`, `temporary_source_passive_matrix`; `ACCELERATE_PASSIVE_SOURCE_RECEIPT`. 기존 PASSIVE 66개 획득과 interaction 30개·golden 2개·Checker 4개를 서로 대체하지 않는다. 연속 VM과 exported-before 재수입의 raw/settled context도 구별한다. |
| White/Black/Miracle/Barricade | `v7-white-box-direct-seed19.json`, `v7-black-box-direct-seed19.json`, `v7-choice/source-e5ed84fc-miracle-barricade.json` | `v7_card_choice::tests`; `ACCELERATE_V7_WHITE_BOX_SOURCE_RECEIPT`, `_BLACK_BOX_SOURCE_RECEIPT`, `_CHOICE_SOURCE_RECEIPT`. 다른 schema의 Clone receipt를 혼용하지 않는다. |
| topology 남은 조합 | `v7-card-coverage/topology-remaining-cases.cjs`의 29 source 사례 | `frozen_direct_effects_when_receipt_is_supplied`; `ACCELERATE_V7_CARD_TOPOLOGY_CASES`. Extinction의 literal `"draw"`도 normal null draw와 구별한다. 이전 canceling 종료 replay 정산 실패를 보존하고 수정 뒤 stable50의 29개 direct comparator PASS를 관측했다. |
| 결정 행동 | `v7-card-coverage/decision-actions-probe.cjs`, `decision-actions.jsonl` 26 source 사례 | `frozen_special_decisions_when_receipts_are_supplied`; `ACCELERATE_V7_DECISION_ACTION_CASES`. source 생성과 후속 26개 native comparator PASS를 구분해 기록한다. 전체 ordinary/public 실행과 별도다. |
| 캡처 콜백 | `v7-capture-reactions/source-probe.cjs` 10사례·19단계 | `frozen_capture_callbacks_when_source_receipt_supplied`; `ACCELERATE_V7_CAPTURE_SOURCE_RECEIPT`. 현재 faithful 10사례·19단계 native comparator PASS를 관측했다. completeMove/public apply 근거는 별도다. aggregate 영수증은 파일 바이트 SHA를 기록하며 각 Position의 JCS 한도를 늘리지 않는다. |
| 보드 자동·왕관 | `v7-board-automata-port/source-probe.cjs`, `source-extra-probe.cjs`, `source-ai-probe.cjs` | `frozen_board_callbacks_when_source_receipts_supplied`, `frozen_crown_capture_callbacks_when_source_receipts_supplied`, `frozen_crown_ai_context_when_source_receipt_supplied`; `ACCELERATE_V7_BOARD_SOURCE_RECEIPTS`. |
| 환경 위험·종료 | `v7-board-hazards/source-probe.cjs`의 10 source 사례 | `v7_board_hazards`, `v7_rule_bombs`, `v7_passive_terminal`. 생성·digest 확인은 완료했으나 최신 native 비교는 대기다. |
| 큐·endMove 보완 | `v7-queued-closure/source-probe.cjs`의 28사례 계획(Lobster/timeTraveler/Gomoku/PawnStorm/delayed/log/monster) | `external_queued_effect_source_receipt_matches_full_position` / `external_end_move_callback_source_receipt_matches_full_position`; `ACCELERATE_V7_QUEUED_FIXTURE` / `_END_MOVE_FIXTURE`. 각 callback JSON을 유한하게 비교하고 새 mapping·공통 count 연결도 확인한다. |
| 승급·Spy | `v7-promotion/source-probe.cjs` 36개, `landing-source-probe.cjs` 17개, `atomic-origin-source-probe.cjs` 24개, `spy-full-movement-source-probe.cjs` 4사례 | `frozen_promotion_callbacks_match_full_state_rng_and_history`, `frozen_landing_promotion_boundaries_match_full_state_rng_and_history`, `frozen_spy_full_movement_fresh_import_matches_state_rng_history_and_position_id`와 각 실행 지도 ENV를 사용한다. 기존 36개 PASS는 이전 프로필 범위다. raw pending 창·atomic/deferred·landing slice와 Spy fresh 5단계 전이를 구별하며 continuous VM 4사례를 fresh native parity로 세지 않는다. |
| incoming·수명 | `v7-turn-entry/incoming-lifecycle-source.cjs`의 현재 20 callback 사례 | `frozen_incoming_lifecycle_callbacks_match_full_positions`; `ACCELERATE_V7_INCOMING_LIFECYCLE_CASES`. 이전 19개 PASS와 현재 20개 source/native comparator PASS는 각각 해당 실행 경계의 증거다. raw callback의 before-microtask 경계와 실제 전체 턴을 별도로 검사한다. |
| Don·Othello 후속 | Don 16개, A* planner 1개와 Othello 12개의 별도 source recipe/출력 | `frozen_don_callbacks_match_full_positions` / `ACCELERATE_V7_DON_CASES`, `frozen_don_shortest_astar_matches_full_position` / `ACCELERATE_V7_DON_PLANNER_CASE`, `external_othello_turn_boundary_matches_whole_position` / `ACCELERATE_V7_OTHELLO_TURN_CASES`. Don raw turn-entry와 pure planner의 실제 A* 확장·path, Othello 전체 turn boundary의 checkpoint를 구별한다. source 생성과 Don 16·planner 1·Othello 12의 각 native PASS를 별도로 기록한다. |
| Judgment milestone 전체 접합 | `v7-turn-entry/milestone-judgment-source.cjs`의 MIDDLE 반환/END carryover 2개 | `frozen_milestone_judgment_returns_match_full_positions`; `ACCELERATE_V7_MILESTONE_RETURN_CASES`. 실제 첫 picks 2개 뒤 합성 완료 10/20턴 입력을 복원하고 maybeStartMilestoneDraft→startDraft·정산 전체 Position을 대조한다. 자연 10/20턴 완료 증거와 raw 반환 10개 PASS로 대체하지 않는다. 새 source 2개 생성은 recheck3에서 성공했고 whole after Position/RNG/history/ID native 비교는 checkpoint105와 전달105에서 PASS였다. |
| 특수 이동 실행 | `v7-move-execution-port/`의 stationary 22·missionary 10·swap 21·action 23·Football 15·large/castle 20·context 23·Siege/Mistake/wrapper 28개 recipe | `ACCELERATE_V7_MOVE_EXECUTION_RECEIPTS`와 각 comparator를 연결한다. 8개 raw family 162개의 recorded provenance13 source 생성과 stable50 native PASS를 관측했다. Colossus timer/history 22개도 stable50 PASS다. raw helper·internal execute·whole public move·trusted Host ticket의 서로 다른 경계를 유지하고 timers-noop에서 예약 없는 completion을 강제로 호출하지 않는다. |
| 일반 이동의 trait 후속 | `v7-move-piece-effects/source-probe.cjs` 18사례(12 stage + Chimera 6), typed `cases.jsonl` | `frozen_move_piece_effect_boundaries_match_full_state_rng_and_history`; `ACCELERATE_V7_MOVE_PIECE_EFFECT_CASES`. 후속 18개 source-slice native PASS를 관측했다. 착지 승급 전후·생존 후·Chimera API의 국소 비교와 ordinary transition 전체를 구분한다. |
| 이동 연속 정산 | `v7-card-coverage/move-continuations-probe.cjs`의 22 합성 사례 계획 | `frozen_move_continuations_when_receipts_are_supplied`; `ACCELERATE_V7_MOVE_CONTINUATION_CASES`. stationary direct HP의 Desperado와 모든 HP의 Frenzy 차이, 이동/승급 뒤 계속 이동을 전체 전이와 연결한다. |
| Otherworld 복귀 | `v7-otherworld-differential/source-context-explicit.cjs`→`source-context-explicit-faithful.json` 26사례 | `temporary_source_otherworld_matrix`; `ACCELERATE_OTHERWORLD_SOURCE_RECEIPT`. source AI/probe 두 private depth를 명시 복원한 raw/settled 전체 Position·RNG·history·ID comparator가 focus4와 전달105에서 PASS다. 이전 원자료·문맥 불일치 실패는 보존하며 callback 자료를 whole public turn 인수로 확대하지 않는다. |
| 관측·fog·양자 | visibility 10×2, quantum state 16, quantum UI 10, seed 19 full observations 6 | `source_pinned_v7_visibility_and_fog_surfaces_match_bounded_cases`, `source_pinned_quantum_state_rng_history_and_identity_match`, `source_pinned_v7_seed19_full_observations_match_regenerated_first_play` 및 실행 지도 ENV/필수 SHA256을 사용한다. 2026-10-01까지 관측한 bounded full 관측 126·visibility 20·양자 state 16·full observations 6·UI 10의 실제 native PASS를 개별 경계로 기록하며 ordinary landing 전체 증거로 확대하지 않는다. |
| 공개 경계 | scalar/eager/page intent와 양측 공개 ObservationIR | `v7_action_`, `v7_adapter_actions`, adapter/native/Python 소비자 검사. opaque token 재사용·stale·비누출·페이지 원자성을 실제 facade로 확인한다. |

이전 프로필의 고정 21사례 JSONL SHA-256은
`77876121c5158d006263f2803781cbcffc4a41cdd90e1c20f7dcbca4fa2d4131`이다.
자연 생성·합성 입력과 제한된 apply 표본을 구분하며 이 파일의 성공만으로 카드 전체·
기물 전체·모든 행동·복합 규칙 인수를 선언하지 않는다. generator의 실제 오류는 name,
message, 단계와 상태 차이를 보존한다. 미지원 사례를 filter에서 제외해 성공률을 높이지 않는다.

## checkpoint와 완료 판정

1. source authority와 유형별 API를 고정한 뒤 정상 생성 값의 활성 guard를 닫는다.
   타입·카드·RULE별 장부에 코드 경로와 필요한 전제조건을 남긴다.
2. 기능 담당자의 국소 검사와 source callback 비교를 실행한다. 다른 유형과 연결되는
   호출 순서, 실패/RNG 재시도, alias·identity 갱신은 메인 담당자가 통합한다.
3. 전체 ordered 후보·선택한 family의 scalar bind·public apply·reject를 비교한다.
   직접 효과 뒤 full state/RNG/replay/history/result와 양측 관측을 대조한다. partial page의
   stop 이유와 `exhausted`를 구분하고, 오류·취소·stale 때 원본과 cursor 원자성을 확인한다.
4. 유형별 조합은 단일 카드 snapshot의 복제 대신 위험 메커니즘별 실제 상호작용 입력으로
   보강한다. 생성 시 source 함수가 호출됐다는 증거와 합성 전제조건을 구분한다.
5. 검증된 checkpoint를 커밋·push하고 PR #32의 원격 SHA를 확인한다. 이번 범위의
   fmt/clippy/test·관련 Node/common 검사와 의도한 stage의 구조 검사를 수행한다.
   Windows/Linux `core_only` CI는 dispatch·실행·관측한 결과를 별도로 기록한다.
   PR #28 base 통합의 merge commit도 확인한다. 실제 Python 봇 연동·sdist/wheel 빌드와
   설치·성능 측정은 현재 범위에서 제외한 미실행·미검증 후속 항목으로 남긴다.

faithful 초기 상태 324개의 2026-10-01 scoped PASS와 이전 대표 복합 설정 7개, acquisition·관측의 국소 근거는 각각 유효한
범위에서 보존한다. faithful 초기화·composite identity와 변경된 규칙·helper·schema·테스트 엔진의 의존 fingerprint가 영향을
주면 다시 검사한다. 이전 성공을 최신 변경의 성공으로 표기하지 않는다. 현재 장부에는
공개 play source prior의 저장된 구현과 bounded gate는 위 실행 범위에서 검증됐다.
이번 PR #32 후속 검증 완료는 새105와 관련 회귀·strict lint·구조 검사, base 통합,
Windows/Linux `core_only` CI와 원격 공유의 실제 결과를 확인해 판정한다. 새105·CI·원격이
pending인 동안 완료로 표기하지 않는다. 제외한 봇/wheel/성능을 포함한 전체 프로젝트 GO는
선언하지 않는다.
