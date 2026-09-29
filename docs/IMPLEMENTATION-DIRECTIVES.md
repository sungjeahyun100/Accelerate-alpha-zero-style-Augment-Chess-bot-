# v7 재설계 구현 지시서

이 문서는 [결정 기록](DECISIONS.md)과 [전체 구현·증거 현황](IMPLEMENTATION.md)을 실제 변경 단위와 검사 가능한 계약으로 풀어 쓴다. 충돌하면 사용자 결정과 D-001~D-017이 우선한다. 각 절의 “완료”는 해당 영역의 checkpoint이며 마지막의 전체 코드 GO와 다르다. 구현 담당자는 실제 소스와 동결 원문을 확인해 세부 source 의미를 채우고, 근거가 없는 경우 Unsupported로 남겨 원인과 범위를 기록한다.
향후 모노레포에서 재사용할 언어 독립 객체형 규칙 어댑터의 범위와 Accelerate 이관 지시는 [RULE-ADAPTER-HANDOFF](RULE-ADAPTER-HANDOFF.md)에 둔다. 여기의 P2 파일 경계는 현 구현의 책임 지도이며 공유 패키지의 타입·디렉터리를 고정하지 않는다.

## 목표 버전과 증거 경계

- 최종 코드 GO의 정답은 `bridge/catalog/site-20260928.json`에 고정된 `augment-site-20260928-e5ed84fcf8e72a24`, headless profile `accelerate-headless-semantic-v7`, 로컬 8×8 normal·chaos·grand 및 공개 카드 256장이다. 원문 client 전체 SHA-256은 `e5ed84fcf8e72a24e6a8cfeb9050787387a616c55184e6501fca2077e302c45c`다.
- PR #29 게임 어댑터는 완료된 **검증용** 실행층이다. 그 768개 카드×모드 표면 조사는 특정 국면·후보의 bounded probe이고 전체 규칙의 완성 증명이 아니다. 동결 JS/사이트를 탐색 시의 자동 대체 엔진으로 사용하지 않는다.
- v6 snapshot·replay·설치 wheel 입력과 기존 세 float32 ONNX bundle은 별도 호환 경로로 보존한다. v6 검사로 v7 최종 GO를 주장하지 않는다. 버전이 다른 규칙·catalog·관측 정책·인코더·모델은 조용히 재해석하지 않는다.
- GO/NO-GO는 코드·계약·동등성·자원 한도의 판정이다. 실제 학습 캠페인, 장시간 자가대국, 대전 승률, 모델 승격, 동적 Hypernetwork는 이번 완료 범위 밖이다. 작은 synthetic optimizer step은 저장·복구 코드 검사에만 사용한다.

## 작업 분리와 통합 순서

각 기능을 한 담당자가 소유한다. 동일한 기능을 두 구현자가 병렬로 작성하지 않고, 서로 다른 계층을 동시에 진행한다. 독립 N-version은 아래 검증 담당자가 별도 reference로 작성할 수 있으나 운영 소유자가 두 명이라는 뜻은 아니다. 공통 `lib.rs`, `transition.rs`, Python 모델 export와 검색 연결은 계약을 확인한 뒤 통합 담당자가 순차적으로 연결한다.

| 순서 | 코드 소유 경계 | 다른 작업에 제공할 계약 | 국소 완료 증거 |
|---|---|---|---|
| P0 | `bridge/catalog/`, `bridge/protocol/`, 동결 oracle | rules/catalog/observation/source/profile 버전과 실행 범위 | source hash·정의 수·기존 v6와 v7 분리 |
| P1 | `rust-engine/src/geometry.rs`, `state.rs` | checked geometry, 단일 기물 identity, 파생 occupancy | 직사각형·붕괴·합성 resize·원자성 |
| P2a | `move_program.rs`, `movement.rs`, `variant_movement.rs` | typed 행마 프로그램과 후보 provenance | 원점/부모/형제/거리/JUMP/SHIFT |
| P2b | `card_registry.rs`, `draft.rs`, `card_effects.rs` | definition/instance/RULE·activation/turn policy | v7 registry와 source 정책, v6 보존 |
| P3 | Rust `transition.rs`, `observation.rs`, `lib.rs` | lossless Position→허용 관측·apply/result | v7 legal/apply/full state/RNG, 기존 v6 |
| P4 | Python `ir.py`, `encoding.py` | 한 공개 ObservationIR와 typed 입력 | 비누출·history·geometry·ordered target |
| P5 | `network/mask_resnet.py`, `entity_transformer.py`, `typed_context.py` | 같은 후보 logit/value 계약 | mask·순열·FiLM·LoRA·유한 출력 |
| P6 | `network/artifacts.py`, `bridge/runtime/`, `bridge/native/` | versioned ONNX와 typed 추론 | 설치 wheel의 ORT/tract 수치·오류 |
| P7 | Python `search.py`, `inference.py`, `replay.py`, `training.py`, `cli.py` | 공개 ISMCTS·유한 session·versioned 기록 | 세 모드의 bounded 실행·취소·복구 |
| P8 | `tests/differential/`와 CI | 독립 source parity/참조 구현 결과 | 최종 같은 SHA의 두 OS 검사와 gap 해소 |

