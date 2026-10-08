# 단계 경계 기반 value 학습 실험

전체 게임 self-play는 terminal value를 얻기 위해 끝까지 탐색해야 한다. 이 실험은 엔드게임을 실제 결과로 먼저 학습하고, 동결한 엔드게임 모델을 미들게임의 target으로 사용한다. 이는 backward curriculum이며 gradient 역전파를 뜻하지 않는다.

## 실제 경계와 상태 전이

동결 v7 Basic(normal)과 Chaos에서는 양측 turnsTaken의 최솟값이 10, 20에 도달하면 엔진이 MIDDLE, END 드래프트를 연다. 총 10 ply와 다르다. 턴을 소모하지 않는 카드 행동도 있어 코드가 ply parity로 경계를 추정하지 않는다. 공개 mode=draft 및 draft.phase를 관측하며, 드래프트 종료 후 남은 공개 카드 선택 창까지 소스 adapter에서 적용한 뒤 완성된 다음 단계 상태를 평가한다.

합성 시작 상태는 정상 초기 상태에서 출발한다. 모든 행동은 legal_intents, bind_public_intent, apply를 거친다. 이 경로가 passive 효과, active 카드, chance 효과와 Chaos bundle 선택을 엔진 그대로 처리한다. 생성 중 드래프트 선택은 공개 정보 policy network의 결정적 decision action이며 chance node로 바꾸지 않는다. stage rollout의 드래프트 선택은 MCTS를 쓴다. seed, format, 선택한 드래프트 intent와 카드 행동, 전체 공개 행동 이력, 전후 position revision, 실제 move count 및 assumed ply를 기록한다.

현재 공개 adapter에는 임의 기물 제거를 위한 합법 전이가 없다. 따라서 보드를 직접 수정하지 않고 합법 행동 후보의 실제 transition을 검사한 다음 포획 행동에 가중치를 준다. 요구된 양측별 제거 수·정수 계수, 제거 후 엔드게임 15±n 및 미들게임 정확히 10수 walk 반복, active 카드 사용/미사용 분기 batch는 아직 지원하지 않는다. 합성 assumed ply는 metadata이며 엔진 history를 덮어쓰지 않는다. terminal 이전에 지정 stage에 도달하지 못하면 실패한다.

## 학습 target

Student는 기존 information-set MCTS의 root visit 분포를 policy target으로 기록한다. 실제 terminal이 먼저 오면 승패를 target으로 삼는다. END 드래프트가 열리면 기존 탐색으로 카드 선택을 모두 마치고 frozen teacher로 완성된 다음 상태의 공개 관측을 평가한다. sample마다 value_target_source와 배포된 teacher ONNX 모델 SHA-256을 기록하고, dataset에는 teacher manifest SHA-256과 base model hash도 둔다. 이 식별자는 training checkpoint 파일의 byte hash와 구분한다. 부호는 sample actor와 teacher viewer를 비교하며 ply 홀짝을 쓰지 않는다. 양측 공개 trace를 초기 상태부터 축적해 belief 탐색에 제공하므로 full hidden state는 모델 입력에 들어가지 않는다.

엔드게임은 terminal까지 진행하고 teacher를 사용하지 않는다. Staged dataset은 기존 terminal replay와 분리한 accelerate-staged-v1 형식이며 train --staged가 encoder/family, 값, teacher identity, root visits, 공개 관측을 검증한다. source=synthetic을 기록해 향후 실제 self-play position과 혼합할 수 있다. 자동 혼합 비율 조정은 구현하지 않았다.

## 작은 실행

외부 ARTIFACT_ROOT를 지정하고 기존 init/export 명령으로 배포 manifest를 준비한다. 다음 환경 변수는 실제 생성된 파일 경로를 가리켜야 한다.

    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" stage-sample --stage end --manifest "$STUDENT_MANIFEST" --seed 37 --run-id end-small
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" stage-sample --stage middle --generate-only --manifest "$STUDENT_MANIFEST" --seed 37 --run-id middle-position
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" stage-sample --stage middle --generate-only --source-provenance "$ARTIFACT_ROOT/datasets/middle-position/position-provenance.json" --manifest "$STUDENT_MANIFEST" --seed 37 --run-id middle-position-replay
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" stage-sample --stage end --generate-only --source-provenance "$ARTIFACT_ROOT/datasets/middle-position/position-provenance.json" --manifest "$STUDENT_MANIFEST" --seed 37 --run-id end-from-middle
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" train --staged --base "$BASE_CHECKPOINT" --replay "$ARTIFACT_ROOT/datasets/end-small/samples.json" --steps 1 --batch-size 2 --run-id end-teacher
    # end-teacher/base.pt를 기존 export 명령으로 배포하여 END_MANIFEST를 동결한다.
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" stage-sample --stage middle --manifest "$STUDENT_MANIFEST" --teacher-manifest "$END_MANIFEST" --seed 37 --run-id middle-small
    python -m accelerate_chess.cli --model-family mask-resnet --artifact-root "$ARTIFACT_ROOT" stage-sample --stage middle --manifest "$STUDENT_MANIFEST" --teacher-manifest "$END_MANIFEST" --seed 38 --run-id middle-benchmark --compare-full --full-max-plies 4096

