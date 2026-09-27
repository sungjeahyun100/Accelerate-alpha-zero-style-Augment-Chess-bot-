# 전체 구현 계획과 실행 checkpoint

현재 전체 판정은 **NO-GO**다. 이는 코드 완성·정답 동등성에 대한 판정이며 실제 학습 또는
봇 실력의 판정이 아니다. 사용자 채택 요구는 [DECISIONS](DECISIONS.md), 실제 실행 계약은
[runtime-v1](../bridge/protocol/runtime-v1.md)에 있다. 아래 관측한 검사와 계획을 혼동하지 않는다.
작업 브랜치는 feature/full-stack-implementation, 출발점은 bfc85c886b489f21f6f1037bc291ab0157c3dc6f다.

## 채택 범위와 작업 순서

1. 최초 동결 사이트 본체의 8x8 normal/chaos/grand, 공개 256 카드 catalog를 기준으로
   Position/Action/Observation/history/RNG/result 계약과 offline oracle을 구현한다.
2. 순수 Rust 독립 규칙 엔진에 전체 기물·카드·RULE·특수 행동·초기/draft/종료를 포팅하고
   client oracle과 normalized full state/result/RNG를 비교한다. worker useful filter를 legal로 쓰지 않는다.
3. bridge의 독립 PyO3 crate를 maturin으로 패키징하고 immutable 객체, 배열 소유권, 오류,
   snapshot roundtrip, JSON/direct 호출 의미와 GIL 경계를 검증한다.
4. Python에서 viewer 관측·공개 history·belief와 행동 의미 payload를 먼저 encoding한다.
   raw full Position·private RNG·positionId가 특징에 들어가지 않는 검사를 유지한다.
5. ResNet·FiLM·LoRA를 구현한다. FiLM 조건은 ONNX 입력/그래프에 남기고 static LoRA 병합은
   원본을 보존한 복사본에서 한다. 기본 ort와 명시 선택 tract의 artifact·shape/dtype·유한값·수치 차이를 검사한다.
6. 실제 decision actor·확률·숨은 정보·공개 hint에 기반한 bounded ISMCTS, legal iterator와
   progressive widening, batch inference·취소·실행 예산을 구현한다.
7. bounded self-play/replay/CLI·dataset 기록·optimizer/evaluation checkpoint와 계약 version을
   연결한다. 이어 CI/독립 오류 경로 리뷰·전체 code GO checklist를 수행한다.

실제 학습·장시간 self-play·대전 성능·모델 승격은 이번 구현에서 제외한다. 작은 synthetic
optimizer/export와 bounded rollout은 구현 경로 검증이며 학습 성능으로 보고하지 않는다.
Hypernetwork는 향후 extension의 생성·적용·병합 가능 여부만 준비한다.

## 관측한 checkpoint와 아직 필요한 증거

| 영역 | 실제 구현/관측 | 코드 완성·의미 coverage의 남은 조건 |
|---|---|---|
| 사이트·계약 | 동결 loader, v1 schema/JSON validators, 실제 client 초기/draft/전이/종료 adapter 구현 | 전체 256 효과·선택 순서·특수 phase·관측 분류·lazy action 경계 미완료 |
| Oracle 검사 | depth0 actual passive settlement 포함 Node 계약/통합 12개 통과 | 짧은 기본 환경 검사. renderAll의 potion 정리, terminal microtask와 presentation RNG 범위의 보완이 필요함 |
| 과거 fixture | 349개, 757 sampled action을 최신 worker에 비교 | worker hash 불일치; 최신 client 전체 정답을 대신할 수 없음 |
| Rust 규칙 | 순수 Rust 구현 진행, 담당자 로컬 compile/단위 검사 checkpoint 존재 | 전체 catalog legal/reject/full-next-state/result/RNG 비교 미완료 |
| PyO3/maturin | 담당자 Linux wheel/FFI checkpoint 검사 존재 | Windows native 검증과 최종 공통 history/관측 재검증은 별도 확인 필요 |
| Python 모델·encoding | 확정된 EncoderSpec 15개 필드, root Windows 모델 7개 검사 통과 | 최종 공개 관측 allowlist와 실제 탐색 입력 통합 필요 |
| ort/tract | Linux 실제 두 backend에서 확정 계약의 FP32 base/adapter 48개 수치 비교 통과 | Windows 및 최종 source 규칙/탐색과의 통합 실행을 확인해야 함 |
| ISMCTS·replay·CLI | 공개 trace/particle filter/availability PUCT 구현 중 | 실제 3개 mode 통합, replay·checkpoint·CLI와 bounded 실행·오류 경로 검증이 남음 |
| CI | 기존 구조/fixture validator 실행 경로 유지 | 이번 feature 최종 로컬 검사·원격 CI 관측은 아직 전체 완료 증거가 아님 |