P1/P2a/P2b/P4/P5/P6/P7은 분리 가능한 경계에서 병렬 진행한다. P3은 안정된 P1/P2 계약을 연결하고 P8은 각 기능 완료 뒤 누적 검증한다. 각 단계는 1~3개 논리 커밋 원칙의 검증된 공유 단위로 묶되, 중간 실패를 최종 완료로 표시하지 않는다.

## P0. 정답과 지원 범위의 고정

1. v7 client·parser·metadata를 각각 SHA-256으로 확인하고 원문 번들을 Git에 넣지 않는다. v6와 v7의 source·policy·schema를 다른 버전으로 유지한다. 원문 실행에는 확정 headless profile을 사용하고 queued settlement·RNG·render hook 중 규칙 변경을 보존한다.
2. v7 256개 공개 카드, 보조 정의 1개, 실제 선택 가능한 RULE 풀과 카탈로그 전체 기물 식별자를 **정의/실제 생성/도달 가능한 기능**으로 구분해 coverage 장부를 만든다. 개수만 같거나 ID가 같다는 사실은 v6↔v7 의미 동등성이 아니다.
3. 비교 입력은 source가 생성한 초기/draft 및 전제조건을 만족하는 합성 국면을 함께 사용한다. worker의 useful-action filter, AI target truncation, 1~2개 후보 접두 probe를 전체 legal로 취급하지 않는다. Portal Gun 같은 **선택 순서가 다른 입력**이 하나의 후보로 누락되는지 별도 확인한다.
4. 비교는 legal 후보, 잘못된 행동 거절, apply 뒤 정규화된 **전체** state·history·result·RNG를 묶어 판정한다. presentation 필드 제외는 정확한 field list와 원문 근거를 남긴다. private positionId 단독 일치는 동등성 증거가 아니다.

## P1. 모델 독립 geometry와 엔진 상태

- `Coord {row:i32,col:i32}`와 `BoardGeometry(min_row,min_col,height,width)`가 포함/인덱스/역인덱스를 checked 연산으로 제공한다. geometry 면적·오버플로·0 크기는 진입 때 거부한다. 8/7/64를 범용 순회·경계·버퍼 크기로 사용하지 않는다. 사이트 시작 배치·홈·승격의 8×8 규칙 숫자는 해당 ruleset 설정에 남긴다.
- 정사각형 배열은 기본 저장 표현을 강제하지 않는다. `PieceStore`의 기물 하나가 identity·color·type·anchor·명시 footprint offset을 소유한다. anchor는 점유 칸이라는 가정을 두지 않는다. `OccupancyIndex`는 단일 원천에서 파생하고 충돌·경계·붕괴 셀과 identity 일관성을 검증한다. 관측이나 신경망 plane/token을 엔진 저장 형태로 사용하지 않는다.
- source v7 붕괴는 **8×8 외곽 배열 크기를 유지하면서** 사용 가능 칸·기물 상태를 바꾸는 규칙으로 구현한다. 실제 외곽 확장·축소는 별도 합성 profile이며 공식 사이트 동등성 근거가 아니다. resize는 새 상태를 완성·검사한 뒤 원자적으로 교체한다. 잘려 나갈 footprint·연결·예약 효과의 처리가 명시되지 않으면 적용 전에 거부한다.
- geometry revision 또는 full snapshot identity가 바뀌면 이전 action·cursor는 stale로 거부한다. 실패한 전이는 원본 Position·RNG·history를 그대로 둔다. 기존 v6 8×8 wire DTO는 별도 유지하고, 새 상태를 선택하는 경계에서 손실 없는 변환 또는 명시적 Unsupported를 적용한다.
- 검사: 음수/양수 시작 좌표 직사각형, 가장자리, 큰 offset overflow, multi-cell 단일 identity, 서로 충돌하는 footprint, 비가시/붕괴 칸, 확장/축소 뒤 참조와 cursor, 실패 원자성, 기존 v6 결과를 확인한다. 같은 예외를 중복 fixture로 쌓지 않는다.

