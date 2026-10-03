# 증체 봇 모델 입력 아키텍처

이 문서는 2026-10-02 현재 코드와 D-018의 새 모델 입력 계약을 구분한다. 설계 채택이나 Python 클래스의 존재는 학습·ONNX 배포·운영 탐색의 완료 증거가 아니다.

## 기존 구조 분석

| 영역 | 현재 기능과 재사용 | D-018의 추가 경계 |
|---|---|---|
| `ObservationIR` | 검증한 공개 관측·이력·geometry·descriptor만 소유한다. private RNG·full Position·실행 ID를 모델 특징에서 배제한다. | IR은 가변 geometry를 유지한다. 모델의 8×8 제한을 IR에 넣지 않는다. |
| `TypedEncoder` | source catalog/policy를 묶고 두 계열에 동일한 후보 행동 tree·mask·기존 8 scalar condition을 제공한다. | record에는 필드 단위 node가 있으므로 의미 단위 entity 투영을 추가한다. |
| `mask_resnet` | 가변 spatial 6채널, typed context, 후보 scorer, FiLM, convolution LoRA와 배포 경로가 있다. | 기존 배포 모델은 유지하고 고정 8×8 동적 기물 embedding 경로를 추가한다. |
| `entity_transformer` | record mask·관계 attention bias·좌표·Q/V LoRA·FiLM·후보 scorer가 있다. | 기존 record의 필드 node 대신 IR에서 직접 만든 의미 entity를 사용한다. |
| `typed_context` | category/numeric/좌표 embedding, relation message, 후보 scorer와 `[B,1]` value를 공유한다. | 새 두 계열도 같은 `CandidateScorer`와 typed 후보 입력을 사용한다. |
| artifact/export | `typed-policy-value-v1` family·encoder hash·condition·mask·value 관점을 검증한다. | 새 입력 버전을 기존 manifest로 가장하지 않는다. 새 ONNX manifest/export/runtime 검증이 필요하다. |
| MCTS evaluator | `TypedInformationSetSearch`는 설치 native `ProductionEvaluator`와 동일 spec digest를 요구한다. | 새 Python 모델을 운영 탐색에 연결하는 새 배포 계약이 필요하다. 탐색 알고리즘과 기존 signature는 바꾸지 않는다. |

## 확정한 계약과 투영

`architecture.py`의 `ArchitectureSpec`은 `fixed8-spatial-v1`, `entity-token-v1`, `global-context-v1`, 기존 `typed-input-v1`의 8 scalar 조건, 기존 후보 행동 tree와 `candidate-policy-value-v1` 출력을 함께 기록한다. `metadata()`와 `digest()`는 family, 버전, catalog/rules, 입력 이름·shape·dtype·mask·value 관점, FiLM·LoRA 구분을 반환한다. 기존 typed checkpoint·ONNX manifest의 버전이나 hash는 변경하지 않는다.

`EntityTokenEncoder`는 먼저 기존 `TypedEncoder`로 공개 IR과 행동을 검증한다. logical piece 하나당 하나의 entity를 만들고 별도 `[E,H,W]` 점유 tensor에 footprint를 기록한다. 카드·공개 규칙/효과·지형/portal·공개 이력 요약/최근 사건은 의미 단위 entity다. category에는 kind/type/owner/phase/state/descriptor, float 속성에는 counter·visibility·anchor·footprint 등을 넣는다. category 해시는 `entity-token-v1`의 고정 4096 bucket 방식이다. 충돌 가능성을 제거하는 명시 vocabulary는 후속 버전에서 필요하다. 관계 index는 global 연관과 위치 일치/효과 연관을 기록한다. batch padding은 entity·relation·candidate mask로 구분한다.

`Fixed8x8SpatialEncoder`는 입력 geometry가 `(0,0,8,8)`인지 검사한다. `Fixed8x8ResNet`은 piece embedding을 점유 칸에 평균 집계하고 점유 수, anchor, footprint, empty/unknown/hole, terrain 상태를 `[B,C,8,8]`로 구성한다. 여러 entity가 같은 칸에 있으면 합산과 점유 수로 평균 내며 overwrite하지 않는다. 카드·규칙·효과와 기존 8 scalar condition은 global context encoder를 거쳐 FiLM에 들어간다.

`EntityTokenTransformer`는 동일 entity tensor와 padding mask, 관계·좌표 attention bias, global context/FiLM, 정적 Q/V LoRA 블록을 사용한다. 두 모델 모두 `configure_training("base"/"adapter")`로 LoRA 학습 파라미터를 분리하고 기존 `CandidateScorer`로 `[B,A]` padded logits와 관측 viewer 관점의 `[B,1]` value를 반환한다. 후보 target은 공개 좌표 또는 card reference가 매칭될 때 entity로 재연결하며, 매칭되지 않는 참조는 `-1`이다. 후보 tree 자체와 행동 순서·mask는 기존 typed 계약이다.

## 현재 구현과 남은 검증

새 Python 경로는 `batch_projected_positions()`와 두 모델의 `evaluate(batch)`까지다. 기존 `PublicEncoder`, `TypedEncoder`, `PolicyValueNetwork`, `MaskResNetPolicyValueNetwork`, `EntityTransformer`, `ProductionEvaluator`, `TypedInformationSetSearch`의 함수명·인자·반환 형식은 바꾸지 않았다. 기존 검색과 학습은 이전 artifact를 계속 사용한다.

새 모델의 ONNX export, 복사본 LoRA 병합·checkpoint, native ort/tract manifest와 `ProductionEvaluator` 연결, 실제 학습·대전은 아직 구현되지 않았다. ONNX에서는 entity/occupancy/후보 mask를 명시적 입력으로 유지하고, `ModelBatch` 변환은 그래프 밖에서 수행해야 한다. 새 family의 수치·성능·동결 v7 전체 규칙에 대한 주장은 export와 native 검증을 통과한 뒤에만 한다. 공개 관측이 공급하지 않는 새 정보는 encoder에서 추측하지 않는다.

다음 순서는 (1) 새 투영의 실제 public v7 및 합성 큰 기물 테스트 실행, (2) 두 모델의 batch·오류·후보/value 경계 검사, (3) 새 versioned checkpoint와 ONNX export·수치 비교, (4) native evaluator와 기존 MCTS 경계 연결, (5) 유한 self-play·학습 smoke test다. 중단 대국을 draw label로 저장하지 않는다.