다른 영역의 작은 test count나 compile 성공은 해당 checkpoint이며 전체 GO로 승격하지 않는다.
CI 요청/관측한 성공, Windows/Linux 확인, 모델 export 수치, 실제 semantic coverage를 따로 기록한다.

Linux sdist에서 만든 wheel 설치 검사 결과는 native 6 + model 7 + runtime 6 = 19개 통과,
skip 0이다. full history와 summary history의 public-intent 모델을 각각 24개 사례로 검사했고
최대 절대 오차는 둘 다 1.2218952178955078e-6이다(`atol=1e-5`, `rtol=1e-4`). 보고서는
`%APPDATA%\Accelerate\reports\native-bot\Linux`에 보관한다. 이 배포 경로와 수치 검사는
현재 native 규칙 경계의 checkpoint이며 전체 카드 의미 coverage의 증거는 아니다.

## 구현 연결 순서와 완료 기준

작업별 담당자는 하나로 유지하고 공통 계약은 담당자끼리 확인한다. 다음 순서는 의존 관계이며,
각 단계의 통과를 전체 구현 완료로 보고하지 않는다.

| 작업 | 전달할 구현 | 완료를 판단하는 코드 증거 |
|---|---|---|
| 1. 동결 source와 oracle | 실제 client 실행, public projection, 정확한 legal iterator, queued settlement | rule에 영향을 주는 UI 정리와 RNG를 보존하고 동결 source에서 legal/reject/state/result/RNG 비교가 재현됨 |
| 2. 순수 Rust 규칙 | 초기·draft·256 카드·RULE·84개 catalog type과 reachable 상태 전이 | 채택된 8x8 normal/chaos/grand의 reachable 기능에 Unsupported나 대체 구현이 남지 않고 source 비교가 통과함 |
| 3. Python 연동 | immutable Position/Action, owned 배열, direct/JSON 경계, maturin sdist/wheel | Windows/Linux에서 실제 배포 wheel을 설치하고 동일 의미·오류·소유권을 확인함 |
| 4. 인코딩과 모델 | 공개 관측·이력, ResNet, static LoRA, FiLM, checkpoint/export | 숨은 상태 불변성, adapter 전용 갱신, 복사본 병합, 명시적 condition 입력과 계약 hash를 확인함 |
| 5. Rust 추론 | 기본 ort, 명시적 tract, strict artifact 검증 | 두 실제 backend에서 base/adapter와 여러 B/A shape의 FP32 오차·condition 효과·오류 경로를 확인함 |
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

CI의 실행 의존 파일은 위 동결 client·worker와 parser다. index는 최초 발견 시점의 provenance로
보존하며 현재 HTML이 바뀌어도 새 main URL을 따라가거나 baseline을 갱신하지 않는다. oracle이
읽지 않는 index를 실행 hash gate에서 제외하는 경우 원래 provenance를 유지하고 이유를 보고한다.

현재 presentation hook 분리는 의미 보존 검토가 완료되지 않았다. `renderAll`은
`pruneBoardPotionEffects`를 통해 실제 기물 상태를 정리하며, terminal replay microtask에는
추가 정산과 notation ID용 RNG 소비가 있다. DOM 유무에 따라 animation/log의 RNG 소비도 달라진다.
이 경로를 모두 무조건 no-op으로 처리한 결과는 전체 future RNG/state 동등성의 근거가 될 수 없다.
후속 adapter는 rule 정리와 queued settlement를 보존하고 presentation RNG의 비교 범위를 명시해야 한다.

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

최종 checkpoint의 depth0 계약/통합 검사는 12개 통과다. 3종 실제 draft lifecycle, passive
settlement, 초기 20개 이동/공개 hint, RNG snapshot 복원, wrong actor 거부, private RNG/hidden
기물 변화에 대한 관측 불변성, 실제 킹 포획 terminal을 검사했다. schema 예시는 18 valid+6 invalid,
전체 8 schemas를 검사한다. 변경된 strict public event·safe integer/depth/node 경계도 기존 검사에 보탰다.

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