## P2. 조합형 행마와 카드

### 행마 프로그램

- C++ 초안의 `moveChunk`를 typed forest로 구현한다. 각 root는 원래 출발점 `originalOrigin`을 보존하고 child는 현재 단계의 `currentOrigin`을 받는다. 형제 노드는 같은 부모 상태에서 독립적으로 평가하며 앞 형제의 도달·도약 상태를 다음 형제에 누적하지 않는다.
- 부모가 요구한 거리·활성 조건을 통과해야 child가 실행된다. `activateSquare`가 만든 각각의 목적지는 독립 실행 후보가 되고, 여러 단계 경로는 최종 선택된 경로 하나에 대해서만 apply한다. 생성 도중 보드·RNG를 변이시키지 않는다.
- MOVE/TAKE/TAKEMOVE/BOTHTAKEMOVE/CATCH/JUMP/SHIFT의 입력·목표·충돌·착지·포획을 typed primitive로 둔다. JUMP는 적격 대상을 넘은 뒤 빈 칸에 착지하고 형제별 도약 상태를 초기화한다. SHIFT는 첫 점유 기물에서 ray를 멈춘 뒤 두 anchor를 맞바꾸되 두 footprint의 경계·상호 겹침·제3기물 충돌을 모두 검증한다. 원문의 특정 swap 효과가 다르면 별도 효과 handler로 둔다.
- `maxDistance`, 기물/게임 제약의 구체 값은 typed descriptor에 두고 source의 순서와 원래 출발점 해석을 보존한다. 동일 public destination을 가진 여러 내부 경로는 순서·포획·후속 상태가 다를 수 있으므로 raw provenance를 유지한다. MCTS에서 같은 **공개 의도**만 병합하며 서로 다른 실제 선택·효과는 합치지 않는다.
- 검사: root/child/형제, 거리 경계, 하나의 root가 여러 후보를 내는 경우, 중간 착지의 가림/포획, 둘 이상의 큰 기물 SHIFT, 실패 후 불변성과 source에서 관찰 가능한 기물별 반례를 사용한다.

### 카드와 RULE

- 카드 **정의**는 고정 registry의 id/effect/category/activation/turn/selection을 소유하고, 손패 **instance**는 instanceId/used/recovering/owner/slot 상태를 소유한다. active RULE은 일반 손패 사용 여부와 별도 상태로 표현·투영한다. `CardType`, `CardActType`, turn policy, selection 및 `PieceConstraint`/`GameConstraint`는 검증된 타입으로 다룬다.
- v7 catalog에는 256 공개 카드와 보조 `shotgun-king` 정의 1개가 있다. source 메타데이터의 PASSIVE/ACTIVE/강제 첫 이동·선택/턴 소비를 registry와 실행 context에서 해석한다. `ACTIVE_FORCED`는 인스턴스·국면 문맥이며 정적 정의의 세 번째 activation 값으로 추측하지 않는다.
- 행동 요청의 비용·actor·source 순서를 엔진이 결정한다. 클라이언트가 보낸 cost나 used flag를 권위로 삼지 않는다. 초기/중간/종반 draft, 자동 passive/OPENING, ACTIVE_FORCED, RULE 선택·유지, card 효과 및 공통 정산이 source와 일치해야 한다. 카탈로그 `turnPolicy`는 정적 설명 정보이며 실제 `finishCard`의 조건부 턴 종료·추가 행동 정리를 대신하지 않는다. 예를 들어 v7 원문은 `summon-colossus`의 종료와 `miracle`/`brainwash`/조건부 `zugzwang`의 extra-action 정리를 구분하고, `trolley`/`premove`에도 별도 종료 분기가 있다. 구현은 source 기반 settle policy를 적용한다. 미구현 effect는 명시적 Unsupported로 남기고 전체 GO 장부에서 제외하지 않는다.
- 기존 v6 분기는 source v6 의미로 보존한다. v7의 registry 정책은 v7일 때만 적용한다. action 생성과 apply는 분리하며, apply는 모든 조건·선택 순서를 다시 검사한다. 실패 시 원본·RNG 불변을 확인한다.