각 실행에는 유한 action/search budget이 있다. 실패는 성공 dataset으로 저장하지 않고 reports/stage-sample-failure.json에 이유를 기록한다.
source-provenance 재생은 각 원본 position revision, draft phase, 적용 후 revision 및 최종 공개 통계를 대조한다. end 요청에 완료된 middle provenance를 주면 먼저 재생한 뒤 그 상태에서 합법 행동으로 END 드래프트까지 이어 간다.

## MCTS 진단 모드

`stage-sample --mcts-profile`은 기본적으로 꺼져 있다. 켜면 root별 수치 진단을 성공 보고서의 `mcts_profiles` 또는 실패 보고서의 `search_diagnostics.mcts_profile`에 기록한다. 실패 시에도 완료된 simulation과 중단된 simulation을 구분한다. 자세한 simulation은 최대 16개, 각 simulation의 depth는 최대 8개, 각 depth의 page는 최대 16개만 저장한다. 집계 count와 시간은 이 상세 저장 한도와 관계없이 전체 root를 센다. node 식별자는 해당 root 안에서만 쓰는 정수다. 공개 intent와 비공개 상태·카드·난수 상태는 profile에 저장하지 않는다.

`action_stream_seconds`는 Python에서 source stream 생성과 native `next_page` 호출을 감싼 시간이며, `candidate_generation_seconds`는 해당 depth에서 같은 호출을 합한 값이다. page의 `examined`와 `returned`는 native가 보고한 수치이고, `candidate_count`는 공개 필터와 중복 제거 뒤 해당 depth에서 선택 가능한 수다. `new_edges_added`는 tree에 새로 등록한 edge 수다. `action_stream_reopened`는 같은 정보 노드를 재방문해 stream을 다시 연 경우이고, `inference_repeated`는 같은 node와 정렬된 candidate key 집합을 다시 평가한 경우다. key 원문은 profile에 출력하지 않는다.

`observe_seconds`는 `position.observe` 호출, `public_projection_verify_seconds`는 Python 공개 관측 검증·복사다. typed 평가의 `observation_ir_seconds`는 `ObservationIR.from_public`, `encode_seconds`는 `TypedEncoder.encode`, `batch_build_seconds`는 batch padding·입력 구성, `inference_seconds`는 evaluator 호출, `postprocess_seconds`는 결과 검사와 softmax를 포함한다. inference batch 시간은 `batches`에 기록하고 각 depth의 `batch_index`로 연결한다. 한 batch가 여러 simulation을 포함하면 그 시간을 simulation마다 복사해 합산하지 않는다. `wall_seconds`는 root 전체 경과 시간이고, simulation `wall_seconds`는 다른 generator가 실행되는 대기 시간도 포함한다. simulation `execution_seconds`는 해당 generator가 실제로 재개되어 실행된 시간이다. 겹치거나 포함된 시간을 단순히 더해 전체 시간으로 해석하지 않는다.

`intent_projection_seconds`는 native action의 공개 intent 추출·검증이고, `canonical_intent_seconds`는 그 결과에 대한 `canonical_json` 호출이다. `allowed_filter_seconds`와 `edge_lookup_update_seconds`는 각각 공개 힌트 필터와 tree edge 조회·추가를 잰다.

## 측정, 위험과 확장

stage-sample은 wall time, 생성 및 stage action 수, samples/s, transitions/s, MCTS nodes/sample, terminal/bootstrap 비율, teacher inference 횟수/시간, Linux peak RSS, piece count/type, king 위치, 공개 move/turn count와 합법 행동 수를 JSON으로 남긴다. --compare-full을 추가하면 같은 seed·모델·탐색 설정으로 기존 full selfplay를 staged 실행의 wall budget 동안 돌린다. full arm이 terminal에 닿지 못했다면 미완료를 그대로 기록한다. 이 방식은 wall budget을 맞추지만 동일 sample 수를 보장하지 않는다. GPU memory와 full arm 단독 peak RSS 계측은 아직 없다. 실측 전에는 속도 개선을 주장할 수 없다.

동결 v7 실행 경로의 일부 카드/효과는 UnsupportedFeature를 반환할 수 있다. 그런 실패를 다른 seed로 조용히 덮지 않는다. 후속 작업은 합법 synthetic removal 경계, 반복 walk, end teacher 실측 smoke, 동일 예산 full-game 비교, 실제 self-play replay와 분포 비교를 완성해야 한다. Grand는 이번 범위 밖이며, 추후 소스의 28개 후보와 양측 6회 draft를 통과하는 별도 stage policy가 필요하다.