## P3/P4. 허용 관측에서 두 모델의 공통 입력까지

1. Rust의 full Position은 환경 실행 내부에만 남긴다. viewer별 Observation·공개 history·실제 UI hint·공개 카드/descriptor·후보 public intent를 allowlist로 투영한다. 새 raw state 필드를 무검토로 버리거나 복사하지 않는다. `positionId`, private RNG, full Position, 숨은 상대 손패/기물, 조건부 제안 확률은 모델 특징이 아니다.
2. Python `ObservationIR`의 의미 버전·policy/catalog/source hash를 고정한다. 같은 IR에서 A의 spatial/condition과 B의 entity/condition을 만든다. **공개 화면이 구분하는** 빈칸, fog처럼 공개적으로 차폐가 표시된 unknown 칸, 붕괴·사용 불능 칸, batch padding을 서로 다른 표현·mask로 구분한다. 화면이 빈칸처럼 보이는 숨은 점유는 private 상태를 이용해 unknown으로 표시하지 않는다. 실측 없이 Python encoder를 Rust로 이전하지 않는다.
3. 공통 typed 입력에는 기록 category/numeric/coord·공간 유효성/record mask, 관계 endpoint/category/numeric/mask, 후보의 **순서 있는 선택 노드 tree** category/numeric/coord/parent/order/target_index/node mask/candidate mask를 둔다. 후보 구조를 마지막 목적지 한 칸으로 축약하지 않는다. A의 spatial/layout mask와 양쪽 FiLM condition도 명시한다. 동적 B/N/R/A/T/H/W의 지원 한도를 encoder와 artifact에 일치시킨다.
   `typed-input-v1`의 A 입력은 다음 순서의 21개다: `spatial` f32[B,6,H,W], `layout_mask` bool[B,1,H,W], `record_category` i64[B,N,4], `record_numeric` f32[B,N,8], `record_coord` f32[B,N,2], `record_spatial_valid` bool[B,N], `record_mask` bool[B,N], `relation_index` i64[B,R,2], `relation_category` i64[B,R,2], `relation_numeric` f32[B,R,4], `relation_mask` bool[B,R], `candidate_category` i64[B,A,T,4], `candidate_numeric` f32[B,A,T,8], `candidate_coord` f32[B,A,T,2], `candidate_coord_valid` bool[B,A,T], `candidate_parent`/`candidate_order`/`candidate_target_index` 각 i64[B,A,T], `candidate_node_mask` bool[B,A,T], `candidate_mask` bool[B,A], `condition` f32[B,8]. B는 `spatial`과 `layout_mask`만 제외한 같은 순서의 19개다. 공간 6채널은 empty/unknown/hole/occupied/own/opponent, `layout_mask`는 실제 geometry 칸(붕괴 포함)을 true, batch padding을 false로 둔다. record 0은 global, 후보 node 0은 root, 연결되지 않은 target index는 -1이다. 이 배열의 의미 버전·vocabulary·허용 source/policy hash를 manifest에 고정한다.
4. 새 typed IR의 history 기본은 `public-history-summary-v2`: 전체 공개 event 수·JCS digest·actor/change 집계와 최근 8개 공개 event의 board-change **절대 좌표**를 보존한다. v2 공개 event에는 당시 geometry가 없으므로 과거 좌표를 현재 board 범위에 강제로 맞추거나 당시 geometry를 추측해 기록하지 않는다. 기존 v6 `public-history-summary-v1`은 호환 경로로 유지하며 두 버전을 자동으로 섞지 않는다. digest는 재현·동일성 metadata이며 임의 hash 정수를 모델 feature로 쓰지 않는다. 원본 public trace/replay를 보존한다. 모델의 value는 `observation.viewer`인 **실제 decision actor**의 관점이고 물리적인 turn과 혼동하지 않는다.
5. 검사는 hidden state/RNG만 바꾼 같은 공개 관측의 동일 IR, viewer별 가림, 카드 순서·ordered target, entity/관계 일관 순열, padding/후보 batch 분할, geometry 변화, unknown 필드의 명시 실패를 포함한다. N-version Python 참조는 이 검사의 독립 기대값으로만 사용하고 운영 encoder는 한 곳에 둔다.

## P5. 비교 가능한 A/B 모델

- A는 가변 H×W mask-aware ResNet이다. 기존 8 block×128 channel과 v6 고정 입력 모델을 호환 기준으로 보존한다. 유효 칸 mask를 convolution 후·pooling·정규화와 candidate head까지 일관되게 사용하여 padding과 붕괴 셀이 값을 오염시키지 않게 한다.
- B는 entity/관계 Transformer의 첫 실험 설정을 pre-LN 4 block, hidden 128, 4 heads, FFN 512, ReLU로 고정한다. record·관계·geometry·공개 효과를 누락하지 않고, padded token과 relation endpoint를 mask한다. A/B가 동일 typed context encoder와 후보 scorer의 계약을 사용하되 backbone 계산은 각자 구현한다.
- FiLM은 공개 condition을 두 모델에 명시적으로 받아 graph 내부에서 특징을 조절한다. LoRA는 base와 별도 static adapter다. A의 기존 convolution rank/alpha 8, dropout 0 기본값은 A 설정으로 남기고 B는 Q/V projection rank 8/alpha 8/dropout 0을 첫 실험 artifact 설정으로 기록한다. B의 위치/rank는 모델별 설정이지 전역 불변값이 아니다.
- 출력은 candidate별 finite `policy_logits [B,A]`와 후보와 독립적인 actor 관점 `value [B,1]`이다. mask=false 후보의 logit은 검색에서 무시한다. value가 후보 개수·순열에 의존하면 실패한다. 후보 순열 시 logit만 동일 순열로 바뀌고, entity 순열과 관계 endpoint 동시 치환은 결과를 보존해야 한다. 빈/전부 가린 공간 입력과 동적 shape에서도 NaN/Inf를 내지 않는다.
- 동일 공개 IR과 후보 계약 위에서 파라미터·처리량·메모리·오차를 기록한다. 실제 학습/대전 전까지 A 또는 B의 우위를 선언하지 않는다.

## P6. Artifact, ONNX, maturin과 두 backend

1. v6 `onnx-policy-value-v2`의 세 float32/고정 8×8 bundle과 기존 바인딩은 strict 호환으로 유지한다. 새 A/B는 별도 versioned typed manifest와 입력 이름·dtype(float32/int64/bool)·named dynamic 축을 사용한다. 모든 input의 순서·shape·mask/condition 의미, 출력 의미, encoder/policy/catalog/rules/descriptor hash, base/adapter/model-config hash를 기록한다.
2. A/B 각각 PyTorch→ONNX opset 18을 export한다. tract 호환이 확인된 표준 연산만 사용하고 unsupported op를 다른 의미의 근사치로 자동 변환하지 않는다. dynamic 축도 명시된 B/N/R/A/T/H/W 범위 안에서만 받는다. oversized·음수 index·NaN/Inf·dtype/shape/hash 오류는 추론 전에 거부한다.
3. PyO3/maturin은 설치 wheel의 owned NumPy/typed 배열을 native 추론 adapter로 넘기는 얇은 경계다. 모델 파일/배열의 수명·GIL·concurrency·입력 한도·오류 전파를 검사한다. 기본 ORT, 명시 선택 tract이며 다른 backend나 JS 모델로 자동 fallback하지 않는다. 규칙 엔진 crate는 PyO3·ONNX·MCTS 의존성을 얻지 않는다.
4. LoRA merge는 **원본 base와 adapter를 보존한 복사본**에서만 수행한다. 분리/병합 수치와 condition 변경 효과를 PyTorch↔ONNX↔실제 ORT/tract에서 비교한다. 동적 Hypernetwork adapter를 고정 LoRA처럼 병합하지 않는다. 처리 성능은 별도로 실측하고 수치 통과를 속도·승률 향상의 근거로 쓰지 않는다.
5. 같은 빌드의 sdist→wheel을 Windows/Linux CPU 환경에 실제 설치한 뒤 두 backend, A/B, v6 호환, 오류 경로를 확인한다. 로컬 소스 import나 export 파일 생성만으로 완료라 하지 않는다.

## P7. 공개 정보 탐색·기록·유한 실행

- ISMCTS의 선택과 정보집합 키는 Observation/public history/실제 공개 hint만 사용한다. private Position은 환경 transition과 source-valid particle 내부에만 있고 네트워크·정책 키에 전달하지 않는다. 상대 공개 선택 카드를 숨은 카드로 재표본화하지 않는다. chance posterior는 독립 seed와 source prior/proposal 확률 `p/q`를 써서 가중하며 설명할 수 없는 chance family는 Unsupported다.
- 탐색 자식 노드에 루트에서 조건화한 `belief_summary`를 그대로 전달한다면 이를 **루트 posterior 문맥**으로 기록한다. 자식의 공개 history 길이와 `trace_steps`가 같아야 한다고 가정하거나, 새 관측으로 조건화하지 않은 값을 자식 posterior로 표시하지 않는다. 자식별 posterior가 필요한 기능은 source-valid 전이·제안 확률을 검증하고 별도 의미 버전으로 도입한다.
- 실제 decision actor가 바뀔 때 value 부호/backup을 변경한다. 중복 public intent는 MCTS에서 합칠 수 있으나 원시 ordered 선택과 effect별 실행 action은 보존한다. legal iterator·progressive widening은 정확한 전체 후보 의미를 유지하고 예산 때문에 조용히 truncate하지 않는다.
- 노드·particle·proposal·batch·시간/step/메모리 한도는 유한하다. 시간 예산 종료와 게임 terminal, 취소와 미완료 rollout을 구분한다. clock을 주입해 deterministic finite-work 의미 검사와 실제 시간 watchdog 검사를 분리한다. 현재 두 OS CI의 CHAOS belief reconstruction 5초 실패를 timeout 수치 확대나 작업 축소만으로 녹색 처리하지 않는다.
- replay/dataset에는 public frame·결정 actor·후보 정책·result의 terminal/unfinished, seed 및 source/rules/catalog/observation/encoder/model/adapter 버전을 보존한다. snapshot/replay는 v6/v7을 교차 자동 승격하지 않는다. 동일 계약 checkpoint의 optimizer/RNG/cursor **resume**과 새 IR에 맞는 가중치 일부 이전 **warm-start**를 분리한다.
- CLI/self-play는 유한 game/step/time/worker/memory 한도·출력 root·취소/자식 종료를 둔다. 예산 소진은 unfinished, SIGINT는 취소, 오류는 오류로 기록한다. 테스트는 짧은 synthetic optimizer 저장/재개와 bounded rollout까지만 실행한다.

## P8. 독립 검증과 최종 코드 GO

검증에는 세 층이 있다. 첫째, unit/property/contract 검사는 geometry·registry·IR·모델·FFI의 국소 불변식을 본다. 둘째, 독립 N-version은 geometry·점유·행마의 작은 참조 구현을 **테스트 전용**으로 만들어 Rust 운영판과 비교한다. 셋째, source-pinned JS oracle은 v7 실제 규칙의 legal/reject/apply/full state/history/result/RNG 정답이다. 두 참조 결과가 충돌하면 다수결 대신 동결 source와 계약을 추적한다. 운영 추론·탐색 경로에는 참조판을 연결하지 않는다.

| Gate | 통과 조건 | 미충족 시 판정 |
|---|---|---|
| source/coverage | v7 source SHA/profile 검증, 256 카드·세 모드의 전제조건별 분기, RULE·기물·phase/종료의 source-reachable 지원 장부에 알려진 누락 없음 | 해당 영역 NO-GO |
| rules | 초기·draft·legal/reject·apply 뒤 full state/history/result/RNG 차분 비교, stale/실패 원자성, v6 호환 | Rust 규칙 NO-GO |
| 공개 경계 | viewer/hidden 차이의 IR 불변성, ordered 선택과 실제 bind 일치, unknown 명시 실패 | AI 입력 NO-GO |
| 모델·배포 | A/B 동일 정보·출력 의미, LoRA/FiLM·finite shape, 실제 설치 wheel의 ORT/tract 수치·오류 | 모델/추론 NO-GO |
| 탐색·session | 세 모드 공개 조건화/유한 탐색·취소·unfinished·replay/checkpoint/CLI 검증 | 실행 흐름 NO-GO |
| 통합 | 의도한 변경 stage 뒤 구조 검사, Rust fmt/clippy/test, Python/Node 관련 검사, 최종 동일 SHA의 Windows/Linux CI 성공을 **관측** | 전체 코드 NO-GO |

검사 실패·skip·미분류/미지원 source 분기·adapter 후보 누락은 보고서의 gap으로 유지한다. 생성 로그와 큰 차분 자료는 저장소 밖 `%APPDATA%/Accelerate/reports` 또는 CI `$RUNNER_TEMP/Accelerate/reports`에 둔다. 작은 재현 입력도 장기 계약을 증명할 때만 기존 검사에 추가하며 일회성 대형 fixture를 누적하지 않는다. v7 최종 GO는 위 모든 gate에 대해 **같은 최종 commit**의 관측 근거가 있을 때만 선언한다.
